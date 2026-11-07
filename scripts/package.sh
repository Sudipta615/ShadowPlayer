#!/usr/bin/env bash
# Unified packaging CLI for ShadowPlayer.
# Usage:
#   scripts/package.sh [format] [binary-path] [target] [output-dir]
#
# Formats:
#   all       - Build all packages appropriate for the current OS
#   tarball   - Portable Linux tar.gz with install.sh
#   deb       - Debian .deb package
#   appimage  - Linux AppImage
#   macos     - macOS .app, .zip, and .dmg
#   windows   - Windows portable .zip and installer
set -euo pipefail

format="${1:-all}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
os="$(uname -s | tr '[:upper:]' '[:lower:]')"
machine="$(uname -m)"

binary="${2:-}"
out_dir="${4:-$here/target/distrib}"

case "$os" in
    linux*)
        target="${3:-${machine}-unknown-linux-gnu}"
        binary="${binary:-$here/target/release/shadowplayer}"
        ;;
    darwin*)
        target="${3:-${machine}-apple-darwin}"
        binary="${binary:-$here/target/release/shadowplayer}"
        ;;
    msys*|mingw*|cygwin*)
        target="${3:-x86_64-pc-windows-msvc}"
        binary="${binary:-$here/target/release/shadowplayer.exe}"
        ;;
    *)
        target="${3:-${machine}-unknown-${os}}"
        binary="${binary:-$here/target/release/shadowplayer}"
        ;;
esac

mkdir -p "$out_dir"

echo "=== ShadowPlayer Packaging Pipeline ==="
echo "Format:  $format"
echo "Binary:  $binary"
echo "Target:  $target"
echo "Output:  $out_dir"
echo "======================================="

case "$format" in
    tarball)
        "$here/scripts/package-tarball.sh" "$binary" "$target" "$out_dir"
        ;;
    deb)
        "$here/scripts/package-deb.sh" "$binary" "$target" "$out_dir"
        ;;
    appimage)
        "$here/scripts/package-appimage.sh" "$binary" "$target" "$out_dir"
        ;;
    macos)
        "$here/scripts/package-macos.sh" "$binary" "$target" "$out_dir"
        ;;
    windows)
        "$here/scripts/package-windows.sh" "$binary" "$target" "$out_dir"
        ;;
    all)
        case "$os" in
            linux*)
                "$here/scripts/package-tarball.sh" "$binary" "$target" "$out_dir"
                if command -v dpkg-deb >/dev/null 2>&1; then
                    "$here/scripts/package-deb.sh" "$binary" "$target" "$out_dir"
                fi
                "$here/scripts/package-appimage.sh" "$binary" "$target" "$out_dir"
                ;;
            darwin*)
                "$here/scripts/package-macos.sh" "$binary" "$target" "$out_dir"
                ;;
            msys*|mingw*|cygwin*)
                "$here/scripts/package-windows.sh" "$binary" "$target" "$out_dir"
                ;;
            *)
                echo "Unknown OS $os, running portable tarball packager..."
                "$here/scripts/package-tarball.sh" "$binary" "$target" "$out_dir"
                ;;
        esac
        ;;
    *)
        echo "error: unknown packaging format '$format'. Valid formats: all, tarball, deb, appimage, macos, windows" >&2
        exit 1
        ;;
esac

echo "Packaging complete! Artefacts located in $out_dir:"
ls -la "$out_dir"

