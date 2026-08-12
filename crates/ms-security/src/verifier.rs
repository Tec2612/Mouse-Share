use crate::fingerprint::sha256_hex;
use crate::trust_store::TrustStore;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, Error as TlsError, SignatureScheme};
use std::sync::{Arc, Mutex};

/// Certificate verifier used for normal (post-pairing) connections in both
/// TLS roles. Rejects any peer whose certificate fingerprint is not
/// already present in the shared `TrustStore` — i.e. is not a device this
/// one has explicitly paired with — before the handshake can complete.
/// This is the control that keeps unauthorized devices on the LAN from
/// ever establishing a session, regardless of what discovery reports.
///
/// It intentionally does **not** perform normal PKI chain validation
/// (there is no CA); the security property comes entirely from pinning
/// plus verifying the peer's handshake signature, which proves possession
/// of the private key matching the pinned certificate.
#[derive(Debug)]
pub struct PinnedVerifier {
    trust_store: Arc<Mutex<TrustStore>>,
    provider: Arc<CryptoProvider>,
}

impl PinnedVerifier {
    pub fn new(trust_store: Arc<Mutex<TrustStore>>) -> Self {
        Self { trust_store, provider: Arc::new(rustls::crypto::ring::default_provider()) }
    }

    fn check_pinned(&self, cert: &CertificateDer<'_>) -> Result<(), TlsError> {
        let fp = sha256_hex(cert);
        let trusted = self
            .trust_store
            .lock()
            .expect("trust store mutex poisoned")
            .is_trusted(&fp);
        if trusted {
            Ok(())
        } else {
            Err(TlsError::General(format!(
                "peer certificate fingerprint {fp} is not in the trust store; pair the devices first"
            )))
        }
    }
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        self.check_pinned(end_entity)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

impl ClientCertVerifier for PinnedVerifier {
    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, TlsError> {
        self.check_pinned(end_entity)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// Certificate verifier used *only* for the brief, explicitly-initiated
/// pairing handshake (see `pairing.rs`). It accepts any well-formed,
/// correctly-signed certificate — trust-on-first-use — and records the
/// peer's fingerprint so the pairing state machine can incorporate it into
/// the short authentication string (SAS) the user visually confirms. It
/// must never be used for any other connection: accepting an unpinned cert
/// is exactly the thing `PinnedVerifier` exists to prevent.
#[derive(Debug)]
pub struct TofuVerifier {
    observed_fingerprint: Arc<Mutex<Option<String>>>,
    provider: Arc<CryptoProvider>,
}

impl TofuVerifier {
    pub fn new() -> Self {
        Self { observed_fingerprint: Arc::new(Mutex::new(None)), provider: Arc::new(rustls::crypto::ring::default_provider()) }
    }

    pub fn observed_fingerprint(&self) -> Option<String> {
        self.observed_fingerprint.lock().expect("mutex poisoned").clone()
    }
}

impl Default for TofuVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        *self.observed_fingerprint.lock().expect("mutex poisoned") = Some(sha256_hex(end_entity));
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

impl ClientCertVerifier for TofuVerifier {
    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, TlsError> {
        *self.observed_fingerprint.lock().expect("mutex poisoned") = Some(sha256_hex(end_entity));
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}
