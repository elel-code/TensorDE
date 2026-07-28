
impl ScenePuppetAnimationClip {
    fn validate(&self, node_id: &str, bone_count: usize) -> Result<(), SceneError> {
        if self.fps <= 0.0 || !self.fps.is_finite() {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} puppet clip {} fps must be positive and finite",
                self.id
            )));
        }
        if self.frame_count == 0 {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} puppet clip {} must contain at least one frame",
                self.id
            )));
        }
        if self.bones.len() != bone_count {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} puppet clip {} bone count {} does not match skin bone count {bone_count}",
                self.id,
                self.bones.len()
            )));
        }
        let expected_sample_count = usize::try_from(self.frame_count)
            .ok()
            .and_then(|frame_count| frame_count.checked_add(1))
            .ok_or_else(|| {
                SceneError::invalid(format!(
                    "scene node {node_id:?} puppet clip {} frame_count overflows sample count",
                    self.id
                ))
            })?;
        for (bone_index, bone) in self.bones.iter().enumerate() {
            bone.validate(node_id, self.id, bone_index, expected_sample_count)?;
        }
        Ok(())
    }

    fn sample(
        &self,
        layer: &ScenePuppetAnimationLayer,
        time_ms: u64,
        bone_count: usize,
    ) -> Option<Vec<ScenePuppetTransform>> {
        if self.bones.len() != bone_count {
            return None;
        }
        let timing = self.sample_timing(layer, time_ms)?;
        let mut pose = Vec::with_capacity(bone_count);
        for bone in &self.bones {
            let first = *bone.frames.get(timing.frame0)?;
            let second = *bone.frames.get(timing.frame1)?;
            pose.push(first.lerp(second, timing.mix));
        }
        Some(pose)
    }

    fn sample_timing(
        &self,
        layer: &ScenePuppetAnimationLayer,
        time_ms: u64,
    ) -> Option<ScenePuppetAnimationFrameTiming> {
        if self.frame_count == 0 || self.fps <= 0.0 {
            return None;
        }
        let duration_frames = f64::from(self.frame_count);
        let phase = layer.initial_phase.clamp(0.0, 1.0) * duration_frames;
        let mut frame = time_ms as f64 * 0.001 * self.fps * layer.rate.max(0.0) + phase;
        if self.looping {
            frame = frame.rem_euclid(duration_frames);
        } else {
            frame = frame.clamp(0.0, duration_frames);
        }
        let last_sample_index = self.frame_count as usize;
        let frame0 = frame.floor().min(last_sample_index as f64) as usize;
        let frame1 = (frame0 + 1).min(last_sample_index);
        let mix = (frame - frame0 as f64).clamp(0.0, 1.0);
        Some(ScenePuppetAnimationFrameTiming {
            frame,
            frame0,
            frame1,
            mix,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenePuppetAnimationBone {
    #[serde(default)]
    pub frames: Vec<ScenePuppetTransform>,
}

impl ScenePuppetAnimationBone {
    fn validate(
        &self,
        node_id: &str,
        clip_id: u32,
        bone_index: usize,
        expected_sample_count: usize,
    ) -> Result<(), SceneError> {
        if self.frames.len() != expected_sample_count {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} puppet clip {clip_id} bone {bone_index} must contain frame_count + 1 ({expected_sample_count}) sampled frames, found {}",
                self.frames.len()
            )));
        }
        for (frame_index, frame) in self.frames.iter().enumerate() {
            frame.validate(
                node_id,
                &format!("puppet clip {clip_id} bone {bone_index} frame {frame_index}"),
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScenePuppetTransform {
    #[serde(default)]
    pub translation: [f64; 3],
    #[serde(default)]
    pub rotation: [f64; 3],
    #[serde(default = "scene_puppet_default_scale")]
    pub scale: [f64; 3],
    #[serde(
        default = "default_opacity",
        skip_serializing_if = "is_default_opacity"
    )]
    pub opacity: f64,
}

impl Default for ScenePuppetTransform {
    fn default() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            opacity: 1.0,
        }
    }
}

