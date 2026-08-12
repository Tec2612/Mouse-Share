use ms_protocol::{read_message, write_message, FramingError, Hello, HelloAck, Message, PROTOCOL_VERSION};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::Duration;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("framing error: {0}")]
    Framing(#[from] FramingError),
    #[error("connection closed before the handshake completed")]
    ClosedDuringHandshake,
    #[error("peer rejected the handshake: {0}")]
    RejectedByPeer(String),
    #[error("protocol version mismatch: local={local}, peer={peer}")]
    VersionMismatch { local: u16, peer: u16 },
}

/// Performs the application-level `Hello`/`HelloAck` exchange (distinct
/// from — and layered on top of — the TLS handshake, which has already
/// authenticated the peer's identity by the time this runs; this exchange
/// is about protocol compatibility and session bookkeeping, not trust).
/// Symmetric: both the connecting and accepting side call this the same
/// way, since each direction of the handshake is identical.
pub async fn handshake<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    local: Hello,
) -> Result<Hello, SessionError> {
    write_message(stream, &Message::Hello(local.clone())).await?;

    let peer_hello = match read_message(stream).await? {
        Some(Message::Hello(h)) => h,
        Some(_) => return Err(SessionError::RejectedByPeer("expected Hello as the first message".into())),
        None => return Err(SessionError::ClosedDuringHandshake),
    };

    let accepted = peer_hello.protocol_version == PROTOCOL_VERSION;
    let ack = HelloAck {
        protocol_version: PROTOCOL_VERSION,
        accepted,
        reason: (!accepted).then(|| format!("expected protocol version {PROTOCOL_VERSION}")),
    };
    write_message(stream, &Message::HelloAck(ack)).await?;

    if !accepted {
        return Err(SessionError::VersionMismatch { local: PROTOCOL_VERSION, peer: peer_hello.protocol_version });
    }

    match read_message(stream).await? {
        Some(Message::HelloAck(ack)) if ack.accepted => Ok(peer_hello),
        Some(Message::HelloAck(ack)) => {
            Err(SessionError::RejectedByPeer(ack.reason.unwrap_or_else(|| "no reason given".into())))
        }
        Some(_) => Err(SessionError::RejectedByPeer("expected HelloAck as the second message".into())),
        None => Err(SessionError::ClosedDuringHandshake),
    }
}

/// Sends a `Heartbeat` on `interval` until the channel it reads shutdown
/// signals from is dropped or the write fails (peer gone). Runs as its
/// own task so heartbeats keep flowing at a steady cadence regardless of
/// how busy the outbound message queue is — heartbeat delivery is what
/// the peer's `HeartbeatMonitor` uses to decide whether to release
/// control, so it must never be starved by a burst of mouse-move traffic.
pub async fn heartbeat_loop<W: AsyncWrite + Unpin>(mut writer: W, interval: Duration) {
    let mut seq: u32 = 0;
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        seq = seq.wrapping_add(1);
        let sent_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if write_message(&mut writer, &Message::Heartbeat { seq, sent_at_ms }).await.is_err() {
            return;
        }
    }
}

/// Reads framed messages off `reader` and forwards them to `sink` until
/// the connection closes or a framing error occurs, at which point it
/// returns so the caller can react (dedup bookkeeping reset, reconnect,
/// `EdgeStateMachine::handle(Event::ConnectionLost)`).
pub async fn read_loop<R: AsyncRead + Unpin>(mut reader: R, sink: mpsc::Sender<Message>) {
    loop {
        match read_message(&mut reader).await {
            Ok(Some(msg)) => {
                if sink.send(msg).await.is_err() {
                    return; // receiver dropped; nothing left to do
                }
            }
            Ok(None) | Err(_) => return,
        }
    }
}

/// Writes every message received on `outbound` to `writer`, applying
/// `TCP_NODELAY`-friendly framing (see `ms-protocol::write_message`) and
/// stopping on the first write error.
pub async fn write_loop<W: AsyncWrite + Unpin>(mut writer: W, mut outbound: mpsc::Receiver<Message>) {
    while let Some(msg) = outbound.recv().await {
        if write_message(&mut writer, &msg).await.is_err() {
            return;
        }
    }
    let _ = writer.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ms_protocol::OperatingSystem;

    fn sample_hello(name: &str) -> Hello {
        Hello {
            protocol_version: PROTOCOL_VERSION,
            device_id: uuid::Uuid::new_v4(),
            device_name: name.to_string(),
            os: OperatingSystem::Windows,
            session_id: uuid::Uuid::new_v4(),
        }
    }

    #[tokio::test]
    async fn matching_protocol_versions_complete_the_handshake_on_both_ends() {
        let (mut a, mut b) = tokio::io::duplex(4096);
        let hello_a = sample_hello("Alice");
        let hello_b = sample_hello("Bob");

        let (result_a, result_b) = tokio::join!(
            handshake(&mut a, hello_a.clone()),
            handshake(&mut b, hello_b.clone()),
        );

        assert_eq!(result_a.unwrap().device_id, hello_b.device_id);
        assert_eq!(result_b.unwrap().device_id, hello_a.device_id);
    }

    #[tokio::test]
    async fn mismatched_protocol_version_is_rejected_on_both_ends() {
        let (mut a, mut b) = tokio::io::duplex(4096);
        let mut hello_a = sample_hello("Alice");
        hello_a.protocol_version = PROTOCOL_VERSION + 1;
        let hello_b = sample_hello("Bob");

        let (result_a, result_b) = tokio::join!(handshake(&mut a, hello_a), handshake(&mut b, hello_b));

        assert!(result_a.is_err());
        assert!(result_b.is_err());
    }

    #[tokio::test]
    async fn heartbeat_reaches_the_peer_over_a_real_duplex_pair() {
        let (a, b) = tokio::io::duplex(4096);
        let (reader_b, writer_a) = (b, a);
        let (tx, mut rx) = mpsc::channel(8);

        tokio::spawn(heartbeat_loop(writer_a, Duration::from_millis(20)));
        tokio::spawn(read_loop(reader_b, tx));

        let received = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for a heartbeat")
            .expect("channel closed unexpectedly");

        assert!(matches!(received, Message::Heartbeat { seq: 1, .. }));
    }

    #[tokio::test]
    async fn write_loop_forwards_queued_messages_in_order() {
        let (a, b) = tokio::io::duplex(4096);
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(write_loop(a, rx));

        tx.send(Message::EdgeRelease).await.unwrap();
        tx.send(Message::Heartbeat { seq: 1, sent_at_ms: 0 }).await.unwrap();
        drop(tx);

        let mut reader = b;
        let first = read_message(&mut reader).await.unwrap().unwrap();
        let second = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(first, Message::EdgeRelease);
        assert!(matches!(second, Message::Heartbeat { seq: 1, .. }));
    }
}
