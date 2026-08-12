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
- **Not yet wired**: the tray's "Stop Sharing" action, the drag-and-drop
  screen-layout canvas (the layout data model and its persistence are
  complete in `ms-config`; only the visual canvas editor is still a text
  summary — see `index.html`'s comment in the Screen Layout card), and
  actually starting/relaying a *live* shared-control session after
  pairing (that's `ms-daemon`'s `listen_port` listener, which today only
  runs in the separate headless binary — nothing yet launches
  `mouse-share-daemon` automatically alongside the UI). A production
  build should also replace the placeholder solid-color icons in
  `src-tauri/icons/` with real artwork.
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
