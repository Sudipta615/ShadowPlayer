#!/usr/bin/env bash
# Wrap a built ShadowPlayer binary in a minimal macOS .app bundle.
#
# cargo-dist produces the .dmg; this script is the bundle step around the
# binary that the release job can run before creating it. It is intentionally
# small and dependency-free: the only inputs are the built executable and the
# committed Info.plist/icon.icns.
#
# Usage:
#   crates/app/macos/package-app.sh [path-to-shadowplayer] [output-dir]
#
# Defaults match a release build from the repository root.
set -euo pipefail

binary="${1:-target/release/shadowplayer}"
out_dir="${2:-target/distrib}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
app="$out_dir/ShadowPlayer.app"
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$here/../Cargo.toml" | head -1)"
if [[ -z "$version" ]]; then
    version="0.0.0"
fi

if [[ ! -x "$binary" ]]; then
    echo "error: $binary is not an executable; build it first" >&2
    exit 1
fi

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/shadowplayer"
chmod 755 "$app/Contents/MacOS/shadowplayer"
cp "$here/Info.plist" "$app/Contents/Info.plist"
cp "$here/../assets/icon.icns" "$app/Contents/Resources/icon.icns"

# Keep the plist in step with the workspace version without requiring PlistBuddy
# (which exists only on macOS).
sed -i.bak "/<key>CFBundleShortVersionString<\\/key>/{n;s|<string>[^<]*</string>|<string>${version}</string>|;}" "$app/Contents/Info.plist"
rm -f "$app/Contents/Info.plist.bak"

# A bundle without NSMicrophoneUsageDescription is the point: the player must
# never ask for a microphone it does not use.
if grep -q "<key>NSMicrophoneUsageDescription</key>" "$app/Contents/Info.plist"; then
    echo "error: macOS bundle unexpectedly requests microphone access" >&2
    exit 1
fi

if ! grep -q "<key>LSUIElement</key>" "$app/Contents/Info.plist" \
    || ! grep -A1 "<key>LSUIElement</key>" "$app/Contents/Info.plist" | grep -q "<false/>"; then
    echo "error: macOS bundle must be a foreground app (LSUIElement = false)" >&2
    exit 1
fi

echo "Created $app (version $version)"
