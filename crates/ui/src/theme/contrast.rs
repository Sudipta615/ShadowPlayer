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

/// Validate WCAG contrast for all text and input border tokens from a colour map.
pub fn validate_contrast(colors: &HashMap<String, Color32>) -> Result<(), ContrastError> {
    let text_tokens = [
        "text_primary",
        "text_secondary",
        "text_muted",
        "text_dim",
        "accent_highlight",
        "accent_warm",
    ];
    let bg_tokens = ["bg_app", "bg_card", "bg_sidebar"];

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
                return Err(ContrastError::ContrastFailure {
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
            return Err(ContrastError::ContrastFailure {
                fg: "border_input".to_string(),
                bg: "bg_input".to_string(),
                ratio,
                required: 3.0,
            });
        }
    }

    Ok(())
}

/// Automatically adjust a foreground colour's lightness so it meets the target contrast
/// ratio against the specified background colour.
pub fn auto_adjust_contrast(fg: Color32, bg: Color32, target_ratio: f32) -> Color32 {
    let current_ratio = contrast_ratio(fg, bg);
    if current_ratio >= target_ratio {
        return fg;
    }

    let bg_lum = luminance(bg);
    let should_lighten = bg_lum < 0.5;

    let mut r = fg.r() as f32;
    let mut g = fg.g() as f32;
    let mut b = fg.b() as f32;

    for _ in 0..100 {
        if should_lighten {
            r = (r + 2.5).min(255.0);
            g = (g + 2.5).min(255.0);
            b = (b + 2.5).min(255.0);
        } else {
            r = (r - 2.5).max(0.0);
            g = (g - 2.5).max(0.0);
            b = (b - 2.5).max(0.0);
        }

        let candidate = Color32::from_rgb(r.round() as u8, g.round() as u8, b.round() as u8);
        if contrast_ratio(candidate, bg) >= target_ratio {
            return candidate;
        }

        if (should_lighten && r >= 255.0 && g >= 255.0 && b >= 255.0)
            || (!should_lighten && r <= 0.0 && g <= 0.0 && b <= 0.0)
        {
            return candidate;
        }
    }

    Color32::from_rgb(r.round() as u8, g.round() as u8, b.round() as u8)
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
}
