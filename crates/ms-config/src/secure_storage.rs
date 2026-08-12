use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecureStorageError {
    #[error("secure storage backend error: {0}")]
    Backend(String),
}

/// Abstraction over the OS-native secret store, used to hold the device's
/// private key and certificate PEM (`ms_security::DeviceIdentity`) outside
/// of the plain-JSON config file — those are the one piece of state in
/// this app that must never be readable by another local user account or
/// casual disk inspection.
///
/// Backed by Windows Credential Manager (DPAPI), macOS/iOS Keychain, or
/// the Linux Secret Service (D-Bus) via the `keyring` crate depending on
/// platform; `InMemorySecureStorage` exists for tests and any environment
/// lacking a secret-service daemon.
pub trait SecureStorage: Send + Sync {
    fn set(&self, key: &str, value: &str) -> Result<(), SecureStorageError>;
    fn get(&self, key: &str) -> Result<Option<String>, SecureStorageError>;
    fn delete(&self, key: &str) -> Result<(), SecureStorageError>;
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
const SERVICE_NAME: &str = "MouseShare";

/// Backed by Windows Credential Manager or macOS Keychain via the
/// `keyring` crate's native (non-D-Bus) backends. Only compiled for the
/// two platforms this app ships on — this crate still builds and tests on
/// other platforms (e.g. Linux CI) via `InMemorySecureStorage`, but a real
/// device identity is never stored anywhere except native OS secret
/// storage in production.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub struct KeyringSecureStorage;

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl SecureStorage for KeyringSecureStorage {
    fn set(&self, key: &str, value: &str) -> Result<(), SecureStorageError> {
        let entry = keyring::Entry::new(SERVICE_NAME, key).map_err(|e| SecureStorageError::Backend(e.to_string()))?;
        entry.set_password(value).map_err(|e| SecureStorageError::Backend(e.to_string()))
    }

    fn get(&self, key: &str) -> Result<Option<String>, SecureStorageError> {
        let entry = keyring::Entry::new(SERVICE_NAME, key).map_err(|e| SecureStorageError::Backend(e.to_string()))?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecureStorageError::Backend(e.to_string())),
        }
    }

    fn delete(&self, key: &str) -> Result<(), SecureStorageError> {
        let entry = keyring::Entry::new(SERVICE_NAME, key).map_err(|e| SecureStorageError::Backend(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecureStorageError::Backend(e.to_string())),
        }
    }
}

/// Process-local, non-persistent implementation used in tests and as a
/// fallback where no OS secret store is reachable. Never used for a real
/// device identity in production — `ms-daemon` selects
/// `KeyringSecureStorage` on Windows/macOS builds.
#[derive(Default)]
pub struct InMemorySecureStorage {
    entries: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl SecureStorage for InMemorySecureStorage {
    fn set(&self, key: &str, value: &str) -> Result<(), SecureStorageError> {
        self.entries.lock().unwrap().insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<String>, SecureStorageError> {
        Ok(self.entries.lock().unwrap().get(key).cloned())
    }

    fn delete(&self, key: &str) -> Result<(), SecureStorageError> {
        self.entries.lock().unwrap().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_value_round_trips() {
        let store = InMemorySecureStorage::default();
        store.set("device-key", "secret-pem-contents").unwrap();
        assert_eq!(store.get("device-key").unwrap(), Some("secret-pem-contents".to_string()));
    }

    #[test]
    fn missing_key_returns_none_not_an_error() {
        let store = InMemorySecureStorage::default();
        assert_eq!(store.get("nope").unwrap(), None);
    }

    #[test]
    fn deleted_key_is_gone() {
        let store = InMemorySecureStorage::default();
        store.set("k", "v").unwrap();
        store.delete("k").unwrap();
        assert_eq!(store.get("k").unwrap(), None);
    }
}
