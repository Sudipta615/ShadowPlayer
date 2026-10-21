# Theming contract

ShadowPlayer has one built-in theme (`Theme::dark()` / `Theme::light()`), and
the UI reads every colour, radius, and spacing step from it. This document
records the contract a future skin — loaded from a file or a plugin — must
satisfy. It is a contract, not a loader: there is no skin file format, no skin
UI, and no runtime theme source in this build.

## 1. The `Theme` value

`crates/ui/src/theme.rs` defines `Theme` as a plain, `Copy` data struct. A skin
produces one `Theme`; nothing else in the app may decide a colour or a layout
constant for itself.

### Surface tokens

| Field | Role |
| :--- | :--- |
| `bg_app` | The window behind every panel. |
| `bg_sidebar` | The navigation rail. |
| `bg_card` | Cards, lists and dialogs. |
| `bg_card_hover` | Hovered card/row. |
| `bg_card_active` | Pressed or selected card/row. |
| `bg_pill` | Chips, badges and inset strips. |
| `bg_pill_hover` | Hovered chip/badge. |
| `bg_input` | Text fields and search boxes. |
| `border_subtle` | Hairlines inside a surface. |
| `border_card` | The outline of a card or dialog. |
| `border_input` | The outline of a text field (see contrast). |
| `slider_track` | The unfilled part of a slider. |
| `bg_active_row` | The row of the track that is playing. |
| `border_active_row` | Its outline. |
| `artwork_placeholder` | Behind missing or loading artwork. |
| `overlay_scrim` | The translucent hover scrim over artwork. |
| `modal_backdrop` | The dimmer behind a modal dialog. |
| `drawer_scrim` | The dimmer behind the slide-over drawer. |
| `shadow` | Drop shadows behind popovers. |
| `meter_bg`, `meter_gutter`, `meter_grid` | The VU/LUFS/spatial instrument panels. |

### Accent and text tokens

| Field | Role |
| :--- | :--- |
| `accent` | Jade primary: active navigation, primary buttons, progress. |
| `accent_highlight` | Mint secondary: icon strokes, focus, selected labels. |
| `accent_warm` | Amber secondary: favourites, ratings, warm status. |
| `info` | Informational blue. |
| `text_primary` | Body copy and titles. |
| `text_secondary` | Supporting copy. |
| `text_muted` | Captions and metadata. |
| `text_dim` | The quietest labels. |
| `bit_perfect_green` | The bit-perfect status. |
| `warning_yellow` | Warnings and degraded states. |
| `destructive` | Remove, delete, clear. |

`Color32::WHITE` and `Color32::TRANSPARENT` are the only colour constants
allowed outside `theme.rs`. White is the foreground painted **on top of** an
accent fill in both themes; transparent means "no fill". Neither is a skin
colour: a skin must not change them, and every other colour — including
translucent glows and fades, which go through `theme::with_alpha` — must come
from the tokens above.

### Layout tokens

Every radius uses a `radius_*` token and every margin/padding uses a `space_*`
token; literal `CornerRadius::same(...)` and `Margin::...(...)` arguments are
rejected by `crates/ui/tests/spacing_scale.rs`.

| Radii | Value | Role |
| :--- | ---: | :--- |
| `radius_xs` | 2 | Slider tracks, meter bars, tick marks. |
| `radius_sm` | 6 | Chips, tags, checkboxes. |
| `radius_md` | 8 | Buttons and list rows. |
| `radius_lg` | 12 | Cards and panels. |
| `radius_xl` | 14 | The largest app surfaces. |
| `radius_pill` | 19 | Fully-round controls. |

| Spacing | Value | | Spacing | Value |
| :--- | ---: | :--- | :--- | ---: |
| `space_xxxs` | 1 | | `space_lg` | 10 |
| `space_xxs` | 2 | | `space_xl` | 12 |
| `space_xs` | 4 | | `space_2xl` | 14 |
| `space_sm` | 6 | | `space_3xl` | 16 |
| `space_md` | 8 | | `space_4xl` | 18 |
| | | | `space_5xl` | 20 |

`card_w` and the card gap are shared by all grid views so their columns line
up; a skin may change them, but every view must keep reading the same fields.

## 2. Contrast requirements

`crates/ui/src/theme.rs` tests every theme with the WCAG 2.1 relative-luminance
formula, and a skin must pass the same bars:

* every text token (`text_primary`, `text_secondary`, `text_muted`,
  `text_dim`) reaches at least **4.5:1** against `bg_app`, `bg_sidebar`,
  `bg_card`, `bg_card_hover`, `bg_card_active`, `bg_pill` and `bg_input`;
* `border_input` reaches at least **3:1** against `bg_input`, because the
  border is the only thing that identifies a text field (WCAG 1.4.11).

A skin that fails either bar fails the build, not just the review.

## 3. Asset requirements

* **Icons are SVG, referenced from `crates/ui/src/components/icons.rs` and
  tinted at paint time.** An icon must be monochrome (white or
  `currentColor`), use a square view box, and contain no baked-in colour; the
  app supplies the tint from a theme token. Every referenced file must exist —
  `crates/ui/tests/font_coverage.rs` checks it.
* **Text uses the bundled Inter faces.** Both `Inter-Regular.ttf` and
  `Inter-Medium.ttf` are in `crates/ui/assets/fonts/`. Any non-ASCII character
  in a user-facing string must exist in both fonts' cmap tables, unless it is
  deliberately drawn as a vector. `crates/ui/tests/font_coverage.rs` enforces
  this at the file and line where the character appears.
* **No Unicode media symbols as button labels.** Previous/next/pause and every
  other transport icon are SVG paths; the font-coverage guard exists because
  Inter does not carry the `U+23EE`/`U+23ED`/`U+23F8` block.

## 4. What a skin may not do

A skin supplies values, not behaviour. It may not:

* paint a colour outside the token set (the theme-purity test fails);
* choose a radius or margin outside the scales (the spacing test fails);
* name a backend, codec, or AutoEQ profile count — those are facts about the
  running binary and are read from the engine (`crates/ui/tests/docs_truth.rs`);
* replace an SVG icon with a font glyph (the font-coverage test fails);
* require a font other than the bundled Inter faces until the font-coverage
  test learns to read it.
