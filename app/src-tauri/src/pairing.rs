//! Accept side of pairing: listens on `NetworkSettings::pairing_port` for
//! incoming trust-on-first-use handshakes and surfaces each one to the
//! frontend as an event, mirroring what `commands::start_pairing` does
//! for the dialing side. Without this, nothing on a freshly-installed
//! device ever answers a pairing attempt from another device — every
//! "Connect by IP" or "Pair" click just times out, since there was
//! previously no listener at all.

use crate::state::{AppState, PendingPairing};
use ms_security::pairing::derive_sas;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingPairingPayload {
    pub remote_name: String,
    pub sas: String,
}

/// Runs for the lifetime of the app. Logs and returns (rather than
/// panicking) on setup failure — e.g. the port already being in use —
/// since a pairing-acceptor problem shouldn't take down the rest of the
/// UI.
pub fn spawn_pairing_acceptor(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let port = {
            let state = app.state::<AppState>();
            match state.config_store.load() {
                Ok(cfg) => cfg.settings.network.pairing_port,
                Err(e) => {
                    tracing::error!("pairing acceptor: could not load settings: {e}");
                    return;
                }
            }
        };

        let listener = match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(port, error = %e, "pairing acceptor: failed to bind; incoming pairing attempts will time out");
                return;
            }
        };
        tracing::info!(port, "pairing acceptor listening");

        loop {
            let (stream, addr) = match listener.accept().await {
                Ok(pair) => pair,
                Err(e) => {
                    tracing::warn!(error = %e, "pairing acceptor: accept() failed");
                    continue;
                }
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = handle_incoming_pairing(&app, stream).await {
                    tracing::warn!(%addr, error = %e, "incoming pairing attempt failed");
                }
            });
        }
    });
}

async fn handle_incoming_pairing(app: &AppHandle, stream: tokio::net::TcpStream) -> anyhow::Result<()> {
    stream.set_nodelay(true)?;

    let (device_id, device_name, identity_fp) = {
        let state = app.state::<AppState>();
        (state.device_id, state.device_name.clone(), state.device_identity.fingerprint())
    };
    let (server_config, server_verifier) = {
        let state = app.state::<AppState>();
        let (_client_config, _client_verifier, server_config, server_verifier) =
            ms_security::tls::pairing_configs(&state.device_identity)?;
        (server_config, server_verifier)
    };

    let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(server_config));
    let mut tls_stream = acceptor.accept(stream).await?;

    let local_hello = ms_protocol::Hello {
        protocol_version: ms_protocol::PROTOCOL_VERSION,
        device_id,
        device_name,
        os: current_os(),
        session_id: uuid::Uuid::new_v4(),
    };
    let peer_hello = ms_core_service::handshake(&mut tls_stream, local_hello).await?;

    let mut exporter = [0u8; 32];
    tls_stream.get_ref().1.export_keying_material(&mut exporter, b"mouse-share-pairing", None).map_err(anyhow::Error::from)?;

    let peer_fp = server_verifier.observed_fingerprint().ok_or_else(|| anyhow::anyhow!("no certificate observed from peer"))?;
    // Both sides derive the SAS from (own fp, peer fp) sorted, so the
    // acceptor computes it with its own identity first and the dialer's
    // observed fingerprint second — `derive_sas` sorts internally, so
    // which side is "own" vs "peer" here doesn't need to match the
    // dialer's argument order for the two computed codes to agree.
    let sas = derive_sas(&exporter, &identity_fp, &peer_fp);

    let state = app.state::<AppState>();
    *state.incoming_pairing.lock().expect("poisoned") = Some(PendingPairing {
        remote_fingerprint: peer_fp,
        remote_device_id: peer_hello.device_id,
        remote_name: peer_hello.device_name.clone(),
        sas: sas.clone(),
    });

    let _ = app.emit("incoming-pairing", IncomingPairingPayload { remote_name: peer_hello.device_name, sas });
    Ok(())
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
