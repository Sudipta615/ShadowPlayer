# AutoEQ parametric corpus

`autoeq_parametric.bin` contains 8,850 parametric headphone and IEM correction
curves for the AutoEQ browser.

## Source and license

The curves are AutoEQ data, created by Jaakko Pasanen and distributed under the
MIT license:

- <https://github.com/jaakkopasanen/AutoEq>
- <https://github.com/jaakkopasanen/AutoEq/blob/master/LICENSE>

Copyright (c) 2018 Jaakko Pasanen. The full license text is in
[`LICENSE-AutoEq`](LICENSE-AutoEq).

The rows were extracted from the AutoEQ-derived preset database bundled with
Poweramp (`auto_eq.db`). Poweramp is not affiliated with ShadowPlayer and does
not endorse it; only the AutoEQ measurement and correction data is used here.
The extraction drops Poweramp's two tone controls, maps its three parametric
filter ids to ShadowPlayer's biquad types, and computes a preamp from the
combined response. See `scripts/generate-autoeq-corpus.py` for the exact
transformation.

## Format

Little-endian, no compression:

```
magic      6 bytes   "SHAEQ\x01"
count      u32
record     count times:
  name_len   u16
  name       UTF-8 bytes
  src_len    u16
  source     UTF-8 bytes   (measurement rig, e.g. "Crinacle 711 in-ear")
  flags      u8            bit 0: in-ear
  preamp_db  f32
  band_count u8
  band       band_count times:
    kind     u8            0 peaking, 1 low shelf, 2 high shelf
    freq_hz  f32
    gain_db  f32
    q        f32
```

Every shipped curve has exactly 10 bands: one low shelf, eight peaking filters,
and one high shelf. The data is treated as untrusted input at load time: the
decoder bounds-checks every length, rejects unknown filter ids, and refuses a
truncated file rather than returning a partial curve.
