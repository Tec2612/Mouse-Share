//! LAN device discovery for Mouse Share: mDNS/Bonjour advertisement and
//! browsing, plus manual IP/hostname connection for networks where
//! multicast discovery doesn't reach (client isolation, VPNs, some guest
//! Wi-Fi setups).

mod announcement;
mod manual;
mod service;

pub use announcement::{AnnouncementError, DeviceAnnouncement, RemoteOs, SERVICE_TYPE};
pub use manual::{parse_manual_target, ManualTarget, ManualTargetError};
pub use service::{translate_event, DiscoveryError, DiscoveryEvent, DiscoveryService};
