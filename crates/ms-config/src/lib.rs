//! Local configuration for Mouse Share: user-visible settings, the
//! multi-computer screen layout graph, and secure storage for the device's
//! private key material.

mod layout;
mod secure_storage;
mod settings;
mod store;

pub use layout::{ComputerNode, EdgeLink, LayoutError, ScreenLayout};
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub use secure_storage::KeyringSecureStorage;
pub use secure_storage::{InMemorySecureStorage, SecureStorage, SecureStorageError};
pub use settings::{EmergencyHotkey, LogLevel, MouseSettings, NetworkSettings, Settings, Theme};
pub use store::{ConfigError, ConfigFile, ConfigStore};
