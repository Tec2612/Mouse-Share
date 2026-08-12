mod input;

use anyhow::{Context, Result};
use ms_config::{ConfigStore, SecureStorage};
use ms_core_service::CoreService;
use ms_daemon::{identity, network};
use ms_discovery::{translate_event, DiscoveryEvent, DiscoveryService, RemoteOs};
use ms_input_core::Event as CoreEvent;
use ms_protocol::OperatingSystem;
use network::ChannelNetworkSender;
use std::path::Path;
use std::sync::Arc;
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
    let _ = tls_connector; // used by connect_out when initiating outbound pairing/reconnects

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

    if config.settings.network.auto_discovery_enabled {
        spawn_discovery(device_id, device_name.clone(), local_os, config.settings.network.listen_port)?;
    }

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

fn spawn_discovery(device_id: uuid::Uuid, device_name: String, os: OperatingSystem, port: u16) -> Result<()> {
    let mut discovery = DiscoveryService::new().context("starting mDNS discovery")?;
    let remote_os = match os {
        OperatingSystem::Windows => RemoteOs::Windows,
        OperatingSystem::MacOs => RemoteOs::MacOs,
    };
    let host_label = device_name.to_lowercase().replace(' ', "-");
    discovery
        .advertise(device_id, &device_name, remote_os, &host_label, port)
        .context("advertising this device over mDNS")?;

    let receiver = discovery.browse().context("browsing for peer devices")?;
    tokio::task::spawn_blocking(move || {
        let _discovery = discovery; // keep the daemon (and its advertisement) alive
        while let Ok(event) = receiver.recv() {
            match translate_event(event) {
                Some(Ok(DiscoveryEvent::Found(ann))) => {
                    tracing::info!(name = %ann.name, os = ?ann.os, addrs = ?ann.addrs, port = ann.port, "discovered device");
                }
                Some(Ok(DiscoveryEvent::Lost { fullname })) => {
                    tracing::info!(%fullname, "device no longer visible");
                }
                Some(Err(e)) => tracing::debug!(error = %e, "ignoring malformed discovery record"),
                None => {}
            }
        }
    });
    Ok(())
}