impl ScenePuppetTransform {
    fn validate(&self, node_id: &str, label: &str) -> Result<(), SceneError> {
        for (field, values) in [
            ("translation", self.translation),
            ("rotation", self.rotation),
            ("scale", self.scale),
        ] {
            for (index, value) in values.iter().enumerate() {
                if !value.is_finite() {
                    return Err(SceneError::invalid(format!(
                        "scene node {node_id:?} {label} {field}[{index}] must be finite"
                    )));
                }
            }
        }
        validate_opacity(self.opacity, label)?;
        Ok(())
    }

    fn lerp(self, target: Self, mix: f64) -> Self {
        let mix = mix.clamp(0.0, 1.0);
        let mut rotation = [0.0; 3];
        for (index, value) in rotation.iter_mut().enumerate() {
            *value = self.rotation[index]
                + scene_puppet_angle_delta(self.rotation[index], target.rotation[index]) * mix;
        }
        Self {
            translation: [
                scene_lerp(self.translation[0], target.translation[0], mix),
                scene_lerp(self.translation[1], target.translation[1], mix),
                scene_lerp(self.translation[2], target.translation[2], mix),
            ],
            rotation,
            scale: [
                scene_lerp(self.scale[0], target.scale[0], mix),
                scene_lerp(self.scale[1], target.scale[1], mix),
                scene_lerp(self.scale[2], target.scale[2], mix),
            ],
            opacity: scene_lerp(self.opacity, target.opacity, mix).clamp(0.0, 1.0),
        }
    }

    fn additive_blend(self, bind: Self, target: Self, mix: f64) -> Self {
        let mix = mix.clamp(0.0, 1.0);
        Self {
            translation: [
                self.translation[0] + (target.translation[0] - bind.translation[0]) * mix,
                self.translation[1] + (target.translation[1] - bind.translation[1]) * mix,
                self.translation[2] + (target.translation[2] - bind.translation[2]) * mix,
            ],
            rotation: [
                self.rotation[0]
                    + scene_puppet_angle_delta(bind.rotation[0], target.rotation[0]) * mix,
                self.rotation[1]
                    + scene_puppet_angle_delta(bind.rotation[1], target.rotation[1]) * mix,
                self.rotation[2]
                    + scene_puppet_angle_delta(bind.rotation[2], target.rotation[2]) * mix,
            ],
            scale: [
                self.scale[0] + (target.scale[0] - bind.scale[0]) * mix,
                self.scale[1] + (target.scale[1] - bind.scale[1]) * mix,
                self.scale[2] + (target.scale[2] - bind.scale[2]) * mix,
            ],
            opacity: (self.opacity + (target.opacity - bind.opacity) * mix).clamp(0.0, 1.0),
        }
    }

    fn blend_opacity_only(self, bind: Self, target: Self, mix: f64, additive: bool) -> Self {
        let mix = mix.clamp(0.0, 1.0);
        let opacity = if additive {
            self.opacity + (target.opacity - bind.opacity) * mix
        } else {
            scene_lerp(self.opacity, target.opacity, mix)
        };
        Self {
            opacity: opacity.clamp(0.0, 1.0),
            ..self
        }
    }

    fn matrix(self) -> [f64; 16] {
        let rx = scene_puppet_rotation_x_matrix(self.rotation[0]);
        let ry = scene_puppet_rotation_y_matrix(self.rotation[1]);
        let rz = scene_puppet_rotation_z_matrix(self.rotation[2]);
        let rotation = scene_puppet_matrix_mul(scene_puppet_matrix_mul(rz, ry), rx);
        let scale = scene_puppet_scale_matrix(self.scale);
        let translation = scene_puppet_translation_matrix(self.translation);
        scene_puppet_matrix_mul(translation, scene_puppet_matrix_mul(rotation, scale))
    }
}

