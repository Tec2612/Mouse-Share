use ms_protocol::{ClipboardFormat, DeviceId, Message};

/// Gates clipboard synchronization behind the user's setting. This is
/// intentionally the *only* place that decides whether clipboard content
/// leaves the machine, so the "never transmit clipboard data unless the
/// user explicitly enables it" requirement has one obvious enforcement
/// point instead of being scattered across the daemon.
pub struct ClipboardGateway {
    enabled: bool,
}

impl ClipboardGateway {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Called when the local clipboard changes while a peer is being
    /// controlled (there is no point announcing clipboard contents to a
    /// device we're not actively sharing input with). Returns the offer to
    /// send, or `None` if sharing is disabled.
    pub fn on_local_clipboard_changed(
        &self,
        active_peer: Option<DeviceId>,
        formats: Vec<ClipboardFormat>,
    ) -> Option<(DeviceId, Message)> {
        if !self.enabled {
            return None;
        }
        let peer = active_peer?;
        if formats.is_empty() {
            return None;
        }
        Some((peer, Message::ClipboardOffer { formats }))
    }

    /// Called when a `ClipboardOffer` arrives from a peer. Returns the
    /// request to send back to pull the data, or `None` if sharing is
    /// disabled — a disabled gateway must not even acknowledge the offer,
    /// let alone request the contents.
    pub fn on_offer_received(&self, from: DeviceId, formats: &[ClipboardFormat]) -> Option<(DeviceId, Message)> {
        if !self.enabled {
            return None;
        }
        let format = *formats.first()?;
        Some((from, Message::ClipboardRequest { format }))
    }

    /// Called when a `ClipboardData` payload arrives. Returns the bytes to
    /// apply to the local clipboard, or `None` if sharing is disabled (in
    /// which case the payload must be dropped unread, not just unapplied,
    /// so a user who disables sharing mid-session gets the full guarantee
    /// immediately rather than after the next restart).
    pub fn on_data_received(&self, data: Vec<u8>) -> Option<Vec<u8>> {
        self.enabled.then_some(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_gateway_never_produces_an_outbound_offer() {
        let gw = ClipboardGateway::new(false);
        let peer = DeviceId::new_v4();
        assert!(gw.on_local_clipboard_changed(Some(peer), vec![ClipboardFormat::PlainTextUtf8]).is_none());
    }

    #[test]
    fn disabled_gateway_ignores_incoming_offers_and_data() {
        let gw = ClipboardGateway::new(false);
        let peer = DeviceId::new_v4();
        assert!(gw.on_offer_received(peer, &[ClipboardFormat::PlainTextUtf8]).is_none());
        assert!(gw.on_data_received(b"secret".to_vec()).is_none());
    }

    #[test]
    fn enabled_gateway_offers_to_the_active_peer_only() {
        let gw = ClipboardGateway::new(true);
        let peer = DeviceId::new_v4();
        let (to, msg) = gw.on_local_clipboard_changed(Some(peer), vec![ClipboardFormat::PlainTextUtf8]).unwrap();
        assert_eq!(to, peer);
        assert!(matches!(msg, Message::ClipboardOffer { .. }));
    }

    #[test]
    fn enabled_gateway_with_no_active_peer_does_not_offer() {
        let gw = ClipboardGateway::new(true);
        assert!(gw.on_local_clipboard_changed(None, vec![ClipboardFormat::PlainTextUtf8]).is_none());
    }

    #[test]
    fn enabled_gateway_requests_and_applies_normally() {
        let gw = ClipboardGateway::new(true);
        let peer = DeviceId::new_v4();
        let (to, msg) = gw.on_offer_received(peer, &[ClipboardFormat::PlainTextUtf8]).unwrap();
        assert_eq!(to, peer);
        assert!(matches!(msg, Message::ClipboardRequest { .. }));

        assert_eq!(gw.on_data_received(b"hello".to_vec()), Some(b"hello".to_vec()));
    }

    #[test]
    fn toggling_takes_effect_immediately_for_subsequent_calls() {
        let mut gw = ClipboardGateway::new(true);
        assert!(gw.on_data_received(vec![1]).is_some());
        gw.set_enabled(false);
        assert!(gw.on_data_received(vec![1]).is_none());
    }
}
