#!/usr/bin/env bash
# Package ShadowPlayer into a portable Linux tarball with install/uninstall scripts.
# Usage: scripts/package-tarball.sh [binary-path] [target-triple] [output-dir]
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

archive_name="shadowplayer-${version}-${target}"
stage="$out_dir/$archive_name"

echo "Creating portable tarball for $archive_name..."
rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/share/applications" "$stage/share/metainfo"

# 1. Install binary
cp "$binary" "$stage/bin/shadowplayer"
chmod 755 "$stage/bin/shadowplayer"

# 2. Desktop file and metainfo
cp "$here/crates/app/assets/shadowplayer.desktop" "$stage/share/applications/"
cp "$here/crates/app/assets/io.github.shadowplayer.metainfo.xml" "$stage/share/metainfo/"

# 3. Icons
for size in 16 24 32 48 64 128 256 512 1024; do
    icon_src="$here/crates/app/assets/icon-${size}.png"
    if [[ -f "$icon_src" ]]; then
        icon_dest="$stage/share/icons/hicolor/${size}x${size}/apps"
        mkdir -p "$icon_dest"
        cp "$icon_src" "$icon_dest/shadowplayer.png"
    fi
done
mkdir -p "$stage/share/icons/hicolor/scalable/apps"
cp "$here/crates/app/assets/icon.svg" "$stage/share/icons/hicolor/scalable/apps/shadowplayer.svg"

# 4. Docs and licenses
cp "$here/README.md" "$stage/"
cp "$here/LICENSE" "$stage/"

# 5. install.sh
cat > "$stage/install.sh" << 'EOF'
#!/usr/bin/env bash
# Install ShadowPlayer to system or user directory
set -euo pipefail

prefix="${PREFIX:-}"
if [[ -z "$prefix" ]]; then
    if [[ $EUID -eq 0 ]]; then
        prefix="/usr/local"
    else
        prefix="$HOME/.local"
    fi
fi

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Installing ShadowPlayer to $prefix..."
mkdir -p "$prefix/bin" "$prefix/share/applications" "$prefix/share/metainfo" "$prefix/share/icons"

cp "$here/bin/shadowplayer" "$prefix/bin/"
chmod 755 "$prefix/bin/shadowplayer"

cp "$here/share/applications/shadowplayer.desktop" "$prefix/share/applications/"
cp "$here/share/metainfo/io.github.shadowplayer.metainfo.xml" "$prefix/share/metainfo/"
cp -r "$here/share/icons/hicolor" "$prefix/share/icons/"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$prefix/share/applications" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$prefix/share/icons/hicolor" || true
fi

echo "Installation complete! Run 'shadowplayer' to start."
EOF
chmod 755 "$stage/install.sh"

# 6. uninstall.sh
cat > "$stage/uninstall.sh" << 'EOF'
#!/usr/bin/env bash
# Uninstall ShadowPlayer
set -euo pipefail

prefix="${PREFIX:-}"
if [[ -z "$prefix" ]]; then
    if [[ $EUID -eq 0 ]]; then
        prefix="/usr/local"
    else
        prefix="$HOME/.local"
    fi
fi

echo "Uninstalling ShadowPlayer from $prefix..."
rm -f "$prefix/bin/shadowplayer"
rm -f "$prefix/share/applications/shadowplayer.desktop"
rm -f "$prefix/share/metainfo/io.github.shadowplayer.metainfo.xml"
rm -f "$prefix/share/icons/hicolor/"*"/apps/shadowplayer."*

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$prefix/share/applications" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$prefix/share/icons/hicolor" || true
fi

echo "Uninstall complete."
EOF
chmod 755 "$stage/uninstall.sh"

# 7. Create tarball
tar -czf "$out_dir/${archive_name}.tar.gz" -C "$out_dir" "$archive_name"
rm -rf "$stage"

echo "Created $out_dir/${archive_name}.tar.gz"

