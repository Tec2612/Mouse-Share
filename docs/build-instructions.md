# Build Instructions

## Easiest path: automated setup scripts

If you don't already have Rust/Node/build tools installed, skip the
manual prerequisite steps below and just run the bootstrap script for
your platform from the repo root. It detects everything missing, installs
it (Rust via rustup, Node via winget/Homebrew, the MSVC C++ build tools on
Windows, Xcode Command Line Tools on macOS), then builds the daemon and
the desktop app. It's safe to re-run if it fails partway — every step
skips itself if already satisfied.

**Windows:** double-click `scripts\setup-windows.bat` (or run
`powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1`).

**macOS:** double-click `scripts/setup-macos.command` in Finder (or run
`bash scripts/setup-macos.sh`). Xcode Command Line Tools installation pops
a system dialog partway through — click "Install" there when it appears
and the script will wait for it to finish before continuing.

When it finishes, it prints the path to the built installer/app bundle.

## Prerequisites (manual setup)

- Rust (stable channel; developed against 1.94) via [rustup](https://rustup.rs).
- For the UI (`app/`): [Node.js](https://nodejs.org) 18+ and the [Tauri
  CLI](https://tauri.app) prerequisites for your platform.
- Windows builds: Visual Studio Build Tools (MSVC) or the `x86_64-pc-windows-gnu`
  target with `mingw-w64` if cross-compiling.
- macOS builds: Xcode Command Line Tools (`xcode-select --install`).

## Cross-platform core (any OS)

```sh
cargo test --workspace
```

This builds and tests every crate except the two platform-specific input
backends, which compile to an intentionally empty library off their
target OS (see [architecture.md](architecture.md)). On a machine with
neither Windows nor macOS available, you can still type-check those two
crates against the real platform API surface without needing that OS's
SDK, since `cargo check` doesn't perform the final link step where
framework/system-library linking would be required:

```sh
rustup target add x86_64-pc-windows-gnu
cargo check -p ms-input-windows --target x86_64-pc-windows-gnu

rustup target add x86_64-apple-darwin
cargo check -p ms-input-macos --target x86_64-apple-darwin
```

(This is exactly how those two crates were validated during development
in a Linux-only sandbox with no Windows or macOS host available — see each
crate's `Cargo.toml`/`lib.rs` doc comment.) A full `cargo build`/`cargo
test` of these two crates still requires the real OS and toolchain, which
is why CI (see below) runs them on `windows-latest` and `macos-latest`.

## Building on Windows

```powershell
cargo build --release --workspace
cargo test --workspace
```

`ms-input-windows` requires no extra SDK beyond what `rustup`'s
`stable-x86_64-pc-windows-msvc` toolchain already brings (the `windows`
crate ships pre-generated Win32 metadata, not headers). Running the input
capture/injection tests for real requires an interactive desktop session
(they install a low-level system hook and read the current cursor
position); running under a headless CI agent without a desktop session
will fail those specific tests, not the rest of the suite.

## Building on macOS

```sh
cargo build --release --workspace
cargo test --workspace
```

`ms-input-macos`'s capture path requires the process to have Accessibility
access (`System Settings → Privacy & Security → Accessibility`) before
`MacOsInputCapture::start` will succeed — see
`ms_input_macos::accessibility_trusted()`. Under CI, grant this to the CI
runner's terminal/agent process once, or run capture-dependent tests with
`--skip` if no interactive session with that permission is available;
injection-only tests (`CGEventPost`) and all pure-logic tests run
regardless.

## Building the UI (`app/`)

```sh
cd app
npm install
npm run tauri dev    # local development, hot reload
npm run tauri build  # produces a platform installer/bundle
```

The Tauri backend links against `ms-daemon` as a library and starts the
same background service the headless binary does, plus the webview
window, tray icon, and IPC commands the frontend calls.

## Running the headless daemon directly

```sh
cargo run -p ms-daemon --release
```

Useful for development/debugging without the UI, and for running Mouse
Share on a machine that only ever acts as a controlled target (no
interactive session needed once paired, aside from the initial
Accessibility/Input Monitoring grant on macOS).

## CI

`.github/workflows/ci.yml` runs, on every push and pull request:

| Job | Runner | What it validates |
|---|---|---|
| `test-linux` | `ubuntu-latest` | `cargo test --workspace` (the entire cross-platform core), plus the two `cargo check --target ...` cross-checks above |
| `test-windows` | `windows-latest` | `cargo test --workspace`, including `ms-input-windows` for real |
| `test-macos` | `macos-latest` | `cargo test --workspace`, including `ms-input-macos` for real |
| `build-installers` | matrix (windows-latest, macos-latest) | Produces the Inno Setup installer and the macOS `.app`/`.dmg` (see [installers/](../installers)) as build artifacts, gated on the corresponding test job passing |

This is the mechanism that closes the gap left by this project having
been developed without direct access to Windows or macOS hardware: every
push is actually built and tested on both real target platforms, not just
cross-checked.

## Workspace layout for `cargo`

The repo is a single Cargo workspace (`Cargo.toml` at the root); every
crate under `crates/` is a member, so `cargo build/test/check -p <crate>`
and `--workspace` both work from the repo root without needing to `cd`
into a crate directory.
