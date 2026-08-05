use thiserror::Error;

use crate::scene::{FocusRingStyle, LinearRgba16, WindowCornerStyle, WindowShadowStyle};

const MAX_FOCUS_RING_WIDTH: u32 = 100_000;
const MAX_SHADOW_EXTENT: u32 = 100_000;
const MAX_CORNER_RADIUS: u32 = 100_000;

/// Resolve typed KDL appearance values into renderer-independent scene policy.
pub(super) fn resolve_focus_ring(
    enabled: Option<bool>,
    width: Option<u32>,
    color: Option<String>,
) -> Result<FocusRingStyle, AppearanceConfigError> {
    let defaults = FocusRingStyle::default();
    let width = width.unwrap_or(defaults.width);
    if width > MAX_FOCUS_RING_WIDTH {
        return Err(AppearanceConfigError::FocusRingWidth { width });
    }
    let color = match color {
        Some(value) => parse_color(&value)
            .map_err(|message| AppearanceConfigError::FocusRingColor { value, message })?,
        None => defaults.color,
    };
    Ok(FocusRingStyle {
        enabled: enabled.unwrap_or(defaults.enabled),
        width,
        color,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_window_shadow(
    enabled: Option<bool>,
    offset_x: Option<i32>,
    offset_y: Option<i32>,
    blur_radius: Option<u32>,
    spread: Option<u32>,
    color: Option<String>,
) -> Result<WindowShadowStyle, AppearanceConfigError> {
    let defaults = WindowShadowStyle::default();
    let offset_x = offset_x.unwrap_or(defaults.offset_x);
    let offset_y = offset_y.unwrap_or(defaults.offset_y);
    for (axis, value) in [("x", offset_x), ("y", offset_y)] {
        if value.unsigned_abs() > MAX_SHADOW_EXTENT {
            return Err(AppearanceConfigError::ShadowOffset { axis, value });
        }
    }
    let blur_radius = blur_radius.unwrap_or(defaults.blur_radius);
    if blur_radius > MAX_SHADOW_EXTENT {
        return Err(AppearanceConfigError::ShadowBlurRadius { blur_radius });
    }
    let spread = spread.unwrap_or(defaults.spread);
    if spread > MAX_SHADOW_EXTENT {
        return Err(AppearanceConfigError::ShadowSpread { spread });
    }
    let color = match color {
        Some(value) => parse_color(&value)
            .map_err(|message| AppearanceConfigError::ShadowColor { value, message })?,
        None => defaults.color,
    };
    Ok(WindowShadowStyle {
        enabled: enabled.unwrap_or(defaults.enabled),
        offset_x,
        offset_y,
        blur_radius,
        spread,
        color,
    })
}

pub(super) fn resolve_window_corners(
    radius: Option<u32>,
) -> Result<WindowCornerStyle, AppearanceConfigError> {
    let radius = radius.unwrap_or_default();
    if radius > MAX_CORNER_RADIUS {
        return Err(AppearanceConfigError::WindowCornerRadius { radius });
    }
    Ok(WindowCornerStyle { radius })
}

/// Parse CSS-style opaque or alpha-bearing color literals without carrying a
/// GUI toolkit parser into the configuration boundary. Scene values use
/// canonical 16-bit scene channels, so each specified byte expands exactly.
fn parse_color(value: &str) -> Result<LinearRgba16, String> {
    let digits = value
        .strip_prefix('#')
        .ok_or_else(|| "must start with `#`".to_owned())?
        .as_bytes();
    if !matches!(digits.len(), 6 | 8) {
        return Err("must use `#RRGGBB` or `#RRGGBBAA`".to_owned());
    }
    let red = parse_channel(digits, 0)?;
    let green = parse_channel(digits, 2)?;
    let blue = parse_channel(digits, 4)?;
    let alpha = if digits.len() == 8 {
        parse_channel(digits, 6)?
    } else {
        u16::MAX
    };
    Ok(LinearRgba16::new(red, green, blue, alpha))
}

fn parse_channel(digits: &[u8], offset: usize) -> Result<u16, String> {
    let high = hex_digit(digits[offset])?;
    let low = hex_digit(digits[offset + 1])?;
    Ok(u16::from((high << 4) | low) * 257)
}

fn hex_digit(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err("contains a non-hexadecimal digit".to_owned()),
    }
}

#[derive(Debug, Error)]
pub enum AppearanceConfigError {
    #[error("focus-ring width {width} exceeds {MAX_FOCUS_RING_WIDTH} logical pixels")]
    FocusRingWidth { width: u32 },
    #[error("invalid focus-ring color {value:?}: {message}")]
    FocusRingColor { value: String, message: String },
    #[error("window-shadow {axis} offset {value} exceeds ±{MAX_SHADOW_EXTENT} logical pixels")]
    ShadowOffset { axis: &'static str, value: i32 },
    #[error("window-shadow blur radius {blur_radius} exceeds {MAX_SHADOW_EXTENT} logical pixels")]
    ShadowBlurRadius { blur_radius: u32 },
    #[error("window-shadow spread {spread} exceeds {MAX_SHADOW_EXTENT} logical pixels")]
    ShadowSpread { spread: u32 },
    #[error("invalid window-shadow color {value:?}: {message}")]
    ShadowColor { value: String, message: String },
    #[error("window-corner-radius {radius} exceeds {MAX_CORNER_RADIUS} logical pixels")]
    WindowCornerRadius { radius: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_focus_ring_overrides_without_losing_defaults() {
        assert_eq!(
            resolve_focus_ring(None, Some(7), Some("#1a2B3c80".to_owned())).unwrap(),
            FocusRingStyle {
                enabled: true,
                width: 7,
                color: LinearRgba16::new(0x1a1a, 0x2b2b, 0x3c3c, 0x8080),
            }
        );
    }

    #[test]
    fn disabled_focus_ring_resolves_to_no_outline() {
        assert_eq!(
            resolve_focus_ring(Some(false), None, None)
                .unwrap()
                .outline(),
            None
        );
    }

    #[test]
    fn rejects_malformed_focus_ring_color_and_unbounded_width() {
        assert!(matches!(
            resolve_focus_ring(None, None, Some("blue".to_owned())),
            Err(AppearanceConfigError::FocusRingColor { .. })
        ));

        assert!(matches!(
            resolve_focus_ring(None, Some(100_001), None),
            Err(AppearanceConfigError::FocusRingWidth { .. })
        ));
    }

    #[test]
    fn resolves_and_bounds_window_shadow_policy() {
        let shadow = resolve_window_shadow(
            Some(true),
            Some(-4),
            Some(8),
            Some(20),
            Some(2),
            Some("#10203080".to_owned()),
        )
        .unwrap();
        assert_eq!(
            shadow.effect(),
            Some(crate::scene::ShadowStyle {
                offset_x: -4,
                offset_y: 8,
                blur_radius: 20,
                spread: 2,
                color: LinearRgba16::new(0x1010, 0x2020, 0x3030, 0x8080),
            })
        );
        assert!(matches!(
            resolve_window_shadow(None, None, None, Some(100_001), None, None),
            Err(AppearanceConfigError::ShadowBlurRadius { .. })
        ));
    }

    #[test]
    fn window_corner_radius_is_bounded_before_scene_extraction() {
        assert_eq!(
            resolve_window_corners(Some(12)).unwrap(),
            WindowCornerStyle { radius: 12 }
        );
        assert!(matches!(
            resolve_window_corners(Some(100_001)),
            Err(AppearanceConfigError::WindowCornerRadius { .. })
        ));
    }
}
