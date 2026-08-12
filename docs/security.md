# Security Architecture

Implemented in `crates/ms-security`. This document explains the trust
model and why it was chosen; see the crate's doc comments and
`crates/ms-security/tests/` (real TLS handshakes over TCP loopback, not
mocked) for the enforcement mechanics.

## Threat model

**In scope:**
- Another device on the same LAN, with no prior relationship to this app,
  attempting to send input events, read clipboard data, or otherwise act
  as a paired peer.
- A passive network observer on the LAN attempting to read mouse/keyboard/
  clipboard traffic.
- An active on-path attacker on the LAN attempting to man-in-the-middle
  the pairing handshake specifically (the one moment before long-term
  trust exists).
- A previously-paired device that the user has since revoked.

**Out of scope** (consistent with a LAN input-sharing utility, not a
general-purpose remote-access tool):
- A compromised OS on either paired device (if the controlling machine is
  compromised, it can already type/click anything locally; this app
  doesn't increase that machine's capability, only extends where its
  *legitimate* input can reach).
- Physical access to an already-paired, already-unlocked device.
- Protecting against the *local* user of a device disabling the app's own
  protections (e.g. denying Accessibility permission is a user choice that
  simply stops the feature from working, not a security bypass to defend
  against).

## Device identity

Each device generates a long-term identity on first run
(`ms_security::DeviceIdentity::generate`): an ECDSA P-256 keypair and a
self-signed X.509 certificate. This is **not** backed by any certificate
authority — there is no natural CA for a consumer LAN app, and requiring
one would push a PKI decision onto users who shouldn't need to make one.

The certificate is long-dated (20 years) and is *never* chain-validated or
checked for expiry by a peer. That's intentional: the trust decision is
made entirely by fingerprint pinning (below), so a certificate's validity
period carries no security meaning here — a short lifetime would only
create spurious re-pairing prompts.

