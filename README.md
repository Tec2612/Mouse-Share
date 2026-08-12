# Mouse Share

Share one physical mouse and keyboard across multiple Windows and macOS
computers on the same network — move the pointer to the edge of one
screen and it continues onto the next, keyboard input follows, and it
never leaves your LAN unencrypted.

Mouse Share is not a UI wrapped around simulated input: every mouse/keyboard
event is captured and injected through the real, low-level OS APIs
(`SetWindowsHookEx`/`SendInput`/Raw Input on Windows, `CGEventTap`/
`CGEventPost` on macOS), and every connection is mutually authenticated,
pinned, encrypted TLS.

## Status

The cross-platform core — wire protocol, device pairing/security, LAN
discovery, configuration, the edge-crossing state machine, and both native
input backends — is implemented and covered by 100+ automated tests (see
[docs/e2e-test-plan.md](docs/e2e-test-plan.md) for what still needs real
Windows/macOS hardware to validate end-to-end). The desktop UI and
installers are scaffolded; see each doc below for exact status.

## Documentation

| Doc | Covers |
|---|---|
| [docs/architecture.md](docs/architecture.md) | Layered design, crate map, tech stack rationale, data flow |
| [docs/protocol-spec.md](docs/protocol-spec.md) | Wire protocol: framing, message types, coalescing, dedup |
| [docs/security.md](docs/security.md) | Device identity, pairing (SAS), pinned mTLS, revocation, threat model |
| [docs/build-instructions.md](docs/build-instructions.md) | Building/testing on Windows, macOS, and this repo's Linux CI |
| [docs/user-guide.md](docs/user-guide.md) | End-user setup: pairing, screen layout, settings, emergency release |
| [docs/e2e-test-plan.md](docs/e2e-test-plan.md) | Manual/hardware-dependent test plan for real Windows + Mac machines |

## Repository layout

```
crates/
  ms-protocol/        wire protocol, framing, mouse-move coalescing
  ms-keymap/           Windows/macOS key-code <-> LogicalKey translation
  ms-security/          device identity, pinned mTLS, TOFU pairing + SAS
  ms-discovery/          mDNS advertise/browse, manual IP/hostname connect
  ms-config/              settings, screen layout graph, secure storage
  ms-input-core/           capture/injector traits, edge state machine, clipboard gate
  ms-input-windows/         Win32 capture (hooks + Raw Input) and injection (SendInput)
  ms-input-macos/            macOS capture/injection (CGEventTap / CGEventPost)
  ms-core-service/            reconnection, heartbeats, dedup, session handling
  ms-daemon/                   the background service binary (ties everything together)
app/                            Tauri desktop UI (dashboard, setup, settings, tray)
installers/                      Windows (Inno Setup) and macOS packaging
.github/workflows/                CI: build + test on windows-latest/macos-latest/ubuntu-latest
```

## Building

**Don't have Rust/Node/build tools installed?** Run the automated setup
script for your platform from the repo root — it detects and installs
everything needed, then builds the app:

- **Windows:** double-click `scripts\setup-windows.bat`
- **macOS:** double-click `scripts/setup-macos.command` in Finder

See [docs/build-instructions.md](docs/build-instructions.md) for details
and manual setup. Quick start
for the cross-platform core (works on any OS, including this repo's CI):

```sh
cargo test --workspace
```

Platform-specific crates (`ms-input-windows`, `ms-input-macos`) compile to
an empty crate off their target OS by design (see each crate's doc
comment) and are validated in this environment via:

```sh
cargo check -p ms-input-windows --target x86_64-pc-windows-gnu
cargo check -p ms-input-macos --target x86_64-apple-darwin
```
