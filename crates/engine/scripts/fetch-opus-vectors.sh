#!/usr/bin/env bash
# Fetch the official RFC 8251 Opus test vectors for the conformance harness.
#
# The archive is ~75 MB and is not committed (the bitstreams and reference
# decodes are 121 MB extracted), so the harness stays opt-in and this script is
# the one reproducible way to opt in.
#
# Usage:
#   crates/engine/scripts/fetch-opus-vectors.sh [destination]
#
# The destination defaults to crates/engine/testdata/opus_testvectors, which is
# where the harness looks when OPUS_TESTVECTORS_DIR is not set.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dest="${1:-${OPUS_TESTVECTORS_DIR:-$here/testdata/opus_testvectors}}"
url="https://opus-codec.org/static/testvectors/opus_testvectors-rfc8251.tar.gz"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $url"
curl -fL --retry 3 --retry-delay 2 -o "$tmp/vectors.tar.gz" "$url"

echo "Extracting into $dest"
mkdir -p "$dest"
tar -xzf "$tmp/vectors.tar.gz" -C "$tmp"
cp "$tmp"/opus_newvectors/*.bit "$tmp"/opus_newvectors/*.dec "$dest"/
cp "$here/testdata/vectors.example.txt" "$dest/vectors.txt"

echo
echo "Installed $(find "$dest" -name '*.bit' | wc -l) bitstreams and their reference decodes."
echo "Run them with:"
echo "  cd $here && OPUS_TESTVECTORS_DIR=\"$dest\" \\"
echo "    cargo test --release -p opus-decoder --test conformance_rfc -- --ignored --nocapture"
