use thiserror::Error;

use crate::scene::{FocusRingStyle, LinearRgba16, SceneAppearance};

const MAX_FOCUS_RING_WIDTH: u32 = 100_000;

/// KDL appearance boundary. It resolves partial user intent into the
/// renderer-independent scene policy consumed by ECS extraction.
#[derive(Debug, knus::Decode)]
pub(super) struct AppearanceFileConfig {
    #[knus(child)]
    focus_ring: Option<FocusRingFileConfig>,
}

impl AppearanceFileConfig {
    pub(super) fn resolve(self) -> Result<SceneAppearance, AppearanceConfigError> {
        let defaults = SceneAppearance::default();
        let focus_ring = self
            .focus_ring
            .map(|configured| configured.resolve(defaults.focus_ring))
            .transpose()?
            .unwrap_or(defaults.focus_ring);
        Ok(SceneAppearance { focus_ring })
    }
}

#[derive(Debug, knus::Decode)]
struct FocusRingFileConfig {
    #[knus(child, unwrap(argument))]
    enabled: Option<bool>,
    #[knus(child, unwrap(argument))]
    width: Option<u32>,
    #[knus(child, unwrap(argument))]
    color: Option<String>,
}

impl FocusRingFileConfig {
    fn resolve(self, defaults: FocusRingStyle) -> Result<FocusRingStyle, AppearanceConfigError> {
        let width = self.width.unwrap_or(defaults.width);
        if width > MAX_FOCUS_RING_WIDTH {
            return Err(AppearanceConfigError::InvalidFocusRingWidth { width });
        }
        let color = match self.color {
            Some(value) => parse_color(&value).map_err(|message| {
                AppearanceConfigError::InvalidFocusRingColor { value, message }
            })?,
            None => defaults.color,
        };
        Ok(FocusRingStyle {
            enabled: self.enabled.unwrap_or(defaults.enabled),
            width,
            color,
        })
    }
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
    InvalidFocusRingWidth { width: u32 },
    #[error("invalid focus-ring color {value:?}: {message}")]
    InvalidFocusRingColor { value: String, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_focus_ring_overrides_without_losing_defaults() {
        let parsed: AppearanceFileConfig = knus::parse(
            "appearance.kdl",
            "focus-ring {\n  width 7\n  color \"#1a2B3c80\"\n}",
        )
        .unwrap();

        assert_eq!(
            parsed.resolve().unwrap().focus_ring,
            FocusRingStyle {
                enabled: true,
                width: 7,
                color: LinearRgba16::new(0x1a1a, 0x2b2b, 0x3c3c, 0x8080),
            }
        );
    }

    #[test]
    fn disabled_focus_ring_resolves_to_no_outline() {
        let parsed: AppearanceFileConfig =
            knus::parse("appearance.kdl", "focus-ring { enabled false\n}").unwrap();

        assert_eq!(parsed.resolve().unwrap().focus_ring.outline(), None);
    }

    #[test]
    fn rejects_malformed_focus_ring_color_and_unbounded_width() {
        let malformed: AppearanceFileConfig =
            knus::parse("appearance.kdl", "focus-ring { color \"blue\"\n}").unwrap();
        assert!(matches!(
            malformed.resolve(),
            Err(AppearanceConfigError::InvalidFocusRingColor { .. })
        ));

        let oversized: AppearanceFileConfig =
            knus::parse("appearance.kdl", "focus-ring { width 100001\n}").unwrap();
        assert!(matches!(
            oversized.resolve(),
            Err(AppearanceConfigError::InvalidFocusRingWidth { .. })
        ));
    }
}
