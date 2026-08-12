use crate::announcement::{AnnouncementError, DeviceAnnouncement, RemoteOs, SERVICE_TYPE};
use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent, ServiceInfo};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("mDNS daemon error: {0}")]
    Daemon(#[from] mdns_sd::Error),
}

/// One update from the browse stream. `Lost` carries just the mDNS
/// fullname (not a full `DeviceAnnouncement`) because that's all a removal
/// record contains — the caller matches it against previously-seen
/// announcements by fullname to know which device went away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEvent {
    Found(DeviceAnnouncement),
    Lost { fullname: String },
}

/// Thin wrapper around `mdns_sd::ServiceDaemon` that advertises this
/// device and browses for peers, translating the crate's generic
/// `ServiceInfo`/`ServiceEvent` types into `DeviceAnnouncement`s at the
/// `_mouseshare._tcp.local.` service type. Manual IP/hostname connections
/// (see `manual.rs`) don't go through this at all — they're for when
/// mDNS is unavailable or blocked (e.g. client-isolated Wi-Fi), which is
/// common enough on consumer networks that it can't be an afterthought.
pub struct DiscoveryService {
    daemon: ServiceDaemon,
    registered_fullname: Option<String>,
}

impl DiscoveryService {
    pub fn new() -> Result<Self, DiscoveryError> {
        Ok(Self { daemon: ServiceDaemon::new()?, registered_fullname: None })
    }

    /// Advertises this device on the LAN so other Mouse Share instances
    /// can discover it. `host_name` should be a DNS-safe label (e.g.
    /// derived from the device name); it need not be globally unique,
    /// only unique enough on this LAN segment for mDNS's built-in conflict
    /// probing to handle.
    pub fn advertise(
        &mut self,
        device_id: uuid::Uuid,
        name: &str,
        os: RemoteOs,
        host_name: &str,
        port: u16,
    ) -> Result<(), DiscoveryError> {
        let properties = DeviceAnnouncement::txt_properties(device_id, name, os);
        let host_fqdn = format!("{host_name}.local.");
        let info = ServiceInfo::new(SERVICE_TYPE, name, &host_fqdn, "", port, &properties[..])?
            .enable_addr_auto();
        let fullname = info.get_fullname().to_string();
        self.daemon.register(info)?;
        self.registered_fullname = Some(fullname);
        Ok(())
    }

    pub fn stop_advertising(&mut self) -> Result<(), DiscoveryError> {
        if let Some(fullname) = self.registered_fullname.take() {
            self.daemon.unregister(&fullname)?;
        }
        Ok(())
    }

    /// Starts browsing for other Mouse Share devices. Returns the raw
    /// mdns-sd receiver; callers translate events with
    /// [`translate_event`] so parsing/validation logic stays unit
    /// testable independent of any real network I/O.
    pub fn browse(&self) -> Result<Receiver<ServiceEvent>, DiscoveryError> {
        Ok(self.daemon.browse(SERVICE_TYPE)?)
    }

    pub fn shutdown(self) -> Result<(), DiscoveryError> {
        self.daemon.shutdown()?;
        Ok(())
    }
}

/// Converts a raw mdns-sd event into a `DiscoveryEvent`, or `None` for
/// event kinds that carry no actionable information for the caller
/// (`SearchStarted`/`SearchStopped`/bare `ServiceFound` before
/// resolution completes).
pub fn translate_event(event: ServiceEvent) -> Option<Result<DiscoveryEvent, AnnouncementError>> {
    match event {
        ServiceEvent::ServiceResolved(info) => Some(announcement_from_service_info(&info).map(DiscoveryEvent::Found)),
        ServiceEvent::ServiceRemoved(_ty, fullname) => Some(Ok(DiscoveryEvent::Lost { fullname })),
        ServiceEvent::SearchStarted(_)
        | ServiceEvent::SearchStopped(_)
        | ServiceEvent::ServiceFound(_, _) => None,
    }
}

fn announcement_from_service_info(info: &ServiceInfo) -> Result<DeviceAnnouncement, AnnouncementError> {
    let device_id_str = info
        .get_property_val_str("device_id")
        .ok_or(AnnouncementError::MissingProperty("device_id"))?;
    let device_id = uuid::Uuid::parse_str(device_id_str)
        .map_err(|_| AnnouncementError::MissingProperty("device_id"))?;

    let name = info
        .get_property_val_str("name")
        .ok_or(AnnouncementError::MissingProperty("name"))?
        .to_string();

    let os_str = info.get_property_val_str("os").ok_or(AnnouncementError::MissingProperty("os"))?;
    let os = RemoteOs::from_txt_value(os_str).ok_or(AnnouncementError::MissingProperty("os"))?;

    let addrs: Vec<_> = info.get_addresses().iter().copied().collect();
    if addrs.is_empty() {
        return Err(AnnouncementError::NoAddresses);
    }

    Ok(DeviceAnnouncement { device_id, name, os, addrs, port: info.get_port() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_service_info(device_id: uuid::Uuid, name: &str, os: RemoteOs) -> ServiceInfo {
        let properties = DeviceAnnouncement::txt_properties(device_id, name, os);
        ServiceInfo::new(SERVICE_TYPE, "instance", "host.local.", "192.168.1.50", 45678, &properties[..])
            .unwrap()
    }

    #[test]
    fn well_formed_service_info_translates_to_an_announcement() {
        let id = uuid::Uuid::new_v4();
        let info = sample_service_info(id, "Alice-PC", RemoteOs::Windows);
        let event = translate_event(ServiceEvent::ServiceResolved(info)).unwrap().unwrap();
        match event {
            DiscoveryEvent::Found(ann) => {
                assert_eq!(ann.device_id, id);
                assert_eq!(ann.name, "Alice-PC");
                assert_eq!(ann.os, RemoteOs::Windows);
                assert_eq!(ann.port, 45678);
                assert!(!ann.addrs.is_empty());
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn removed_service_translates_to_lost_by_fullname() {
        let event = ServiceEvent::ServiceRemoved(SERVICE_TYPE.to_string(), "Alice-PC.instance._mouseshare._tcp.local.".to_string());
        let translated = translate_event(event).unwrap().unwrap();
        assert!(matches!(translated, DiscoveryEvent::Lost { .. }));
    }

    #[test]
    fn bookkeeping_events_translate_to_none() {
        assert!(translate_event(ServiceEvent::SearchStarted(SERVICE_TYPE.to_string())).is_none());
        assert!(translate_event(ServiceEvent::SearchStopped(SERVICE_TYPE.to_string())).is_none());
        assert!(translate_event(ServiceEvent::ServiceFound(SERVICE_TYPE.to_string(), "x".to_string())).is_none());
    }

    #[test]
    fn missing_device_id_property_is_rejected() {
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            "instance",
            "host.local.",
            "192.168.1.50",
            45678,
            &[("name", "Alice-PC"), ("os", "windows")][..],
        )
        .unwrap();
        let result = translate_event(ServiceEvent::ServiceResolved(info)).unwrap();
        assert_eq!(result, Err(AnnouncementError::MissingProperty("device_id")));
    }
}
