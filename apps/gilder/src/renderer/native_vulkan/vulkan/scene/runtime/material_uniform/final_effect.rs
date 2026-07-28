use super::*;

pub(super) fn final_effect_program_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    shader_key: &str,
    scene_time_seconds: f32,
    spectrum: Option<&[f32; 32]>,
    audio_material_values: &[ResolvedAudioBandMaterialValue],
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    match shader_key.to_ascii_lowercase().as_str() {
        "we/image-waterwaves-final" => {
            final_waterwaves_values(parameters, storage, draw, scene_time_seconds)
        }
        "we/image-waterripple-final" | "we/image-waterripple-modulate-final" => {
            final_waterripple_values(parameters, storage, draw, scene_time_seconds)
        }
        "we/image-scroll-final" => final_scroll_values(parameters, draw, scene_time_seconds),
        "we/image-colorkey-scroll-final" => {
            final_colorkey_scroll_values(parameters, draw, scene_time_seconds)
        }
        "we/image-cloudmotion-final" => {
            final_cloudmotion_values(parameters, storage, draw, scene_time_seconds)
        }
        "we/puppet-opacity-final" | "we/puppet-opacity-clipping-final" => {
            final_puppet_opacity_values(parameters, storage, draw)
        }
        "we/puppet-iris-waterripple-final" | "we/puppet-iris-waterripple-clipping-final" => {
            final_puppet_iris_waterripple_values(parameters, storage, draw, scene_time_seconds)
        }
        "we/flat-rounded-opacity-final" => final_flat_rounded_opacity_values(parameters, draw),
        "we/tech-circle-final" => final_tech_circle_values(
            parameters,
            draw,
            scene_time_seconds,
            audio_material_value(
                audio_material_values,
                draw,
                SceneAudioBandMaterialTarget::TechCircleSectorWidth,
            ),
        ),
        "we/audio-bars-final" => final_audio_bars_values(parameters, storage, draw, spectrum),
        "we/framebuffer-water-quantized-water-opacity" => {
            framebuffer_water_opacity_values(parameters, draw, scene_time_seconds)
        }
        "we/framebuffer-water-quantized-shake-final" => framebuffer_water_shake_values(
            parameters,
            storage,
            draw,
            scene_time_seconds,
        ),
        _ => [0.0; SCENE_MATERIAL_UNIFORM_FLOATS],
    }
}

fn framebuffer_water_opacity_values(
    parameters: &MaterialParameters<'_>,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["waves.speed"], 5.0);
    values[2] = parameters.scalar(&["waves.scale"], 200.0);
    values[3] = parameters.scalar(&["waves.strength"], 0.1);
    values[4] = parameters.scalar(&["waves.direction"], 0.0);
    values[5] = parameters.scalar(&["waves.exponent"], 1.0);
    values[6] = parameters.scalar(&["opacity.alpha", "opacity.opacity"], 1.0);
    values[8] = bool_float(draw_effect_enabled(draw, 0));
    values[9] = bool_float(draw_effect_enabled(draw, 1));
    values
}

fn framebuffer_water_shake_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["shake.speed"], 1.0);
    values[2] = parameters.scalar(&["shake.strength"], 0.1);
    values[4..6].copy_from_slice(&[0.0, 1.0]);
    values[6..8].copy_from_slice(&[1.0, 1.0]);
    set_vector(
        &mut values,
        4,
        &parameters.values(&["shake.bounds"]),
        2,
    );
    set_vector(
        &mut values,
        6,
        &parameters.values(&["shake.friction"]),
        2,
    );
    values[8..12].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values[12] = bool_float(draw_effect_enabled(draw, 0));
    values
}

fn final_cloudmotion_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&final_effect_color(parameters, draw));
    values[4] = scene_time_seconds;
    values[5] = parameters.scalar(&["cloud.ui_editor_properties_speed", "cloud.speed"], 0.02);
    values[6] = if draw_effect_enabled(draw, 0) {
        parameters.scalar(&["cloud.ui_editor_properties_amount", "cloud.amount"], 0.1)
    } else {
        0.0
    };
    values[7] = parameters.scalar(
        &["cloud.ui_editor_properties_direction", "cloud.direction"],
        1.5707963,
    );
    values[8] = parameters.scalar(
        &["cloud.ui_editor_properties_granularity", "cloud.scale"],
        2.0,
    );
    values[9] = parameters.scalar(
        &[
            "cloud.ui_editor_properties_granularity_horizontal",
            "cloud.scalex",
        ],
        0.5,
    );
    values[10] = storage.project().logical_width.max(1) as f32
        / storage.project().logical_height.max(1) as f32;
    values
}