fn scene_puppet_default_scale() -> [f64; 3] {
    [1.0, 1.0, 1.0]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenePuppetAnimationLayer {
    pub clip_id: u32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub additive: bool,
    #[serde(default)]
    pub lock_transforms: bool,
    #[serde(default = "default_opacity")]
    pub blend: f64,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_opacity")]
    pub rate: f64,
    #[serde(default)]
    pub initial_phase: f64,
}

impl ScenePuppetAnimationLayer {
    fn validate(&self, node_id: &str, clip_ids: &BTreeSet<u32>) -> Result<(), SceneError> {
        if !clip_ids.contains(&self.clip_id) {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} puppet animation layer references unknown clip {}",
                self.clip_id
            )));
        }
        for (field, value) in [
            ("blend", self.blend),
            ("rate", self.rate),
            ("initial_phase", self.initial_phase),
        ] {
            if !value.is_finite() {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} puppet animation layer {field} must be finite"
                )));
            }
        }
        if self.rate < 0.0 {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} puppet animation layer rate must be non-negative"
            )));
        }
        Ok(())
    }
}

fn scene_lerp(start: f64, end: f64, mix: f64) -> f64 {
    start + (end - start) * mix
}

fn scene_puppet_angle_delta(start: f64, end: f64) -> f64 {
    let mut delta = end - start;
    while delta > std::f64::consts::PI {
        delta -= std::f64::consts::TAU;
    }
    while delta < -std::f64::consts::PI {
        delta += std::f64::consts::TAU;
    }
    delta
}

fn scene_puppet_world_matrices<P, M>(parents: P, local_matrices: M) -> Option<Vec<[f64; 16]>>
where
    P: IntoIterator<Item = Option<usize>>,
    M: IntoIterator<Item = [f64; 16]>,
{
    let parents = parents.into_iter().collect::<Vec<_>>();
    let locals = local_matrices.into_iter().collect::<Vec<_>>();
    if parents.len() != locals.len() {
        return None;
    }
    let mut worlds = vec![scene_puppet_identity_matrix(); locals.len()];
    for index in 0..locals.len() {
        worlds[index] = if let Some(parent) = parents[index] {
            if parent >= index {
                return None;
            }
            scene_puppet_matrix_mul(worlds[parent], locals[index])
        } else {
            locals[index]
        };
    }
    Some(worlds)
}

