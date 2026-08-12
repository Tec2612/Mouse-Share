# Mouse Share UI

Tauri 2 desktop UI: Dashboard, Computer Setup (manual connect + real
trust-on-first-use pairing with SAS confirmation), Settings, tray/menu-bar
icon, and macOS Accessibility/Input Monitoring onboarding.

## Status

- **Backend (`src-tauri/`)**: real Rust commands backed by the same
  `ms-config`/`ms-security`/`ms-discovery` crates the headless daemon
  uses — reading/writing the same on-disk settings, screen layout, and
  trust store. Pairing (`start_pairing`/`confirm_pairing`) performs an
  actual TLS 1.3 handshake and computes the real short authentication
  string described in `docs/security.md`, not placeholder data.
- **Frontend (`src/`)**: plain HTML/CSS/JS (no framework/bundler — Tauri's
  `withGlobalTauri` config exposes `window.__TAURI__` directly), calling
  those commands.
- **Pairing** (`src-tauri/src/pairing.rs`): the app now runs both sides —
  it dials out (`start_pairing`) *and* listens for incoming pairing
  attempts on `NetworkSettings::pairing_port` (default 45678, distinct
  from the session port 45677 — see that setting's doc comment), showing
  an "Incoming pairing request" card with the same SAS-comparison flow.
  It also advertises itself over mDNS at that port so discovered-device
  pairing works, not just manual IP/hostname. Closing the main window
  hides it rather than quitting, so the pairing acceptor (and the tray)
  keep running in the background like a normal tray app.
- **Live sharing**: the UI now launches `mouse-share-daemon` (the
  headless capture/injection/session binary) as a background process on
  startup if it's present next to the UI executable (`spawn_daemon_if_present`
  in `lib.rs`) — previously nothing started it at all. The daemon also now
  actively dials every paired device it discovers over mDNS (a
  "mesh-connect" loop in `ms-daemon/src/main.rs`), rather than only ever
  accepting inbound connections, which is what actually turns "paired"
  into "reachable for control." A fix earlier in this same effort also
  aligned the UI's and daemon's config-directory resolution — they
  previously used two different directories and never saw each other's
  trust store or layout at all.
- **Screen layout**: pairing now auto-registers both devices as
  `ScreenLayout` nodes, and Computer Setup's Screen Layout card has a
  simple "my edge ↔ paired device" linking form (`link_layout_edge`/
  `unlink_layout_edge`) as an interim substitute for a drag-and-drop
  canvas, which isn't built yet. The layout data model and persistence
  are complete in `ms-config`; only the visual canvas editor is still a
  form instead of a canvas.
- **Not yet wired**: the tray's "Stop Sharing" action, the drag-and-drop
  canvas itself, and a proper daemon singleton check (spawning the UI
  twice currently spawns two daemon attempts; the second harmlessly fails
  to bind the session port and exits). A production build should also
  replace the placeholder solid-color icons in `src-tauri/icons/` with
  real artwork.
- **Build validation**: this repository's own sandbox has no system
  WebView toolkit available (Linux's webkit2gtk packages were unavailable
  from the configured package mirror during development — see
  `docs/build-instructions.md`), so this app could not be fully built or
  visually tested here. `npx tauri info` confirms the project/config
  itself is structurally valid; `.github/workflows/ci.yml`'s
  `build-installers` job does the real `npm ci && npm run tauri build` on
  `windows-latest`/`macos-latest`, both of which ship their native
  WebView toolkit (WebView2, WKWebView) with the OS.

## Development

```sh
npm install
npm run tauri dev
```

## Building

```sh
npm install
npm run tauri build
```

Or use the platform installer scripts in `../installers/`, which build
this app as part of producing the final installer/package.
