use anyhow::{Context, Result};
use ms_security::{DeviceIdentity, TrustStore};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CERT_KEY: &str = "device-cert-pem";
const PRIVATE_KEY_KEY: &str = "device-key-pem";

#[derive(Serialize, Deserialize)]
struct DeviceMeta {
    device_id: uuid::Uuid,
    device_name: String,
}

/// Loads this device's stable id and display name from `device.json` in
/// the config directory, generating both on first run. Kept separate from
/// `ConfigFile` (which is meant to be freely hand-editable/portable)
/// because `device_id` must never change once other devices have paired
/// against it — pairing trusts a certificate `ms_security::DeviceIdentity`
/// binds to this same id, so treating it as just another setting risks a
/// user "fixing" it and silently breaking every existing pairing.
pub fn load_or_create_device_meta(config_dir: &std::path::Path) -> Result<(uuid::Uuid, String)> {
    let path = config_dir.join("device.json");
    if path.exists() {
        let contents = std::fs::read_to_string(&path).context("reading device.json")?;
        let meta: DeviceMeta = serde_json::from_str(&contents).context("parsing device.json")?;
        return Ok((meta.device_id, meta.device_name));
    }

    let hostname = hostname_guess();
    let meta = DeviceMeta { device_id: uuid::Uuid::new_v4(), device_name: hostname };
    std::fs::write(&path, serde_json::to_string_pretty(&meta)?).context("writing device.json")?;
    Ok((meta.device_id, meta.device_name))
}

fn hostname_guess() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Mouse Share Device".to_string())
}

/// Loads this device's long-term identity from secure storage, generating
/// and persisting a new one on first run. `device_id`/`device_name` are
/// re-supplied on every load (they come from `ConfigFile`, not from the
/// secure-storage blob itself) since only the key material — the part
/// that actually needs to survive in a place other apps/users can't read
/// — belongs there; the human-facing name is free to change without
/// touching the cryptographic identity.
pub fn load_or_generate(
    storage: &dyn ms_config::SecureStorage,
    device_id: uuid::Uuid,
    device_name: &str,
) -> Result<DeviceIdentity> {
    let existing = storage.get(CERT_KEY).context("reading device certificate from secure storage")?;
    let existing_key = storage.get(PRIVATE_KEY_KEY).context("reading device private key from secure storage")?;

    if let (Some(cert_pem), Some(key_pem)) = (existing, existing_key) {
        tracing::info!("loaded existing device identity from secure storage");
        return DeviceIdentity::from_pem(device_id, device_name, &cert_pem, &key_pem)
            .context("parsing stored device identity");
    }

    tracing::info!("no existing device identity found; generating one");
    let identity = DeviceIdentity::generate(device_id, device_name).context("generating device identity")?;
    storage.set(CERT_KEY, &identity.cert_pem()).context("persisting device certificate")?;
    storage.set(PRIVATE_KEY_KEY, &identity.key_pem()).context("persisting device private key")?;
    Ok(identity)
}

/// The trust store is not a secret (see `docs/security.md`), so it's
/// plain JSON next to the rest of the config rather than in secure
/// storage — this also makes it trivial for a user to inspect what's
/// paired without the app's UI.
pub fn trust_store_path(config_dir: &std::path::Path) -> PathBuf {
    config_dir.join("trust_store.json")
}

pub fn load_trust_store(path: &std::path::Path) -> Result<TrustStore> {
    if !path.exists() {
        return Ok(TrustStore::new());
    }
    let contents = std::fs::read_to_string(path).context("reading trust store")?;
    serde_json::from_str(&contents).context("parsing trust store")
}

/// Called once a pairing flow (SAS confirmed on both sides) adds a device
/// to the in-memory `TrustStore`, and on revocation. Not yet wired to a
/// caller in this minimal daemon — pairing is currently initiated only
/// through the not-yet-built UI's "Pair" action — but is the real,
/// complete persistence path for when that command lands, not a stub.
#[allow(dead_code)]
pub fn save_trust_store(path: &std::path::Path, store: &TrustStore) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(store)?;
    std::fs::write(path, json).context("writing trust store")
}
