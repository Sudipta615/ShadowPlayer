#!/usr/bin/env bash
# Package ShadowPlayer for Windows into portable zip and setup installer.
# Usage: scripts/package-windows.sh [binary-path] [target-triple] [output-dir]
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${1:-$here/target/release/shadowplayer.exe}"
target="${2:-x86_64-pc-windows-msvc}"
out_dir="${3:-$here/target/distrib}"

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$here/crates/app/Cargo.toml" | head -1)"
if [[ -z "$version" ]]; then
    version="0.30.0"
fi

echo "Packaging Windows distribution for target $target..."
mkdir -p "$out_dir"

base_name="shadowplayer-${version}-${target}"
stage="$out_dir/$base_name"

rm -rf "$stage"
mkdir -p "$stage"

# 1. Install binary
cp "$binary" "$stage/shadowplayer.exe"
cp "$here/crates/app/assets/icon.ico" "$stage/"
cp "$here/README.md" "$stage/"
cp "$here/LICENSE" "$stage/"

# 2. Create portable zip
if command -v zip >/dev/null 2>&1; then
    (cd "$out_dir" && zip -r "${base_name}.zip" "$base_name")
    echo "Created $out_dir/${base_name}.zip"
elif command -v 7z >/dev/null 2>&1; then
    (cd "$out_dir" && 7z a "${base_name}.zip" "$base_name")
    echo "Created $out_dir/${base_name}.zip"
fi
rm -rf "$stage"

# 3. Compile Inno Setup installer if iscc is available.
#
# The script lives in `scripts/`, so the `.iss` resolves relative `Source:`
# paths against that directory. A cross-target build puts the binary under
# `target/<triple>/release/`, which the script's default `target\release`
# does not match — pass the real path and the architecture instead of
# assuming the host layout.
if command -v iscc >/dev/null 2>&1; then
    case "$target" in
        aarch64*) arch="arm64" ;;
        *) arch="x86_64" ;;
    esac
    arm64=0
    if [[ "$arch" == "arm64" ]]; then
        arm64=1
    fi

    iscc_binary="$binary"
    if command -v cygpath >/dev/null 2>&1; then
        iscc_binary="$(cygpath -w "$binary")"
    fi

    echo "Compiling Inno Setup installer for $arch..."
    iscc "/DMyAppVersion=$version" "/DMyAppArch=$arch" \
        "/DMyAppArm64=$arm64" "/DMyAppBinary=$iscc_binary" \
        "$here/scripts/shadowplayer.iss"
    echo "Installer generated successfully."
else
    echo "Notice: iscc (Inno Setup Compiler) not found in PATH; skipping installer generation."
fi

