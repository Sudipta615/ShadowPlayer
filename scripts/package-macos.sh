#!/usr/bin/env bash
# Package ShadowPlayer for macOS into .app bundle, .dmg, and .zip.
# Usage: scripts/package-macos.sh [binary-path] [target-triple] [output-dir]
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${1:-$here/target/release/shadowplayer}"
target="${2:-$(uname -m)-apple-darwin}"
out_dir="${3:-$here/target/distrib}"

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$here/crates/app/Cargo.toml" | head -1)"
if [[ -z "$version" ]]; then
    version="0.30.0"
fi

echo "Packaging macOS bundle for target $target..."
mkdir -p "$out_dir"

# 1. Create .app bundle
"$here/crates/app/macos/package-app.sh" "$binary" "$out_dir"

app_path="$out_dir/ShadowPlayer.app"
base_name="shadowplayer-${version}-${target}"

# 2. Create .zip containing .app
(cd "$out_dir" && zip -r "${base_name}.zip" "ShadowPlayer.app")
echo "Created $out_dir/${base_name}.zip"

# 3. Create .dmg if hdiutil is available (macOS runner)
if command -v hdiutil >/dev/null 2>&1; then
    dmg_temp="$out_dir/dmg_temp"
    rm -rf "$dmg_temp"
    mkdir -p "$dmg_temp"
    cp -R "$app_path" "$dmg_temp/"
    ln -s /Applications "$dmg_temp/Applications"
    
    dmg_path="$out_dir/${base_name}.dmg"
    rm -f "$dmg_path"
    hdiutil create -volname "ShadowPlayer" \
                   -srcfolder "$dmg_temp" \
                   -ov -format UDZO \
                   "$dmg_path"
    rm -rf "$dmg_temp"
    echo "Created $dmg_path"
else
    echo "Notice: hdiutil not available (not running on macOS). DMG creation skipped."
fi

