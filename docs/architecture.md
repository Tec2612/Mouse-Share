# Architecture

## Layers

Mouse Share is organized as eight layers, each isolated behind a trait or
a well-defined message boundary so platform-specific code never leaks into
shared logic:

```
┌─────────────────────────────────────────────────────────────┐
│ 1. UI layer                          app/ (Tauri: Rust+web)  │
├─────────────────────────────────────────────────────────────┤
│ 2. Application/service layer          ms-daemon               │
├─────────────────────────────────────────────────────────────┤
│ 3. Input capture layer   ms-input-core traits                 │
│      ├── ms-input-windows (WH_MOUSE_LL/WH_KEYBOARD_LL + Raw Input) │
│      └── ms-input-macos   (CGEventTap)                        │
├─────────────────────────────────────────────────────────────┤
│ 4. Input injection layer  ms-input-core traits                │
│      ├── ms-input-windows (SendInput)                         │
│      └── ms-input-macos   (CGEventPost)                       │
├─────────────────────────────────────────────────────────────┤
│ 5. Network layer                      ms-protocol, ms-core-service │
├─────────────────────────────────────────────────────────────┤
│ 6. Discovery layer                    ms-discovery (mDNS)     │
├─────────────────────────────────────────────────────────────┤
│ 7. Authentication/security layer      ms-security (mTLS, pairing) │
├─────────────────────────────────────────────────────────────┤
│ 8. Configuration/storage layer        ms-config               │
└─────────────────────────────────────────────────────────────┘
```

Orchestration (`ms-core-service::CoreService`) sits between layers 2–5: it
owns an `EdgeStateMachine` (layer-agnostic decision logic, in
`ms-input-core`) and dispatches its `Action`s to whichever concrete
collaborator layer 3/4/5 provides, via three narrow traits
(`InputInjector`, `PassThroughControl`, `NetworkSender`). Nothing above
layer 2 knows whether it's talking to Windows or macOS on either end of a
connection — that's the entire point of the split.

## Crate map and why each one is separate

| Crate | Owns | Depends on |
|---|---|---|
| `ms-protocol` | Wire message types, length-prefixed framing, mouse-move coalescing | — |
| `ms-keymap` | Windows VK / macOS `CGKeyCode` ↔ `LogicalKey`, cross-OS modifier remapping | `ms-protocol` |
| `ms-security` | Device identity (self-signed cert), pinned mTLS configs, TOFU pairing + SAS, trust store | — |
| `ms-discovery` | mDNS advertise/browse, manual IP/hostname parsing | `ms-protocol` (for `OperatingSystem`) |
| `ms-config` | Settings, multi-computer screen-layout graph, secure-storage abstraction | `ms-keymap`, `ms-protocol` |
| `ms-input-core` | `InputCapture`/`InputInjector` traits, `EdgeStateMachine`, `ClipboardGateway` | `ms-protocol`, `ms-config` |
| `ms-input-windows` | Real Win32 capture/injection implementing the above traits | `ms-input-core`, `ms-keymap` |
| `ms-input-macos` | Real CGEvent capture/injection implementing the above traits | `ms-input-core`, `ms-keymap` |
| `ms-core-service` | Reconnection backoff, heartbeat liveness, replay/dedup guard, session handshake, `CoreService` composition root | `ms-protocol`, `ms-security`, `ms-discovery`, `ms-config`, `ms-input-core` |
| `ms-daemon` | The actual background-service binary: loads config/identity, starts capture, owns the tokio runtime, exposes a local control API for the UI | everything above |
| `app` (Tauri) | Dashboard, computer setup, settings, tray/menu-bar, onboarding | talks to `ms-daemon` over its local control API |

Each platform-specific crate (`ms-input-windows`, `ms-input-macos`)
compiles to an *empty* library on any other target — see the `#![cfg(...)]`
at the top of each crate's `lib.rs` — so `cargo build --workspace` /
`cargo test --workspace` succeed on any host, including this repo's Linux
CI, while the platform code itself is validated by (a) `cargo check
--target <platform-triple>` against the real `windows`/`core-graphics`
crates on every host, and (b) a full `cargo test` on `windows-latest` /
`macos-latest` in CI (see [build-instructions.md](build-instructions.md)).

## Why this decomposition (and not fewer, bigger crates)

- **`ms-protocol` has zero platform dependencies and zero async runtime
  opinions beyond `tokio::io` traits.** It's the one piece every other
  component — including a future non-Rust reimplementation of one side —
  needs to agree on, so it's kept minimal and stable.
- **`ms-input-core` defines the state machine and traits, never an OS
  API.** This is what makes `EdgeStateMachine` unit-testable with plain
  mock structs instead of a Windows or macOS box — see its test module for
  16 scenarios (edge crossing, emergency hotkey, connection loss, replay
  protection, multi-peer hijack attempts) that all run in this sandbox.
- **`ms-security` has no knowledge of the app's message types.** Pairing
  and pinned mTLS are a generic "two devices establish and maintain
  mutual trust" concern; keeping it protocol-agnostic means the trust
  model can be audited (and tested against a real TLS handshake over
  loopback — see `crates/ms-security/tests/`) independent of anything
  input-related.
- **`ms-core-service` is the only crate that knows about *all* the
  others.** That's deliberate: it's the composition root. Everything it
  depends on could in principle be swapped (a different transport, a
  different platform backend) without it needing to change shape, because
  it only ever talks to trait objects.

## Data flow: one edge crossing, end to end

