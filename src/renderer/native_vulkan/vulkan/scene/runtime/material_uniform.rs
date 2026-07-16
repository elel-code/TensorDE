//! Scene material/effect constant packing for the Vulkanalia scene runtime.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`

use std::mem::size_of;
use std::sync::OnceLock;

use serde_json::Value;

mod final_effect;
mod particle;

use final_effect::{
    final_effect_program_values, final_waterripple_values, final_waterwaves_values,
    material_texture_resolution, object_source_texture_resolution, ripple_flow_composite_values,
};
#[cfg(test)]
use final_effect::final_audio_bars_values;

use crate::engine::scene::{
    INVALID_MATERIAL_ID, SceneAudioBandMaterialTarget, SceneMaterialConstantRecord,
    SceneMaterialHandle, SceneMaterialPassRecord, SceneRenderingDeviceMeshDraw, SceneStorage,
    SceneTextureRecord,
};
use crate::engine::scene::semantic_world::ResolvedAudioBandMaterialValue;
use crate::renderer::native_vulkan::native_vulkan_scene_backend_plan;
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneParameterLayout, native_vulkan_scene_shader_for_key,
};

pub(super) const SCENE_MATERIAL_UNIFORM_BYTES: u64 = 512;
const SCENE_MATERIAL_UNIFORM_FLOATS: usize =
    SCENE_MATERIAL_UNIFORM_BYTES as usize / size_of::<f32>();
static SCENE_AUDIO_SPECTRUM32_DIAGNOSTIC: OnceLock<Option<[f32; 32]>> = OnceLock::new();

pub(super) fn pack_scene_material_uniforms(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    scene_time_seconds: f32,
) -> Vec<u8> {
    let spectrum = scene_audio_spectrum32();
    pack_scene_material_uniforms_with_spectrum(
        storage,
        draws,
        scene_time_seconds,
        spectrum.as_ref(),
    )
}

fn pack_scene_material_uniforms_with_spectrum(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    scene_time_seconds: f32,
    spectrum: Option<&[f32; 32]>,
) -> Vec<u8> {
    pack_scene_material_uniforms_with_frame_inputs(
        storage,
        draws,
        scene_time_seconds,
        spectrum,
        &[],
    )
}

