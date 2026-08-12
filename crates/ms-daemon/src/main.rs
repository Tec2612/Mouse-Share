mod input;

use anyhow::{Context, Result};
use ms_config::{ConfigStore, SecureStorage};
use ms_core_service::CoreService;
use ms_daemon::{identity, network};
use ms_discovery::{translate_event, DiscoveryEvent, DiscoveryService};
use ms_input_core::Event as CoreEvent;
use ms_protocol::OperatingSystem;
use network::ChannelNetworkSender;
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let config_dir = ConfigStore::default_path()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .context("could not determine a config directory for this platform")?;
    std::fs::create_dir_all(&config_dir)?;

    let config_store = ConfigStore::new(config_dir.join("config.toml"));
    let config = config_store.load().context("loading configuration")?;

    let (device_id, device_name) = identity::load_or_create_device_meta(&config_dir)?;
    tracing::info!(%device_id, %device_name, "starting Mouse Share daemon");

    let secure_storage = platform_secure_storage();
    let device_identity = identity::load_or_generate(secure_storage.as_ref(), device_id, &device_name)?;
    tracing::info!(fingerprint = %device_identity.fingerprint(), "device identity ready");

    let trust_store_path = identity::trust_store_path(&config_dir);
    let trust_store = identity::load_trust_store(&trust_store_path)?;
    let trust_store = Arc::new(std::sync::Mutex::new(trust_store));

    let (tls_client_config, tls_server_config) =
        ms_security::tls::pinned_configs(&device_identity, trust_store.clone())
            .context("building TLS configuration")?;
    let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_server_config));
    let tls_connector = tokio_rustls::TlsConnector::from(Arc::new(tls_client_config));

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.settings.network.listen_port))
        .await
        .with_context(|| format!("binding TCP listener on port {}", config.settings.network.listen_port))?;
    tracing::info!(port = config.settings.network.listen_port, "listening for peer connections");

    let network_sender = Arc::new(ChannelNetworkSender::new());
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<CoreEvent>();

    let local_os = current_os();
    tokio::spawn(network::run_listener(
        listener,
        tls_acceptor,
        (device_id, device_name.clone(), local_os),
        network_sender.clone(),
        event_tx.clone(),
        Duration::from_millis(config.settings.network.heartbeat_interval_ms),
    ));

    // Reloads the on-disk trust store into the same Arc<Mutex<..>> the
    // listener's/dialer's PinnedVerifier already holds, so a pairing
    // completed later through the UI (a separate process — see
    // docs/architecture.md) takes effect here without a restart.
    tokio::spawn(reload_trust_store_loop(trust_store_path.clone(), trust_store.clone()));

    // Discovered peer addresses, keyed by device_id, kept fresh by
    // spawn_discovery below and consumed by the mesh-connect loop.
    let discovered_addrs: Arc<Mutex<HashMap<uuid::Uuid, IpAddr>>> = Arc::new(Mutex::new(HashMap::new()));

    if config.settings.network.auto_discovery_enabled {
        // Browse only — this device is already advertised (at
        // NetworkSettings::pairing_port) by the desktop UI process if
        // it's running; a second simultaneous advertisement for the same
        // identity from this process would just be redundant and risks
        // an mDNS name-conflict probe. See NetworkSettings::pairing_port's
        // doc comment.
        spawn_discovery_browse(discovered_addrs.clone())?;
    }

    // Dials every paired (trust-store) device this daemon has a
    // discovered address for and doesn't already have a live session
    // with. This is what turns "paired" into "actually reachable for
    // control" — without it, two daemons that only ever accept
    // connections never actually connect to each other.
    tokio::spawn(mesh_connect_loop(
        trust_store_path,
        discovered_addrs,
        device_id,
        device_name.clone(),
        local_os,
        config.settings.network.listen_port,
        tls_connector,
        network_sender.clone(),
        event_tx.clone(),
        Duration::from_millis(config.settings.network.heartbeat_interval_ms),
    ));

    match input::build_platform_input() {
        Ok((capture, injector)) => {
            let capture_adapter = input::CaptureAdapter::new(event_tx.clone());
            let mut capture = capture;
            capture
                .start(capture_adapter)
                .context("starting local input capture")?;
            let pass_through = Arc::new(input::PassThroughAdapter::new(capture));

            let mut service = CoreService::new(
                device_id,
                config.settings.emergency_hotkey.clone(),
                config.layout.clone(),
                config.settings.clipboard_sharing_enabled,
                injector,
                pass_through,
                network_sender.clone(),
            );
            tracing::info!("local input capture active; this device can control and be controlled");

            tokio::spawn(async move {
                while let Some(event) = event_rx.recv().await {
                    service.handle(event);
                }
            });
        }
        Err(e) => {
            tracing::warn!(error = %e, "local input capture unavailable; running network-only (this device cannot initiate control, and cannot be controlled, until this is resolved)");
            tokio::spawn(async move { while event_rx.recv().await.is_some() {} });
        }
    }

    tokio::signal::ctrl_c().await.ok();
    tracing::info!("shutting down");
    Ok(())
}

