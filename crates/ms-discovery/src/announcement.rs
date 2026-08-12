use serde::{Deserialize, Serialize};
use std::net::IpAddr;

pub const SERVICE_TYPE: &str = "_mouseshare._tcp.local.";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RemoteOs {
    Windows,
    MacOs,
}

impl RemoteOs {
    pub fn as_txt_value(self) -> &'static str {
        match self {
            RemoteOs::Windows => "windows",
            RemoteOs::MacOs => "macos",
        }
    }

    pub fn from_txt_value(s: &str) -> Option<Self> {
        match s {
            "windows" => Some(RemoteOs::Windows),
            "macos" => Some(RemoteOs::MacOs),
            _ => None,
        }
    }
}

/// A device seen on the LAN, either via mDNS or entered manually. This is
/// purely discovery information — it says nothing about whether the two
/// devices are paired; `ms-security::TrustStore` is the source of truth
/// for that, keyed by certificate fingerprint rather than anything
/// broadcast here (fingerprints are not advertised over mDNS, since they
/// only matter once two specific devices decide to pair).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceAnnouncement {
    pub device_id: uuid::Uuid,
    pub name: String,
    pub os: RemoteOs,
    pub addrs: Vec<IpAddr>,
    pub port: u16,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AnnouncementError {
    #[error("missing or invalid required TXT property `{0}`")]
    MissingProperty(&'static str),
    #[error("resolved service advertised no usable IP addresses")]
    NoAddresses,
}

impl DeviceAnnouncement {
    /// Builds the TXT record set advertised alongside this device's mDNS
    /// service registration.
    pub fn txt_properties(device_id: uuid::Uuid, name: &str, os: RemoteOs) -> Vec<(String, String)> {
        vec![
            ("device_id".to_string(), device_id.to_string()),
            ("name".to_string(), name.to_string()),
            ("os".to_string(), os.as_txt_value().to_string()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_txt_value_round_trips() {
        for os in [RemoteOs::Windows, RemoteOs::MacOs] {
            assert_eq!(RemoteOs::from_txt_value(os.as_txt_value()), Some(os));
        }
    }

    #[test]
    fn unknown_os_txt_value_is_rejected() {
        assert_eq!(RemoteOs::from_txt_value("beos"), None);
    }

    #[test]
    fn txt_properties_contain_all_required_keys() {
        let id = uuid::Uuid::new_v4();
        let props = DeviceAnnouncement::txt_properties(id, "Alice-PC", RemoteOs::Windows);
        let keys: Vec<_> = props.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"device_id"));
        assert!(keys.contains(&"name"));
        assert!(keys.contains(&"os"));
    }
}
