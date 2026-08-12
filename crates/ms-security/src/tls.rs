use crate::identity::DeviceIdentity;
use crate::trust_store::TrustStore;
use crate::verifier::{PinnedVerifier, TofuVerifier};
use rustls::{ClientConfig, ServerConfig};
use std::sync::{Arc, Mutex};

/// Builds the mTLS config pair used for normal, post-pairing connections:
/// both client and server require and pin the peer's certificate against
/// `trust_store`. Every input-carrying connection in the app uses these —
/// there is no code path that opens a socket and starts forwarding
/// mouse/keyboard events without going through mutual, pinned TLS first.
pub fn pinned_configs(
    identity: &DeviceIdentity,
    trust_store: Arc<Mutex<TrustStore>>,
) -> Result<(ClientConfig, ServerConfig), rustls::Error> {
    let verifier = Arc::new(PinnedVerifier::new(trust_store));
    let cert_chain = vec![identity.cert_der()];

    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let client = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_client_auth_cert(cert_chain.clone(), identity.key_der())?;

    let server = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_client_cert_verifier(verifier)
        .with_single_cert(cert_chain, identity.key_der())?;

    Ok((client, server))
}

/// Builds the short-lived, trust-on-first-use TLS config pair used only
/// during an explicit pairing operation. Returns the `TofuVerifier`s
/// alongside the configs so the caller can read back the fingerprint each
/// side observed once the handshake completes, for the SAS check in
/// `pairing.rs`.
pub fn pairing_configs(
    identity: &DeviceIdentity,
) -> Result<(ClientConfig, Arc<TofuVerifier>, ServerConfig, Arc<TofuVerifier>), rustls::Error> {
    let client_verifier = Arc::new(TofuVerifier::new());
    let server_verifier = Arc::new(TofuVerifier::new());
    let cert_chain = vec![identity.cert_der()];

    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let client = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(client_verifier.clone())
        .with_client_auth_cert(cert_chain.clone(), identity.key_der())?;

    let server = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_client_cert_verifier(server_verifier.clone())
        .with_single_cert(cert_chain, identity.key_der())?;

    Ok((client, client_verifier, server, server_verifier))
}
