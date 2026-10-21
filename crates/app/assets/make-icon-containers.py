#!/usr/bin/env python3
"""Pack the ShadowPlayer PNG icons into the Windows `.ico` and macOS `.icns`.

Both formats are container formats around PNG payloads, so they are written by
hand here rather than by shelling out to `icotool` / `iconutil` — the latter
only exist on some platforms, and the output must be byte-reproducible
everywhere.

Run through `make-icons.sh`.
"""

from __future__ import annotations

import struct
import sys
from pathlib import Path

# ── Windows .ico ──────────────────────────────────────────────────────────────
# ICONDIR: reserved(2)=0, type(2)=1 (icon), count(2)
# ICONDIRENTRY per image: width(1) height(1) palette(1) reserved(1)
#                          planes(2) bpp(2) bytes(4) offset(4)
# The payloads are PNGs, which every Windows version since Vista understands,
# and which carry the alpha channel an `.ico` otherwise cannot.

ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)


def write_ico(src_dir: Path, out_path: Path) -> None:
    images: list[tuple[int, bytes]] = []
    for size in ICO_SIZES:
        png = src_dir / f"icon-{size}.png"
        if not png.exists():
            raise SystemExit(f"missing {png}; run the rasteriser for every size first")
        images.append((size, png.read_bytes()))

    header = struct.pack("<HHH", 0, 1, len(images))
    offset = len(header) + 16 * len(images)

    directory = b""
    payload = b""
    for size, data in images:
        # 256 is stored as 0 in the byte-sized width/height fields.
        dim = 0 if size >= 256 else size
        directory += struct.pack(
            "<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset
        )
        payload += data
        offset += len(data)

    out_path.write_bytes(header + directory + payload)
    print(f"wrote {out_path} ({len(images)} images)")


# ── macOS .icns ───────────────────────────────────────────────────────────────
# 'icns' magic(4) length(4), then per entry: type(4) length(4) payload.
# The PNG-capable entry types are used so the alpha channel survives:
#   icp4=16  icp5=32  icp6=64  ic07=128  ic08=256  ic09=512  ic10=1024

ICNS_ENTRIES = (
    ("icp4", 16),
    ("icp5", 32),
    ("icp6", 64),
    ("ic07", 128),
    ("ic08", 256),
    ("ic09", 512),
    ("ic10", 1024),
)


def write_icns(src_dir: Path, out_path: Path) -> None:
    body = b""
    for kind, size in ICNS_ENTRIES:
        png = src_dir / f"icon-{size}.png"
        if not png.exists():
            raise SystemExit(f"missing {png}; run the rasteriser for every size first")
        data = png.read_bytes()
        body += struct.pack(">4sI", kind.encode("ascii"), len(data) + 8) + data

    out_path.write_bytes(struct.pack(">4sI", b"icns", len(body) + 8) + body)
    print(f"wrote {out_path} ({len(ICNS_ENTRIES)} images)")


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: make-icon-containers.py <icons-dir> <assets-dir>", file=sys.stderr)
        return 2
    src_dir = Path(sys.argv[1])
    assets_dir = Path(sys.argv[2])
    write_ico(src_dir, assets_dir / "icon.ico")
    write_icns(src_dir, assets_dir / "icon.icns")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
