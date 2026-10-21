#!/usr/bin/env bash
#
# Regenerate ShadowPlayer's application icons from the SVG source of truth.
#
#   crates/app/assets/icon.svg          ← the source of truth (edit this)
#   crates/app/assets/icon-*.png        ← generated, committed
#   crates/app/assets/icon.ico          ← generated, committed (Windows)
#   crates/app/assets/icon.icns         ← generated, committed (macOS)
#
# The PNGs are rasterised with `resvg` — the same rasteriser the app uses to
# draw its SVG icons — so the launcher mark and the in-app mark are the same
# shape at every size.
#
# Usage:  ./make-icons.sh
#
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

# Every size the desktop shells ask for, plus the intermediate sizes the
# Windows and macOS containers are built from.
SIZES=(16 24 32 48 64 128 256 512 1024)

echo "▸ rasterising icon.svg"
cargo build --release --manifest-path rasterizer/Cargo.toml
./rasterizer/target/release/shadowplayer-icon-rasterizer \
    icon.svg "$here" "${SIZES[@]}"

echo "▸ packing icon.ico and icon.icns"
python3 make-icon-containers.py "$here" "$here"

echo "▸ done"
ls -la icon.svg icon.ico icon.icns icon-*.png