The private key is persisted in the OS-native secret store (Windows
Credential Manager, macOS Keychain, via `ms_config::KeyringSecureStorage`
/ the `keyring` crate's native — not D-Bus — backends), never in the
plain-JSON settings file. The certificate and its SHA-256 fingerprint are
not secret (the fingerprint is quite literally displayed on screen during
pairing) and live in ordinary config storage.

## Pairing: trust-on-first-use with a cryptographic, not decorative, PIN

Two devices establish trust exactly once, through an explicit action on
both sides (never automatically, never merely because a device appears on
the LAN):

1. Both devices open a short-lived TLS 1.3 connection using
   `ms_security::TofuVerifier`, which accepts any well-formed,
   correctly-signed certificate (trust-on-first-use) and records the
   observed peer fingerprint. **This verifier is used for nothing else in
   the app** — it exists only for this bootstrap step.
2. Once the handshake completes, both sides derive a 6-digit **short
   authentication string (SAS)** via `ms_security::derive_sas`, from:
   - The TLS 1.3 **exporter keying material** for this specific
     connection (`export_keying_material`), which comes from the
     (EC)DHE secret negotiated for this session.
   - Both sides' observed certificate fingerprints, sorted so both
     participants compute the identical digest regardless of TLS role.
3. Both users compare the 6-digit codes shown on their two screens.

**Why this defeats an active MITM, unlike a PIN the app just makes up:**
an on-path attacker splitting the connection into two TLS legs gets two
*independent* (EC)DHE secrets — one per leg — because it cannot make both
legs agree on a shared secret without knowing the private keys involved.
The exporter keying material therefore differs between what the two real
endpoints compute, so the SAS each victim sees would differ, and the
human comparison step catches it. This is the same principle Bluetooth
"Numbers Match" and ZRTP pairing use — the code is verifying the
*channel*, not just being retyped as an OTP. Verified in
`crates/ms-security/tests/pairing_flow.rs` against a real TLS 1.3
handshake (both sides derive byte-identical exporter material and thus
identical SAS codes, with no data sent over the wire to compute it).
4. Once confirmed, both sides add each other's certificate fingerprint to
   their `TrustStore` (persisted as plain JSON — see [Why fingerprints
   don't need secrecy](#why-fingerprints-dont-need-secrecy) below). This
   is the *only* place a PIN/SAS plays any role — it is never used again
   after this one bootstrap.

## Long-term authentication: pinned mutual TLS

Every connection after pairing uses `ms_security::PinnedVerifier` on
**both** the client and server side (`ms_security::tls::pinned_configs`
builds a `ClientConfig` and `ServerConfig` that both require and check a
client certificate — this is genuine mutual TLS, not server-only TLS with
a bolted-on token):

- The verifier checks the peer's certificate SHA-256 fingerprint against
  the local `TrustStore`. An unpinned fingerprint fails the handshake
  outright — `verify_server_cert`/`verify_client_cert` return an error
  before any application data is exchanged.
- Signature verification (`verify_tls12_signature`/`verify_tls13_signature`,
  delegated to rustls's own crypto-provider implementation) still runs in
  full: pinning replaces *chain-of-trust* validation, not the proof that
  the peer actually holds the private key matching its pinned certificate.
  A stolen/observed certificate without the private key cannot complete a
  handshake.
- **No shared PIN, password, or other long-term secret is used for
  ongoing authentication** — only possession of the private key
  corresponding to a pinned certificate. This directly satisfies "do not
  rely solely on a shared PIN for long-term authentication": the PIN/SAS
  is single-use, at pairing time only.

Verified end-to-end over a real TCP loopback connection in
`crates/ms-security/tests/mtls_handshake.rs`: a paired device completes
the handshake; an unpaired device's handshake fails on both ends; a
revoked device is rejected on its very next connection attempt.

### Why fingerprints don't need secrecy

A certificate fingerprint proves nothing by itself — proving control of
the device requires the matching private key, which never leaves secure
storage. Treating the trust store as sensitive would only complicate
backup/sync for no security benefit, so it's stored as plain, readable
JSON alongside other non-secret settings.

## Revocation

`TrustStore::revoke(fingerprint)` removes a device from the trust store;
the very next connection attempt from that device fails
`PinnedVerifier`'s check (there is no cached "still trusted" state to
invalidate separately — the trust store *is* the check). `ms-core-service`
is expected to also proactively close any currently-live session for a
revoked fingerprint immediately after calling `revoke`, so a revocation
takes effect without waiting for the next reconnect attempt.

## Encrypted transport

Every connection that carries mouse, keyboard, or clipboard data is TLS
1.3 (rustls, `ring` crypto provider) end to end. There is no code path
that opens a socket and begins forwarding input before the mTLS handshake
completes and `PinnedVerifier` accepts the peer — the pairing (TOFU)
verifier and the pinned verifier are distinct types used in entirely
separate connection setup functions
(`ms_security::tls::{pairing_configs, pinned_configs}`), so a device can't
accidentally end up accepting unpinned input traffic through
misconfiguration of "the same" verifier.

## Clipboard

Clipboard sync defaults to **off** (`Settings::clipboard_sharing_enabled
== false` by default — see `ms-config`'s tests) and is gated in exactly
one place, `ms_input_core::ClipboardGateway`:

- Disabled: no outbound offer is ever generated, incoming offers are
  never acknowledged (not even to say "no thanks" — silently ignored),
  and incoming data payloads are dropped unread rather than merely left
  unapplied.
- Enabled: sync is pull-based (`ClipboardOffer` → `ClipboardRequest` →
  `ClipboardData`) so content only crosses the network when the receiving
  side actually wants it, not proactively on every copy.

## Emergency release

The `EdgeStateMachine`'s `force_release` path (triggered by the
configured emergency hotkey, or by `Event::ConnectionLost`) is the single
code path both the panic-hotkey and connection-failure release use — see
[protocol-spec.md](protocol-spec.md#heartbeats--reconnection). This means
the safety guarantee ("never allow the user to become permanently locked
out of their local mouse or keyboard") isn't a separate feature to keep in
sync with the rest of the state machine; it's structurally the same
transition regardless of trigger, and is covered by dedicated tests in
`crates/ms-input-core/src/state_machine.rs`
(`emergency_hotkey_forces_release_even_mid_session`,
`connection_lost_while_controlling_forces_local_release`).

## Logging

Structured logs (via `tracing`) cover discovery, pairing, authentication,
connection lifecycle, and errors. **Never logged**: actual keyboard
characters/keys typed, clipboard contents, passwords, or private key
material — logging is restricted to event *kinds* and metadata (which
device, which message type, timing, error category), never payload
content. See [architecture.md](architecture.md) for where each subsystem's
logs originate.
