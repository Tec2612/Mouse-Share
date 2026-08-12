use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("certificate generation failed: {0}")]
    Generation(#[from] rcgen::Error),
    #[error("failed to parse stored PEM material: {0}")]
    Parse(String),
}

/// A device's long-term cryptographic identity: an ECDSA P-256 keypair and
/// a self-signed X.509 certificate binding it to this device's id and
/// name. This is *not* backed by any certificate authority — trust is
/// established per-pair during pairing (see `pairing.rs`) and thereafter
/// enforced by pinning the certificate's SHA-256 fingerprint, the same
/// trust model SSH host keys use rather than the web PKI model. That
/// avoids the app depending on a CA and gives users an explicit,
/// revocable, mutual trust decision between exactly the two devices
/// involved.
pub struct DeviceIdentity {
    pub device_id: uuid::Uuid,
    pub device_name: String,
    cert_der: CertificateDer<'static>,
    key_der_bytes: Vec<u8>,
    cert_pem: String,
    key_pem: String,
}

impl DeviceIdentity {
    /// Generates a fresh identity. Called once per device on first run;
    /// callers are expected to persist the PEM output (`cert_pem`/
    /// `key_pem`) in platform secure storage (Keychain / Credential
    /// Manager, see `ms-config`) and reload via `from_pem` afterward so the
    /// identity — and therefore every peer's pinned trust of it — stays
    /// stable across restarts.
    pub fn generate(device_id: uuid::Uuid, device_name: impl Into<String>) -> Result<Self, IdentityError> {
        let device_name = device_name.into();
        let key_pair = KeyPair::generate()?;

        let mut params = CertificateParams::new(Vec::<String>::new())?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, device_name.clone());
        params.distinguished_name = dn;
        // Long-dated: this cert is never chain-validated against a CA or
        // checked for expiry by peers (pinning only cares about the public
        // key fingerprint), so there is no security benefit to a short
        // lifetime, and a short one would just cause spurious re-pairing.
        params.not_before = time::OffsetDateTime::now_utc() - time::Duration::days(1);
        params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(365 * 20);

        let cert = params.self_signed(&key_pair)?;
        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();
        let cert_der = cert.der().clone();
        let key_der_bytes = key_pair.serialized_der().to_vec();

        Ok(Self { device_id, device_name, cert_der, key_der_bytes, cert_pem, key_pem })
    }

    /// Reloads a previously generated identity from its persisted PEM
    /// material, without re-deriving or re-signing anything.
    pub fn from_pem(
        device_id: uuid::Uuid,
        device_name: impl Into<String>,
        cert_pem: &str,
        key_pem: &str,
    ) -> Result<Self, IdentityError> {
        let cert_der = rustls_pemfile::certs(&mut cert_pem.as_bytes())
            .next()
            .ok_or_else(|| IdentityError::Parse("no certificate found in PEM".into()))?
            .map_err(|e| IdentityError::Parse(e.to_string()))?
            .into_owned();

        let key_pair = KeyPair::from_pem(key_pem).map_err(|e| IdentityError::Parse(e.to_string()))?;
        let key_der_bytes = key_pair.serialized_der().to_vec();

        Ok(Self {
            device_id,
            device_name: device_name.into(),
            cert_der,
            key_der_bytes,
            cert_pem: cert_pem.to_string(),
            key_pem: key_pem.to_string(),
        })
    }

    pub fn cert_der(&self) -> CertificateDer<'static> {
        self.cert_der.clone()
    }

    pub fn key_der(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(self.key_der_bytes.clone().into())
    }

    pub fn cert_pem(&self) -> String {
        self.cert_pem.clone()
    }

    pub fn key_pem(&self) -> String {
        self.key_pem.clone()
    }

    /// SHA-256 fingerprint of the DER-encoded certificate, hex-encoded.
    /// This is the value users compare during pairing and the value
    /// pinned in each peer's trust store thereafter.
    pub fn fingerprint(&self) -> String {
        crate::fingerprint::sha256_hex(&self.cert_der)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_identity_round_trips_through_pem() {
        let id = DeviceIdentity::generate(uuid::Uuid::new_v4(), "Alice-PC").unwrap();
        let fp_before = id.fingerprint();

        let reloaded = DeviceIdentity::from_pem(id.device_id, "Alice-PC", &id.cert_pem(), &id.key_pem()).unwrap();
        assert_eq!(fp_before, reloaded.fingerprint(), "fingerprint must survive a persist/reload cycle");
    }

    #[test]
    fn two_generated_identities_have_different_fingerprints() {
        let a = DeviceIdentity::generate(uuid::Uuid::new_v4(), "A").unwrap();
        let b = DeviceIdentity::generate(uuid::Uuid::new_v4(), "B").unwrap();
        assert_ne!(a.fingerprint(), b.fingerprint());
    }
}
