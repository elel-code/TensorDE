use super::{
    SceneBlendMode, SceneEffect, SceneEffectPass, SceneNativeEffectMotion, SceneNodeKind,
    ScenePathFillRule, SceneSnapshotLayer, SceneTransform,
};
use crate::core::manifest::FitMode;
use serde_json::Value;

pub(super) fn push_native_effect_snapshot_layers(
    time_ms: u64,
    effects: &[SceneEffect],
    base: &SceneSnapshotLayer,
    output: &mut Vec<SceneSnapshotLayer>,
) {
    for (effect_index, effect) in effects.iter().enumerate() {
        if effect.runtime.as_deref() == Some("native-text-glow")
            && base.kind == SceneNodeKind::Text
            && base.text.as_deref().is_some_and(|text| !text.is_empty())
        {
            push_native_text_glow_snapshot_layers(effect_index, effect, base, output);
        }
        let file = effect.file.to_ascii_lowercase();
        if file.contains("lightshafts") {
            push_native_lightshaft_snapshot_layers(effect_index, effect, base, time_ms, output);
        } else if file.contains("watercaustics") {
            push_native_water_caustics_snapshot_layers(effect_index, effect, base, time_ms, output);
        } else if file.contains("enhanced_simple_audio_bars") {
            push_native_audio_bar_snapshot_layers(effect_index, effect, base, time_ms, output);
        } else if file.contains("tech_circle") {
            push_native_tech_circle_snapshot_layers(effect_index, effect, base, time_ms, output);
        }
    }
}

fn push_native_water_caustics_snapshot_layers(
    effect_index: usize,
    effect: &SceneEffect,
    base: &SceneSnapshotLayer,
    time_ms: u64,
    output: &mut Vec<SceneSnapshotLayer>,
) {
    let Some((width, height)) = base.width.zip(base.height) else {
        return;
    };
    if width <= 0.0 || height <= 0.0 || base.opacity <= 0.0 {
        return;
    };
    let pass = effect.passes.first();
    let color = pass
        .and_then(|pass| {
            scene_effect_pass_color(
                pass,
                &[
                    "ui_editor_properties_color_start",
                    "ui_editor_properties_color_end",
                    "color",
                ],
            )
        })
        .unwrap_or_else(|| "#4fcfff".to_owned());
    let brightness = pass
        .map(|pass| scene_effect_pass_f64(pass, &["ui_editor_properties_brightness"], 1.0))
        .unwrap_or(1.0)
        .clamp(0.0, 4.0);
    let speed = pass
        .map(|pass| scene_effect_pass_f64(pass, &["ui_editor_properties_speed", "speed"], 0.25))
        .unwrap_or(0.25);
    let distortion = pass
        .map(|pass| scene_effect_pass_f64(pass, &["ui_editor_properties_distortion"], 1.0))
        .unwrap_or(1.0)
        .abs()
        .clamp(0.0, 4.0);
    let time = time_ms as f64 / 1000.0;
    let phase = time * speed * std::f64::consts::TAU + effect.id.unwrap_or_default() as f64 * 0.11;
    let base_opacity = (0.045 + brightness * 0.035).clamp(0.035, 0.18) * base.opacity;
    for index in 0..5 {
        let t = index as f64 / 4.0;
        let wave = (phase + index as f64 * 1.37).sin();
        let cross = (phase * 0.73 + index as f64 * 0.91).cos();
        let transform = base.transform.compose(SceneTransform {
            x: (t - 0.5) * width * 0.72 + wave * width * 0.025 * distortion,
            y: cross * height * 0.08,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: -24.0 + index as f64 * 12.0 + wave * 3.0,
            anchor_x: 0.5,
            anchor_y: 0.5,
        });
        output.push(scene_native_effect_visual_layer(
            format!("{}::water-caustics-{effect_index}-{index}", base.id),
            SceneNodeKind::Rectangle,
            Some(width * (0.28 + t * 0.08)),
            Some((height * 0.09).max(8.0)),
            Some(color.clone()),
            None,
            None,
            base_opacity * (1.0 - t * 0.25),
            transform,
            base.fit,
        ));
    }
}