pub(super) fn pack_scene_material_uniforms_with_frame_inputs(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    scene_time_seconds: f32,
    spectrum: Option<&[f32; 32]>,
    audio_material_values: &[ResolvedAudioBandMaterialValue],
) -> Vec<u8> {
    let mut payload =
        Vec::with_capacity(draws.len() * SCENE_MATERIAL_UNIFORM_FLOATS * size_of::<f32>());
    for draw in draws {
        for value in material_uniform_values(
            storage,
            draw,
            scene_time_seconds,
            spectrum,
            audio_material_values,
        ) {
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    payload
}

fn material_uniform_values(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
    spectrum: Option<&[f32; 32]>,
    audio_material_values: &[ResolvedAudioBandMaterialValue],
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let Some(pass) = first_material_pass(storage, draw.material) else {
        return [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    };
    let shader_key = storage.string(pass.shader_key).unwrap_or_default();
    let layout = native_vulkan_scene_shader_for_key(shader_key)
        .map(|shader| shader.parameter_layout)
        .unwrap_or(BuiltinSceneParameterLayout::None);
    let parameters = MaterialParameters { storage, pass };
    let mut values = match layout {
        BuiltinSceneParameterLayout::None => [0.0; SCENE_MATERIAL_UNIFORM_FLOATS],
        BuiltinSceneParameterLayout::Particle => {
            particle::particle_values(storage, draw, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::AudioBars => audio_bars_values(&parameters, spectrum),
        BuiltinSceneParameterLayout::StandardMaterial => {
            if draw.apply_resolved_visual
                && draw.primitive
                    == crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
            {
                return resolved_visual_material_values(draw.resolved_color, draw.resolved_alpha);
            }
            let (resolved_color, resolved_alpha) = if draw.apply_resolved_visual {
                (draw.resolved_color, draw.resolved_alpha)
            } else {
                (
                    crate::engine::scene::SceneVec3 {
                        x: 1.0,
                        y: 1.0,
                        z: 1.0,
                    },
                    1.0,
                )
            };
            standard_material_values(&parameters, resolved_color, resolved_alpha)
        }
        BuiltinSceneParameterLayout::Caustics => {
            caustics_values(&parameters, storage, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::CloudMotion => {
            cloudmotion_values(&parameters, storage, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::ColorKey => colorkey_values(&parameters, shader_key),
        BuiltinSceneParameterLayout::Iris => iris_fragment_values(&parameters, shader_key),
        BuiltinSceneParameterLayout::Opacity => opacity_values(&parameters),
        BuiltinSceneParameterLayout::RoundedMask => {
            rounded_mask_values(&parameters, draw.resolved_color, draw.resolved_alpha)
        }
        BuiltinSceneParameterLayout::Scroll => scroll_values(&parameters, scene_time_seconds),
        BuiltinSceneParameterLayout::Skew => skew_values(&parameters),
        BuiltinSceneParameterLayout::TechCircle => tech_circle_values(
            &parameters,
            scene_time_seconds,
            audio_material_value(
                audio_material_values,
                draw,
                SceneAudioBandMaterialTarget::TechCircleSectorWidth,
            ),
        ),
        BuiltinSceneParameterLayout::FoliageSway => {
            foliage_sway_values(&parameters, storage, draw, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::FoliageRippleComposite => {
            foliage_ripple_composite_values(&parameters, storage, draw, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::FinalEffectProgram => final_effect_program_values(
            &parameters,
            storage,
            draw,
            shader_key,
            scene_time_seconds,
            spectrum,
            audio_material_values,
        ),
        BuiltinSceneParameterLayout::FinalWaterRipple => {
            final_waterripple_values(&parameters, storage, draw, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::FinalWaterWaves => {
            final_waterwaves_values(&parameters, storage, draw, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::RippleFlowComposite => {
            ripple_flow_composite_values(&parameters, storage, draw, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::Shake => shake_values(&parameters, scene_time_seconds),
        BuiltinSceneParameterLayout::WaterWaves => {
            waterwaves_values(&parameters, storage, shader_key, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::WaterWavesDirect => waterwaves_direct_values(
            &parameters,
            storage,
            draw,
            scene_time_seconds,
        ),
        BuiltinSceneParameterLayout::WaterWavesUvField => {
            waterwaves_uv_field_values(&parameters, storage, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::WaterRipple => {
            waterripple_values(&parameters, storage, shader_key, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::WaterFlow => {
            waterflow_values(&parameters, storage, scene_time_seconds)
        }
    };
    let [columns, rows] = draw.effect_batch_atlas_grid;
    if layout == BuiltinSceneParameterLayout::StandardMaterial {
        if columns != 0 && rows != 0 && draw.effect_batch_atlas_tile != u32::MAX {
            let layer = draw.effect_batch_atlas_tile;
            values[12..16].copy_from_slice(&[
                (columns as f32).recip(),
                (rows as f32).recip(),
                (layer % columns) as f32 / columns as f32,
                (layer / columns) as f32 / rows as f32,
            ]);
        } else {
            values[12..16].copy_from_slice(&[1.0, 1.0, 0.0, 0.0]);
        }
    }
    values
}

fn resolved_visual_material_values(
    resolved_color: crate::engine::scene::SceneVec3,
    resolved_alpha: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[..4].copy_from_slice(&[
        resolved_color.x,
        resolved_color.y,
        resolved_color.z,
        resolved_alpha,
    ]);
    values
}

fn waterflow_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["speed"], 1.0);
    values[2] = parameters.scalar(&["feather"], 0.4);
    values[3] = parameters.scalar(&["strength"], 1.0);
    values[4] = parameters.scalar(&["phasescale"], 2.0);
    values[8..12].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values
}

fn audio_bars_values(
    parameters: &MaterialParameters<'_>,
    spectrum: Option<&[f32; 32]>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..3].copy_from_slice(&[1.0, 1.0, 1.0]);
    set_vector(&mut values, 0, &parameters.values(&["Bar Color"]), 3);
    values[3] = parameters.scalar(&["ui_editor_properties_opacity"], 1.0);
    values[4] = parameters.scalar(&["Bar Count"], 32.0);
    values[5] = parameters.scalar(&["Bar Spacing"], 0.1);
    values[6..8].copy_from_slice(&[0.0, 1.0]);
    set_vector(
        &mut values,
        6,
        &parameters.values(&["Lower/Upper Bar Bounds"]),
        2,
    );
    values[8] = parameters.scalar(
        &["Minimum Height (Will be multiplied by the bar width) "],
        0.0,
    );
    values[9] = parameters.scalar(&["Radius"], 1.0);
    values[10] = parameters.scalar(&["Volume Factor"], 1.0);
    values[11..13].copy_from_slice(&[0.05, 0.0]);
    set_vector(
        &mut values,
        11,
        &parameters.values(&["Anti-alias blurring "]),
        2,
    );
    if let Some(spectrum) = spectrum {
        values[16..48].copy_from_slice(spectrum);
        values[48..80].copy_from_slice(spectrum);
    }
    values
}

pub(super) fn scene_audio_spectrum32() -> Option<[f32; 32]> {
    use crate::renderer::native_vulkan::audio::clock::native_vulkan_audio_spectrum32_packed;

    if let Some(spectrum) = diagnostic_scene_audio_spectrum32().as_ref() {
        return Some(*spectrum);
    }
    native_vulkan_audio_spectrum32_packed().map(|packed| {
        std::array::from_fn(|band| {
            let shift = (band & 1) * 16;
            ((packed[band / 2] >> shift) & 0xffff) as f32 / 65535.0
        })
    })
}

fn diagnostic_scene_audio_spectrum32() -> Option<[f32; 32]> {
    *SCENE_AUDIO_SPECTRUM32_DIAGNOSTIC.get_or_init(|| {
        std::env::var("GILDER_SCENE_AUDIO_SPECTRUM32")
            .ok()
            .and_then(|value| parse_scene_audio_spectrum32(&value))
    })
}

pub(super) fn scene_audio_spectrum_status() -> (&'static str, bool) {
    use crate::renderer::native_vulkan::audio::clock::native_vulkan_audio_spectrum32_packed;
    use crate::renderer::native_vulkan::audio::system_monitor::system_audio_monitor_spectrum_status;

    if diagnostic_scene_audio_spectrum32().is_some() {
        ("diagnostic-spectrum32-override", true)
    } else if let Some(status) = system_audio_monitor_spectrum_status() {
        status
    } else if native_vulkan_audio_spectrum32_packed().is_some() {
        ("decoded-audio-goertzel32-mono-duplicated-stereo", true)
    } else {
        ("zero-spectrum-no-publisher", false)
    }
}

pub(super) fn scene_audio_spectrum_summary() -> (f32, u32) {
    let spectrum = scene_audio_spectrum32().unwrap_or([0.0; 32]);
    let peak = spectrum.iter().copied().fold(0.0f32, f32::max);
    let active_bands = spectrum
        .iter()
        .filter(|value| **value > 1.0 / 65535.0)
        .count()
        .min(u32::MAX as usize) as u32;
    (peak, active_bands)
}

fn parse_scene_audio_spectrum32(value: &str) -> Option<[f32; 32]> {
    if let Some(value) = value.trim().strip_prefix("flat:") {
        let value = value.parse::<f32>().ok()?;
        return (value.is_finite() && (0.0..=1.0).contains(&value)).then_some([value; 32]);
    }
    let values = value
        .split([',', ' '])
        .filter(|value| !value.is_empty())
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() != 32
        || values
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return None;
    }
    values.try_into().ok()
}

fn rounded_mask_values(
    parameters: &MaterialParameters<'_>,
    resolved_color: crate::engine::scene::SceneVec3,
    resolved_alpha: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..3].copy_from_slice(&[1.0, 1.0, 1.0]);
    set_vector(&mut values, 0, &parameters.values(&["Color"]), 3);
    values[3] = parameters.scalar(&["Radius"], 0.5);
    values[4..6].copy_from_slice(&[1.0, 1.0]);
    set_vector(&mut values, 4, &parameters.values(&["Size"]), 2);
    values[6] = parameters.scalar(&["Softness"], 0.5);
    values[7] = parameters.scalar(&["ui_editor_properties_opacity"], 1.0);
    values[8] = parameters.scalar(&["Border width", "BorderWidth"], 0.025);
    values[12..16].copy_from_slice(&[
        resolved_color.x,
        resolved_color.y,
        resolved_color.z,
        resolved_alpha,
    ]);
    values
}

fn scroll_values(
    parameters: &MaterialParameters<'_>,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["speedx"], 0.2);
    values[2] = parameters.scalar(&["speedy"], 0.2);
    values[4..6].copy_from_slice(&[1.0, 1.0]);
    set_vector(&mut values, 4, &parameters.values(&["repeat"]), 2);
    values
}

fn skew_values(parameters: &MaterialParameters<'_>) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = parameters.scalar(&["top"], 0.0);
    values[1] = parameters.scalar(&["bottom"], 0.0);
    values[2] = parameters.scalar(&["left"], 0.0);
    values[3] = parameters.scalar(&["right"], 0.0);
    values
}

fn tech_circle_values(
    parameters: &MaterialParameters<'_>,
    scene_time_seconds: f32,
    sector_width_override: Option<f32>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..3].copy_from_slice(&[1.0, 1.0, 1.0]);
    set_vector(
        &mut values,
        0,
        &parameters.values(&["ui_editor_properties_1_color"]),
        3,
    );
    values[3] = parameters.scalar(&["ui_editor_properties_2_alpha"], 1.0);
    values[4] = scene_time_seconds;
    values[5] = parameters.scalar(&["ui_editor_properties_3_speed"], 0.1);
    values[6] = parameters.scalar(&["ui_editor_properties_6_skew"], 0.0);
    values[7] = parameters.scalar(&["ui_editor_properties_4_ring_1_radius"], 0.5);
    values[8] = parameters.scalar(&["ui_editor_properties_4_ring_1_width"], 0.2);
    values[9] = parameters.scalar(&["ui_editor_properties_4_ring_2_segment_count"], 2.0);
    values[10] = parameters.scalar(&["ui_editor_properties_4_ring_2_segment_width"], 0.25);
    values[11] = parameters.scalar(&["ui_editor_properties_5_sector_1_offset"], 0.0);
    values[12] = sector_width_override
        .unwrap_or_else(|| parameters.scalar(&["ui_editor_properties_5_sector_1_width"], 0.3));
    values[13] = parameters.scalar(&["ui_editor_properties_5_sector_segment_count"], 5.0);
    values[14] = parameters.scalar(&["ui_editor_properties_5_sector_segment_width"], 0.75);
    values
}

fn audio_material_value(
    values: &[ResolvedAudioBandMaterialValue],
    draw: &SceneRenderingDeviceMeshDraw,
    target: SceneAudioBandMaterialTarget,
) -> Option<f32> {
    values
        .iter()
        .find(|value| value.object == draw.object && value.target == target)
        .map(|value| value.value)
}

fn caustics_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["ui_editor_properties_speed", "speed"], 1.0);
    values[2] = parameters.scalar(&["ui_editor_properties_granularity", "scale"], 2.0);
    values[3] = parameters.scalar(&["ui_editor_properties_brightness", "brightness"], 1.0);
    values[4] = parameters.scalar(&["ui_editor_properties_glow", "glow"], 0.5);
    values[5] = parameters.scalar(&["ui_editor_properties_distortion", "distortion"], 1.0);
    values[6] = parameters.scalar(
        &["ui_editor_properties_chromatic_aberration", "chromatic"],
        0.0,
    );
    values[7] = parameters.scalar(&["ui_editor_properties_blur", "blur"], 0.0);
    values[8..11].copy_from_slice(&[1.0, 1.0, 1.0]);
    values[12..15].copy_from_slice(&[1.0, 1.0, 1.0]);
    set_vector(
        &mut values,
        8,
        &parameters.values(&["ui_editor_properties_color_start", "color_start"]),
        3,
    );
    set_vector(
        &mut values,
        12,
        &parameters.values(&["ui_editor_properties_color_end", "color_end"]),
        3,
    );
    values[11] = parameters.scalar(&["ui_editor_properties_time_offset", "time_offset"], 0.0);
    values[15] = scene_logical_aspect_ratio(storage);
    values
}

fn cloudmotion_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["ui_editor_properties_speed", "speed"], 0.02);
    values[2] = parameters.scalar(&["ui_editor_properties_amount", "amount"], 0.1);
    values[3] = parameters.scalar(&["ui_editor_properties_direction", "direction"], 1.5707963);
    values[4] = parameters.scalar(&["ui_editor_properties_granularity", "scale"], 2.0);
    values[5] = parameters.scalar(
        &["ui_editor_properties_granularity_horizontal", "scalex"],
        0.5,
    );
    values[6] = scene_logical_aspect_ratio(storage);
    values
}

fn scene_logical_aspect_ratio(storage: &SceneStorage) -> f32 {
    let project = storage.project();
    project.logical_width.max(1) as f32 / project.logical_height.max(1) as f32
}

fn colorkey_values(
    parameters: &MaterialParameters<'_>,
    shader_key: &str,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = parameters.scalar(&["alpha"], 0.0);
    values[1] = parameters.scalar(&["fuzziness"], 0.0);
    values[2] = parameters.scalar(&["tolerance"], 0.1);
    values[3] = bool_float(shader_combo_enabled(shader_key, "INVERT"));
    values[4..7].copy_from_slice(&[1.0, 1.0, 1.0]);
    set_vector(&mut values, 4, &parameters.values(&["color"]), 3);
    values[7] = bool_float(shader_combo_enabled(shader_key, "FLATTEN"));
    values
}

fn shake_values(
    parameters: &MaterialParameters<'_>,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["speed"], 1.0);
    values[2] = parameters.scalar(&["strength"], 0.1);
    values[4..6].copy_from_slice(&[0.0, 1.0]);
    values[6..8].copy_from_slice(&[1.0, 1.0]);
    set_vector(&mut values, 4, &parameters.values(&["bounds"]), 2);
    set_vector(&mut values, 6, &parameters.values(&["friction"]), 2);
    values
}

fn first_material_pass(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
) -> Option<&SceneMaterialPassRecord> {
    if material.0 == INVALID_MATERIAL_ID {
        return None;
    }
    let material = storage.document().materials.get(material.0 as usize)?;
    storage
        .document()
        .material_passes
        .get(material.pass_start as usize)
}

pub(super) fn material_pass_constants<'a>(
    storage: &'a SceneStorage,
    pass: &SceneMaterialPassRecord,
) -> &'a [SceneMaterialConstantRecord] {
    let start = pass.constant_start as usize;
    let end = start.saturating_add(pass.constant_count as usize);
    storage
        .document()
        .material_constants
        .get(start..end)
        .unwrap_or(&[])
}

pub(super) fn material_parameter_values(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
    names: &[&str],
) -> Vec<f32> {
    let Some(pass) = first_material_pass(storage, material) else {
        return Vec::new();
    };
    MaterialParameters { storage, pass }.values(names)
}

pub(super) fn material_parameter_layout(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
) -> BuiltinSceneParameterLayout {
    first_material_pass(storage, material)
        .and_then(|pass| storage.string(pass.shader_key))
        .and_then(native_vulkan_scene_shader_for_key)
        .map(|shader| shader.parameter_layout)
        .unwrap_or(BuiltinSceneParameterLayout::None)
}

pub(super) fn scene_uses_audio_spectrum(storage: &SceneStorage) -> bool {
    !storage.audio_band_material_bindings().is_empty()
        || native_vulkan_scene_backend_plan(storage)
        .rendering_device_graph
        .mesh_draws
        .iter()
        .any(|draw| material_uses_audio_spectrum(storage, draw.material))
}

fn material_uses_audio_spectrum(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
) -> bool {
    let Some(pass) = first_material_pass(storage, material) else {
        return false;
    };
    let Some(shader_key) = storage.string(pass.shader_key) else {
        return false;
    };
    native_vulkan_scene_shader_for_key(shader_key).is_some_and(|shader| {
        shader.parameter_layout == BuiltinSceneParameterLayout::AudioBars
            || (shader.parameter_layout == BuiltinSceneParameterLayout::FinalEffectProgram
                && shader_key.eq_ignore_ascii_case("we/audio-bars-final"))
    })
}

struct MaterialParameters<'a> {
    storage: &'a SceneStorage,
    pass: &'a SceneMaterialPassRecord,
}

impl MaterialParameters<'_> {
    fn values(&self, names: &[&str]) -> Vec<f32> {
        for name in names {
            if let Some(values) = material_pass_constants(self.storage, self.pass)
                .iter()
                .find_map(|constant| {
                    let constant_name = self.storage.string(constant.name)?;
                    constant_name.eq_ignore_ascii_case(name).then(|| {
                        parse_constant_values(
                            self.storage.string(constant.value_json).unwrap_or_default(),
                        )
                    })
                })
                .filter(|values| !values.is_empty())
            {
                return values;
            }
        }
        Vec::new()
    }

    fn scalar(&self, names: &[&str], default: f32) -> f32 {
        self.values(names).first().copied().unwrap_or(default)
    }
}

fn standard_material_values(
    parameters: &MaterialParameters<'_>,
    resolved_color: crate::engine::scene::SceneVec3,
    resolved_alpha: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[..4].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
    set_vector(
        &mut values,
        0,
        &parameters.values(&["color4", "g_color4", "color", "tint"]),
        4,
    );
    values[0] *= resolved_color.x;
    values[1] *= resolved_color.y;
    values[2] *= resolved_color.z;
    values[3] *= resolved_alpha;
    values[4] = parameters.scalar(&["roughness"], 0.0);
    values[5] = parameters.scalar(&["metallic"], 0.0);
    set_vector(
        &mut values,
        8,
        &parameters.values(&["speculartint", "specularcolor"]),
        4,
    );
    values
}

pub(super) fn resolved_standard_material_color(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
) -> Option<[f32; 4]> {
    let pass = first_material_pass(storage, draw.material)?;
    let shader_key = storage.string(pass.shader_key)?;
    let shader = native_vulkan_scene_shader_for_key(shader_key)?;
    if shader.parameter_layout != BuiltinSceneParameterLayout::StandardMaterial {
        return None;
    }
    let (resolved_color, resolved_alpha) = if draw.apply_resolved_visual {
        (draw.resolved_color, draw.resolved_alpha)
    } else {
        (
            crate::engine::scene::SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            1.0,
        )
    };
    let values = standard_material_values(
        &MaterialParameters { storage, pass },
        resolved_color,
        resolved_alpha,
    );
    Some([values[0], values[1], values[2], values[3]])
}

fn iris_fragment_values(
    parameters: &MaterialParameters<'_>,
    shader_key: &str,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[..3].copy_from_slice(&[1.0, 1.0, 1.0]);
    set_vector(
        &mut values,
        0,
        &parameters.values(&["color", "eyecolor"]),
        3,
    );
    values[3] = bool_float(shader_combo_enabled(shader_key, "BACKGROUND"));
    values
}

fn opacity_values(parameters: &MaterialParameters<'_>) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = parameters.scalar(&["alpha", "opacity"], 1.0);
    values[4..8].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
    values
}

fn foliage_sway_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["speeduv", "speed"], 5.0);
    values[2] = parameters.scalar(&["strength"], 0.4);
    values[3] = parameters.scalar(&["phase"], 0.5);
    values[4] = parameters.scalar(&["power"], 1.0);
    values[5] = parameters.scalar(&["scale", "noisescale"], 0.05);
    values[6] = parameters.scalar(&["ratio"], 0.3);
    values[7] = parameters.scalar(&["scrolldirection", "direction"], 0.0);
    values[8..12].copy_from_slice(&object_source_texture_resolution(storage, draw.object));
    values
}

fn foliage_ripple_composite_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&[1.0; 4]);
    set_vector(
        &mut values,
        0,
        &parameters.values(&["base.color4", "base.g_color4", "base.color", "base.tint"]),
        4,
    );
    values[0] *= draw.resolved_color.x;
    values[1] *= draw.resolved_color.y;
    values[2] *= draw.resolved_color.z;
    values[3] *= draw.resolved_alpha;
    values[4] = scene_time_seconds;
    values[5] = parameters.scalar(&["foliage.speeduv", "foliage.speed"], 5.0);
    values[6] = parameters.scalar(&["foliage.strength"], 0.4);
    values[7] = parameters.scalar(&["foliage.phase"], 0.5);
    values[8] = parameters.scalar(&["foliage.power"], 1.0);
    values[9] = parameters.scalar(&["foliage.scale", "foliage.noisescale"], 0.05);
    values[10] = parameters.scalar(&["foliage.ratio"], 0.3);
    values[11] = parameters.scalar(&["foliage.scrolldirection", "foliage.direction"], 0.0);
    values[12..16].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 0));
    values[16] = scene_time_seconds;
    values[17] = parameters.scalar(&["ripple.animationspeed"], 0.15);
    values[18] = parameters.scalar(&["ripple.scale"], 1.0);
    values[19] = parameters.scalar(&["ripple.scrollspeed"], 0.0);
    values[20] = parameters.scalar(&["ripple.scrolldirection", "ripple.direction"], 0.0);
    values[21] = parameters.scalar(&["ripple.ripplestrength", "ripple.strength"], 0.1);
    let ripple_ratio = parameters.scalar(&["ripple.ratio"], 1.0);
    values[22] = ripple_ratio;
    values[23] = ripple_ratio;
    values
}

fn waterwaves_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    shader_key: &str,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    let speed = parameters.scalar(&["speed"], 5.0);
    let scale = parameters.scalar(&["scale"], 200.0);
    values[0] = scene_time_seconds;
    values[1] = speed;
    values[2] = scale;
    values[3] = parameters.scalar(&["strength"], 0.1);
    values[4] = parameters.scalar(&["direction"], 0.0);
    values[5] = parameters.scalar(&["speed2"], speed);
    values[6] = parameters.scalar(&["scale2"], scale);
    values[7] = parameters.scalar(&["direction2"], 0.0);
    values[8] = parameters.scalar(&["offset2"], 0.0);
    values[9] = bool_float(shader_combo_enabled(shader_key, "DUALWAVES"));
    values[10] = parameters.scalar(&["exponent"], 1.0);
    values[11] = parameters.scalar(&["exponent2"], 1.0);
    values[12..16].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values
}

fn waterwaves_uv_field_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    waterwaves_displacement_values(parameters, storage, scene_time_seconds, 0, 4)
}

fn waterwaves_direct_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = waterwaves_displacement_values(
        parameters,
        storage,
        scene_time_seconds,
        4,
        8,
    );
    let (resolved_color, resolved_alpha) = if draw.apply_resolved_visual {
        (draw.resolved_color, draw.resolved_alpha)
    } else {
        (
            crate::engine::scene::SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            1.0,
        )
    };
    let standard = standard_material_values(parameters, resolved_color, resolved_alpha);
    values[..4].copy_from_slice(&standard[..4]);
    values
}

fn waterwaves_displacement_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    scene_time_seconds: f32,
    chain_base: usize,
    stage_start: usize,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    const MAX_STAGES: usize = 7;
    const STAGE_FLOATS: usize = 16;
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[chain_base] = parameters
        .scalar(&["waterwaves.stage_count"], 0.0)
        .clamp(0.0, MAX_STAGES as f32);
    values[chain_base + 1] = scene_time_seconds;
    for stage in 0..MAX_STAGES {
        let speed = waterwaves_stage_scalar(parameters, stage, "speed", 5.0);
        let scale = waterwaves_stage_scalar(parameters, stage, "scale", 200.0);
        let strength = waterwaves_stage_scalar(parameters, stage, "strength", 0.1);
        let direction_angle = waterwaves_stage_scalar(parameters, stage, "direction", 0.0);
        let speed2 = waterwaves_stage_scalar(parameters, stage, "speed2", speed);
        let scale2 = waterwaves_stage_scalar(parameters, stage, "scale2", scale);
        let direction2_angle =
            waterwaves_stage_scalar(parameters, stage, "direction2", 0.0);
        let offset2 = waterwaves_stage_scalar(parameters, stage, "offset2", 0.0);
        let dual_waves = waterwaves_stage_scalar(parameters, stage, "dualwaves", 0.0) > 0.5;
        let direction = [-direction_angle.sin(), direction_angle.cos()];
        let direction2 = [-direction2_angle.sin(), direction2_angle.cos()];
        let base = stage_start + stage * STAGE_FLOATS;
        values[base] = scene_time_seconds * speed;
        values[base + 1] = scale;
        values[base + 2] = strength * strength;
        values[base + 3] = waterwaves_stage_scalar(parameters, stage, "mask", 0.0);
        values[base + 4..base + 6].copy_from_slice(&direction);
        values[base + 6] = (scene_time_seconds + offset2) * speed2;
        values[base + 7] = if dual_waves { scale2 } else { 0.0 };
        values[base + 8..base + 10].copy_from_slice(&direction2);
        values[base + 10] = waterwaves_stage_scalar(parameters, stage, "exponent", 1.0);
        values[base + 11] = waterwaves_stage_scalar(parameters, stage, "exponent2", 1.0);
        values[base + 12..base + 16].copy_from_slice(&material_texture_resolution(
            storage,
            parameters.pass,
            stage as u32 + 1,
        ));
    }
    values
}

fn waterwaves_stage_scalar(
    parameters: &MaterialParameters<'_>,
    stage: usize,
    name: &str,
    default: f32,
) -> f32 {
    let name = format!("waterwaves.{stage}.{name}");
    parameters.scalar(&[name.as_str()], default)
}

fn waterripple_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    shader_key: &str,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    let ratio = parameters.scalar(&["ratio"], 1.0);
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["animationspeed"], 0.15);
    values[2] = parameters.scalar(&["scale"], 1.0);
    values[3] = parameters.scalar(&["scrollspeed"], 0.0);
    values[4] = parameters.scalar(&["scrolldirection", "direction"], 0.0);
    values[5] = parameters.scalar(&["ripplestrength", "strength"], 0.1);
    values[6] = ratio;
    values[7] = 1.0;
    values[8] = bool_float(shader_texture_slot_enabled(shader_key, 1));
    values[9] = bool_float(shader_texture_slot_enabled(shader_key, 2));
    values[11] = ratio;
    values[12..16].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values
}

