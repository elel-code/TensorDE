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

use crate::engine::scene::{
    INVALID_MATERIAL_ID, SceneMaterialConstantRecord, SceneMaterialHandle,
    SceneMaterialPassRecord, SceneRenderingDeviceMeshDraw, SceneStorage, SceneTextureRecord,
};
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneParameterLayout, native_vulkan_scene_shader_for_key,
};

pub(super) const SCENE_MATERIAL_UNIFORM_BYTES: u64 = 320;
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
    let mut payload =
        Vec::with_capacity(draws.len() * SCENE_MATERIAL_UNIFORM_FLOATS * size_of::<f32>());
    for draw in draws {
        for value in material_uniform_values(storage, draw, scene_time_seconds, spectrum) {
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
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let Some(pass) = first_material_pass(storage, draw.material) else {
        return [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    };
    let shader_key = storage.string(pass.shader_key).unwrap_or_default();
    let layout = native_vulkan_scene_shader_for_key(shader_key)
        .map(|shader| shader.parameter_layout)
        .unwrap_or(BuiltinSceneParameterLayout::None);
    let parameters = MaterialParameters { storage, pass };
    match layout {
        BuiltinSceneParameterLayout::None => [0.0; SCENE_MATERIAL_UNIFORM_FLOATS],
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
        BuiltinSceneParameterLayout::RoundedMask => rounded_mask_values(&parameters),
        BuiltinSceneParameterLayout::Scroll => scroll_values(&parameters, scene_time_seconds),
        BuiltinSceneParameterLayout::Skew => skew_values(&parameters),
        BuiltinSceneParameterLayout::TechCircle => {
            tech_circle_values(&parameters, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::FoliageSway => {
            foliage_sway_values(&parameters, storage, draw, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::Shake => shake_values(&parameters, scene_time_seconds),
        BuiltinSceneParameterLayout::WaterWaves => waterwaves_values(
            &parameters,
            storage,
            shader_key,
            scene_time_seconds,
        ),
        BuiltinSceneParameterLayout::WaterRipple => waterripple_values(
            &parameters,
            storage,
            shader_key,
            scene_time_seconds,
        ),
        BuiltinSceneParameterLayout::WaterFlow => {
            waterflow_values(&parameters, storage, scene_time_seconds)
        }
    }
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

fn scene_audio_spectrum32() -> Option<[f32; 32]> {
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
    values[12] = parameters.scalar(&["ui_editor_properties_5_sector_1_width"], 0.3);
    values[13] = parameters.scalar(&["ui_editor_properties_5_sector_segment_count"], 5.0);
    values[14] = parameters.scalar(&["ui_editor_properties_5_sector_segment_width"], 0.75);
    values
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
    values[11] = parameters.scalar(
        &["ui_editor_properties_time_offset", "time_offset"],
        0.0,
    );
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

fn opacity_values(
    parameters: &MaterialParameters<'_>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
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

fn object_source_texture_resolution(
    storage: &SceneStorage,
    object: crate::engine::scene::SceneObjectHandle,
) -> [f32; 4] {
    let mesh = storage.meshes().iter().find(|mesh| mesh.object == object);
    if let Some(texture) = mesh
        .and_then(|mesh| storage.material(mesh.material))
        .and_then(|material| storage.material_passes(material).first())
        .and_then(|pass| {
            storage
                .material_pass_textures(pass)
                .iter()
                .find(|texture| texture.slot == 0)
        })
        .and_then(|binding| storage.texture(binding.resource))
    {
        return texture_resolution(texture);
    }
    if let Some(mesh) = mesh {
        let width = mesh.width.max(1.0);
        let height = mesh.height.max(1.0);
        return [width, height, width, height];
    }
    let width = storage.project().logical_width.max(1) as f32;
    let height = storage.project().logical_height.max(1) as f32;
    [width, height, width, height]
}

fn material_texture_resolution(
    storage: &SceneStorage,
    pass: &SceneMaterialPassRecord,
    slot: u32,
) -> [f32; 4] {
    storage
        .material_pass_textures(pass)
        .iter()
        .find(|texture| texture.slot == slot)
        .and_then(|binding| storage.texture(binding.resource))
        .map(texture_resolution)
        .unwrap_or([1.0; 4])
}

fn texture_resolution(texture: &SceneTextureRecord) -> [f32; 4] {
    [
        texture.storage_width.max(1) as f32,
        texture.storage_height.max(1) as f32,
        texture.width.max(1) as f32,
        texture.height.max(1) as f32,
    ]
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
mod tests {
    use super::*;
    use crate::engine::scene::{
        SceneBinaryDocument, SceneCullMode, SceneDepthTest, SceneMaterialConstantRecord,
        SceneMaterialHandle, SceneMaterialPassRecord, SceneMaterialRecord,
        SceneMaterialTextureRecord, ScenePipelineBlend, SceneRenderingDeviceDrawPrimitive,
        SceneResourceId, SceneResourceKind, SceneResourceRecord, SceneStringId,
        SceneTextureFormat,
    };

    #[test]
    fn material_uniform_uses_default_when_draw_has_no_material() {
        let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
        let draw = draw_with_material(SceneMaterialHandle(INVALID_MATERIAL_ID));

        let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

        assert_eq!(payload.len(), SCENE_MATERIAL_UNIFORM_FLOATS * size_of::<f32>());
        assert!(payload.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn material_uniform_packs_color_constant_into_first_vec4() {
        let storage = storage_with_constants(
            "genericimage4",
            &[("tint", "[0.25,0.5,0.75,0.9]")],
        );
        let draw = draw_with_material(SceneMaterialHandle(0));

        let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

        assert_eq!(f32_from_payload(&payload, 0), 0.25);
        assert_eq!(f32_from_payload(&payload, 4), 0.5);
        assert_eq!(f32_from_payload(&payload, 8), 0.75);
        assert_eq!(f32_from_payload(&payload, 12), 0.9);
    }

    #[test]
    fn standard_material_multiplies_resolved_object_shadow_tint_and_alpha() {
        let storage = storage_with_constants(
            "genericimage4",
            &[("tint", "[0.8,0.6,0.4,0.5]")],
        );
        let mut draw = draw_with_material(SceneMaterialHandle(0));
        draw.resolved_color = crate::engine::scene::SceneVec3 {
            x: 0.25,
            y: 0.5,
            z: 0.75,
        };
        draw.resolved_alpha = 0.3;

        let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

        assert_eq!(f32_from_payload(&payload, 0), 0.2);
        assert_eq!(f32_from_payload(&payload, 4), 0.3);
        assert!((f32_from_payload(&payload, 8) - 0.3).abs() < f32::EPSILON);
        assert_eq!(f32_from_payload(&payload, 12), 0.15);
    }

    #[test]
    fn offscreen_object_source_defers_resolved_visual_to_object_composite() {
        let storage = storage_with_constants(
            "genericimage4",
            &[("tint", "[0.8,0.6,0.4,0.5]")],
        );
        let mut draw = draw_with_material(SceneMaterialHandle(0));
        draw.resolved_color = crate::engine::scene::SceneVec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        draw.resolved_alpha = 0.3;
        draw.apply_resolved_visual = false;

        let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

        assert_eq!(f32_from_payload(&payload, 0), 0.8);
        assert_eq!(f32_from_payload(&payload, 4), 0.6);
        assert_eq!(f32_from_payload(&payload, 8), 0.4);
        assert_eq!(f32_from_payload(&payload, 12), 0.5);
    }

    #[test]
    fn object_composite_applies_only_resolved_visual_not_base_material_twice() {
        let storage = storage_with_constants(
            "genericimage4",
            &[("tint", "[0.8,0.6,0.4,0.5]")],
        );
        let mut draw = draw_with_material(SceneMaterialHandle(0));
        draw.primitive = crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
        draw.resolved_color = crate::engine::scene::SceneVec3 {
            x: 0.1,
            y: 0.2,
            z: 0.3,
        };
        draw.resolved_alpha = 0.3;

        let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

        assert_eq!(f32_from_payload(&payload, 0), 0.1);
        assert_eq!(f32_from_payload(&payload, 4), 0.2);
        assert_eq!(f32_from_payload(&payload, 8), 0.3);
        assert_eq!(f32_from_payload(&payload, 12), 0.3);
    }

    #[test]
    fn waterwaves_uniform_uses_named_lanes_and_scene_time() {
        let storage = storage_with_constants(
            "effects/waterwaves__SLOTS_3__DUALWAVES_1",
            &[
                ("speed", "2.0"),
                ("scale", "5.0"),
                ("strength", "0.03"),
                ("direction", "-0.9"),
                ("speed2", "3.0"),
                ("scale2", "7.0"),
                ("direction2", "0.5"),
                ("offset2", "0.25"),
                ("exponent", "1.75"),
                ("exponent2", "2.25"),
            ],
        );
        let payload = pack_scene_material_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            4.25,
        );

        assert_eq!(f32_from_payload(&payload, 0), 4.25);
        assert_eq!(f32_from_payload(&payload, 4), 2.0);
        assert_eq!(f32_from_payload(&payload, 8), 5.0);
        assert_eq!(f32_from_payload(&payload, 12), 0.03);
        assert_eq!(f32_from_payload(&payload, 16), -0.9);
        assert_eq!(f32_from_payload(&payload, 20), 3.0);
        assert_eq!(f32_from_payload(&payload, 24), 7.0);
        assert_eq!(f32_from_payload(&payload, 28), 0.5);
        assert_eq!(f32_from_payload(&payload, 32), 0.25);
        assert_eq!(f32_from_payload(&payload, 36), 1.0);
        assert_eq!(f32_from_payload(&payload, 40), 1.75);
        assert_eq!(f32_from_payload(&payload, 44), 2.25);
    }

    #[test]
    fn rounded_mask_uniform_packs_sdf_shape_parameters() {
        let storage = storage_with_constants(
            "workshop/3083593512/effects/rounded_mask__SLOTS_1__B_SQUARE_0__C_ALPHA_ONLY_0__SOFT_1",
            &[
                ("Color", "\"0.2 0.4 0.6\""),
                ("Radius", "0.35"),
                ("Size", "\"0.8 0.9\""),
                ("Softness", "1.75"),
                ("ui_editor_properties_opacity", "0.7"),
            ],
        );
        let payload = pack_scene_material_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            0.0,
        );

        for (lane, expected) in [0.2, 0.4, 0.6, 0.35, 0.8, 0.9, 1.75, 0.7]
            .into_iter()
            .enumerate()
        {
            assert_eq!(f32_from_payload(&payload, lane * 4), expected);
        }
    }

    #[test]
    fn scroll_uniform_packs_time_signed_speed_and_repeat_inputs() {
        let storage = storage_with_constants(
            "effects/scroll__SLOTS_1",
            &[("speedx", "-0.4"), ("speedy", "0.25"), ("repeat", "\"2 3\"")],
        );
        let payload = pack_scene_material_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            6.5,
        );

        assert_eq!(f32_from_payload(&payload, 0), 6.5);
        assert_eq!(f32_from_payload(&payload, 4), -0.4);
        assert_eq!(f32_from_payload(&payload, 8), 0.25);
        assert_eq!(f32_from_payload(&payload, 16), 2.0);
        assert_eq!(f32_from_payload(&payload, 20), 3.0);
    }

    #[test]
    fn skew_uniform_packs_authored_edge_offsets() {
        let storage = storage_with_constants(
            "effects/skew__SLOTS_1",
            &[
                ("top", "0.1"),
                ("bottom", "-0.39"),
                ("left", "0.2"),
                ("right", "-0.3"),
            ],
        );
        let payload = pack_scene_material_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            0.0,
        );

        for (lane, expected) in [0.1, -0.39, 0.2, -0.3].into_iter().enumerate() {
            assert_eq!(f32_from_payload(&payload, lane * 4), expected);
        }
    }

    #[test]
    fn tech_circle_uniform_packs_bound_sector_value_and_time() {
        let storage = storage_with_constants(
            "workshop/2123274886/effects/tech_circle__SLOTS_1__SECTOR_SEGMENTS_1",
            &[
                ("ui_editor_properties_1_color", "{\"value\":\"0.2 0.4 0.6\"}"),
                ("ui_editor_properties_2_alpha", "0.8"),
                ("ui_editor_properties_3_speed", "0.1"),
                ("ui_editor_properties_4_ring_1_radius", "0.54"),
                ("ui_editor_properties_4_ring_1_width", "0.04"),
                ("ui_editor_properties_5_sector_1_width", "{\"script\":\"ignored\",\"value\":0.3}"),
                ("ui_editor_properties_5_sector_segment_count", "5"),
                ("ui_editor_properties_5_sector_segment_width", "0.75"),
            ],
        );
        let payload = pack_scene_material_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            4.5,
        );

        assert_eq!(f32_from_payload(&payload, 0), 0.2);
        assert_eq!(f32_from_payload(&payload, 4), 0.4);
        assert_eq!(f32_from_payload(&payload, 8), 0.6);
        assert_eq!(f32_from_payload(&payload, 12), 0.8);
        assert_eq!(f32_from_payload(&payload, 16), 4.5);
        assert_eq!(f32_from_payload(&payload, 20), 0.1);
        assert_eq!(f32_from_payload(&payload, 28), 0.54);
        assert_eq!(f32_from_payload(&payload, 32), 0.04);
        assert_eq!(f32_from_payload(&payload, 48), 0.3);
        assert_eq!(f32_from_payload(&payload, 52), 5.0);
        assert_eq!(f32_from_payload(&payload, 56), 0.75);
    }

    #[test]
    fn audio_bars_uniform_packs_zero_spectrum_baseline_shape() {
        let storage = storage_with_constants(
            "workshop/3082978660/effects/Simple_Audio_Bars__SLOTS_1__SHAPE_7",
            &[
                ("Bar Color", "{\"value\":\"0.2 0.4 0.6\"}"),
                ("ui_editor_properties_opacity", "0.8"),
                ("Bar Count", "12"),
                ("Bar Spacing", "0.31"),
                ("Lower/Upper Bar Bounds", "\"0.1 0.1\""),
                ("Minimum Height (Will be multiplied by the bar width) ", "1"),
                ("Radius", "1"),
                ("Volume Factor", "0.5"),
                ("Anti-alias blurring ", "\"0.01 0.04\""),
            ],
        );
        let payload = pack_scene_material_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            0.0,
        );

        for (lane, expected) in [
            0.2, 0.4, 0.6, 0.8, 12.0, 0.31, 0.1, 0.1, 1.0, 1.0, 0.5, 0.01, 0.04,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(f32_from_payload(&payload, lane * 4), expected);
        }
        assert_eq!(f32_from_payload(&payload, 16 * 4), 0.0);
        assert_eq!(f32_from_payload(&payload, 48 * 4), 0.0);
    }

    #[test]
    fn audio_bars_uniform_duplicates_mono_spectrum_into_stereo_vec4_arrays() {
        let storage = storage_with_constants(
            "workshop/3082978660/effects/Simple_Audio_Bars__SLOTS_1__SHAPE_7",
            &[],
        );
        let spectrum = std::array::from_fn(|band| band as f32 / 31.0);
        let payload = pack_scene_material_uniforms_with_spectrum(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            0.0,
            Some(&spectrum),
        );

        assert_eq!(payload.len(), SCENE_MATERIAL_UNIFORM_BYTES as usize);
        for band in 0..32 {
            assert_eq!(f32_from_payload(&payload, (16 + band) * 4), spectrum[band]);
            assert_eq!(f32_from_payload(&payload, (48 + band) * 4), spectrum[band]);
        }
    }

    #[test]
    fn scene_audio_spectrum_diagnostic_override_is_bounded_and_explicit() {
        assert_eq!(parse_scene_audio_spectrum32("flat:0.75"), Some([0.75; 32]));
        assert!(parse_scene_audio_spectrum32("flat:1.1").is_none());
        let bands = (0..32)
            .map(|band| (band as f32 / 31.0).to_string())
            .collect::<Vec<_>>()
            .join(",");
        let parsed = parse_scene_audio_spectrum32(&bands).expect("32 bands");
        assert_eq!(parsed[0], 0.0);
        assert_eq!(parsed[31], 1.0);
        assert!(parse_scene_audio_spectrum32("0,1").is_none());
    }

    #[test]
    fn waterflow_uniform_packs_motion_and_logical_flow_extent() {
        let storage = storage_with_padded_mask("effects/waterflow__SLOTS_7", 128, 64, 100, 50);
        let mut document = storage.document().clone();
        document.strings.extend([
            "speed".to_owned(),
            "0.03".to_owned(),
            "feather".to_owned(),
            "0.5".to_owned(),
            "strength".to_owned(),
            "2.6".to_owned(),
            "phasescale".to_owned(),
            "2.99".to_owned(),
        ]);
        document.material_passes[0].constant_start = 0;
        document.material_passes[0].constant_count = 4;
        document.material_constants = (0..4)
            .map(|index| SceneMaterialConstantRecord {
                name: SceneStringId(1 + index * 2),
                value_json: SceneStringId(2 + index * 2),
            })
            .collect();
        let storage = SceneStorage::from_document(document).expect("waterflow storage");
        let payload = pack_scene_material_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            4.25,
        );

        for (lane, expected) in [4.25, 0.03, 0.5, 2.6, 2.99].into_iter().enumerate() {
            assert_eq!(f32_from_payload(&payload, lane * 4), expected);
        }
        assert_eq!(f32_from_payload(&payload, 32), 128.0);
        assert_eq!(f32_from_payload(&payload, 36), 64.0);
        assert_eq!(f32_from_payload(&payload, 40), 100.0);
        assert_eq!(f32_from_payload(&payload, 44), 50.0);
    }

    #[test]
    fn waterwaves_uniform_uses_storage_and_logical_mask_extents() {
        let storage = storage_with_padded_mask("effects/waterwaves__SLOTS_3", 128, 64, 100, 50);
        let payload = pack_scene_material_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            0.0,
        );

        assert_eq!(f32_from_payload(&payload, 48), 128.0);
        assert_eq!(f32_from_payload(&payload, 52), 64.0);
        assert_eq!(f32_from_payload(&payload, 56), 100.0);
        assert_eq!(f32_from_payload(&payload, 60), 50.0);
    }

    #[test]
    fn foliage_sway_uniform_uses_authored_uv_motion_parameters() {
        let storage = storage_with_constants(
            "workshop/2790231929/effects/foliagesway__SLOTS_1",
            &[
                ("speeduv", "5.0"),
                ("strength", "0.5"),
                ("phase", "2.0"),
                ("power", "2.0"),
                ("scale", "0.05"),
                ("ratio", "2.11"),
                ("scrolldirection", "0.25"),
            ],
        );
        let payload = pack_scene_material_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            3.5,
        );

        assert_eq!(f32_from_payload(&payload, 0), 3.5);
        assert_eq!(f32_from_payload(&payload, 4), 5.0);
        assert_eq!(f32_from_payload(&payload, 8), 0.5);
        assert_eq!(f32_from_payload(&payload, 12), 2.0);
        assert_eq!(f32_from_payload(&payload, 16), 2.0);
        assert_eq!(f32_from_payload(&payload, 20), 0.05);
        assert_eq!(f32_from_payload(&payload, 24), 2.11);
        assert_eq!(f32_from_payload(&payload, 28), 0.25);
    }

    #[test]
    fn opacity_uniform_maps_instance_alpha() {
        let storage = storage_with_constants(
            "effects/opacity__SLOTS_1",
            &[("alpha", "0.97")],
        );
        let payload = pack_scene_material_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            0.0,
        );

        assert_eq!(f32_from_payload(&payload, 0), 0.97);
    }

    fn storage_with_constants(shader: &str, constants: &[(&str, &str)]) -> SceneStorage {
        let mut strings = vec![shader.to_owned()];
        let mut material_constants = Vec::with_capacity(constants.len());
        for (name, value) in constants {
            let name_id = SceneStringId(strings.len() as u32);
            strings.push((*name).to_owned());
            let value_id = SceneStringId(strings.len() as u32);
            strings.push((*value).to_owned());
            material_constants.push(SceneMaterialConstantRecord {
                name: name_id,
                value_json: value_id,
            });
        }
        SceneStorage::from_document(SceneBinaryDocument {
            strings,
            materials: vec![SceneMaterialRecord {
                id: SceneMaterialHandle(0),
                resource: SceneResourceId::NONE,
                pass_start: 0,
                pass_count: 1,
            }],
            material_passes: vec![SceneMaterialPassRecord {
                material: SceneMaterialHandle(0),
                shader_key: SceneStringId(0),
                target: SceneStringId::NONE,
                texture_start: 0,
                texture_count: 0,
                constant_start: 0,
                constant_count: material_constants.len() as u32,
                pipeline_blend: ScenePipelineBlend::Normal,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
                alpha_writing: SceneStringId::NONE,
                clear_target: false,
            }],
            material_constants,
            ..SceneBinaryDocument::default()
        })
        .expect("storage")
    }

    fn storage_with_padded_mask(
        shader: &str,
        storage_width: u32,
        storage_height: u32,
        width: u32,
        height: u32,
    ) -> SceneStorage {
        let storage = storage_with_constants(shader, &[]);
        let mut document = storage.document().clone();
        let resource = SceneResourceId(7);
        document.resources.push(SceneResourceRecord {
            id: resource,
            kind: SceneResourceKind::TextureTex,
            path: SceneStringId::NONE,
            source: SceneStringId::NONE,
            payload_offset: 0,
            payload_len: 0,
        });
        document.textures.push(SceneTextureRecord {
            resource,
            format: SceneTextureFormat::Bc4UnormBlock,
            source_runtime_format: 9,
            payload_format: 0,
            sampler_flags: 0,
            width,
            height,
            storage_width,
            storage_height,
            mip_start: 0,
            mip_count: 0,
            texv_tag: SceneStringId::NONE,
            texb_tag: SceneStringId::NONE,
            payload_offset: 0,
            payload_len: 0,
        });
        document.material_textures.push(SceneMaterialTextureRecord {
            slot: 1,
            resource,
            path: SceneStringId::NONE,
        });
        document.material_passes[0].texture_count = 1;
        SceneStorage::from_document(document).expect("storage with padded mask")
    }

    fn draw_with_material(material: SceneMaterialHandle) -> SceneRenderingDeviceMeshDraw {
        SceneRenderingDeviceMeshDraw {
            primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            mesh_index: 0,
            resolved_object_index: 0,
            clip_transform: [[0.0; 4]; 4],
            authored_source_extent: [0.0; 2],
            skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
            skinning_palette_count: 0,
            resolved_color: crate::engine::scene::SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            resolved_alpha: 1.0,
            apply_resolved_visual: true,
            object: crate::engine::scene::SceneObjectHandle(0),
            material,
            vertex_start: 0,
            vertex_count: 4,
            index_start: 0,
            index_count: 6,
        }
    }

    fn f32_from_payload(payload: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
    }
}