fn final_tech_circle_values(
    parameters: &MaterialParameters<'_>,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
    sector_width_override: Option<f32>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&final_effect_color(parameters, draw));
    values[4..7].copy_from_slice(&[1.0, 1.0, 1.0]);
    set_vector(
        &mut values,
        4,
        &parameters.values(&["tech.ui_editor_properties_1_color"]),
        3,
    );
    values[7] = if draw_effect_enabled(draw, 0) {
        parameters.scalar(&["tech.ui_editor_properties_2_alpha"], 1.0)
    } else {
        0.0
    };
    values[8] = scene_time_seconds;
    values[9] = parameters.scalar(&["tech.ui_editor_properties_3_speed"], 0.1);
    values[10] = parameters.scalar(&["tech.ui_editor_properties_6_skew"], 0.0);
    values[11] = parameters.scalar(&["tech.ui_editor_properties_4_ring_1_radius"], 0.5);
    values[12] = parameters.scalar(&["tech.ui_editor_properties_4_ring_1_width"], 0.2);
    values[13] = parameters.scalar(&["tech.ui_editor_properties_4_ring_2_segment_count"], 2.0);
    values[14] = parameters.scalar(&["tech.ui_editor_properties_4_ring_2_segment_width"], 0.25);
    values[15] = parameters.scalar(&["tech.ui_editor_properties_5_sector_1_offset"], 0.0);
    values[16] = sector_width_override
        .unwrap_or_else(|| parameters.scalar(&["tech.ui_editor_properties_5_sector_1_width"], 0.3));
    values[17] = parameters.scalar(&["tech.ui_editor_properties_5_sector_segment_count"], 5.0);
    values[18] = parameters.scalar(&["tech.ui_editor_properties_5_sector_segment_width"], 0.75);
    values
}

pub(super) fn final_audio_bars_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    spectrum: Option<&[f32; 32]>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&final_effect_color(parameters, draw));
    values[4..7].copy_from_slice(&[1.0, 1.0, 1.0]);
    set_vector(&mut values, 4, &parameters.values(&["audio.Bar Color"]), 3);
    values[7] = if draw_effect_enabled(draw, 0) {
        parameters.scalar(&["audio.ui_editor_properties_opacity"], 1.0)
    } else {
        0.0
    };
    values[8] = parameters.scalar(&["audio.Bar Count"], 32.0);
    values[9] = parameters.scalar(&["audio.Bar Spacing"], 0.1);
    values[10..12].copy_from_slice(&[0.0, 1.0]);
    set_vector(
        &mut values,
        10,
        &parameters.values(&["audio.Lower/Upper Bar Bounds"]),
        2,
    );
    values[12] = parameters.scalar(
        &["audio.Minimum Height (Will be multiplied by the bar width) "],
        0.0,
    );
    values[13] = parameters.scalar(&["audio.Radius"], 1.0);
    values[14] = parameters.scalar(&["audio.Volume Factor"], 1.0);
    values[15] = parameters
        .values(&["audio.Anti-alias blurring "])
        .first()
        .copied()
        .unwrap_or(0.05);
    if draw_effect_enabled(draw, 1) {
        values[16] = parameters.scalar(&["skew.top"], 0.0);
        values[17] = parameters.scalar(&["skew.bottom"], 0.0);
        values[18] = parameters.scalar(&["skew.left"], 0.0);
        values[19] = parameters.scalar(&["skew.right"], 0.0);
    }
    if draw_effect_enabled(draw, 2) {
        values[20] = parameters.scalar(&["opacity.alpha", "opacity.opacity"], 1.0);
        values[21] = parameters.scalar(&["opacity.mask_enabled"], 0.0);
    } else {
        values[20] = 1.0;
    }
    values[24..28].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 0));
    values[28] = parameters
        .values(&["audio.Anti-alias blurring "])
        .get(1)
        .copied()
        .unwrap_or(0.0);
    let source_width = draw.authored_source_extent[0];
    let source_height = draw.authored_source_extent[1];
    let (source_width, source_height) = if source_width.is_finite()
        && source_height.is_finite()
        && source_width > 0.0
        && source_height > 0.0
    {
        (source_width, source_height)
    } else {
        (
            storage.project().logical_width.max(1) as f32,
            storage.project().logical_height.max(1) as f32,
        )
    };
    values[29] = source_width / source_height;
    values[30] = source_width.min(source_height);
    if let Some(spectrum) = spectrum {
        values[32..64].copy_from_slice(spectrum);
        values[64..96].copy_from_slice(spectrum);
    }
    values
}

