#!/usr/bin/env bash
# Builds the macOS release binaries, assembles a .app bundle, and packages
# it into a .dmg. Run from anywhere; paths are resolved relative to this
# script. Requires Xcode Command Line Tools (for `hdiutil`, `codesign`)
# and a real macOS host — this cannot be run in a Linux sandbox, unlike
# the `cargo check --target x86_64-apple-darwin` cross-check used
# elsewhere in this repo for compile-time validation only.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
APP_NAME="Mouse Share"
BUNDLE_ID="com.mouseshare.app"
VERSION="0.1.0"
OUT_DIR="$SCRIPT_DIR/output"
STAGING="$SCRIPT_DIR/.staging"

echo "==> cargo build --release --workspace"
(cd "$REPO_ROOT" && cargo build --release --workspace)

if [ -d "$REPO_ROOT/app" ] && [ -d "$REPO_ROOT/app/node_modules" ]; then
  echo "==> Building the Tauri UI"
  (cd "$REPO_ROOT/app" && npm run tauri build)
fi

rm -rf "$STAGING"
BUNDLE="$STAGING/$APP_NAME.app"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"

cp "$SCRIPT_DIR/Info.plist" "$BUNDLE/Contents/Info.plist"

# The daemon binary always exists once cargo build succeeds. The UI binary
# is built by Tauri into app/src-tauri's OWN target directory (that crate
# is deliberately excluded from the root Cargo workspace — see
# Cargo.toml's [workspace] exclude and docs/build-instructions.md), not
# the shared $REPO_ROOT/target this daemon binary uses — pointing here at
# $REPO_ROOT/target/release/mouse-share instead (an earlier bug) always
# missed it and silently fell into the daemon-only fallback below,
# producing an app bundle with no visible UI at all when opened.
UI_BIN="$REPO_ROOT/app/src-tauri/target/release/mouse-share"
cp "$REPO_ROOT/target/release/mouse-share-daemon" "$BUNDLE/Contents/MacOS/mouse-share-daemon"
if [ -f "$UI_BIN" ]; then
  cp "$UI_BIN" "$BUNDLE/Contents/MacOS/mouse-share"
else
  echo "warning: $UI_BIN not found; bundling the daemon binary as the main executable instead (this bundle will have no visible UI)."
  cp "$REPO_ROOT/target/release/mouse-share-daemon" "$BUNDLE/Contents/MacOS/mouse-share"
fi
chmod +x "$BUNDLE/Contents/MacOS/"*

if [ -f "$SCRIPT_DIR/AppIcon.icns" ]; then
  cp "$SCRIPT_DIR/AppIcon.icns" "$BUNDLE/Contents/Resources/AppIcon.icns"
fi

# Ad-hoc signing (`-s -`) so Gatekeeper doesn't outright refuse to launch
# the bundle during local testing; a real release build should instead
# sign with a Developer ID certificate and notarize via `notarytool`,
# which requires Apple Developer credentials not available in CI here.
echo "==> Ad-hoc code signing (replace with Developer ID signing + notarization for distribution)"
codesign --force --deep --sign - "$BUNDLE"

mkdir -p "$OUT_DIR"
DMG_PATH="$OUT_DIR/MouseShare-$VERSION.dmg"
rm -f "$DMG_PATH"

echo "==> Creating $DMG_PATH"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGING" -ov -format UDZO "$DMG_PATH"

rm -rf "$STAGING"
echo "==> Done: $DMG_PATH"
