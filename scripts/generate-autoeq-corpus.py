#!/usr/bin/env python3
"""Build the compact AutoEQ parametric corpus shipped with ShadowPlayer.

The correction curves originate from AutoEQ (https://github.com/jaakkopasanen/AutoEq,
MIT).  This script does not invent curves: it extracts the parametric presets
from a Poweramp `auto_eq.db`, drops the two tone controls (bass/treble are not
part of the correction), maps Poweramp's filter ids to ShadowPlayer's, and
writes a small binary the UI decodes on a worker thread.

Poweramp stores no preamp in these presets (all rows are 0.0 dB).  Shipping a
+6 dB curve with 0 dB preamp would clip, so the generator computes the peak of
the *combined* biquad response over 20 Hz-22 kHz and stores its negative as the
preamp.  This mirrors AutoEQ's headroom recommendation closely and is rounded
down to 0.1 dB.

The output is deliberately a flat, fixed-width format: decoding allocates once,
does no per-entry parsing, and cannot execute data from the file.

Usage:
    scripts/generate-autoeq-corpus.py [path/to/auto_eq.db]
"""

from __future__ import annotations

import math
import sqlite3
import struct
import sys
from pathlib import Path

MAGIC = b"SHAEQ\x01"
DEFAULT_DB = Path("/tmp/opencode/pamp/auto_eq.db")
OUTPUT = Path(__file__).resolve().parent.parent / "crates/ui/assets/autoeq/autoeq_parametric.bin"

# Poweramp Band.type -> ShadowPlayer filter.
# 0/1 are the bass/treble tone controls, 2 is a graphic band, 3..5 are the
# parametric peaking/low-shelf/high-shelf ids.
FILTER_KIND = {3: 0, 4: 1, 5: 2}  # 0 peaking, 1 low shelf, 2 high shelf
KIND_NAMES = {0: "peaking", 1: "low shelf", 2: "high shelf"}


def biquad_response(kind: int, freq: float, gain_db: float, q: float, rate: float, at: float) -> float:
    """Magnitude in dB of one RBJ biquad, matching engine::dsp::biquad."""
    a = 10.0 ** (gain_db / 40.0)
    w0 = 2.0 * math.pi * freq / rate
    cos_w0 = math.cos(w0)
    sin_w0 = math.sin(w0)
    alpha = sin_w0 / (2.0 * q)

    if kind == 0:  # peaking
        b0 = 1.0 + alpha * a
        b1 = -2.0 * cos_w0
        b2 = 1.0 - alpha * a
        a0 = 1.0 + alpha / a
        a1 = -2.0 * cos_w0
        a2 = 1.0 - alpha / a
    elif kind == 1:  # low shelf
        two_sqrt_a_alpha = 2.0 * math.sqrt(a) * alpha
        b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha)
        b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0)
        b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha)
        a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha
        a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0)
        a2 = (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha
    elif kind == 2:  # high shelf
        two_sqrt_a_alpha = 2.0 * math.sqrt(a) * alpha
        b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha)
        b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0)
        b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha)
        a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha
        a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0)
        a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha
    else:
        raise ValueError(f"unknown filter kind {kind}")

    # Evaluate H(z) at z = e^{jw}.  Normalise by a0 first.
    w = 2.0 * math.pi * at / rate
    z1 = complex(math.cos(-w), math.sin(-w))
    z2 = z1 * z1
    numerator = (b0 / a0) + (b1 / a0) * z1 + (b2 / a0) * z2
    denominator = 1.0 + (a1 / a0) * z1 + (a2 / a0) * z2
    return 20.0 * math.log10(abs(numerator / denominator))


def compute_preamp(bands: list[tuple[int, float, float, float]]) -> float:
    """Negative of the combined response peak, floored to 0.1 dB."""
    rate = 48_000.0
    peak = 0.0
    # 20 Hz-22 kHz, about 1/24 octave plus every corner frequency.  The
    # corners are added separately because a high-Q peak can sit between two
    # log-grid points.
    points = [22_000.0 * (20.0 / 22_000.0) ** (i / 240.0) for i in range(241)]
    points.extend(freq for _, freq, _, _ in bands)
    for _, freq, _, _ in bands:
        points.extend((freq * 2.0 ** (-1.0 / 48.0), freq * 2.0 ** (1.0 / 48.0)))
    for at in points:
        db = sum(
            biquad_response(kind, freq, gain, q, rate, at)
            for kind, freq, gain, q in bands
        )
        peak = max(peak, db)
    # A single band's gain is a floor: shelves and overlapping peaks can sum
    # higher, but never lower, so this keeps a degenerate response safe.
    peak = max(peak, max((gain) for _, _, gain, _ in bands))
    if peak <= 0.0:
        return 0.0
    return -math.floor(peak * 10.0 + 1e-9) / 10.0


