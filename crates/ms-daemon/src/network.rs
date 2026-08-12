use ms_core_service::{HeartbeatMonitor, NetworkSender};
use ms_input_core::Event as CoreEvent;
use ms_protocol::{DeviceId, Hello, Message};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;

/// Routes `EdgeStateMachine::Action::SendMessage` to whichever live
/// connection matches the target device, by forwarding onto that
/// connection's outbound channel (shared with its heartbeat ticker — see
/// `handle_incoming` — so there is exactly one writer per socket).
#[derive(Default)]
pub struct ChannelNetworkSender {
    peers: Mutex<HashMap<DeviceId, mpsc::UnboundedSender<Message>>>,
}

impl ChannelNetworkSender {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, device_id: DeviceId, tx: mpsc::UnboundedSender<Message>) {
        self.peers.lock().expect("poisoned").insert(device_id, tx);
    }

    pub fn unregister(&self, device_id: DeviceId) {
        self.peers.lock().expect("poisoned").remove(&device_id);
    }
}

impl NetworkSender for ChannelNetworkSender {
    fn send(&self, to: DeviceId, message: Message) {
        if let Some(tx) = self.peers.lock().expect("poisoned").get(&to) {
            let _ = tx.send(message);
        } else {
            tracing::warn!(peer = %to, "dropping outbound message: no live connection");
        }
    }
}

/// Accepts inbound connections indefinitely, spawning one task per
/// connection. Runs until the listener itself errors (which normally only
/// happens on shutdown).
pub async fn run_listener(
    listener: TcpListener,
    tls_acceptor: TlsAcceptor,
    local_identity: (DeviceId, String, ms_protocol::OperatingSystem),
    network: Arc<ChannelNetworkSender>,
    event_tx: mpsc::UnboundedSender<CoreEvent>,
    heartbeat_interval: Duration,
) {
    loop {
        let (stream, addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(error = %e, "accept() failed");
                continue;
            }
        };
        let acceptor = tls_acceptor.clone();
        let network = network.clone();
        let event_tx = event_tx.clone();
        let (device_id, device_name, os) = local_identity.clone();
        tokio::spawn(async move {
            let hello = Hello {
                protocol_version: ms_protocol::PROTOCOL_VERSION,
                device_id,
                device_name,
                os,
                session_id: uuid::Uuid::new_v4(),
            };
            if let Err(e) = handle_incoming(stream, acceptor, hello, network, event_tx, heartbeat_interval).await {
                tracing::warn!(%addr, error = %e, "inbound connection ended");
            }
        });
    }
}

/// Connects out to a peer at `addr` and runs the same session lifecycle as
/// an accepted inbound connection. Used when this device initiates
/// pairing/reconnection to a discovered or manually-entered peer; not yet
/// invoked in this minimal daemon (no UI command dispatcher exists to
/// trigger it yet), but is the complete real implementation for that path.
#[allow(dead_code)]
pub async fn connect_out(
    addr: std::net::SocketAddr,
    tls_connector: tokio_rustls::TlsConnector,
    server_name: rustls::pki_types::ServerName<'static>,
    local_hello: Hello,
    network: Arc<ChannelNetworkSender>,
    event_tx: mpsc::UnboundedSender<CoreEvent>,
    heartbeat_interval: Duration,
) -> anyhow::Result<()> {
    let stream = tokio::net::TcpStream::connect(addr).await?;
    stream.set_nodelay(true)?;
    let tls_stream = tls_connector.connect(server_name, stream).await?;
    run_session(tls_stream, local_hello, network, event_tx, heartbeat_interval).await
}

async fn handle_incoming(
    stream: tokio::net::TcpStream,
    acceptor: TlsAcceptor,
    local_hello: Hello,
    network: Arc<ChannelNetworkSender>,
    event_tx: mpsc::UnboundedSender<CoreEvent>,
    heartbeat_interval: Duration,
) -> anyhow::Result<()> {
    stream.set_nodelay(true)?;
    let tls_stream = acceptor.accept(stream).await?;
    run_session(tls_stream, local_hello, network, event_tx, heartbeat_interval).await
}

