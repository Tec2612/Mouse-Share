//! Device identity, mutual TLS, and secure pairing for Mouse Share.
//!
//! Trust model: there is no certificate authority. Each device holds a
//! self-signed long-term identity (`identity::DeviceIdentity`). Two
//! devices establish trust exactly once via an explicit, human-verified
//! pairing operation (`pairing::derive_sas` + `verifier::TofuVerifier`)
//! that pins the peer's certificate fingerprint into a `TrustStore`. Every
//! subsequent connection is mutually authenticated TLS restricted to
//! pinned fingerprints only (`verifier::PinnedVerifier`,
//! `tls::pinned_configs`) — an unpaired device on the LAN cannot complete
//! a handshake at all, let alone send input events.

pub mod fingerprint;
pub mod identity;
pub mod pairing;
pub mod tls;
pub mod trust_store;
pub mod verifier;

pub use identity::DeviceIdentity;
pub use pairing::derive_sas;
pub use trust_store::{PairedDevice, TrustStore};
pub use verifier::{PinnedVerifier, TofuVerifier};
