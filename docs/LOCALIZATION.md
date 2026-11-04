# Localisation

ShadowPlayer ships six interface catalogues. English is the source of truth;
the other five are partial and fall back to English string by string. The
Settings picker shows each catalogue's coverage before you switch, and the
in-app note repeats the fallback rule. A partial catalogue is never presented
as a complete translation.

## Shipped catalogues

Coverage is counted against the English source keys and floored, so a partial
catalogue is never rounded up to a complete one.

| Locale | Code | Strings | Coverage |
| --- | --- | ---: | ---: |
| English | `en` | 1008 | 100% |
| German | `de` | 657 | 65% |
| French | `fr` | 319 | 31% |
| Spanish | `es` | 319 | 31% |
| Japanese | `ja` | 319 | 31% |
| Chinese | `zh` | 319 | 31% |

The counts move whenever a `tr!` call site is added or removed. The live
numbers come from `ui::i18n::coverage`, not from this table; recompute them
with:

```bash
python3 - <<'PY'
import json
en = json.load(open("crates/ui/locales/en.json"))
for code in ("de", "fr", "es", "ja", "zh"):
    cat = json.load(open(f"crates/ui/locales/{code}.json"))
    n = len(set(cat) & set(en))
    print(code, n, "/", len(en), "=", n * 100 // len(en), "%")
PY
```

## How the catalogue works

- `crates/ui/locales/en.json` is the source of truth. Each key is
  `S` + the 64-bit FNV-1a hash of the English text, so the key is stable even
  when the copy moves between modules.
- `crates/ui/build.rs` scans every `tr!("…")` and `tr_args!("…", …)` call site,
  generates the `Msg` enum, and fails the build in both directions: a call site
  without an entry, and an entry without a call site.
- `tr!`/`tr_args!` also accept `Msg::S…` variants. The generated enum is
  checked at compile time; there is no runtime key lookup for literals.
- A missing translation falls back to English and logs the missing key once.
  The UI never shows a raw key.

## Adding or changing a string

1. Write the user-facing string with `tr!` or `tr_args!`.
2. Run `cargo check -p ui`. The build script prints the expected key for any
   missing literal, for example:
   `tr! literal "Interface" has no catalogue entry (expected key SF866…)`.
3. Add the key and English text to `locales/en.json`.
4. Add a German translation to `locales/de.json` when possible. German is the
   second reference catalogue and is expected to stay substantially complete.
5. Run `cargo test -p ui i18n`.

Keys are sorted in every catalogue so diffs stay reviewable. Keep `{name}`
placeholders identical across languages; `tr_args!` substitutes by name and
leaves an unknown placeholder visible rather than dropping it.

## Adding a locale

1. Add a variant to `Locale` in `crates/ui/src/i18n/mod.rs`, plus its `code`,
   `native_name`, `from_code`, and `ALL` entries.
2. Add a `LazyLock<HashMap<String, String>>` for the catalogue and wire it into
   `catalogue_for`.
3. Add `locales/<code>.json` with whatever subset is genuinely translated.
4. The Settings picker lists every `Locale::ALL` entry automatically, with its
   live coverage, so no second list can drift.
5. Add the locale to the `every_non_english_key_exists_in_english` test.

Do not translate the long-form Help manual by machine. It is English in every
locale and the README says so; the interface catalogue is the translated
surface.

## Coverage definition

`coverage(locale)` counts English keys that have a non-empty entry in that
locale, divided by the number of English keys. Keys that exist only in a
translation file are ignored, so a stale entry cannot inflate the percentage.
English is defined as complete by construction. `Coverage::percent` floors the
result; the UI must never round a partial catalogue up to a complete one.
