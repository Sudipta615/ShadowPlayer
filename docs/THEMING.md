# ShadowPlayer Theme System Contract & Architecture

ShadowPlayer provides a unified, high-performance theming architecture.
The UI reads every colour, radius, and spacing step from [`ui::theme::Theme`].
Themes can be system presets (Obsidian Jade, Studio Mint, OLED Midnight) or custom
user themes loaded from `.shadowtheme` files.

---

## 1. Subsystem Architecture (`crates/ui/src/theme/`)

The theming engine is organized into modular components under `crates/ui/src/theme/`:

* **`tokens.rs`**: Defines the `Theme` value as a plain, `Copy` data struct without heap allocations in the rendering loop. Provides font scaling (`TEXT_SCALE`), alpha blending (`with_alpha`), and color utilities.
* **`system.rs`**: Built-in system theme presets:
  - `dark()` — *Obsidian Jade*: Deep obsidian surfaces with jade (#0F766E) and mint (#5EEAD4) highlights.
  - `light()` — *Studio Mint*: Crisp studio-light surfaces with teal accents.
  - `oled()` — *OLED Midnight*: Pure black (#000000) background engineered for zero-power OLED displays.
* **`contrast.rs`**: WCAG 2.1 relative luminance calculator, contrast ratio evaluator, and an iterative lightness-stepping algorithm (`auto_adjust_contrast`) to rescue low-contrast tokens.
* **`schema.rs`**: Theme file serialization supporting the `.shadowtheme` format.
* **`manager.rs`**: Filesystem discovery across the `themes/` directory, active theme selection, and staged live preview lifecycle.
* **`preview.rs`**: Sandbox rendering a mini playback shell with simulated album artwork, 3-track playlist table, transport controls, volume slider, and VU meters.
* **`gallery.rs`**: Responsive card grid with search, category filtering (All, Dark, Light, OLED, Custom), and active state indicators.
* **`editor.rs`**: In-app color customizer with grouped pickers, real-time WCAG AA compliance validation, auto-fix button, and JSON file export.

---

## 2. The `Theme` Value & Tokens

`crates/ui/src/theme/tokens.rs` defines `Theme` as a `Copy` data struct. Nothing else in the app may decide a colour or layout constant for itself.

### Surface Tokens

| Field | Role |
| :--- | :--- |
| `bg_app` | The window behind every panel. |
| `bg_sidebar` | The navigation rail. |
| `bg_card` | Cards, lists, and dialogs. |
| `bg_card_hover` | Hovered card/row. |
| `bg_card_active` | Pressed or selected card/row. |
| `bg_pill` | Chips, badges, and inset strips. |
| `bg_pill_hover` | Hovered chip/badge. |
| `bg_input` | Text fields and search boxes. |
| `border_subtle` | Hairlines inside a surface. |
| `border_card` | The outline of a card or dialog. |
| `border_input` | The outline of a text field (see contrast). |
| `slider_track` | The unfilled part of a slider. |
| `bg_active_row` | The row of the track that is currently playing. |
| `border_active_row` | Outline of the playing row. |
| `artwork_placeholder` | Background for missing or loading artwork. |
| `overlay_scrim` | Translucent hover scrim over artwork. |
| `modal_backdrop` | Dimmer behind modal dialogs. |
| `drawer_scrim` | Dimmer behind slide-over drawers. |
| `shadow` | Drop shadows behind popovers. |
| `meter_bg`, `meter_gutter`, `meter_grid` | VU, LUFS, and spatial visualization surfaces. |

### Accent and Text Tokens

| Field | Role |
| :--- | :--- |
| `accent` | Primary accent: active navigation, primary buttons, progress bar. |
| `accent_highlight` | Mint/highlight accent: icon strokes, focus rings, selected labels. |
| `accent_warm` | Warm amber: favorites, ratings, warm status badges. |
| `info` | Informational blue. |
| `text_primary` | High-contrast body copy and titles. |
| `text_secondary` | Supporting copy and secondary labels. |
| `text_muted` | Captions, timecodes, and metadata. |
| `text_dim` | Quietest non-essential labels. |
| `bit_perfect_green` | Bit-perfect audio indicator. |
| `warning_yellow` | Warnings and degraded states. |
| `destructive` | Remove, delete, and destructive actions. |

`Color32::WHITE` and `Color32::TRANSPARENT` are the only colour constants
allowed outside `theme.rs` and the `theme` module. White is the foreground painted **on top of** an
accent fill in both themes; transparent means "no fill". Neither is a skin or theme
colour: custom themes must not change them, and every other colour — including
translucent glows and fades, which go through `theme::with_alpha` — must come
from the tokens above.

### Layout Tokens

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

---

## 3. Contrast Requirements (WCAG 2.1 AA)

All themes are strictly validated against WCAG 2.1 AA standards:

* Every text token (`text_primary`, `text_secondary`, `text_muted`, `text_dim`) must achieve at least **4.5:1** contrast against `bg_app`, `bg_sidebar`, `bg_card`, `bg_card_hover`, `bg_card_active`, `bg_pill`, and `bg_input`.
* `border_input` must reach at least **3.0:1** contrast against `bg_input` (WCAG 1.4.11 non-text contrast).

### Contrast API

| Function | Returns | Use |
| :--- | :--- | :--- |
| `contrast_ratio(a, b)` | `f32` (1.0 – 21.0) | Raw ratio query. |
| `validate_contrast(map)` | `Result<(), ContrastError>` | Pass/fail for file loading; short-circuits on first failure. |
| `validate_contrast_all(map)` | `Vec<ContrastError>` | All failing pairs; used by the in-app editor to show every issue at once. |
| `auto_adjust_contrast(fg, bg, ratio)` | `Color32` | HSL lightness-only rescue: hue and saturation are preserved. |

The **Auto-Fix Contrast** button in the Theme Editor calls `auto_adjust_contrast` for every text token.  The algorithm walks the HSL `L` channel in 0.5 % steps, keeping hue and saturation fixed, so warm amber stays amber and cool mint stays mint.

---

## 4. Theme File Format (`.shadowtheme`)

Themes are stored in JSON files under the `themes/` directory:

| Field | Required | Description |
| :--- | :---: | :--- |
| `schema_version` | No | Integer. Omitting it means v1 (legacy). Current version is **2**. |
| `id` | No | Unique slug (e.g. `nord-aurora`). Falls back to filename stem. |
| `name` | No | Human-readable display name. Falls back to `id`. |
| `author` | No | Author credit string. |
| `description` | No | Short one-line summary. |
| `is_light` | No | `true` for a light-background theme; defaults to `false`. |
| `colors` | Yes | Flat map of `token_name → "#RRGGBB"` or `"#RRGGBBAA"` strings. |
| `radii` | No | Optional radius overrides (`radius_xs`, `radius_sm`, …). |

```json
{
  "schema_version": 2,
  "id": "nord-aurora",
  "name": "Nord Aurora",
  "author": "Community",
  "description": "Arctic, north-bluish clean aesthetic",
  "is_light": false,
  "colors": {
    "bg_app": "#2E3440",
    "bg_sidebar": "#242933",
    "bg_card": "#3B4252",
    "bg_card_hover": "#434C5E",
    "bg_card_active": "#4C566A",
    "bg_pill": "#434C5E",
    "bg_pill_hover": "#4C566A",
    "bg_input": "#2E3440",
    "border_subtle": "#434C5E",
    "border_card": "#4C566A",
    "border_input": "#88C0D0",
    "slider_track": "#434C5E",
    "bg_active_row": "#3B4252",
    "border_active_row": "#88C0D0",
    "artwork_placeholder": "#2E3440",
    "overlay_scrim": "#00000099",
    "modal_backdrop": "#000000B3",
    "drawer_scrim": "#00000080",
    "shadow": "#00000066",
    "meter_bg": "#242933",
    "meter_gutter": "#2E3440",
    "meter_grid": "#434C5E",
    "accent": "#88C0D0",
    "accent_highlight": "#8FBCBB",
    "accent_warm": "#EBCB8B",
    "info": "#81A1C1",
    "text_primary": "#ECEFF4",
    "text_secondary": "#E5E9F0",
    "text_muted": "#D8DEE9",
    "text_dim": "#9DA7B8",
    "bit_perfect_green": "#A3BE8C",
    "warning_yellow": "#EBCB8B",
    "destructive": "#BF616A"
  }
}
```

---

### Schema Version Migration (v1 → v2)

ShadowPlayer includes an automated migration pipeline (`schema::migrate_theme_file`):
* **Backward Compatibility**: Files lacking a `schema_version` field or declaring `schema_version: 1` (including legacy `.shadowskin` formats) are automatically migrated in-memory to schema version 2 upon load.
* **Token Synthesis**: Any newly mandated v2 tokens (e.g., `scan_banner_bg`, `meter_bg`, `meter_gutter`, `bg_active_row`, `border_active_row`, `artwork_placeholder`, `destructive`) missing from a legacy file are intelligently synthesized from existing palette tokens (`bg_card_hover`, `bg_pill`, `accent`, etc.) to guarantee validation success and visual consistency.
* **Metadata Structuring**: Root-level metadata fields are migrated into a structured `ThemeMetadata` block.

---

## 5. UI Integration, Custom Themes & Live Staged Previews

ShadowPlayer features a dedicated **Themes & Appearance** category in Settings:
* **Custom Themes Folder (`themes/custom-theme/`)**: Custom user themes reside inside `<data_dir>/themes/custom-theme/`. The theme manager monitors this directory and loads all `.shadowtheme` and `.json` files.
* **Active Theme Persistence**: When a theme is committed or applied, its identifier is persisted to `<data_dir>/themes/active_theme.txt` so the selected theme survives application restarts seamlessly.
* **Optimized Import & Invalidation**: Importing a `.shadowtheme` file immediately registers and stages the theme in-memory without expensive full-disk rescans, with dirty-flag cache invalidation (`reload_if_dirty`) managing cache freshness.
* **Import Theme Button**: In Settings → Appearance, clicking **Import Theme…** opens the system's native file picker (`rfd`, XDG portal / native OS dialog) allowing users to select `.shadowtheme` files from anywhere on their storage. Imported themes are validated, copied into `themes/custom-theme/`, and immediately staged or applied.
* **Open Folder Button**: Directly opens `<data_dir>/themes/custom-theme/` in the OS file manager (Nautilus, Dolphin, Finder, Explorer) with a single click.
* **Gallery**: Filter and search through installed and system themes. Each card displays author, theme version (or `built-in`), color swatches, description, and tag chips (`#dark`, `#oled`, etc.). Selecting a card stages it for live preview.
* **Sandbox Preview**: Real-time rendering of mini transport controls, volume slider, VU meter, and track listing.
* **Apply / Revert**: Staged themes are visually previewed instantly and only persist when applied. Reverting restores the active theme.
* **Theme Editor**: Direct parameter tuning and instant saving into `themes/custom-theme/`. Uses `for_each_required_color_token` macro as the single source of truth for color token definitions.

---

## 6. What a Theme May Not Do

A theme supplies values, not arbitrary UI code or behavior:
* It may not paint colours outside the token set (enforced by `theme_purity.rs`).
* It may not choose arbitrary numeric margins or radii outside the spacing scale (`spacing_scale.rs`).
* It may not bypass WCAG AA contrast standards.
* It may not alter playback or audio processing pipelines.

