use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairedDevice {
    pub device_id: uuid::Uuid,
    pub name: String,
    /// Colon-separated hex SHA-256 fingerprint of the peer's certificate,
    /// as produced by `fingerprint::sha256_hex`. This — not `device_id`,
    /// which is just a self-reported label — is what authentication
    /// actually checks against on every connection.
    pub fingerprint: String,
    pub paired_at_unix_ms: u64,
}

/// The set of devices this device has completed pairing with. Persisted by
/// the caller (see `ms-config`) as plain JSON — the fingerprints in it are
/// public information (they're literally displayed on screen during
/// pairing), so this file needs integrity, not secrecy; the corresponding
/// private key lives in platform secure storage instead, via
/// `DeviceIdentity`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustStore {
    devices: HashMap<String, PairedDevice>, // keyed by fingerprint
}

impl TrustStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, device: PairedDevice) {
        self.devices.insert(device.fingerprint.clone(), device);
    }

    pub fn is_trusted(&self, fingerprint: &str) -> bool {
        self.devices.contains_key(fingerprint)
    }

    pub fn get(&self, fingerprint: &str) -> Option<&PairedDevice> {
        self.devices.get(fingerprint)
    }

    /// Removes a paired device so it can no longer authenticate. Existing
    /// TLS connections from that device are not automatically torn down by
    /// this call alone — `ms-core-service` is expected to also close any
    /// live session for the revoked fingerprint immediately after calling
    /// this, so revocation takes effect without waiting for a reconnect.
    pub fn revoke(&mut self, fingerprint: &str) -> Option<PairedDevice> {
        self.devices.remove(fingerprint)
    }

    pub fn list(&self) -> impl Iterator<Item = &PairedDevice> {
        self.devices.values()
    }

    pub fn all_fingerprints(&self) -> Vec<String> {
        self.devices.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(fingerprint: &str) -> PairedDevice {
        PairedDevice {
            device_id: uuid::Uuid::new_v4(),
            name: "Test Device".into(),
            fingerprint: fingerprint.into(),
            paired_at_unix_ms: 0,
        }
    }

    #[test]
    fn added_device_is_trusted_and_revoked_device_is_not() {
        let mut store = TrustStore::new();
        store.add(sample("AA:BB"));
        assert!(store.is_trusted("AA:BB"));

        store.revoke("AA:BB");
        assert!(!store.is_trusted("AA:BB"));
    }

    #[test]
    fn unknown_fingerprint_is_never_trusted() {
        let store = TrustStore::new();
        assert!(!store.is_trusted("00:00"));
    }

    #[test]
    fn serializes_round_trip_as_json() {
        let mut store = TrustStore::new();
        store.add(sample("11:22"));
        let json = serde_json_for_test(&store);
        let restored: TrustStore = serde_json::from_str(&json).unwrap();
        assert!(restored.is_trusted("11:22"));
    }

    fn serde_json_for_test(store: &TrustStore) -> String {
        serde_json::to_string(store).unwrap()
    }
}