fn push_native_lightshaft_snapshot_layers(
    effect_index: usize,
    effect: &SceneEffect,
    base: &SceneSnapshotLayer,
    time_ms: u64,
    output: &mut Vec<SceneSnapshotLayer>,
) {
    let Some((width, height)) = base.width.zip(base.height) else {
        return;
    };
    if width <= 0.0 || height <= 0.0 || base.opacity <= 0.0 {
        return;
    }
    let pass = effect.passes.first();
    let color = pass
        .and_then(|pass| scene_effect_pass_color(pass, &["colorend", "color"]))
        .unwrap_or_else(|| "#6fe2ff".to_owned());
    let speed = pass
        .map(|pass| scene_effect_pass_f64(pass, &["rayspeed", "speed"], 0.5))
        .unwrap_or(0.5);
    let phase = (time_ms as f64 / 1000.0 * speed * std::f64::consts::TAU).sin();
    for index in 0..3 {
        let t = index as f64 / 2.0;
        let x = (-0.2 + t * 0.55 + phase * 0.015) * width;
        let y = (-0.38 + t * 0.12) * height;
        let mut transform = SceneTransform {
            x,
            y,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: -18.0 + index as f64 * 7.0,
            anchor_x: 0.5,
            anchor_y: 0.0,
        };
        transform = base.transform.compose(transform);
        output.push(scene_native_effect_visual_layer(
            format!("{}::lightshaft-{effect_index}-{index}", base.id),
            SceneNodeKind::Rectangle,
            Some(width * (0.08 + t * 0.04)),
            Some(height * 0.92),
            Some(color.clone()),
            None,
            None,
            base.opacity * (0.18 + t * 0.08),
            transform,
            base.fit,
        ));
    }
}

fn push_native_audio_bar_snapshot_layers(
    effect_index: usize,
    effect: &SceneEffect,
    base: &SceneSnapshotLayer,
    time_ms: u64,
    output: &mut Vec<SceneSnapshotLayer>,
) {
    let Some((width, height)) = base.width.zip(base.height) else {
        return;
    };
    if width <= 0.0 || height <= 0.0 || base.opacity <= 0.0 {
        return;
    }
    let pass = effect.passes.first();
    let count = pass
        .map(|pass| scene_effect_pass_f64(pass, &["Bar Count", "bar_count", "bars"], 12.0))
        .unwrap_or(12.0)
        .round()
        .clamp(1.0, 48.0) as usize;
    let color = pass
        .and_then(|pass| scene_effect_pass_color(pass, &["Bar Color", "bar_color", "color"]))
        .unwrap_or_else(|| "#ffffff".to_owned());
    let spacing = pass
        .map(|pass| scene_effect_pass_f64(pass, &["Bar Spacing", "bar_spacing"], 0.25))
        .unwrap_or(0.25)
        .clamp(0.0, 2.0);
    let slot = width / count as f64;
    let bar_width = (slot / (1.0 + spacing)).max(1.0);
    let time = time_ms as f64 / 1000.0;
    for index in 0..count {
        let wave = (time.mul_add(5.0, index as f64 * 0.73)).sin().abs();
        let bar_height = height * (0.18 + wave * 0.62);
        let x = -width * 0.5 + slot * (index as f64 + 0.5);
        let y = height * 0.5 - bar_height * 0.5;
        let transform = base.transform.compose(SceneTransform {
            x,
            y,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: 0.0,
            anchor_x: 0.5,
            anchor_y: 0.5,
        });
        output.push(scene_native_effect_visual_layer(
            format!("{}::audio-bars-{effect_index}-{index}", base.id),
            SceneNodeKind::Rectangle,
            Some(bar_width),
            Some(bar_height),
            Some(color.clone()),
            None,
            None,
            base.opacity * 0.9,
            transform,
            base.fit,
        ));
    }
}