async fn run_session<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    mut tls_stream: S,
    local_hello: Hello,
    network: Arc<ChannelNetworkSender>,
    event_tx: mpsc::UnboundedSender<CoreEvent>,
    heartbeat_interval: Duration,
) -> anyhow::Result<()> {
    let peer_hello = ms_core_service::handshake(&mut tls_stream, local_hello).await?;
    let peer_id = peer_hello.device_id;
    tracing::info!(peer = %peer_id, name = %peer_hello.device_name, os = ?peer_hello.os, "session established");

    let (mut read_half, mut write_half) = tokio::io::split(tls_stream);
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
    network.register(peer_id, out_tx.clone());

    let monitor = Arc::new(Mutex::new(HeartbeatMonitor::new(heartbeat_interval, Instant::now())));

    let writer_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if ms_protocol::write_message(&mut write_half, &msg).await.is_err() {
                break;
            }
        }
        let _ = write_half.shutdown().await;
    });

    let heartbeat_tx = out_tx.clone();
    let heartbeat_task = tokio::spawn(async move {
        let mut seq: u32 = 0;
        let mut ticker = tokio::time::interval(heartbeat_interval);
        loop {
            ticker.tick().await;
            seq = seq.wrapping_add(1);
            let sent_at_ms = now_unix_ms();
            if heartbeat_tx.send(Message::Heartbeat { seq, sent_at_ms }).is_err() {
                return;
            }
        }
    });

    let watchdog_monitor = monitor.clone();
    let watchdog_event_tx = event_tx.clone();
    let watchdog_network = network.clone();
    let watchdog_task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(heartbeat_interval);
        loop {
            ticker.tick().await;
            if !watchdog_monitor.lock().expect("poisoned").is_alive(Instant::now()) {
                tracing::warn!(peer = %peer_id, "heartbeat timeout; treating connection as lost");
                watchdog_network.unregister(peer_id);
                let _ = watchdog_event_tx.send(CoreEvent::ConnectionLost { device: peer_id });
                return;
            }
        }
    });

    let read_result: anyhow::Result<()> = loop {
        match ms_protocol::read_message(&mut read_half).await {
            Ok(Some(Message::Heartbeat { .. })) => {
                monitor.lock().expect("poisoned").on_heartbeat_received(Instant::now());
            }
            Ok(Some(Message::EdgeEnter { edge, position, .. })) => {
                let _ = event_tx.send(CoreEvent::PeerEdgeEnter { from: peer_id, edge, position });
            }
            Ok(Some(Message::EdgeRelease)) => {
                let _ = event_tx.send(CoreEvent::PeerEdgeRelease { from: peer_id });
            }
            Ok(Some(msg @ (Message::MouseMove { .. }
            | Message::MouseWarp { .. }
            | Message::MouseButton { .. }
            | Message::MouseWheel { .. }
            | Message::KeyEvent { .. }))) => {
                let _ = event_tx.send(CoreEvent::PeerInputMessage { from: peer_id, message: msg });
            }
            Ok(Some(Message::ClipboardOffer { .. } | Message::ClipboardRequest { .. } | Message::ClipboardData { .. })) => {
                // Clipboard message routing to the local OS clipboard is
                // not yet implemented (see docs/architecture.md's status
                // note) — logged rather than silently dropped so the gap
                // is visible in diagnostics instead of looking like a
                // working, no-op feature.
                tracing::debug!(peer = %peer_id, "received clipboard message; OS clipboard integration not yet implemented");
            }
            Ok(Some(Message::Disconnect { reason })) => {
                tracing::info!(peer = %peer_id, %reason, "peer requested graceful disconnect");
                break Ok(());
            }
            Ok(Some(Message::Hello(_) | Message::HelloAck(_))) => {
                tracing::warn!(peer = %peer_id, "unexpected Hello/HelloAck after handshake completed; ignoring");
            }
            Ok(Some(Message::HeartbeatAck { .. })) => {
                // Not currently sent by this implementation (liveness is
                // inferred from `Heartbeat` receipt alone); ignored rather
                // than treated as unexpected, since a future peer version
                // sending one is harmless.
            }
            Ok(None) => break Ok(()),
            Err(e) => break Err(e.into()),
        }
    };

    network.unregister(peer_id);
    let _ = event_tx.send(CoreEvent::ConnectionLost { device: peer_id });
    writer_task.abort();
    heartbeat_task.abort();
    watchdog_task.abort();
    read_result
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