fn set_vector(values: &mut [f32], start: usize, parameter: &[f32], count: usize) {
    for (lane, value) in parameter.iter().take(count).enumerate() {
        if let Some(destination) = values.get_mut(start + lane) {
            *destination = *value;
        }
    }
}

fn shader_combo_enabled(shader_key: &str, name: &str) -> bool {
    let prefix = format!("{}_", name.to_ascii_uppercase());
    shader_key.split("__").any(|part| {
        part.to_ascii_uppercase()
            .strip_prefix(&prefix)
            .and_then(|value| value.parse::<i64>().ok())
            .is_some_and(|value| value != 0)
    })
}

fn shader_texture_slot_enabled(shader_key: &str, slot: u32) -> bool {
    shader_key
        .split("__")
        .find_map(|part| {
            part.strip_prefix("SLOTS_")
                .or_else(|| part.strip_prefix("slots_"))
                .and_then(|mask| u32::from_str_radix(mask, 16).ok())
        })
        .is_some_and(|mask| slot < 32 && mask & (1 << slot) != 0)
}

fn bool_float(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

fn parse_constant_values(value_json: &str) -> Vec<f32> {
    let Ok(value) = serde_json::from_str::<Value>(value_json) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    collect_constant_values(&value, &mut values);
    values
}

fn collect_constant_values(value: &Value, out: &mut Vec<f32>) {
    match value {
        Value::Number(number) => {
            if let Some(value) = number.as_f64().filter(|value| value.is_finite()) {
                out.push(value as f32);
            }
        }
        Value::Bool(value) => out.push(if *value { 1.0 } else { 0.0 }),
        Value::String(value) => {
            for value in value.split_ascii_whitespace() {
                if let Ok(value) = value.parse::<f32>() {
                    out.push(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_constant_values(value, out);
            }
        }
        Value::Object(object) => {
            if let Some(value) = object.get("value") {
                collect_constant_values(value, out);
                return;
            }
            for key in ["x", "y", "z", "w", "r", "g", "b", "a"] {
                if let Some(value) = object.get(key) {
                    collect_constant_values(value, out);
                }
            }
        }
        Value::Null => {}
    }
}

#[cfg(test)]
#[path = "material_uniform/tests.rs"]
mod tests;