fn push_native_tech_circle_snapshot_layers(
    effect_index: usize,
    effect: &SceneEffect,
    base: &SceneSnapshotLayer,
    time_ms: u64,
    output: &mut Vec<SceneSnapshotLayer>,
) {
    let Some((width, height)) = base.width.zip(base.height) else {
        return;
    };
    let size = width.abs().min(height.abs());
    if size <= 0.0 || base.opacity <= 0.0 {
        return;
    }
    let pass = effect.passes.first();
    let color = pass
        .and_then(|pass| scene_effect_pass_color(pass, &["ui_editor_properties_1_color", "color"]))
        .unwrap_or_else(|| "#ffffff".to_owned());
    let speed = pass
        .map(|pass| scene_effect_pass_f64(pass, &["ui_editor_properties_3_speed", "speed"], 0.1))
        .unwrap_or(0.1);
    let rotation = time_ms as f64 / 1000.0 * speed * 360.0;
    for index in 0..2 {
        let diameter = size * (0.48 + index as f64 * 0.22);
        let transform = base.transform.compose(SceneTransform {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: rotation * if index == 0 { 1.0 } else { -0.65 },
            anchor_x: 0.5,
            anchor_y: 0.5,
        });
        output.push(scene_native_effect_visual_layer(
            format!("{}::tech-circle-{effect_index}-{index}", base.id),
            SceneNodeKind::Ellipse,
            Some(diameter),
            Some(diameter),
            None,
            Some(color.clone()),
            Some((size * 0.012).max(1.0)),
            base.opacity * 0.75,
            transform,
            base.fit,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn scene_native_effect_visual_layer(
    id: String,
    kind: SceneNodeKind,
    width: Option<f64>,
    height: Option<f64>,
    color: Option<String>,
    stroke_color: Option<String>,
    stroke_width: Option<f64>,
    opacity: f64,
    transform: SceneTransform,
    fit: FitMode,
) -> SceneSnapshotLayer {
    SceneSnapshotLayer {
        id,
        kind,
        source: None,
        texture_region: None,
        effect_motion: SceneNativeEffectMotion::default(),
        blend_mode: SceneBlendMode::Alpha,
        audio: Vec::new(),
        color,
        stroke_color,
        stroke_width,
        corner_radius: None,
        width,
        height,
        mesh: None,
        parallax_depth: None,
        text: None,
        font_size: None,
        font_family: None,
        font_source: None,
        font_weight: None,
        text_align: None,
        path_data: None,
        path_fill_rule: ScenePathFillRule::default(),
        fit,
        opacity: opacity.clamp(0.0, 1.0),
        transform,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SceneNativeEffectAdjustment {
    translate_x: f64,
    translate_y: f64,
    rotation_deg: f64,
    opacity_multiplier: f64,
    pub(super) motion: SceneNativeEffectMotion,
}

impl Default for SceneNativeEffectAdjustment {
    fn default() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            rotation_deg: 0.0,
            opacity_multiplier: 1.0,
            motion: SceneNativeEffectMotion::default(),
        }
    }
}

impl SceneNativeEffectAdjustment {
    pub(super) fn apply_transform(self, mut transform: SceneTransform) -> SceneTransform {
        transform.x += self.translate_x;
        transform.y += self.translate_y;
        transform.rotation_deg += self.rotation_deg;
        transform
    }

    pub(super) fn apply_opacity(self, opacity: f64) -> f64 {
        (opacity * self.opacity_multiplier).clamp(0.0, 1.0)
    }
}

pub(super) fn scene_native_effect_adjustment_at(
    effects: &[SceneEffect],
    width: Option<f64>,
    height: Option<f64>,
    time_ms: u64,
) -> SceneNativeEffectAdjustment {
    let mut adjustment = SceneNativeEffectAdjustment::default();
    let extent = width
        .zip(height)
        .map(|(width, height)| width.abs().min(height.abs()))
        .filter(|extent| extent.is_finite() && *extent > 0.0)
        .unwrap_or(1024.0);
    let time_seconds = time_ms as f64 / 1000.0;
    for effect in effects {
        if !scene_effect_is_visible(effect) {
            continue;
        }
        let file = effect.file.to_ascii_lowercase();
        if file.contains("opacity") {
            for pass in &effect.passes {
                adjustment.opacity_multiplier *=
                    scene_effect_pass_f64(pass, &["alpha", "opacity"], 1.0).clamp(0.0, 1.0);
            }
        }
        let native_motion = file.contains("waterwaves")
            || file.contains("waterripple")
            || file.contains("waterflow")
            || file.contains("watercaustics")
            || file.contains("cloudmotion")
            || file.contains("foliagesway")
            || file.contains("auto_sway")
            || file.contains("shake")
            || file.contains("skew");
        if !native_motion {
            continue;
        }
        let phase_seed = effect.id.unwrap_or_default() as f64 * 0.017;
        for pass in &effect.passes {
            let speed = scene_effect_pass_f64(
                pass,
                &[
                    "speed",
                    "animationspeed",
                    "scrollspeed",
                    "speeduv",
                    "speed_uv",
                ],
                1.0,
            );
            let strength = scene_effect_pass_f64(
                pass,
                &["strength", "ripplestrength", "ripple_strength", "power"],
                0.0,
            )
            .abs();
            if strength <= 0.0 {
                continue;
            }
            let direction = scene_effect_pass_f64(pass, &["direction", "scrolldirection"], 0.0);
            let phase_radians = time_seconds.mul_add(speed.max(0.0), phase_seed);
            let (wave, cross_wave) = phase_radians.sin_cos();
            let amplitude = (extent * strength * 0.012).clamp(0.0, 18.0);
            let (direction_sin, direction_cos) = direction.sin_cos();
            if file.contains("shake") {
                adjustment.translate_x += direction_cos * wave * amplitude;
                adjustment.translate_y += direction_sin * cross_wave * amplitude;
            }
            let scale = scene_effect_pass_f64(pass, &["scale", "scale1"], 8.0)
                .abs()
                .max(0.001);
            let spatial_period = (extent / scale).clamp(8.0, extent.max(8.0));
            adjustment.motion.wave_x += direction_cos * amplitude * 0.75;
            adjustment.motion.wave_y += direction_sin * amplitude * 0.75;
            adjustment.motion.wave_direction_x += direction_cos;
            adjustment.motion.wave_direction_y += direction_sin;
            adjustment.motion.wave_spatial_frequency += std::f64::consts::TAU / spatial_period;
            adjustment.motion.wave_phase += phase_radians + phase_seed;
            adjustment.motion.wave_count = adjustment.motion.wave_count.saturating_add(1);
            if file.contains("auto_sway") || file.contains("foliagesway") || file.contains("shake")
            {
                adjustment.rotation_deg += wave * strength.min(1.0) * 0.35;
                adjustment.motion.sway_amplitude += (extent * strength * 0.02).clamp(0.0, 16.0);
                adjustment.motion.sway_phase += phase_radians;
                adjustment.motion.sway_spatial_frequency +=
                    std::f64::consts::TAU / (extent * 0.75).clamp(8.0, extent.max(8.0));
            }
        }
    }
    adjustment.motion.normalize();
    adjustment
}

fn scene_effect_is_visible(effect: &SceneEffect) -> bool {
    match &effect.visible {
        Some(Value::Bool(value)) => *value,
        Some(Value::Object(object)) => object.get("value").and_then(Value::as_bool).unwrap_or(true),
        _ => true,
    }
}

fn scene_effect_pass_f64(pass: &SceneEffectPass, keys: &[&str], fallback: f64) -> f64 {
    keys.iter()
        .find_map(|key| {
            pass.constant_shader_values
                .get(*key)
                .and_then(scene_effect_value_f64)
        })
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

fn scene_effect_pass_color(pass: &SceneEffectPass, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        pass.constant_shader_values
            .get(*key)
            .and_then(scene_effect_value_color)
    })
}

fn scene_effect_value_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.trim().parse().ok(),
        Value::Object(object) => object.get("value").and_then(scene_effect_value_f64),
        _ => None,
    }
}

pub(super) fn scene_effect_value_color(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => scene_effect_color_string(value),
        Value::Object(object) => object.get("value").and_then(scene_effect_value_color),
        Value::Array(values) => {
            let r = values.first().and_then(scene_effect_value_f64)?;
            let g = values.get(1).and_then(scene_effect_value_f64)?;
            let b = values.get(2).and_then(scene_effect_value_f64)?;
            Some(scene_effect_rgb_hex(r, g, b))
        }
        Value::Number(_) | Value::Bool(_) | Value::Null => None,
    }
}

