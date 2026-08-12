# Wire Protocol Specification

Implemented in `crates/ms-protocol`. Version: `PROTOCOL_VERSION = 1`.

## Transport

One TCP connection per device pair, secured with mutual TLS 1.3 (see
[security.md](security.md)). `TCP_NODELAY` is expected to be set by the
connection owner (`ms-core-service`) — batching is handled explicitly at
the application layer (see [Mouse-move coalescing](#mouse-move-coalescing)
below), not left to Nagle's algorithm, so disabling it removes pointless
added latency without losing any of the intended batching.

Only one connection exists per device pair at a time; a device with more
than two peers (see the hub topology in the [user
guide](user-guide.md#multiple-computers)) holds one independent connection
per peer.

## Framing

Every message is:

```
┌─────────────────────┬───────────────────────────┐
│ length: u32, LE      │ body: bincode(Message)     │
│ (4 bytes)            │ (length bytes)              │
└─────────────────────┴───────────────────────────┘
```

- `length` is the byte length of the bincode-serialized `Message` that
  follows, little-endian.
- Messages larger than `MAX_MESSAGE_BYTES` (16 MiB) are rejected before
  the body is read, bounding how much a corrupt or hostile length prefix
  can force a receiver to allocate.
- Encoding is [bincode](https://docs.rs/bincode) 1.x: compact, no schema
  negotiation beyond the `PROTOCOL_VERSION` field in `Hello` (see below).

Reference implementation: `ms_protocol::{read_message, write_message,
encode_message, decode_message}`, tested for round-tripping, EOF handling,
and oversized-length rejection in `crates/ms-protocol/src/framing.rs`.

## Message catalogue

All messages are variants of the single `Message` enum
(`crates/ms-protocol/src/messages.rs`):

| Message | Direction | Purpose |
|---|---|---|
| `Hello { protocol_version, device_id, device_name, os, session_id }` | First message, either direction | Announces protocol compatibility and a fresh `session_id` for this connection. Sent *after* the TLS handshake — the peer's identity is already authenticated by then; this is metadata, not auth. |
| `HelloAck { protocol_version, accepted, reason }` | Reply to `Hello` | Confirms or rejects based on protocol-version compatibility. |
| `Heartbeat { seq, sent_at_ms }` / `HeartbeatAck { seq }` | Both directions, periodic | Liveness signal; see [Heartbeats & reconnection](#heartbeats--reconnection). |
| `MouseMove { dx, dy, seq }` | Controller → controlled | Coalesced relative movement (see below). `dx`/`dy` are in the sender's DPI-normalized pixel space. |
| `MouseWarp { x, y }` | Controller → controlled | Absolute placement, sent exactly once per hand-off so the cursor appears at the right point along the shared edge. |
| `MouseButton { button, pressed }` | Controller → controlled | Left/Right/Middle/Back/Forward/Other(n). |
| `MouseWheel { delta_x, delta_y, high_resolution }` | Controller → controlled | Vertical + horizontal scroll; `high_resolution` flags sub-notch (precision trackpad / high-res mouse) deltas. |
| `KeyEvent { key, pressed, modifiers, seq }` | Controller → controlled | `key` is a `LogicalKey` (position-based, platform-independent — see [architecture.md](architecture.md) and `ms-keymap`), not a raw platform key code. |
| `EdgeEnter { edge, position, sender_scale }` | Controller → controlled | Sent when the mouse crosses a configured edge; `position` is normalized 0.0–1.0 along the edge, `sender_scale` is the sender's DPI scale factor (96 DPI = 1.0) for rescaling subsequent deltas. |
| `EdgeRelease` | Controlled → controller | Sent when the controlled device's own cursor reaches *its* configured return edge; hands control back. |
| `ClipboardOffer { formats }` / `ClipboardRequest { format }` / `ClipboardData { format, data }` | Both directions | Pull-based clipboard sync (offer, then the receiver explicitly requests before any content moves) — see [security.md](security.md#clipboard). Never sent unless clipboard sharing is enabled locally. |
| `Disconnect { reason }` | Either direction | Graceful-disconnect notice before closing. |

`LogicalKey` (in `ms-protocol`, translated by `ms-keymap`) and
`Modifiers` are documented in [architecture.md](architecture.md) and the
crate doc comments; they cover letters/digits, F1–F20, all documented
navigation/editing keys, the full numpad, punctuation, and media keys.

## Mouse-move coalescing

Implemented in `ms_protocol::MouseMoveCoalescer`. A fast mouse swipe can
generate a hardware sample roughly every 1ms; sending one `MouseMove` per
sample would flood the connection for no perceptible benefit. The
coalescer instead:

- Accumulates `(dx, dy)` as raw samples arrive.
- Flushes (emits one `MouseMove`) when **either** `min_interval` has
  elapsed since the last flush (default 8ms, i.e. ≥125Hz — above typical
  mouse polling rates, so this never adds perceptible latency) **or**
  accumulated displacement exceeds `flush_threshold_px` (default 64px),
  whichever comes first.
- The threshold branch exists so a very fast, large swipe still flushes
  promptly rather than waiting out the interval with a large backlog.

This bounds the outbound mouse-move message rate to roughly 125/s
regardless of input device polling rate, while keeping added latency below
the ~8ms threshold, without any protocol-level negotiation — it's purely a
sender-side transmission policy. Every message still carries a
monotonically increasing `seq`, used for the duplicate/replay protection
described next.

## Duplicate-event prevention

Implemented in `ms_core_service::SequenceGuard`, not in `ms-protocol`
itself, since it's a connection-lifecycle concern:

- Each TLS connection is tagged with the `session_id` from its `Hello`.
  Any message tagged with a `session_id` other than the currently active
  one is rejected — this catches a straggling message from a connection
  that a reconnect has already superseded (a possible race, not a normal
  occurrence, since TCP itself guarantees no duplication/reordering
  *within* one live connection).
- Within a session, `seq` must strictly advance (wrapping-aware, so it
  correctly handles wraparound after ~4 billion events on a very
  long-lived connection). A non-advancing `seq` is rejected as a defense-
  in-depth measure against a bug or a malicious peer — a well-behaved peer
  on one TCP connection should never actually trigger this.
- `seq = 0` is reserved as "unsequenced" for message kinds that don't need
  ordering guarantees.

## Heartbeats & reconnection

- Each side sends `Heartbeat { seq, sent_at_ms }` on a configurable
  interval (`NetworkSettings::heartbeat_interval_ms`, default 1000ms),
  independent of the outbound message queue's load — implemented as its
  own task (`ms_core_service::heartbeat_loop`) so a burst of mouse-move
  traffic can never starve heartbeat delivery.
- `ms_core_service::HeartbeatMonitor` declares a connection dead after
  missing **three** consecutive expected heartbeats (`3 * interval` with
  no received heartbeat), long enough to absorb one dropped packet or
  scheduling hiccup, short enough that a genuinely dead peer is detected
  within a few seconds.
- On detected death (or any transport error), `ms-core-service` emits
  `Event::ConnectionLost` into the `EdgeStateMachine`, which — regardless
  of whether this device was controlling or being controlled — forces an
  immediate return to `Idle` and (if it was controlling) re-enables local
  pass-through. This is the same code path the emergency hotkey uses (see
  [security.md](security.md#emergency-release)), so "safe release of
  captured input if the connection fails" is not a special case, it's the
  same guarantee.
- Reconnection uses `ms_core_service::ReconnectBackoff`: starts at 500ms,
  doubles each failed attempt, caps at 30s, and resets to 500ms on the
  next successful connection. No jitter is added — with one or two peers
  per device rather than many clients hammering a server, the
  thundering-herd problem jitter defends against doesn't apply.

## Versioning

`Hello.protocol_version` is compared during the handshake
(`ms_core_service::handshake`); a mismatch causes both sides to reject the
connection with `HelloAck { accepted: false, reason: Some(...) }` before
any input-carrying message is ever sent. Because message framing already
gives every message a length prefix, a future protocol version can add
new `Message` variants without breaking length-based parsing for older
messages — only the enum's *meaning* needs the version check, not the
byte layout.