```
Windows PC (controlling)                    Mac (being controlled)
─────────────────────────                   ──────────────────────
WH_MOUSE_LL hook sees cursor
at x=1919 (screen edge)
        │
        ▼
CaptureSink::on_cursor_at_edge(Right, 0.4)
        │
        ▼
EdgeStateMachine: Idle ─────────────────►  Controlling{mac}
        │  Action::DisableLocalPassThrough
        │  Action::SendMessage(mac, EdgeEnter{Left, 0.4})
        ▼
CoreService.pass_through.set(false)   ──TLS──►  read_loop receives EdgeEnter
CoreService.network.send(...)                          │
                                                          ▼
                                          EdgeStateMachine: Idle ──► BeingControlled{pc}
                                                Action::WarpLocalCursor{x:0, y:0.4*height}
                                                          │
                                                          ▼
                                          CGWarpMouseCursorPosition(...)

Raw Input reports dx/dy deltas
        │
        ▼
EdgeStateMachine (Controlling) forwards
Action::SendMessage(mac, MouseMove{dx,dy})  ──TLS──►  read_loop receives MouseMove
                                                          │
                                                Action::InjectLocally(MouseMove)
                                                          ▼
                                                CGEventPost(mouseMoved, dx, dy)

... mac's own CGEventTap now sees ITS cursor approaching ITS left edge ...
                                          EdgeStateMachine: on_local_cursor_at_edge
                                          resolves back to the pc via the layout's
                                          bidirectional link ──► Idle
                                          Action::SendMessage(pc, EdgeRelease)
        ◄──────────────────────────────────────────────────────────
EdgeStateMachine: Controlling ─► Idle
Action::EnableLocalPassThrough
```

Every step above except the two OS API calls (`CGWarpMouseCursorPosition`,
`CGEventPost`) is exercised by an automated test today: the state
transitions in `ms-input-core`, the bidirectional layout resolution in
`ms-config`, and the message round-trip in `ms-core-service`/`ms-security`.

## Technology stack, and why

| Choice | Rationale |
|---|---|
| **Rust** for the daemon and all platform code | Memory safety for code that runs elevated/with Accessibility access and processes untrusted network input; one language for both native backends via `windows-rs` and `core-graphics`; small, dependency-light binaries; `cargo test` gives real, fast, cross-platform-by-default testing for everything that isn't OS-API-bound. |
| **A custom binary protocol over one mTLS TCP stream**, not UDP or an existing RPC framework | LAN-only, low-peer-count use case where TCP's reliability/ordering is exactly what's wanted for keyboard events (a dropped keystroke is much worse than a dropped mouse-move sample), and mixing TCP-for-reliable + UDP-for-fast adds a second transport's worth of NAT/firewall/security surface for a LAN app that doesn't need it. Packet-rate concerns from TCP's stream model are addressed explicitly by `MouseMoveCoalescer` rather than by switching transports. See [protocol-spec.md](protocol-spec.md). |
| **rustls (TLS 1.3) with a custom pinned certificate verifier**, not a CA-based PKI | There is no natural CA for a consumer LAN app, and users shouldn't need one. Pinning each paired device's certificate fingerprint (the SSH host-key model) gives the same "prevent unauthorized LAN devices" guarantee without any external trust anchor. See [security.md](security.md). |
| **mDNS (`mdns-sd` crate) for discovery**, with manual IP/hostname as a fallback | Zero-configuration on the common case (both devices on the same LAN segment with multicast enabled); the fallback exists because client-isolated Wi-Fi and some VPNs block multicast entirely, which is common enough on consumer/office networks to require day-one support, not a "nice to have." |
| **Tauri** for the UI (Rust backend + system webview), not Electron | The app already has a substantial native Rust backend (`ms-daemon`); Tauri reuses that process model with a systemwebview instead of bundling a second Chromium, keeping the idle-memory footprint low, which matters for a background utility users expect to forget is running. |
| **`windows-rs`** (Windows) / **`core-graphics` + `core-foundation`** (macOS) | Official/canonical low-level bindings for each platform rather than a cross-platform input-simulation library — the task explicitly calls for real OS-level capture/injection, not a higher-level automation API, and both crates expose the exact `SetWindowsHookEx`/`SendInput`/`CGEventTap`/`CGEventPost` surface needed. |
| **`keyring` crate** (Credential Manager / Keychain) for the device private key | The one piece of local state that must not be plain-readable: everything else (settings, layout, trust-store fingerprints) is non-secret and lives in a plain TOML/JSON file (see [security.md](security.md) for why fingerprints don't need secrecy). |

## Phased build order

The implementation followed the phases below; each phase left the
workspace in a state where `cargo test --workspace` passes:

1. **Wire protocol + coalescing** (`ms-protocol`) — transport-agnostic, so
   it could be built and fully tested before any networking existed.
2. **Cross-platform key mapping** (`ms-keymap`) — Windows↔Windows,
   macOS↔macOS, and Windows↔macOS translation all go through the same
   `LogicalKey` table, so "Phase 3: Windows↔macOS" for keyboard handling
   was really phase 2, not a separate later effort.
3. **Security** (`ms-security`) — device identity, pinned mTLS, and the
   pairing/SAS handshake, built and tested (including a real loopback TLS
   handshake) before any input code existed, since no input event is ever
   allowed to cross the network unencrypted or unauthenticated.
4. **Discovery** (`ms-discovery`) and **configuration** (`ms-config`) —
   LAN device discovery and the multi-computer screen-layout graph.
5. **Input core + both native backends** (`ms-input-core`,
   `ms-input-windows`, `ms-input-macos`) — the edge-crossing state
   machine first (pure, mock-tested), then both real platform
   implementations against it.
6. **Orchestration** (`ms-core-service`) — reconnection, heartbeats,
   replay protection, and the session handshake, tying every earlier
   layer together behind `CoreService`.
7. **Daemon, UI, installers, CI** (`ms-daemon`, `app`,
   `.github/workflows`, `installers/`) — production packaging around the
   now-complete and tested core.
