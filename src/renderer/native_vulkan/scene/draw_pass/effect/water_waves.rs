use crate::core::scene::SceneNativeEffectMotion;

pub(super) fn matches(normalized_effect_file: &str) -> bool {
    normalized_effect_file.contains("waterwaves") || normalized_effect_file.contains("water_waves")
}

pub(super) fn max_frequency(motion: SceneNativeEffectMotion) -> f64 {
    let mut frequency: f64 = 0.0;
    if motion.wave_count > 0 {
        frequency = frequency.max(motion.wave_spatial_frequency.abs());
    }
    if motion.wave2_count > 0 {
        frequency = frequency.max(motion.wave2_spatial_frequency.abs());
    }
    frequency
}

pub(super) fn max_amplitude(motion: SceneNativeEffectMotion) -> f64 {
    let mut amplitude: f64 = 0.0;
    if motion.wave_count > 0 {
        amplitude = amplitude.max(motion.wave_x.hypot(motion.wave_y));
    }
    if motion.wave2_count > 0 {
        amplitude = amplitude.max(motion.wave2_x.hypot(motion.wave2_y));
    }
    amplitude
}

pub(super) fn delta(x: f64, y: f64, motion: SceneNativeEffectMotion) -> (f64, f64) {
    let mut dx = 0.0;
    let mut dy = 0.0;
    if motion.wave_count > 0 {
        let (wave_dx, wave_dy) = wave_delta(
            x,
            y,
            motion.wave_x,
            motion.wave_y,
            motion.wave_direction_x,
            motion.wave_direction_y,
            motion.wave_spatial_frequency,
            motion.wave_phase,
        );
        dx += wave_dx;
        dy += wave_dy;
    }
    if motion.wave2_count > 0 {
        let (wave_dx, wave_dy) = wave_delta(
            x + dx,
            y + dy,
            motion.wave2_x,
            motion.wave2_y,
            motion.wave2_direction_x,
            motion.wave2_direction_y,
            motion.wave2_spatial_frequency,
            motion.wave2_phase,
        );
        dx += wave_dx;
        dy += wave_dy;
    }
    (dx, dy)
}

#[allow(clippy::too_many_arguments)]
fn wave_delta(
    x: f64,
    y: f64,
    wave_x: f64,
    wave_y: f64,
    direction_x: f64,
    direction_y: f64,
    spatial_frequency: f64,
    phase: f64,
) -> (f64, f64) {
    let wave = super::motion::fast_sin(
        x.mul_add(
            direction_x * spatial_frequency,
            y * direction_y * spatial_frequency,
        ) + phase,
    );
    (wave_x * wave, wave_y * wave)
}
