//! Named output rules and mode selection (pure functions).

use std::collections::BTreeMap;

use tensor_host::PhysicalMode;
use tensor_util::{OutputScale, Size};

/// Requested mode from configuration (logical width/height + optional refresh).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputModeRequest {
    pub width: u32,
    pub height: u32,
    pub refresh_millihertz: Option<u32>,
}

/// Per-connector policy row (mirrors compositor config without knus/toml).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputRule {
    pub scale: Option<OutputScale>,
    pub mode: Option<OutputModeRequest>,
    pub position: Option<(i32, i32)>,
    pub enabled: bool,
    pub max_refresh_millihertz: Option<u32>,
}

impl Default for OutputRule {
    fn default() -> Self {
        Self {
            scale: None,
            mode: None,
            position: None,
            enabled: true,
            max_refresh_millihertz: None,
        }
    }
}

/// Name → rule table used by planning.
#[derive(Clone, Debug, Default)]
pub struct OutputRuleTable {
    rules: BTreeMap<String, OutputRule>,
}

impl OutputRuleTable {
    pub fn new(rules: BTreeMap<String, OutputRule>) -> Self {
        Self { rules }
    }

    pub fn rules(&self) -> &BTreeMap<String, OutputRule> {
        &self.rules
    }

    pub fn upsert(&mut self, name: impl Into<String>, rule: OutputRule) {
        self.rules.insert(name.into(), rule);
    }

    pub fn get(&self, name: &str) -> Option<&OutputRule> {
        self.rules.get(name)
    }
}

/// Select a mode from the connector's list given a preferred mode and rule.
pub fn select_mode(
    modes: &[PhysicalMode],
    preferred: PhysicalMode,
    rule: Option<&OutputRule>,
) -> Option<PhysicalMode> {
    if let Some(requested) = rule.and_then(|r| r.mode)
        && let Some(mode) = select_requested_mode(modes, requested)
    {
        return Some(mode);
    }
    let selected = highest_refresh_at_size(modes, preferred.size(), None).or(Some(preferred));
    let Some(cap) = rule.and_then(|r| r.max_refresh_millihertz) else {
        return selected;
    };
    highest_refresh_at_size(modes, preferred.size(), Some(cap)).or(selected)
}

pub fn select_requested_mode(
    modes: &[PhysicalMode],
    requested: OutputModeRequest,
) -> Option<PhysicalMode> {
    let width = i32::try_from(requested.width).ok()?;
    let height = i32::try_from(requested.height).ok()?;
    let mut matching = modes
        .iter()
        .copied()
        .filter(|mode| mode.width == width && mode.height == height);
    match requested.refresh_millihertz {
        Some(refresh) => i32::try_from(refresh)
            .ok()
            .and_then(|refresh| matching.find(|mode| mode.refresh_millihertz == refresh)),
        None => matching.max_by_key(|mode| mode.refresh_millihertz),
    }
}

pub fn highest_refresh_at_size(
    modes: &[PhysicalMode],
    size: (i32, i32),
    max_refresh_millihertz: Option<u32>,
) -> Option<PhysicalMode> {
    let max_refresh = max_refresh_millihertz.and_then(|value| i32::try_from(value).ok());
    modes
        .iter()
        .copied()
        .filter(|mode| mode.size() == size)
        .filter(|mode| max_refresh.is_none_or(|cap| mode.refresh_millihertz <= cap))
        .max_by_key(|mode| mode.refresh_millihertz)
}

const MOBILE_TARGET_DPI: f64 = 135.0;
const LARGE_TARGET_DPI: f64 = 110.0;
const LARGE_MIN_SIZE_INCHES: f64 = 20.0;
const MIN_LOGICAL_AREA: f64 = 384_000.0;

/// Niri/Mutter-style quarter-step scale from physical mm + resolution.
pub fn guess_monitor_scale(physical_mm: (i32, i32), resolution: Size) -> OutputScale {
    let (width_mm, height_mm) = physical_mm;
    if width_mm <= 0 || height_mm <= 0 {
        return OutputScale::ONE;
    }

    let diagonal_mm = f64::from(width_mm).hypot(f64::from(height_mm));
    let diagonal_inches = diagonal_mm / 25.4;
    let target_dpi = if diagonal_inches < LARGE_MIN_SIZE_INCHES {
        MOBILE_TARGET_DPI
    } else {
        LARGE_TARGET_DPI
    };
    let physical_diagonal = f64::from(resolution.width).hypot(f64::from(resolution.height));
    let ideal = physical_diagonal / diagonal_inches / target_dpi;

    (120..=480)
        .step_by(30)
        .filter_map(OutputScale::from_units)
        .filter(|scale| {
            let width = (f64::from(resolution.width) / scale.as_f64()).round();
            let height = (f64::from(resolution.height) / scale.as_f64()).round();
            width * height >= MIN_LOGICAL_AREA
        })
        .min_by(|left, right| {
            (left.as_f64() - ideal)
                .abs()
                .total_cmp(&(right.as_f64() - ideal).abs())
        })
        .unwrap_or(OutputScale::ONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(w: i32, h: i32, r: i32) -> PhysicalMode {
        PhysicalMode::new(w, h, r)
    }

    #[test]
    fn highest_refresh_prefers_faster_same_size() {
        let modes = [
            mode(1920, 1080, 60_000),
            mode(1920, 1080, 144_000),
            mode(2560, 1440, 60_000),
        ];
        assert_eq!(
            highest_refresh_at_size(&modes, (1920, 1080), None),
            Some(mode(1920, 1080, 144_000))
        );
    }

    #[test]
    fn max_refresh_cap_is_honored() {
        let modes = [mode(1920, 1080, 60_000), mode(1920, 1080, 144_000)];
        assert_eq!(
            highest_refresh_at_size(&modes, (1920, 1080), Some(60_000)),
            Some(mode(1920, 1080, 60_000))
        );
    }

    #[test]
    fn requested_mode_without_refresh_picks_highest() {
        let modes = [mode(2560, 1440, 60_000), mode(2560, 1440, 165_000)];
        let req = OutputModeRequest {
            width: 2560,
            height: 1440,
            refresh_millihertz: None,
        };
        assert_eq!(
            select_requested_mode(&modes, req),
            Some(mode(2560, 1440, 165_000))
        );
    }

    #[test]
    fn monitor_scale_matches_reference_dpi_cases() {
        assert_eq!(
            guess_monitor_scale((509, 286), Size::new(1920, 1080)),
            OutputScale::ONE
        );
        assert_eq!(
            guess_monitor_scale((598, 336), Size::new(3840, 2160)),
            OutputScale::from_f64(1.5).unwrap()
        );
        assert_eq!(
            guess_monitor_scale((286, 179), Size::new(2560, 1600)),
            OutputScale::from_f64(1.75).unwrap()
        );
    }
}
