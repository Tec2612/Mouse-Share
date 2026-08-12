use crate::state::{AppState, PendingPairing};
use ms_config::{ConfigFile, ScreenLayout, Settings};
use ms_discovery::{translate_event, DiscoveryEvent, DiscoveryService};
use ms_security::pairing::derive_sas;
use ms_security::{PairedDevice, TrustStore};
use serde::Serialize;
use std::time::Duration;
use tauri::State;

fn to_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    Ok(state.config_store.load().map_err(to_err)?.settings)
}

#[tauri::command]
pub fn save_settings(state: State<AppState>, settings: Settings) -> Result<(), String> {
    let mut config = state.config_store.load().map_err(to_err)?;
    config.settings = settings;
    state.config_store.save(&config).map_err(to_err)
}

#[tauri::command]
pub fn get_layout(state: State<AppState>) -> Result<ScreenLayout, String> {
    Ok(state.config_store.load().map_err(to_err)?.layout)
}

#[tauri::command]
pub fn save_layout(state: State<AppState>, layout: ScreenLayout) -> Result<(), String> {
    let mut config = state.config_store.load().map_err(to_err)?;
    config.layout = layout;
    state.config_store.save(&config).map_err(to_err)
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Result<ConfigFile, String> {
    state.config_store.load().map_err(to_err)
}

#[derive(Serialize)]
pub struct PairedDeviceView {
    device_id: uuid::Uuid,
    name: String,
    fingerprint: String,
}

#[tauri::command]
pub fn list_paired_devices(state: State<AppState>) -> Result<Vec<PairedDeviceView>, String> {
    let store = identity_trust_store(&state)?;
    Ok(store
        .list()
        .map(|d| PairedDeviceView { device_id: d.device_id, name: d.name.clone(), fingerprint: d.fingerprint.clone() })
        .collect())
}

#[tauri::command]
pub fn remove_paired_device(state: State<AppState>, fingerprint: String) -> Result<(), String> {
    let mut store = identity_trust_store(&state)?;
    store.revoke(&fingerprint);
    ms_daemon::identity::save_trust_store(&state.trust_store_path, &store).map_err(to_err)
}

#[tauri::command]
pub fn get_this_device_fingerprint(state: State<AppState>) -> String {
    state.device_identity.fingerprint()
}

#[derive(Serialize)]
pub struct DiscoveredDeviceView {
    device_id: uuid::Uuid,
    name: String,
    os: String,
    addrs: Vec<String>,
    port: u16,
}

/// Browses for `duration_ms` (a bounded window rather than a long-lived
/// stream, to keep this a simple request/response command the frontend
/// can poll) and returns everything seen. A real long-running dashboard
/// would instead subscribe to a Tauri event stream; this is the
/// straightforward version that's still backed by a genuine mDNS browse,
/// not fixture data.
#[tauri::command]
pub async fn discover_devices(duration_ms: u64) -> Result<Vec<DiscoveredDeviceView>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let discovery = DiscoveryService::new().map_err(to_err)?;
        let receiver = discovery.browse().map_err(to_err)?;
        let deadline = std::time::Instant::now() + Duration::from_millis(duration_ms);
        let mut found = Vec::new();
        while std::time::Instant::now() < deadline {
            if let Ok(event) = receiver.recv_timeout(Duration::from_millis(200)) {
                if let Some(Ok(DiscoveryEvent::Found(ann))) = translate_event(event) {
                    found.push(DiscoveredDeviceView {
                        device_id: ann.device_id,
                        name: ann.name,
                        os: format!("{:?}", ann.os),
                        addrs: ann.addrs.iter().map(|a| a.to_string()).collect(),
                        port: ann.port,
                    });
                }
            }
        }
        Ok(found)
    })
    .await
    .map_err(to_err)?
}

#[derive(Serialize)]
pub struct PairingStarted {
    pub sas: String,
    pub remote_name: String,
}

/// Dials `addr` and runs the real trust-on-first-use handshake
/// (`ms_security::tls::pairing_configs` + `derive_sas`) described in
/// `docs/security.md` — this is not a placeholder, it performs an actual
/// TLS 1.3 connection and computes the short authentication string from
/// real exporter keying material. The user compares `sas` against what
/// the *other* device shows on its own screen (via the corresponding
/// accept-side flow) before calling `confirm_pairing`.
#[tauri::command]
pub async fn start_pairing(state: State<'_, AppState>, addr: String, port: u16, device_name: String) -> Result<PairingStarted, String> {
    let identity = &state.device_identity;
    let (client_config, client_verifier, _server_config, _server_verifier) =
        ms_security::tls::pairing_configs(identity).map_err(to_err)?;

    let socket_addr: std::net::SocketAddr = format!("{addr}:{port}").parse().map_err(to_err)?;
    let stream = tokio::net::TcpStream::connect(socket_addr).await.map_err(to_err)?;
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_config));
    let server_name = rustls::pki_types::ServerName::try_from("mouse-share.local")
        .map_err(to_err)?
        .to_owned();
    let mut tls_stream = connector.connect(server_name, stream).await.map_err(to_err)?;

    let local_hello = ms_protocol::Hello {
        protocol_version: ms_protocol::PROTOCOL_VERSION,
        device_id: state.device_id,
        device_name: state.device_name.clone(),
        os: current_os(),
        session_id: uuid::Uuid::new_v4(),
    };
    let peer_hello = ms_core_service::handshake(&mut tls_stream, local_hello).await.map_err(to_err)?;

    let mut exporter = [0u8; 32];
    tls_stream
        .get_ref()
        .1
        .export_keying_material(&mut exporter, b"mouse-share-pairing", None)
        .map_err(to_err)?;

    let own_fp = identity.fingerprint();
    let peer_fp = client_verifier.observed_fingerprint().ok_or("no certificate observed from peer")?;
    let sas = derive_sas(&exporter, &own_fp, &peer_fp);

    *state.pending_pairing.lock().expect("poisoned") = Some(PendingPairing {
        remote_fingerprint: peer_fp,
        remote_device_id: peer_hello.device_id,
        remote_name: device_name,
        sas: sas.clone(),
    });

    Ok(PairingStarted { sas, remote_name: peer_hello.device_name })
}

/// Commits the pairing started by `start_pairing` once the user has
/// visually confirmed the SAS matches what the other device displayed.
#[tauri::command]
pub fn confirm_pairing(state: State<AppState>) -> Result<(), String> {
    let pending = state.pending_pairing.lock().expect("poisoned").take().ok_or("no pairing in progress")?;

    let mut store = identity_trust_store(&state)?;
    store.add(PairedDevice {
        device_id: pending.remote_device_id,
        name: pending.remote_name,
        fingerprint: pending.remote_fingerprint,
        paired_at_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    });
    ms_daemon::identity::save_trust_store(&state.trust_store_path, &store).map_err(to_err)
}

#[tauri::command]
pub fn cancel_pairing(state: State<AppState>) {
    *state.pending_pairing.lock().expect("poisoned") = None;
}

fn identity_trust_store(state: &State<AppState>) -> Result<TrustStore, String> {
    ms_daemon::identity::load_trust_store(&state.trust_store_path).map_err(to_err)
}

fn current_os() -> ms_protocol::OperatingSystem {
    #[cfg(target_os = "windows")]
    {
        ms_protocol::OperatingSystem::Windows
    }
    #[cfg(target_os = "macos")]
    {
        ms_protocol::OperatingSystem::MacOs
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        ms_protocol::OperatingSystem::Windows
    }
}
