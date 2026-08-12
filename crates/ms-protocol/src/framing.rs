use crate::Message;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Messages above this size are rejected before allocating a receive
/// buffer for them. Generous headroom over the largest legitimate message
/// (a clipboard payload) while bounding memory a malicious/corrupt peer
/// could force us to allocate.
pub const MAX_MESSAGE_BYTES: u32 = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum FramingError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("message of {0} bytes exceeds the {MAX_MESSAGE_BYTES} byte limit")]
    TooLarge(u32),
    #[error("failed to decode message: {0}")]
    Decode(#[from] bincode::Error),
}

/// Serializes a message to its wire representation (length prefix + body),
/// independent of any I/O so it can also be used for tests and for
/// size/throughput accounting.
pub fn encode_message(msg: &Message) -> Result<Vec<u8>, FramingError> {
    let body = bincode::serialize(msg)?;
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode_message(body: &[u8]) -> Result<Message, FramingError> {
    Ok(bincode::deserialize(body)?)
}

/// Writes one length-prefixed message to an async stream. Callers are
/// expected to disable Nagle's algorithm (`TCP_NODELAY`) on the underlying
/// socket, since batching is handled explicitly by `MouseMoveCoalescer`
/// rather than left to the kernel.
pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &Message,
) -> Result<(), FramingError> {
    let bytes = encode_message(msg)?;
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

/// Reads one length-prefixed message from an async stream, returning
/// `Ok(None)` on clean EOF (peer closed the connection).
pub async fn read_message<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Option<Message>, FramingError> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_le_bytes(len_buf);
    if len > MAX_MESSAGE_BYTES {
        return Err(FramingError::TooLarge(len));
    }
    let mut body = vec![0u8; len as usize];
    reader.read_exact(&mut body).await?;
    Ok(Some(decode_message(&body)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MouseButton, Message};

    #[tokio::test]
    async fn round_trips_over_an_in_memory_duplex_stream() {
        let (mut client, mut server) = tokio::io::duplex(4096);

        let msg = Message::MouseButton {
            button: MouseButton::Left,
            pressed: true,
        };
        write_message(&mut client, &msg).await.unwrap();

        let received = read_message(&mut server).await.unwrap().unwrap();
        assert_eq!(received, msg);
    }

    #[tokio::test]
    async fn clean_close_yields_none_instead_of_an_error() {
        let (client, mut server) = tokio::io::duplex(4096);
        drop(client);
        let received = read_message(&mut server).await.unwrap();
        assert!(received.is_none());
    }

    #[tokio::test]
    async fn oversized_length_prefix_is_rejected_before_reading_a_body() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        let bad_len = MAX_MESSAGE_BYTES + 1;
        client.write_all(&bad_len.to_le_bytes()).await.unwrap();

        let err = read_message(&mut server).await.unwrap_err();
        assert!(matches!(err, FramingError::TooLarge(n) if n == bad_len));
    }

    #[test]
    fn encode_decode_round_trip_for_every_variant_family() {
        let samples = vec![
            Message::Heartbeat { seq: 1, sent_at_ms: 42 },
            Message::MouseMove { dx: 1.5, dy: -2.25, seq: 7 },
            Message::EdgeRelease,
            Message::ClipboardRequest {
                format: crate::ClipboardFormat::PlainTextUtf8,
            },
        ];
        for msg in samples {
            let bytes = encode_message(&msg).unwrap();
            let len = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
            let decoded = decode_message(&bytes[4..4 + len]).unwrap();
            assert_eq!(decoded, msg);
        }
    }
}
