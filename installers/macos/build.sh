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

# The daemon binary always exists once cargo build succeeds; the UI binary
# (built by Tauri, named after app/src-tauri's package name) is copied in
# if present so this script also works before the UI is fully wired up —
# producing a daemon-only bundle rather than failing.
cp "$REPO_ROOT/target/release/mouse-share-daemon" "$BUNDLE/Contents/MacOS/mouse-share-daemon"
if [ -f "$REPO_ROOT/target/release/mouse-share" ]; then
  cp "$REPO_ROOT/target/release/mouse-share" "$BUNDLE/Contents/MacOS/mouse-share"
else
  echo "warning: target/release/mouse-share (Tauri UI binary) not found; bundling the daemon binary as the main executable instead."
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