fn final_scroll_values(
    parameters: &MaterialParameters<'_>,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    final_scroll_values_for_stage(parameters, draw, scene_time_seconds, 0)
}

fn final_scroll_values_for_stage(
    parameters: &MaterialParameters<'_>,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
    local_index: usize,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&final_effect_color(parameters, draw));
    values[4] = scene_time_seconds;
    if draw_effect_enabled(draw, local_index) {
        values[5] = parameters.scalar(&["scroll.speedx"], 0.2);
        values[6] = parameters.scalar(&["scroll.speedy"], 0.2);
        values[7] = 1.0;
    }
    values[8..10].copy_from_slice(&[1.0, 1.0]);
    set_vector(&mut values, 8, &parameters.values(&["scroll.repeat"]), 2);
    values
}

fn final_colorkey_scroll_values(
    parameters: &MaterialParameters<'_>,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = final_scroll_values_for_stage(parameters, draw, scene_time_seconds, 1);
    values[12] = 1.0;
    values[16..19].copy_from_slice(&[1.0, 1.0, 1.0]);
    if draw_effect_enabled(draw, 0) {
        values[12] = parameters.scalar(&["colorkey.alpha"], 0.0);
        values[13] = parameters.scalar(&["colorkey.fuzziness"], 0.0);
        values[14] = parameters.scalar(&["colorkey.tolerance"], 0.1);
        values[15] = parameters.scalar(&["colorkey.invert"], 0.0);
        set_vector(&mut values, 16, &parameters.values(&["colorkey.color"]), 3);
        values[19] = parameters.scalar(&["colorkey.flatten"], 0.0);
    }
    values
}

fn final_puppet_opacity_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&final_effect_color(parameters, draw));
    if draw_effect_enabled(draw, 0) {
        values[4] = parameters.scalar(&["opacity.alpha", "opacity.opacity"], 1.0);
        values[5] = parameters.scalar(&["opacity.mask_enabled"], 0.0);
    } else {
        values[4] = 1.0;
        values[5] = 0.0;
    }
    values[8..12].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values
}

fn final_puppet_iris_waterripple_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&final_effect_color(parameters, draw));
    values[4] = scene_time_seconds;
    values[5] = parameters.scalar(&["iris.speed"], 1.0);
    values[6] = parameters.scalar(&["iris.rough"], 0.2);
    values[7] = parameters.scalar(&["iris.noiseamount"], 0.5);
    values[8..10].copy_from_slice(&[1.0, 1.0]);
    set_vector(&mut values, 8, &parameters.values(&["iris.scale"]), 2);
    values[10] = parameters.scalar(&["iris.phase"], 0.0);
    values[11] = parameters.scalar(&["iris.mask_enabled"], 0.0);
    values[12..16].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values[16..19].copy_from_slice(&[1.0, 1.0, 1.0]);
    set_vector(
        &mut values,
        16,
        &parameters.values(&["iris.color", "iris.eyecolor"]),
        3,
    );
    values[19] = parameters.scalar(&["iris.background"], 0.0);
    let ratio = parameters.scalar(&["ripple.ratio"], 1.0);
    values[20] = scene_time_seconds;
    values[21] = parameters.scalar(&["ripple.animationspeed"], 0.15);
    values[22] = parameters.scalar(&["ripple.scale"], 1.0);
    values[23] = parameters.scalar(&["ripple.scrollspeed"], 0.0);
    values[24] = parameters.scalar(&["ripple.scrolldirection", "ripple.direction"], 0.0);
    values[25] = parameters.scalar(&["ripple.ripplestrength", "ripple.strength"], 0.1);
    values[26] = ratio;
    values[27] = ratio;
    values[28] = parameters.scalar(&["ripple.mask_enabled"], 0.0);
    values[29] = parameters.scalar(&["ripple.normal_enabled"], 0.0);
    values[32..36].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 2));
    values[36] = bool_float(draw_effect_enabled(draw, 0));
    values[37] = bool_float(draw_effect_enabled(draw, 1));
    values
}