def decode(blob: bytes) -> tuple[float, list[tuple[int, float, float, float]]]:
    if blob[:4] != b"PaP\x01":
        raise ValueError("not a PaP preset")
    flags = struct.unpack_from("<i", blob, 4)[0]
    if not flags & 0x10000:
        raise ValueError("not parametric")
    count = flags & 0x0FFF
    preamp = struct.unpack_from("<f", blob, 8)[0]
    offset = 12
    bands: list[tuple[int, float, float, float]] = []
    for _ in range(count):
        packed = struct.unpack_from("<i", blob, offset)[0]
        gain = struct.unpack_from("<f", blob, offset + 4)[0]
        freq = struct.unpack_from("<i", blob, offset + 8)[0]
        offset += 12
        ptype = packed & 0xFF
        extras = bool(packed & 0x2000)
        q = None
        if extras:
            q = struct.unpack_from("<f", blob, offset)[0]
            offset += 8
        if ptype in FILTER_KIND:
            if q is None:
                raise ValueError("parametric band without Q")
            bands.append((FILTER_KIND[ptype], float(freq), float(gain), float(q)))
    if len(bands) != 10:
        raise ValueError(f"expected 10 EQ bands, got {len(bands)}")
    if offset != len(blob):
        raise ValueError("trailing bytes after the last band")
    return preamp, bands


def main() -> int:
    db_path = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_DB
    if not db_path.exists():
        print(f"error: {db_path} not found", file=sys.stderr)
        return 1

    db = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    rows = db.execute(
        "select name, meta, data_blob from eq_presets where parametric = 1"
    ).fetchall()

    entries = []
    skipped = 0
    for name, source, blob in rows:
        try:
            stored_preamp, bands = decode(blob)
        except ValueError as error:
            skipped += 1
            print(f"warning: skipping {name!r}: {error}", file=sys.stderr)
            continue
        if stored_preamp != 0.0:
            print(
                f"warning: {name!r} carries a non-zero preamp "
                f"({stored_preamp:+.1f} dB); the computed headroom replaces it",
                file=sys.stderr,
            )
        preamp = compute_preamp(bands)
        is_iem = "in-ear" in source
        entries.append(
            (
                name.strip(),
                source.strip(),
                is_iem,
                preamp,
                bands,
            )
        )

    entries.sort(key=lambda entry: (entry[0].casefold(), entry[1].casefold()))

    payload = bytearray(MAGIC)
    payload += struct.pack("<I", len(entries))
    for name, source, is_iem, preamp, bands in entries:
        name_bytes = name.encode("utf-8")
        source_bytes = source.encode("utf-8")
        if len(name_bytes) > 0xFFFF or len(source_bytes) > 0xFFFF:
            raise ValueError(f"name or source too long: {name!r}")
        payload += struct.pack("<H", len(name_bytes)) + name_bytes
        payload += struct.pack("<H", len(source_bytes)) + source_bytes
        payload += struct.pack("<B", 1 if is_iem else 0)
        payload += struct.pack("<f", preamp)
        payload += struct.pack("<B", len(bands))
        for kind, freq, gain, q in bands:
            payload += struct.pack("<Bfff", kind, freq, gain, q)

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_bytes(payload)

    print(f"wrote {OUTPUT} ({len(payload):,} bytes, {len(entries):,} presets)")
    print(f"skipped {skipped} unreadable rows")
    print(
        "preamp range: "
        f"{min(entry[3] for entry in entries):+.1f} .. "
        f"{max(entry[3] for entry in entries):+.1f} dB"
    )
    print(f"first: {entries[0][0]} / {entries[0][1]}")
    print(f"last:  {entries[-1][0]} / {entries[-1][1]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