async fn reload_trust_store_loop(path: std::path::PathBuf, shared: Arc<std::sync::Mutex<ms_security::TrustStore>>) {
    let mut ticker = tokio::time::interval(Duration::from_secs(5));
    loop {
        ticker.tick().await;
        match identity::load_trust_store(&path) {
            Ok(fresh) => *shared.lock().expect("poisoned") = fresh,
            Err(e) => tracing::debug!(error = %e, "failed to reload trust store; keeping previous contents"),
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn mesh_connect_loop(
    trust_store_path: std::path::PathBuf,
    discovered_addrs: Arc<Mutex<HashMap<uuid::Uuid, IpAddr>>>,
    device_id: uuid::Uuid,
    device_name: String,
    os: OperatingSystem,
    session_port: u16,
    tls_connector: tokio_rustls::TlsConnector,
    network_sender: Arc<ChannelNetworkSender>,
    event_tx: tokio::sync::mpsc::UnboundedSender<CoreEvent>,
    heartbeat_interval: Duration,
) {
    let mut ticker = tokio::time::interval(Duration::from_secs(5));
    loop {
        ticker.tick().await;

        let trust_store = match identity::load_trust_store(&trust_store_path) {
            Ok(store) => store,
            Err(e) => {
                tracing::debug!(error = %e, "mesh-connect: failed to read trust store");
                continue;
            }
        };

        let targets: Vec<(uuid::Uuid, IpAddr)> = {
            let addrs = discovered_addrs.lock().expect("poisoned");
            trust_store
                .list()
                .filter(|peer| !network_sender.is_connected(peer.device_id))
                .filter_map(|peer| addrs.get(&peer.device_id).map(|ip| (peer.device_id, *ip)))
                .collect()
        };

        for (peer_id, ip) in targets {
            let addr = std::net::SocketAddr::new(ip, session_port);
            let hello = ms_protocol::Hello {
                protocol_version: ms_protocol::PROTOCOL_VERSION,
                device_id,
                device_name: device_name.clone(),
                os,
                session_id: uuid::Uuid::new_v4(),
            };
            let server_name = rustls::pki_types::ServerName::from(ip);
            let connector = tls_connector.clone();
            let network_sender = network_sender.clone();
            let event_tx = event_tx.clone();
            tokio::spawn(async move {
                tracing::info!(peer = %peer_id, %addr, "mesh-connect: dialing paired device");
                if let Err(e) =
                    network::connect_out(addr, connector, server_name, hello, network_sender, event_tx, heartbeat_interval).await
                {
                    tracing::debug!(peer = %peer_id, error = %e, "mesh-connect: dial failed; will retry");
                }
            });
        }
    }
}

fn platform_secure_storage() -> Box<dyn SecureStorage> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        Box::new(ms_config::KeyringSecureStorage)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        tracing::warn!("no native secure storage backend on this platform; using a non-persistent in-memory store (development/CI only)");
        Box::new(ms_config::InMemorySecureStorage::default())
    }
}

fn current_os() -> OperatingSystem {
    #[cfg(target_os = "windows")]
    {
        OperatingSystem::Windows
    }
    #[cfg(target_os = "macos")]
    {
        OperatingSystem::MacOs
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // Development-only fallback for running/testing this binary on
        // Linux; Mouse Share ships Windows and macOS builds only.
        OperatingSystem::Windows
    }
}

/// Browses for peer devices (advertised by the desktop UI process — see
/// `NetworkSettings::pairing_port`'s doc comment) and keeps
/// `discovered_addrs` current. Does not advertise this device itself.
fn spawn_discovery_browse(discovered_addrs: Arc<Mutex<HashMap<uuid::Uuid, IpAddr>>>) -> Result<()> {
    let discovery = DiscoveryService::new().context("starting mDNS discovery")?;
    let receiver = discovery.browse().context("browsing for peer devices")?;
    tokio::task::spawn_blocking(move || {
        let _discovery = discovery; // keep the browser alive
        while let Ok(event) = receiver.recv() {
            match translate_event(event) {
                Some(Ok(DiscoveryEvent::Found(ann))) => {
                    if let Some(ip) = ann.addrs.first() {
                        tracing::info!(name = %ann.name, os = ?ann.os, %ip, "discovered device");
                        discovered_addrs.lock().expect("poisoned").insert(ann.device_id, *ip);
                    }
                }
                Some(Ok(DiscoveryEvent::Lost { fullname })) => {
                    tracing::info!(%fullname, "device no longer visible");
                    // Deliberately not removed from discovered_addrs: a
                    // brief mDNS flap shouldn't drop a reachable address
                    // the mesh-connect loop could otherwise still reach;
                    // a stale address just fails to connect next tick.
                }
                Some(Err(e)) => tracing::debug!(error = %e, "ignoring malformed discovery record"),
                None => {}
            }
        }
    });
    Ok(())
}