fn scene_puppet_identity_matrix() -> [f64; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn scene_puppet_translation_matrix(translation: [f64; 3]) -> [f64; 16] {
    let mut matrix = scene_puppet_identity_matrix();
    matrix[12] = translation[0];
    matrix[13] = translation[1];
    matrix[14] = translation[2];
    matrix
}

fn scene_puppet_scale_matrix(scale: [f64; 3]) -> [f64; 16] {
    [
        scale[0], 0.0, 0.0, 0.0, 0.0, scale[1], 0.0, 0.0, 0.0, 0.0, scale[2], 0.0, 0.0, 0.0, 0.0,
        1.0,
    ]
}

fn scene_puppet_rotation_x_matrix(angle: f64) -> [f64; 16] {
    let (sin, cos) = angle.sin_cos();
    [
        1.0, 0.0, 0.0, 0.0, 0.0, cos, sin, 0.0, 0.0, -sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn scene_puppet_rotation_y_matrix(angle: f64) -> [f64; 16] {
    let (sin, cos) = angle.sin_cos();
    [
        cos, 0.0, -sin, 0.0, 0.0, 1.0, 0.0, 0.0, sin, 0.0, cos, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn scene_puppet_rotation_z_matrix(angle: f64) -> [f64; 16] {
    let (sin, cos) = angle.sin_cos();
    [
        cos, sin, 0.0, 0.0, -sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn scene_puppet_matrix_mul(a: [f64; 16], b: [f64; 16]) -> [f64; 16] {
    let mut output = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            output[column * 4 + row] = (0..4)
                .map(|index| a[index * 4 + row] * b[column * 4 + index])
                .sum();
        }
    }
    output
}

fn scene_puppet_inverse_affine_matrix(matrix: [f64; 16]) -> Option<[f64; 16]> {
    let a00 = matrix[0];
    let a01 = matrix[4];
    let a02 = matrix[8];
    let a10 = matrix[1];
    let a11 = matrix[5];
    let a12 = matrix[9];
    let a20 = matrix[2];
    let a21 = matrix[6];
    let a22 = matrix[10];
    let det = a00 * (a11 * a22 - a12 * a21) - a01 * (a10 * a22 - a12 * a20)
        + a02 * (a10 * a21 - a11 * a20);
    if !det.is_finite() || det.abs() <= f64::EPSILON {
        return None;
    }
    let inv_det = 1.0 / det;
    let b00 = (a11 * a22 - a12 * a21) * inv_det;
    let b01 = (a02 * a21 - a01 * a22) * inv_det;
    let b02 = (a01 * a12 - a02 * a11) * inv_det;
    let b10 = (a12 * a20 - a10 * a22) * inv_det;
    let b11 = (a00 * a22 - a02 * a20) * inv_det;
    let b12 = (a02 * a10 - a00 * a12) * inv_det;
    let b20 = (a10 * a21 - a11 * a20) * inv_det;
    let b21 = (a01 * a20 - a00 * a21) * inv_det;
    let b22 = (a00 * a11 - a01 * a10) * inv_det;
    let tx = matrix[12];
    let ty = matrix[13];
    let tz = matrix[14];
    Some([
        b00,
        b10,
        b20,
        0.0,
        b01,
        b11,
        b21,
        0.0,
        b02,
        b12,
        b22,
        0.0,
        -(b00 * tx + b01 * ty + b02 * tz),
        -(b10 * tx + b11 * ty + b12 * tz),
        -(b20 * tx + b21 * ty + b22 * tz),
        1.0,
    ])
}

fn scene_puppet_transform_point_3d(matrix: [f64; 16], x: f64, y: f64, z: f64) -> [f64; 3] {
    [
        matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12],
        matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13],
        matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14],
    ]
}

fn scene_puppet_matrix_rotation_z(matrix: [f64; 16]) -> Option<f64> {
    let scale_x = (matrix[0] * matrix[0] + matrix[1] * matrix[1])
        .sqrt()
        .max(f64::EPSILON);
    let angle = (matrix[1] / scale_x).atan2(matrix[0] / scale_x);
    angle.is_finite().then_some(angle)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScenePathFillRule {
    #[default]
    Nonzero,
    Evenodd,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneTextureRegion {
    pub u_min: f64,
    pub v_min: f64,
    pub u_max: f64,
    pub v_max: f64,
    pub frame_index: u32,
    pub frame_count: u32,
    #[serde(default)]
    pub columns: u32,
    #[serde(default)]
    pub rows: u32,
    #[serde(default)]
    pub fps: Option<f64>,
    #[serde(default = "default_scene_texture_region_loop_playback")]
    pub loop_playback: bool,
}

impl SceneTextureRegion {
    fn validate(self) -> Option<Self> {
        if self.u_min.is_finite()
            && self.v_min.is_finite()
            && self.u_max.is_finite()
            && self.v_max.is_finite()
            && self.u_min >= 0.0
            && self.v_min >= 0.0
            && self.u_max <= 1.0
            && self.v_max <= 1.0
            && self.u_min < self.u_max
            && self.v_min < self.v_max
            && self.frame_count > 0
            && self.frame_index < self.frame_count
            && self.columns > 0
            && self.rows > 0
            && self.fps.is_none_or(|fps| fps.is_finite() && fps > 0.0)
        {
            Some(self)
        } else {
            None
        }
    }
}

fn default_scene_texture_region_loop_playback() -> bool {
    true
}

fn scene_texture_region_from_properties(
    properties: &BTreeMap<String, Value>,
    time_ms: u64,
) -> Option<SceneTextureRegion> {
    let spritesheet = properties.get("spritesheet")?.as_object()?;
    let atlas_width = scene_property_u32(spritesheet, "atlas_width")?;
    let atlas_height = scene_property_u32(spritesheet, "atlas_height")?;
    let frame_width = scene_property_u32(spritesheet, "frame_width")?;
    let frame_height = scene_property_u32(spritesheet, "frame_height")?;
    let columns = scene_property_u32(spritesheet, "columns").unwrap_or_else(|| {
        if frame_width == 0 {
            0
        } else {
            atlas_width / frame_width
        }
    });
    let rows = scene_property_u32(spritesheet, "rows").unwrap_or_else(|| {
        if frame_height == 0 {
            0
        } else {
            atlas_height / frame_height
        }
    });
    let frame_count = scene_property_u32(spritesheet, "frame_count")
        .unwrap_or_else(|| columns.saturating_mul(rows));
    if atlas_width == 0
        || atlas_height == 0
        || frame_width == 0
        || frame_height == 0
        || columns == 0
        || rows == 0
        || frame_count == 0
    {
        return None;
    }
    let max_frames = columns.saturating_mul(rows);
    let frame_count = frame_count.min(max_frames);
    if frame_count == 0 {
        return None;
    }
    let fps = scene_property_f64(spritesheet, "fps")
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(12.0);
    let loop_playback = scene_property_bool(spritesheet, "loop").unwrap_or(true);
    let frame = ((time_ms as f64 / 1000.0) * fps).floor();
    let frame_index = if frame.is_finite() && frame >= 0.0 {
        let frame = frame as u64;
        if loop_playback {
            (frame % u64::from(frame_count)) as u32
        } else {
            frame.min(u64::from(frame_count - 1)) as u32
        }
    } else {
        0
    };
    let column = frame_index % columns;
    let row = frame_index / columns;
    SceneTextureRegion {
        u_min: f64::from(column * frame_width) / f64::from(atlas_width),
        v_min: f64::from(row * frame_height) / f64::from(atlas_height),
        u_max: f64::from((column + 1) * frame_width) / f64::from(atlas_width),
        v_max: f64::from((row + 1) * frame_height) / f64::from(atlas_height),
        frame_index,
        frame_count,
        columns,
        rows,
        fps: Some(fps),
        loop_playback,
    }
    .validate()
}

fn scene_blend_mode_from_properties(properties: &BTreeMap<String, Value>) -> SceneBlendMode {
    properties
        .get("wallpaper_engine_blend")
        .and_then(Value::as_object)
        .and_then(|blend| blend.get("colorBlendMode"))
        .and_then(scene_blend_mode_from_wallpaper_engine_color_blend_mode)
        .or_else(|| {
            properties
                .get("material")
                .and_then(Value::as_object)
                .and_then(scene_blend_mode_from_material)
        })
        .unwrap_or_default()
}

fn scene_blend_mode_from_wallpaper_engine_color_blend_mode(
    value: &Value,
) -> Option<SceneBlendMode> {
    let mode = value
        .as_i64()
        .or_else(|| value.as_str()?.parse::<i64>().ok())?;
    match mode {
        2 => Some(SceneBlendMode::Multiply),
        3 => Some(SceneBlendMode::Multiply),
        6 => Some(SceneBlendMode::Max),
        7 => Some(SceneBlendMode::Screen),
        8 => Some(SceneBlendMode::Screen),
        28 => Some(SceneBlendMode::HslColor),
        31 => Some(SceneBlendMode::Additive),
        32 => Some(SceneBlendMode::Modulate),
        _ => None,
    }
}

fn scene_blend_mode_from_material(
    material: &serde_json::Map<String, Value>,
) -> Option<SceneBlendMode> {
    material
        .get("passes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|pass| pass.get("blending").and_then(Value::as_str))
        .find_map(scene_blend_mode_from_material_blending)
}

pub(crate) fn scene_blend_mode_from_material_blending(blending: &str) -> Option<SceneBlendMode> {
    match blending.to_ascii_lowercase().as_str() {
        "translucent" | "alpha" => Some(SceneBlendMode::Alpha),
        "normal" => Some(SceneBlendMode::Normal),
        "additive" | "add" => Some(SceneBlendMode::Additive),
        "multiply" => Some(SceneBlendMode::Multiply),
        "screen" => Some(SceneBlendMode::Screen),
        "alphatocoverage" | "alpha-to-coverage" => Some(SceneBlendMode::AlphaToCoverage),
        _ => None,
    }
}

fn scene_text_from_properties(
    properties: &BTreeMap<String, Value>,
    resolve_text_property: &impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let property = properties
        .get("text_binding")
        .and_then(Value::as_object)
        .and_then(|binding| binding.get("property"))
        .and_then(Value::as_str)?;
    resolve_scene_text_property(resolve_text_property, property)
}

fn scene_color_from_properties(
    properties: &BTreeMap<String, Value>,
    binding_key: &str,
    resolve_text_property: &impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let binding = properties.get(binding_key)?.as_object()?;
    if binding
        .get("runtime")
        .and_then(Value::as_str)
        .is_some_and(|runtime| runtime != "wallpaper-engine-user-color")
    {
        return None;
    }
    let property = binding.get("property").and_then(Value::as_str)?;
    resolve_scene_text_property(resolve_text_property, property)
        .as_deref()
        .and_then(scene_effect_color_string)
        .or_else(|| binding.get("default").and_then(scene_effect_value_color))
}

fn scene_tint_from_color(color: Option<&str>) -> [f32; 4] {
    color
        .filter(|color| !color.is_empty())
        .and_then(scene_rgba_from_hex)
        .unwrap_or(SCENE_SAMPLED_IMAGE_DEFAULT_TINT)
}

fn scene_rgba_from_hex(color: &str) -> Option<[f32; 4]> {
    let hex = color.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
    Some([r, g, b, 1.0])
}

fn scene_audio_cues_for_snapshot(
    cues: &[SceneAudioCue],
    resolve_property: &impl Fn(&str) -> Option<f64>,
) -> Vec<SceneAudioCue> {
    cues.iter()
        .filter_map(|cue| {
            if cue.active_conditions.is_empty() {
                return Some(cue.clone());
            }
            scene_audio_cue_conditions_active(&cue.active_conditions, resolve_property).then(|| {
                let mut cue = cue.clone();
                cue.start_silent = Some(false);
                cue
            })
        })
        .collect()
}

fn scene_audio_cue_conditions_active(
    conditions: &[SceneAudioCueCondition],
    resolve_property: &impl Fn(&str) -> Option<f64>,
) -> bool {
    conditions.iter().all(|condition| {
        let Some(value) = resolve_scene_property(resolve_property, &[condition.property.as_str()])
        else {
            return false;
        };
        if let Some(expected) = condition.equals {
            (value - expected).abs() <= f64::EPSILON
        } else {
            value > 0.0
        }
    })
}

fn scene_property_u32(object: &serde_json::Map<String, Value>, key: &str) -> Option<u32> {
    match object.get(key)? {
        Value::Number(value) => value.as_u64().and_then(|value| u32::try_from(value).ok()),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn scene_property_f64(object: &serde_json::Map<String, Value>, key: &str) -> Option<f64> {
    match object.get(key)? {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn scene_property_bool(object: &serde_json::Map<String, Value>, key: &str) -> Option<bool> {
    match object.get(key)? {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => value.as_i64().map(|value| value != 0),
        Value::String(value) => match value.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn scene_particle_value<'a>(
    object: Option<&'a serde_json::Map<String, Value>>,
    key: &str,
) -> Option<&'a Value> {
    let value = object?.get(key)?;
    Some(value.get("value").unwrap_or(value))
}

fn scene_particle_f64(object: Option<&serde_json::Map<String, Value>>, key: &str) -> Option<f64> {
    match scene_particle_value(object, key)? {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn scene_particle_u32(object: Option<&serde_json::Map<String, Value>>, key: &str) -> Option<u32> {
    match scene_particle_value(object, key)? {
        Value::Number(value) => value.as_u64().and_then(|value| u32::try_from(value).ok()),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn scene_particle_u64(object: Option<&serde_json::Map<String, Value>>, key: &str) -> Option<u64> {
    match scene_particle_value(object, key)? {
        Value::Number(value) => value.as_u64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn scene_particle_bool(object: Option<&serde_json::Map<String, Value>>, key: &str) -> Option<bool> {
    match scene_particle_value(object, key)? {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => value.as_i64().map(|value| value != 0),
        Value::String(value) => match value.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn scene_particle_string(
    object: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Option<String> {
    match scene_particle_value(object, key)? {
        Value::String(value) if !value.trim().is_empty() => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn scene_particle_seed_from_id(id: &str) -> u64 {
    let mut seed = 0xcbf29ce484222325u64;
    for byte in id.as_bytes() {
        seed ^= u64::from(*byte);
        seed = seed.wrapping_mul(0x100000001b3);
    }
    seed
}

#[inline]
fn scene_particle_unit(seed: u64, index: u32, salt: u64) -> f64 {
    let mut value = seed
        ^ (u64::from(index).wrapping_mul(0x9e3779b97f4a7c15))
        ^ salt.wrapping_mul(0xbf58476d1ce4e5b9);
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^= value >> 31;
    ((value >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
}

fn apply_scene_animated_value(
    transform: &mut SceneTransform,
    opacity: &mut f64,
    width: &mut Option<f64>,
    height: &mut Option<f64>,
    corner_radius: &mut Option<f64>,
    property: SceneAnimatedProperty,
    value: f64,
) {
    match property {
        SceneAnimatedProperty::Opacity => *opacity = value.clamp(0.0, 1.0),
        SceneAnimatedProperty::X => transform.x = value,
        SceneAnimatedProperty::Y => transform.y = value,
        SceneAnimatedProperty::ScaleX if value > 0.0 => transform.scale_x = value,
        SceneAnimatedProperty::ScaleY if value > 0.0 => transform.scale_y = value,
        SceneAnimatedProperty::ScaleX | SceneAnimatedProperty::ScaleY => {}
        SceneAnimatedProperty::RotationDeg => transform.rotation_deg = value,
        SceneAnimatedProperty::Width => *width = Some(value.max(0.0)),
        SceneAnimatedProperty::Height => *height = Some(value.max(0.0)),
        SceneAnimatedProperty::CornerRadius => *corner_radius = Some(value.max(0.0)),
        SceneAnimatedProperty::Custom => {}
    }
}

fn resolve_scene_property(
    resolve_property: &impl Fn(&str) -> Option<f64>,
    names: &[&str],
) -> Option<f64> {
    names
        .iter()
        .filter_map(|name| resolve_property(name))
        .find(|value| value.is_finite())
}

fn resolve_scene_text_property(
    resolve_text_property: &impl Fn(&str) -> Option<String>,
    property: &str,
) -> Option<String> {
    let property = property.trim();
    if property.is_empty() {
        None
    } else {
        resolve_text_property(property).filter(|text| !text.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneError {
    message: String,
}

impl SceneError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SceneError {}

fn validate_required_text(field: &str, value: &str) -> Result<(), SceneError> {
    if value.trim().is_empty() {
        Err(SceneError::invalid(format!("{field} must not be empty")))
    } else {
        Ok(())
    }
}

fn validate_optional_text(field: &str, value: &Option<String>) -> Result<(), SceneError> {
    if let Some(value) = value
        && value.trim().is_empty()
    {
        return Err(SceneError::invalid(format!("{field} must not be empty")));
    }
    Ok(())
}

fn validate_opacity(opacity: f64, owner: &str) -> Result<(), SceneError> {
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        Err(SceneError::invalid(format!(
            "scene {owner:?} opacity must be finite and between 0 and 1"
        )))
    } else {
        Ok(())
    }
}

fn validate_optional_finite(field: &str, value: Option<f64>) -> Result<(), SceneError> {
    if let Some(value) = value
        && !value.is_finite()
    {
        return Err(SceneError::invalid(format!("{field} must be finite")));
    }
    Ok(())
}

const fn default_scene_version() -> u32 {
    SCENE_VERSION
}

const fn default_true() -> bool {
    true
}

const fn default_opacity() -> f64 {
    1.0
}

const fn default_effect_uv_scale() -> [f64; 2] {
    [1.0, 1.0]
}

const fn default_scale() -> f64 {
    1.0
}

const fn default_anchor() -> f64 {
    0.5
}