pub(super) fn scene_effect_color_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with('#') && (value.len() == 7 || value.len() == 9) {
        return Some(value[..7].to_owned());
    }
    let mut components = value
        .split_ascii_whitespace()
        .filter_map(|component| component.parse::<f64>().ok());
    let r = components.next()?;
    let g = components.next()?;
    let b = components.next()?;
    Some(scene_effect_rgb_hex(r, g, b))
}

fn scene_effect_rgb_hex(r: f64, g: f64, b: f64) -> String {
    fn byte(value: f64) -> u8 {
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    }
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

fn push_native_text_glow_snapshot_layers(
    effect_index: usize,
    effect: &SceneEffect,
    base: &SceneSnapshotLayer,
    output: &mut Vec<SceneSnapshotLayer>,
) {
    let radius = scene_effect_property_f64(effect, "radius", 2.0).clamp(0.25, 16.0);
    let opacity =
        (base.opacity * scene_effect_property_f64(effect, "opacity", 0.12)).clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let samples = effect
        .properties
        .get("samples")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(8)
        .clamp(4, NATIVE_TEXT_GLOW_OFFSETS.len());
    for (sample_index, [x, y]) in NATIVE_TEXT_GLOW_OFFSETS.iter().take(samples).enumerate() {
        let mut layer = base.clone();
        layer.id = format!(
            "{}::native-text-glow-{}-{}",
            base.id, effect_index, sample_index
        );
        layer.opacity = opacity;
        layer.transform.x += x * radius;
        layer.transform.y += y * radius;
        output.push(layer);
    }
}

const NATIVE_TEXT_GLOW_OFFSETS: [[f64; 2]; 8] = [
    [-1.0, 0.0],
    [1.0, 0.0],
    [0.0, -1.0],
    [0.0, 1.0],
    [-0.70710678118, -0.70710678118],
    [0.70710678118, -0.70710678118],
    [-0.70710678118, 0.70710678118],
    [0.70710678118, 0.70710678118],
];

fn scene_effect_property_f64(effect: &SceneEffect, key: &str, default: f64) -> f64 {
    effect
        .properties
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(default)
}
