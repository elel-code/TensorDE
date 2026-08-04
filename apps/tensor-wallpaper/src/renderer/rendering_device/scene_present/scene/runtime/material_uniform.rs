//! Scene material/effect constant packing for the shared-renderer scene runtime.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/material-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/effect-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/exe/global-uniforms.md`

use std::mem::size_of;

mod audio_usage;
mod color_effect;
mod deformation;
mod depth_parallax;
mod final_effect;
mod particle;
mod pulse;
mod shader_key;
mod source_extent;
mod value_writer;
mod weather_effect;

#[cfg(test)]
use audio_usage::material_uses_audio_spectrum;
pub(super) use audio_usage::scene_uses_audio_spectrum;
use color_effect::{blend_gradient_values, blend_values, shimmer_values, tint_values};
use deformation::{
    draw_effect_enabled, foliage_ripple_composite_values, foliage_sway_values, shake_values,
    waterripple_values, waterwaves_direct_values, waterwaves_uv_field_values, waterwaves_values,
};
use depth_parallax::depth_parallax_values;
use pulse::pulse_values;
use shader_key::{shader_combo_enabled, shader_combo_value, shader_texture_slot_enabled};
use source_extent::draw_source_aspect_ratio;
pub(super) use value_writer::parse_constant_values;
use value_writer::set_vector;

use super::draw_uniform::{depth_parallax_inverse_values, iris_draw_values};

#[cfg(test)]
use final_effect::final_audio_bars_values;
use final_effect::{
    final_effect_program_values, final_waterripple_values, final_waterwaves_values,
    material_texture_resolution, object_source_texture_resolution, ripple_flow_composite_values,
};

use crate::engine::scene::semantic_world::{
    ResolvedAudioBandMaterialValue, ResolvedMaterialScalarValue,
};
use crate::engine::scene::{
    INVALID_MATERIAL_ID, SceneAudioBandMaterialTarget, SceneMaterialConstantRecord,
    SceneMaterialHandle, SceneMaterialPassRecord, SceneRenderingDeviceMeshDraw, SceneStorage,
    SceneTextureRecord, StereoSpectrum64,
};
use crate::renderer::rendering_device::scene::{
    BuiltinSceneParameterLayout, rendering_device_scene_shader_for_key,
};

pub(super) const SCENE_MATERIAL_UNIFORM_BYTES: u64 = 768;
const SCENE_MATERIAL_UNIFORM_FLOATS: usize =
    SCENE_MATERIAL_UNIFORM_BYTES as usize / size_of::<f32>();
pub(super) const AUTHORED_CLOUDMOTION_DEFAULT_DIRECTION: f32 = f32::from_bits(0x3FC9_0FDA);

#[derive(Clone, Copy)]
pub(super) struct SceneMaterialFrameInputs<'a> {
    pub average_spectrum32: Option<&'a [f32; 32]>,
    pub stereo_spectrum64: Option<&'a StereoSpectrum64>,
    pub parallax_position: [f32; 2],
    pub audio_material_values: &'a [ResolvedAudioBandMaterialValue],
    pub material_scalar_values: &'a [ResolvedMaterialScalarValue],
}

pub(super) fn pack_scene_material_uniforms(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    scene_time_seconds: f32,
    output_extent: [u32; 2],
) -> Vec<u8> {
    pack_scene_material_uniforms_with_spectrum(
        storage,
        draws,
        scene_time_seconds,
        output_extent,
        None,
    )
}

fn pack_scene_material_uniforms_with_spectrum(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    scene_time_seconds: f32,
    output_extent: [u32; 2],
    average_spectrum32: Option<&[f32; 32]>,
) -> Vec<u8> {
    pack_scene_material_uniforms_with_frame_inputs(
        storage,
        draws,
        scene_time_seconds,
        output_extent,
        SceneMaterialFrameInputs {
            average_spectrum32,
            stereo_spectrum64: None,
            parallax_position: [0.5; 2],
            audio_material_values: &[],
            material_scalar_values: &[],
        },
    )
}

pub(super) fn pack_scene_material_uniforms_with_frame_inputs(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    scene_time_seconds: f32,
    output_extent: [u32; 2],
    frame_inputs: SceneMaterialFrameInputs<'_>,
) -> Vec<u8> {
    let mut payload =
        Vec::with_capacity(draws.len() * SCENE_MATERIAL_UNIFORM_FLOATS * size_of::<f32>());
    pack_scene_material_uniforms_with_frame_inputs_into(
        &mut payload,
        storage,
        draws,
        scene_time_seconds,
        output_extent,
        frame_inputs,
    );
    payload
}

