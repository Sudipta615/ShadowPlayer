#!/usr/bin/env bash
# Package ShadowPlayer into a Linux AppImage.
# Usage: scripts/package-appimage.sh [binary-path] [target-triple] [output-dir]
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${1:-$here/target/release/shadowplayer}"
target="${2:-$(uname -m)-unknown-linux-gnu}"
out_dir="${3:-$here/target/distrib}"

remote_binary="${REMOTE_BINARY:-$(dirname "$binary")/shadowplayer-remote}"

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$here/crates/app/Cargo.toml" | head -1)"
if [[ -z "$version" ]]; then
    version="0.30.0"
fi

if [[ ! -x "$binary" ]]; then
    echo "error: $binary is not an executable; build it first (e.g. cargo build --release -p shadowplayer)" >&2
    exit 1
fi

case "$target" in
    x86_64*) arch="x86_64" ;;
    aarch64*) arch="aarch64" ;;
    *) arch="$(uname -m)" ;;
esac

appdir="$out_dir/ShadowPlayer.AppDir"
appimage="$out_dir/shadowplayer-${version}-${arch}.AppImage"

echo "Creating AppDir for ShadowPlayer..."
rm -rf "$appdir"
mkdir -p "$appdir/usr/bin" \
         "$appdir/usr/share/applications" \
         "$appdir/usr/share/metainfo" \
         "$appdir/usr/share/icons/hicolor/256x256/apps" \
         "$appdir/usr/share/icons/hicolor/scalable/apps"

# 1. Install binaries
cp "$binary" "$appdir/usr/bin/shadowplayer"
chmod 755 "$appdir/usr/bin/shadowplayer"
if [[ -x "$remote_binary" ]]; then
    cp "$remote_binary" "$appdir/usr/bin/shadowplayer-remote"
    chmod 755 "$appdir/usr/bin/shadowplayer-remote"
fi

# 2. Desktop file and metainfo
cp "$here/crates/app/assets/shadowplayer.desktop" "$appdir/usr/share/applications/"
cp "$here/crates/app/assets/shadowplayer.desktop" "$appdir/"
cp "$here/crates/app/assets/io.github.shadowplayer.metainfo.xml" "$appdir/usr/share/metainfo/"

# 3. Icons
cp "$here/crates/app/assets/icon-256.png" "$appdir/usr/share/icons/hicolor/256x256/apps/shadowplayer.png"
cp "$here/crates/app/assets/icon-256.png" "$appdir/shadowplayer.png"
cp "$here/crates/app/assets/icon-256.png" "$appdir/.DirIcon"
cp "$here/crates/app/assets/icon.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/shadowplayer.svg"

# 4. AppRun
cat > "$appdir/AppRun" << 'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "${0}")")"
export PATH="${HERE}/usr/bin:${PATH}"
export LD_LIBRARY_PATH="${HERE}/usr/lib:${HERE}/usr/lib64:${LD_LIBRARY_PATH:-}"
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
exec "${HERE}/usr/bin/shadowplayer" "$@"
EOF
chmod 755 "$appdir/AppRun"

# 5. Pack into AppImage if appimagetool is available
appimagetool_bin="${APPIMAGETOOL:-appimagetool}"
if command -v "$appimagetool_bin" >/dev/null 2>&1; then
    APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$arch" "$appimagetool_bin" "$appdir" "$appimage"
    echo "Created $appimage"
    rm -rf "$appdir"
else
    echo "Notice: appimagetool not found in PATH."
    echo "AppDir prepared successfully at: $appdir"
    echo "To produce .AppImage, run: ARCH=$arch appimagetool $appdir $appimage"
fi

