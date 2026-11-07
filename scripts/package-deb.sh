#!/usr/bin/env bash
# Package ShadowPlayer into a Debian (.deb) package.
# Usage: scripts/package-deb.sh [binary-path] [target-triple] [output-dir]
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${1:-$here/target/release/shadowplayer}"
target="${2:-$(uname -m)-unknown-linux-gnu}"
out_dir="${3:-$here/target/distrib}"

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$here/crates/app/Cargo.toml" | head -1)"
if [[ -z "$version" ]]; then
    version="0.30.0"
fi

if [[ ! -x "$binary" ]]; then
    echo "error: $binary is not an executable; build it first (e.g. cargo build --release -p shadowplayer)" >&2
    exit 1
fi

case "$target" in
    x86_64*) arch="amd64" ;;
    aarch64*) arch="arm64" ;;
    armv7*) arch="armhf" ;;
    *) arch="all" ;;
esac

deb_name="shadowplayer_${version}_${arch}"
pkg_dir="$out_dir/$deb_name"

echo "Creating Debian package for $deb_name..."
rm -rf "$pkg_dir"
mkdir -p "$pkg_dir/DEBIAN" \
         "$pkg_dir/usr/bin" \
         "$pkg_dir/usr/share/applications" \
         "$pkg_dir/usr/share/metainfo" \
         "$pkg_dir/usr/share/doc/shadowplayer"

# 1. Install binary
cp "$binary" "$pkg_dir/usr/bin/shadowplayer"
chmod 755 "$pkg_dir/usr/bin/shadowplayer"

# 2. Desktop file and metainfo
cp "$here/crates/app/assets/shadowplayer.desktop" "$pkg_dir/usr/share/applications/"
cp "$here/crates/app/assets/io.github.shadowplayer.metainfo.xml" "$pkg_dir/usr/share/metainfo/"

# 3. Icons
for size in 16 24 32 48 64 128 256 512 1024; do
    icon_src="$here/crates/app/assets/icon-${size}.png"
    if [[ -f "$icon_src" ]]; then
        icon_dest="$pkg_dir/usr/share/icons/hicolor/${size}x${size}/apps"
        mkdir -p "$icon_dest"
        cp "$icon_src" "$icon_dest/shadowplayer.png"
    fi
done
mkdir -p "$pkg_dir/usr/share/icons/hicolor/scalable/apps"
cp "$here/crates/app/assets/icon.svg" "$pkg_dir/usr/share/icons/hicolor/scalable/apps/shadowplayer.svg"

# 4. Docs
cp "$here/README.md" "$pkg_dir/usr/share/doc/shadowplayer/"
cp "$here/LICENSE" "$pkg_dir/usr/share/doc/shadowplayer/copyright"

# 5. DEBIAN/control
cat > "$pkg_dir/DEBIAN/control" << EOF
Package: shadowplayer
Version: ${version}
Section: sound
Priority: optional
Architecture: ${arch}
Maintainer: ShadowPlayer Developers <https://github.com/shadowplayer/shadowplayer>
Description: Audiophile playback for the music you already own.
 A local-first desktop music player built in Rust: bit-perfect output,
 studio-grade DSP chain, 100,000-track library that stays fast, and
 zero accounts, cloud dependencies, or telemetry.
EOF

# 6. Post-install and post-removal triggers
cat > "$pkg_dir/DEBIAN/postinst" << 'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor || true
fi
EOF
chmod 755 "$pkg_dir/DEBIAN/postinst"

cat > "$pkg_dir/DEBIAN/postrm" << 'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor || true
fi
EOF
chmod 755 "$pkg_dir/DEBIAN/postrm"

# 7. Build package
mkdir -p "$out_dir"
dpkg-deb --build "$pkg_dir" "$out_dir/${deb_name}.deb"
rm -rf "$pkg_dir"

echo "Created $out_dir/${deb_name}.deb"