pub(super) fn pack_scene_material_uniforms_with_frame_inputs_into(
    payload: &mut Vec<u8>,
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    scene_time_seconds: f32,
    output_extent: [u32; 2],
    frame_inputs: SceneMaterialFrameInputs<'_>,
) {
    let byte_count = draws
        .len()
        .saturating_mul(SCENE_MATERIAL_UNIFORM_FLOATS)
        .saturating_mul(size_of::<f32>());
    payload.clear();
    payload.reserve(byte_count.saturating_sub(payload.capacity()));
    for draw in draws {
        for value in material_uniform_values(
            storage,
            draw,
            scene_time_seconds,
            output_extent,
            frame_inputs,
        ) {
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    debug_assert_eq!(payload.len(), byte_count);
}

fn material_uniform_values(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
    output_extent: [u32; 2],
    frame_inputs: SceneMaterialFrameInputs<'_>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let Some(pass) = first_material_pass(storage, draw.material) else {
        return [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    };
    let shader_key = storage
        .string(draw.shader_key)
        .or_else(|| storage.string(pass.shader_key))
        .unwrap_or_default();
    let layout = rendering_device_scene_shader_for_key(shader_key)
        .map(|shader| shader.parameter_layout)
        .unwrap_or(BuiltinSceneParameterLayout::None);
    let parameters = MaterialParameters {
        storage,
        pass,
        scalar_overrides: frame_inputs.material_scalar_values,
    };
    let mut values = match layout {
        BuiltinSceneParameterLayout::None => [0.0; SCENE_MATERIAL_UNIFORM_FLOATS],
        BuiltinSceneParameterLayout::Particle => {
            particle::particle_values(storage, draw, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::Blend => blend_values(&parameters, storage, shader_key),
        BuiltinSceneParameterLayout::BlendGradient => {
            blend_gradient_values(&parameters, storage, shader_key)
        }
        BuiltinSceneParameterLayout::BlurCombine => blur_combine_values(&parameters),
        BuiltinSceneParameterLayout::BlurGaussian => blur_gaussian_values(&parameters),
        BuiltinSceneParameterLayout::DepthParallax => depth_parallax_values(
            &parameters,
            storage,
            draw,
            output_extent,
            frame_inputs.parallax_position,
        ),
        BuiltinSceneParameterLayout::StandardMaterial => {
            if draw.apply_resolved_visual
                && draw.primitive
                    == crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
                && shader_key != "we/objectcomposite-screen-group"
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
        BuiltinSceneParameterLayout::SceneColorBlend => scene_color_blend_values(storage, draw),
        BuiltinSceneParameterLayout::Caustics => {
            caustics_values(&parameters, storage, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::CloudMotion => {
            cloudmotion_values(&parameters, draw, scene_time_seconds, output_extent)
        }
        BuiltinSceneParameterLayout::ColorKey => colorkey_values(&parameters, shader_key),
        BuiltinSceneParameterLayout::Iris => iris_fragment_values(
            &parameters,
            shader_key,
            storage,
            draw.material,
            scene_time_seconds,
        ),
        BuiltinSceneParameterLayout::Opacity => opacity_values(&parameters),
        BuiltinSceneParameterLayout::Pulse => pulse_values(
            &parameters,
            storage,
            shader_key,
            scene_time_seconds,
            frame_inputs.stereo_spectrum64,
        ),
        BuiltinSceneParameterLayout::RoundedMask => {
            rounded_mask_values(&parameters, draw, draw_effect_enabled(draw, 0))
        }
        BuiltinSceneParameterLayout::Scroll => scroll_values(&parameters, scene_time_seconds),
        BuiltinSceneParameterLayout::Skew => skew_values(&parameters),
        BuiltinSceneParameterLayout::Spin => spin_values(&parameters, scene_time_seconds),
        BuiltinSceneParameterLayout::Shimmer => shimmer_values(&parameters, scene_time_seconds),
        BuiltinSceneParameterLayout::Swing => {
            weather_effect::swing_values(&parameters, storage, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::Tint => tint_values(&parameters, storage),
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
            frame_inputs.average_spectrum32,
            frame_inputs.audio_material_values,
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
        BuiltinSceneParameterLayout::Shake => {
            shake_values(&parameters, storage, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::WaterWaves => {
            waterwaves_values(&parameters, storage, shader_key, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::WaterWavesDirect => {
            waterwaves_direct_values(&parameters, storage, draw, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::WaterWavesUvField => {
            waterwaves_uv_field_values(&parameters, storage, draw, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::WaterRipple => {
            waterripple_values(&parameters, storage, draw, shader_key, scene_time_seconds)
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

fn scene_color_blend_values(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = draw.resolved_color.x;
    values[1] = draw.resolved_color.y;
    values[2] = draw.resolved_color.z;
    values[3] = draw.resolved_alpha;
    values[4] = storage
        .document()
        .objects
        .get(draw.object.0 as usize)
        .map_or(0.0, |object| object.color_blend_mode as f32);
    values
}

fn blur_gaussian_values(
    parameters: &MaterialParameters<'_>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[..2].copy_from_slice(&[1.0, 1.0]);
    set_vector(&mut values, 0, &parameters.values(&["scale"]), 2);
    values
}

fn blur_combine_values(
    parameters: &MaterialParameters<'_>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = parameters.scalar(&["compositealpha"], 1.0);
    set_vector(&mut values, 1, &parameters.values(&["compositeoffset"]), 2);
    values[4..8].copy_from_slice(&[1.0; 4]);
    set_vector(&mut values, 4, &parameters.values(&["compositecolor"]), 3);
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

fn rounded_mask_values(
    parameters: &MaterialParameters<'_>,
    draw: &SceneRenderingDeviceMeshDraw,
    effect_visible: bool,
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
    values[9] = bool_float(effect_visible);
    values[10..12].copy_from_slice(&draw.authored_source_extent);
    values[12..16].copy_from_slice(&[
        draw.resolved_color.x,
        draw.resolved_color.y,
        draw.resolved_color.z,
        draw.resolved_alpha,
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

fn spin_values(
    parameters: &MaterialParameters<'_>,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["speed"], 1.0);
    values[2] = parameters.scalar(&["ratio"], 1.0);
    values[3] = parameters.scalar(&["angle"], 0.0);
    values[4] = parameters.scalar(&["phase"], 0.0);
    values[5..7].copy_from_slice(&[0.5, 0.5]);
    set_vector(&mut values, 5, &parameters.values(&["center"]), 2);
    values[8] = parameters.scalar(&["size"], 0.1);
    values[9] = parameters.scalar(&["feather"], 0.002);
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
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
    output_extent: [u32; 2],
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["ui_editor_properties_speed", "speed"], 0.02);
    values[2] = parameters.scalar(&["ui_editor_properties_amount", "amount"], 0.1);
    values[3] = parameters.scalar(
        &["ui_editor_properties_direction", "direction"],
        AUTHORED_CLOUDMOTION_DEFAULT_DIRECTION,
    );
    values[4] = parameters.scalar(&["ui_editor_properties_granularity", "scale"], 2.0);
    values[5] = parameters.scalar(
        &["ui_editor_properties_granularity_horizontal", "scalex"],
        0.5,
    );
    values[6] = draw_source_aspect_ratio(draw, output_extent);
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
    MaterialParameters {
        storage,
        pass,
        scalar_overrides: &[],
    }
    .values(names)
}

pub(super) fn material_parameter_layout(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
) -> BuiltinSceneParameterLayout {
    first_material_pass(storage, material)
        .and_then(|pass| storage.string(pass.shader_key))
        .and_then(rendering_device_scene_shader_for_key)
        .map(|shader| shader.parameter_layout)
        .unwrap_or(BuiltinSceneParameterLayout::None)
}

pub(super) fn draw_parameter_layout(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
) -> BuiltinSceneParameterLayout {
    storage
        .string(draw.shader_key)
        .and_then(rendering_device_scene_shader_for_key)
        .map(|shader| shader.parameter_layout)
        .unwrap_or_else(|| material_parameter_layout(storage, draw.material))
}

struct MaterialParameters<'a> {
    storage: &'a SceneStorage,
    pass: &'a SceneMaterialPassRecord,
    scalar_overrides: &'a [crate::engine::scene::semantic_world::ResolvedMaterialScalarValue],
}

impl MaterialParameters<'_> {
    fn values(&self, names: &[&str]) -> Vec<f32> {
        for name in names {
            if let Some(values) = material_pass_constants(self.storage, self.pass)
                .iter()
                .enumerate()
                .find_map(|constant| {
                    let (local_index, constant) = constant;
                    let constant_name = self.storage.string(constant.name)?;
                    constant_name.eq_ignore_ascii_case(name).then(|| {
                        let constant_index = self.pass.constant_start + local_index as u32;
                        if let Some(value) = self
                            .scalar_overrides
                            .iter()
                            .find(|value| value.constant_index == constant_index)
                        {
                            return vec![value.value];
                        }
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
    let shader = rendering_device_scene_shader_for_key(shader_key)?;
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
        &MaterialParameters {
            storage,
            pass,
            scalar_overrides: &[],
        },
        resolved_color,
        resolved_alpha,
    );
    Some([values[0], values[1], values[2], values[3]])
}

fn iris_fragment_values(
    parameters: &MaterialParameters<'_>,
    shader_key: &str,
    storage: &SceneStorage,
    material: SceneMaterialHandle,
    scene_time_seconds: f32,
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
    values[4..16].copy_from_slice(&iris_draw_values(storage, material, scene_time_seconds)[..12]);
    values
}

fn opacity_values(parameters: &MaterialParameters<'_>) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = parameters.scalar(&["alpha", "opacity"], 1.0);
    values[4..8].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
    values
}

fn bool_float(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

#[cfg(test)]
#[path = "material_uniform/tests.rs"]
mod tests;
