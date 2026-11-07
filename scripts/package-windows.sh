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

# 3. Compile Inno Setup installer if iscc is available
if command -v iscc >/dev/null 2>&1; then
    echo "Compiling Inno Setup installer..."
    iscc "/DMyAppVersion=$version" "$here/scripts/shadowplayer.iss"
    echo "Installer generated successfully."
else
    echo "Notice: iscc (Inno Setup Compiler) not found in PATH; skipping installer generation."
fi

