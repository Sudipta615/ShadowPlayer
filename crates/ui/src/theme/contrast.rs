use std::collections::HashMap;
use eframe::egui::Color32;

/// Errors that can occur when validating or calculating contrast.
#[derive(Debug, Clone, PartialEq)]
pub enum ContrastError {
    ContrastFailure {
        fg: String,
        bg: String,
        ratio: f32,
        required: f32,
    },
}

impl std::fmt::Display for ContrastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContrastFailure {
                fg,
                bg,
                ratio,
                required,
            } => write!(
                f,
                "Contrast check failed: '{fg}' on '{bg}' is {ratio:.2}:1 (required {required:.1}:1)"
            ),
        }
    }
}

/// The relative luminance of a colour, per WCAG 2.1.
pub fn luminance(c: Color32) -> f32 {
    fn channel(v: u8) -> f32 {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// The WCAG contrast ratio between two colours: 1.0 to 21.0.
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (hi, lo) = {
        let (la, lb) = (luminance(a), luminance(b));
        if la >= lb {
            (la, lb)
        } else {
            (lb, la)
        }
    };
    (hi + 0.05) / (lo + 0.05)
}

// ─── Colour space helpers ─────────────────────────────────────────────────────

/// Convert `Color32` to (h in [0,360), s in [0,1], l in [0,1]).
fn to_hsl(c: Color32) -> (f32, f32, f32) {
    let r = c.r() as f32 / 255.0;
    let g = c.g() as f32 / 255.0;
    let b = c.b() as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let l = (max + min) / 2.0;
    if delta < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    let h = if (max - r).abs() < f32::EPSILON {
        ((g - b) / delta).rem_euclid(6.0) * 60.0
    } else if (max - g).abs() < f32::EPSILON {
        ((b - r) / delta + 2.0) * 60.0
    } else {
        ((r - g) / delta + 4.0) * 60.0
    };
    (h, s, l)
}

/// Convert (h, s, l) back to `Color32`.  Alpha is preserved from `original`.
fn from_hsl(h: f32, s: f32, l: f32, original: Color32) -> Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let to_u8 = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(to_u8(r1), to_u8(g1), to_u8(b1), original.a())
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Validate WCAG contrast for all text and input border tokens from a colour map.
/// Returns the *first* failure (backward-compatible single-error path).
/// For a complete report of every failing pair use [`validate_contrast_all`].
pub fn validate_contrast(colors: &HashMap<String, Color32>) -> Result<(), ContrastError> {
    validate_contrast_all(colors)
        .into_iter()
        .next()
        .map_or(Ok(()), Err)
}

/// Validate WCAG contrast and return *all* failures, not just the first.
/// Used by the Theme Editor to surface every problematic pair simultaneously.
pub fn validate_contrast_all(colors: &HashMap<String, Color32>) -> Vec<ContrastError> {
    let text_tokens = [
        "text_primary",
        "text_secondary",
        "text_muted",
        "text_dim",
        "accent_highlight",
        "accent_warm",
    ];
    let bg_tokens = ["bg_app", "bg_card", "bg_sidebar"];

    let mut failures = Vec::new();

    for fg_name in text_tokens {
        let Some(&fg) = colors.get(fg_name) else {
            continue;
        };
        for bg_name in bg_tokens {
            let Some(&bg) = colors.get(bg_name) else {
                continue;
            };
            let ratio = contrast_ratio(fg, bg);
            if ratio < 4.5 {
                failures.push(ContrastError::ContrastFailure {
                    fg: fg_name.to_string(),
                    bg: bg_name.to_string(),
                    ratio,
                    required: 4.5,
                });
            }
        }
    }

    // Input border must be at least 3:1 on bg_input.
    if let (Some(&bi), Some(&bgi)) = (colors.get("border_input"), colors.get("bg_input")) {
        let ratio = contrast_ratio(bi, bgi);
        if ratio < 3.0 {
            failures.push(ContrastError::ContrastFailure {
                fg: "border_input".to_string(),
                bg: "bg_input".to_string(),
                ratio,
                required: 3.0,
            });
        }
    }

    failures
}

/// Automatically adjust a foreground colour's **lightness** so it meets the
/// target contrast ratio against the specified background colour.
///
/// This operates in HSL space, stepping only the L channel so the original
/// hue and saturation are preserved.  Warm amber stays amber; cool mint stays
/// mint — only luminance is nudged until the WCAG bar is cleared.
///
/// Achromatic colours (saturation ~0) fall back to a uniform RGB walk because
/// HSL hue is undefined for pure greys.
pub fn auto_adjust_contrast(fg: Color32, bg: Color32, target_ratio: f32) -> Color32 {
    if contrast_ratio(fg, bg) >= target_ratio {
        return fg;
    }

    let bg_lum = luminance(bg);
    let should_lighten = bg_lum < 0.5;

    let (h, s, mut l) = to_hsl(fg);

    // Achromatic path: walk RGB uniformly (HSL hue is meaningless for greys).
    if s < 0.01 {
        let mut r = fg.r() as f32;
        let mut g = fg.g() as f32;
        let mut b_ch = fg.b() as f32;
        for _ in 0..255 {
            if should_lighten {
                r = (r + 1.0).min(255.0);
                g = (g + 1.0).min(255.0);
                b_ch = (b_ch + 1.0).min(255.0);
            } else {
                r = (r - 1.0).max(0.0);
                g = (g - 1.0).max(0.0);
                b_ch = (b_ch - 1.0).max(0.0);
            }
            let candidate =
                Color32::from_rgba_unmultiplied(r as u8, g as u8, b_ch as u8, fg.a());
            if contrast_ratio(candidate, bg) >= target_ratio {
                return candidate;
            }
        }
        return Color32::from_rgba_unmultiplied(r as u8, g as u8, b_ch as u8, fg.a());
    }

    // Chromatic path: walk L in 0.5% steps, keeping H and S fixed.
    let step = 0.005_f32;
    for _ in 0..200 {
        if should_lighten {
            l = (l + step).min(1.0);
        } else {
            l = (l - step).max(0.0);
        }
        let candidate = from_hsl(h, s, l, fg);
        if contrast_ratio(candidate, bg) >= target_ratio {
            return candidate;
        }
        // Clamped at white or black — cannot improve further.
        if (should_lighten && l >= 1.0) || (!should_lighten && l <= 0.0) {
            return candidate;
        }
    }

    from_hsl(h, s, l, fg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_adjust_rescues_low_contrast() {
        let dark_bg = Color32::from_rgb(11, 15, 16);
        let poor_fg = Color32::from_rgb(30, 40, 40); // Too dark on dark_bg
        assert!(contrast_ratio(poor_fg, dark_bg) < 4.5);

        let fixed_fg = auto_adjust_contrast(poor_fg, dark_bg, 4.5);
        assert!(contrast_ratio(fixed_fg, dark_bg) >= 4.5);
    }

    #[test]
    fn auto_adjust_preserves_hue_of_amber() {
        let bg = Color32::from_rgb(11, 15, 16);
        // A low-contrast amber
        let amber = Color32::from_rgb(120, 60, 5);
        let fixed = auto_adjust_contrast(amber, bg, 4.5);
        assert!(contrast_ratio(fixed, bg) >= 4.5);

        // Hue must remain in the orange-amber range (roughly 10 to 60 degrees)
        let (h, _s, _l) = to_hsl(fixed);
        assert!(
            (10.0..=60.0).contains(&h),
            "adjusted amber left the orange hue range: h={h:.1}"
        );
    }

    #[test]
    fn validate_contrast_all_returns_every_failure() {
        let mut colors = std::collections::HashMap::new();
        let dark = Color32::from_rgb(11, 15, 16);
        colors.insert("bg_app".into(), dark);
        colors.insert("bg_card".into(), dark);
        colors.insert("bg_sidebar".into(), dark);
        // Both nearly invisible on dark backgrounds
        colors.insert("text_primary".into(), Color32::from_rgb(15, 18, 18));
        colors.insert("text_secondary".into(), Color32::from_rgb(18, 22, 22));
        let failures = validate_contrast_all(&colors);
        // Each bad fg token fails on 3 bg surfaces, so expect at least 6 failures
        assert!(
            failures.len() >= 6,
            "expected at least 6 failures, got {}",
            failures.len()
        );
    }

    #[test]
    fn hsl_round_trip_is_stable() {
        let colors = [
            Color32::from_rgb(94, 234, 212),  // mint
            Color32::from_rgb(245, 158, 11),  // amber
            Color32::from_rgb(251, 113, 133), // rose
            Color32::from_rgb(100, 100, 100), // mid grey
        ];
        for c in colors {
            let (h, s, l) = to_hsl(c);
            let roundtrip = from_hsl(h, s, l, c);
            for (a, b) in [
                (c.r(), roundtrip.r()),
                (c.g(), roundtrip.g()),
                (c.b(), roundtrip.b()),
            ] {
                assert!(
                    a.abs_diff(b) <= 1,
                    "HSL round-trip off by more than 1: {c:?} -> {roundtrip:?}"
                );
            }
        }
    }
}
