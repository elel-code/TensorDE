use tensor_util::{OutputScale, Size};

const MOBILE_TARGET_DPI: f64 = 135.0;
const LARGE_TARGET_DPI: f64 = 110.0;
const LARGE_MIN_SIZE_INCHES: f64 = 20.0;
const MIN_LOGICAL_AREA: f64 = 384_000.0;

/// Choose a representable quarter-step using the Niri/Mutter monitor DPI
/// heuristic. An explicit connector rule bypasses this policy entirely.
pub(super) fn guess_monitor_scale(physical_mm: (i32, i32), resolution: Size) -> OutputScale {
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

    #[test]
    fn unknown_physical_size_uses_one() {
        assert_eq!(
            guess_monitor_scale((0, 0), Size::new(3840, 2160)),
            OutputScale::ONE
        );
    }
}
