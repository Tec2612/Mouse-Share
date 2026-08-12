use ms_keymap::ModifierPolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Theme {
    System,
    Light,
    Dark,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::System
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Info
    }
}

/// A single chord for the emergency "stop sharing / return control"
/// action. Represented as logical keys (not platform codes) so the same
/// config works after moving a device between OSes, and stored separately
/// from per-device settings since it must always be honored locally
/// regardless of which remote device currently has control — see the
/// "never allow the user to become permanently locked out" requirement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmergencyHotkey {
    pub keys: Vec<ms_protocol::LogicalKey>,
}

impl Default for EmergencyHotkey {
    fn default() -> Self {
        // Ctrl+Alt+Esc-style panic chord: rare in normal use on either OS,
        // reachable with one hand, and doesn't collide with common app
        // shortcuts on either platform.
        use ms_protocol::LogicalKey::*;
        Self { keys: vec![ControlLeft, AltLeft, Escape] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MouseSettings {
    /// Multiplier applied to inbound relative deltas before injection,
    /// letting a user compensate for a persistent feel mismatch between
    /// two machines' native pointer speed curves. 1.0 = pass through
    /// unchanged.
    pub sensitivity_multiplier: f64,
    /// When true, the receiving OS's own pointer acceleration curve is
    /// left to act on injected relative deltas as if they were live
    /// hardware input (the default, and the closest to "feels local").
    /// When false, deltas are injected in a way that bypasses the
    /// receiving OS's acceleration (where the platform backend supports
    /// it), useful for users who find double-acceleration (sender's
    /// curve, applied before deltas were even coalesced, stacked with the
    /// receiver's curve) makes the pointer feel inconsistent.
    pub respect_receiver_acceleration: bool,
    pub natural_scrolling: bool,
}

impl Default for MouseSettings {
    fn default() -> Self {
        Self { sensitivity_multiplier: 1.0, respect_receiver_acceleration: true, natural_scrolling: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkSettings {
    /// Port for already-paired sessions (pinned mutual TLS) — owned by
    /// the background daemon (`ms-daemon`).
    pub listen_port: u16,
    /// Port for the trust-on-first-use pairing handshake — a distinct
    /// port from `listen_port` because the two use different TLS
    /// verifiers (TOFU-accept-any vs. pinned-only; see `docs/security.md`)
    /// and, in the current build, different owning processes: the
    /// desktop UI runs the pairing acceptor directly (since pairing is
    /// inherently something the user is present and looking at the
    /// screen for), while `listen_port` is reserved for the always-on
    /// background daemon once it's wired to actually run continuously.
    pub pairing_port: u16,
    pub auto_discovery_enabled: bool,
    /// Heartbeat interval; also drives dead-connection detection (a
    /// session is considered lost after `heartbeat_interval * 3` with no
    /// response — see `ms-core-service`).
    pub heartbeat_interval_ms: u64,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self { listen_port: 45677, pairing_port: 45678, auto_discovery_enabled: true, heartbeat_interval_ms: 1000 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub start_on_boot: bool,
    pub clipboard_sharing_enabled: bool,
    pub modifier_policy: ModifierPolicy,
    pub emergency_hotkey: EmergencyHotkey,
    pub mouse: MouseSettings,
    pub network: NetworkSettings,
    pub log_level: LogLevel,
    pub theme: Theme,
    /// BCP-47 language tag, e.g. "en-US".
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_on_boot: false,
            // Off by default: clipboard contents are sensitive, so sharing
            // must be an explicit opt-in per the spec, never silently on.
            clipboard_sharing_enabled: false,
            modifier_policy: ModifierPolicy::default(),
            emergency_hotkey: EmergencyHotkey::default(),
            mouse: MouseSettings::default(),
            network: NetworkSettings::default(),
            log_level: LogLevel::default(),
            theme: Theme::default(),
            language: "en-US".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_sharing_defaults_to_disabled() {
        assert!(!Settings::default().clipboard_sharing_enabled, "must never transmit clipboard data unless explicitly enabled");
    }

    #[test]
    fn settings_round_trip_through_toml() {
        let settings = Settings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let restored: Settings = toml::from_str(&toml_str).unwrap();
        assert_eq!(settings, restored);
    }
}