fn final_flat_rounded_opacity_values(
    parameters: &MaterialParameters<'_>,
    draw: &SceneRenderingDeviceMeshDraw,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..3].copy_from_slice(&[1.0, 1.0, 1.0]);
    set_vector(&mut values, 0, &parameters.values(&["rounded.Color"]), 3);
    values[3] = parameters.scalar(&["rounded.Radius"], 0.5);
    values[4..6].copy_from_slice(&[1.0, 1.0]);
    set_vector(&mut values, 4, &parameters.values(&["rounded.Size"]), 2);
    values[6] = parameters.scalar(&["rounded.Softness"], 0.5);
    values[7] = parameters.scalar(&["rounded.ui_editor_properties_opacity"], 1.0);
    values[8] = parameters.scalar(&["rounded.Border width", "rounded.BorderWidth"], 0.025);
    values[9] = if draw_effect_enabled(draw, 1) {
        parameters.scalar(&["opacity.alpha", "opacity.opacity"], 1.0)
    } else {
        1.0
    };
    values[10] = bool_float(draw_effect_enabled(draw, 0));
    values[12..16].copy_from_slice(&[
        draw.resolved_color.x,
        draw.resolved_color.y,
        draw.resolved_color.z,
        draw.resolved_alpha,
    ]);
    values
}

fn final_effect_color(
    parameters: &MaterialParameters<'_>,
    draw: &SceneRenderingDeviceMeshDraw,
) -> [f32; 4] {
    let mut color = [1.0; 4];
    set_vector(
        &mut color,
        0,
        &parameters.values(&["base.color4", "base.g_color4", "base.color", "base.tint"]),
        4,
    );
    color[0] *= draw.resolved_color.x;
    color[1] *= draw.resolved_color.y;
    color[2] *= draw.resolved_color.z;
    color[3] *= draw.resolved_alpha;
    color
}

pub(super) fn final_waterwaves_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&final_effect_color(parameters, draw));
    let speed = parameters.scalar(&["effect.speed"], 5.0);
    let scale = parameters.scalar(&["effect.scale"], 200.0);
    values[4] = scene_time_seconds;
    values[5] = speed;
    values[6] = scale;
    values[7] = if draw_effect_enabled(draw, 0) {
        parameters.scalar(&["effect.strength"], 0.1)
    } else {
        0.0
    };
    values[8] = parameters.scalar(&["effect.direction"], 0.0);
    values[9] = parameters.scalar(&["effect.speed2"], speed);
    values[10] = parameters.scalar(&["effect.scale2"], scale);
    values[11] = parameters.scalar(&["effect.direction2"], 0.0);
    values[12] = parameters.scalar(&["effect.offset2"], 0.0);
    values[13] = parameters.scalar(&["effect.dualwaves"], 0.0);
    values[14] = parameters.scalar(&["effect.exponent"], 1.0);
    values[15] = parameters.scalar(&["effect.exponent2"], 1.0);
    values[16] = parameters.scalar(&["effect.mask_enabled"], 0.0);
    values[20..24].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values
}

pub(super) fn final_waterripple_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&final_effect_color(parameters, draw));
    let ratio = parameters.scalar(&["effect.ratio"], 1.0);
    values[4] = scene_time_seconds;
    values[5] = parameters.scalar(&["effect.animationspeed"], 0.15);
    values[6] = parameters.scalar(&["effect.scale"], 1.0);
    values[7] = parameters.scalar(&["effect.scrollspeed"], 0.0);
    values[8] = parameters.scalar(&["effect.scrolldirection", "effect.direction"], 0.0);
    values[9] = if draw_effect_enabled(draw, 0) {
        parameters.scalar(&["effect.ripplestrength", "effect.strength"], 0.1)
    } else {
        0.0
    };
    values[10] = ratio;
    values[11] = 1.0;
    values[12] = parameters.scalar(&["effect.mask_enabled"], 0.0);
    values[15] = ratio;
    values[16..20].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values
}

pub(super) fn ripple_flow_composite_values(
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
    values[5] = parameters.scalar(&["flow.speed"], 1.0);
    values[6] = parameters.scalar(&["flow.feather"], 0.4);
    values[7] = if draw_effect_enabled(draw, 0) {
        parameters.scalar(&["flow.strength"], 1.0)
    } else {
        0.0
    };
    values[8] = parameters.scalar(&["flow.phasescale"], 2.0);
    values[12..16].copy_from_slice(&material_texture_resolution(storage, parameters.pass, 1));
    values
}

pub(super) fn object_source_texture_resolution(
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

pub(super) fn material_texture_resolution(
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
