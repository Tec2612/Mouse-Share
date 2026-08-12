use ms_config::ConfigStore;
use ms_security::DeviceIdentity;
use std::path::PathBuf;
use std::sync::Mutex;

/// A pairing dial that has completed its TLS/SAS handshake and is
/// awaiting the user's on-screen confirmation that both sides' codes
/// matched (see `commands::start_pairing`/`confirm_pairing`). Kept
/// in-memory only, cleared on confirm/cancel or app restart — there is
/// nothing here worth persisting; a stale pending pairing is simply
/// re-attempted.
pub struct PendingPairing {
    pub remote_fingerprint: String,
    pub remote_device_id: uuid::Uuid,
    pub remote_name: String,
    pub sas: String,
}

pub struct AppState {
    pub config_store: ConfigStore,
    pub trust_store_path: PathBuf,
    pub device_identity: DeviceIdentity,
    pub device_id: uuid::Uuid,
    pub device_name: String,
    /// A pairing we dialed out for (see `commands::start_pairing`).
    pub pending_pairing: Mutex<Option<PendingPairing>>,
    /// A pairing someone else dialed into us for (see
    /// `pairing::spawn_pairing_acceptor`). Kept separate from
    /// `pending_pairing` since both can legitimately be in flight at once
    /// (this device pairing out to one peer while a different peer pairs
    /// in to it).
    pub incoming_pairing: Mutex<Option<PendingPairing>>,
    /// Kept alive for as long as the app runs so its mDNS advertisement
    /// stays active — dropping it withdraws the advertisement. Never
    /// otherwise touched after `lib.rs`'s setup starts it.
    pub discovery: Mutex<Option<ms_discovery::DiscoveryService>>,
}
