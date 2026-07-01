use crate::core::scene::SceneNativeEffectMotion;

pub(super) fn matches(normalized_effect_file: &str) -> bool {
    normalized_effect_file.contains("sway") || normalized_effect_file.contains("shake")
}

pub(super) fn max_frequency(motion: SceneNativeEffectMotion) -> f64 {
    if motion.sway_count > 0 {
        motion.sway_spatial_frequency.abs()
    } else {
        0.0
    }
}

pub(super) fn max_amplitude(motion: SceneNativeEffectMotion) -> f64 {
    if motion.sway_count > 0 {
        motion.sway_amplitude.abs()
    } else {
        0.0
    }
}

pub(super) fn delta(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    motion: SceneNativeEffectMotion,
) -> (f64, f64) {
    if motion.sway_count == 0 || motion.sway_amplitude.abs() <= f64::EPSILON {
        return (0.0, 0.0);
    }
    let vertical = if height.abs() > f64::EPSILON {
        ((y / height) + 0.5).clamp(0.0, 1.0)
    } else {
        0.5
    };
    let horizontal = if width.abs() > f64::EPSILON {
        (x / width).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let direction_length = motion
        .sway_direction_x
        .hypot(motion.sway_direction_y)
        .max(f64::EPSILON);
    let direction_x = if direction_length > f64::EPSILON {
        motion.sway_direction_x / direction_length
    } else {
        1.0
    };
    let direction_y = if direction_length > f64::EPSILON {
        motion.sway_direction_y / direction_length
    } else {
        0.0
    };
    let tip_weight = vertical.powf(motion.sway_power.max(1.0));
    let sway = super::motion::fast_sin(y * motion.sway_spatial_frequency + motion.sway_phase)
        * motion.sway_amplitude
        * tip_weight;
    (
        direction_x * sway,
        direction_y * sway + sway * horizontal * 0.12,
    )
}
