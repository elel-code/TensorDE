#[derive(Debug, Clone, PartialEq)]
struct SceneParticleEmitterSettings {
    count: u32,
    seed: u64,
    lifetime_ms: u64,
    loop_playback: bool,
    spawn_width: f64,
    spawn_height: f64,
    particle_width: f64,
    particle_height: f64,
    speed_min: f64,
    speed_max: f64,
    direction_deg: f64,
    spread_deg: f64,
    gravity_x: f64,
    gravity_y: f64,
    fade: bool,
    color: String,
    shape: SceneNodeKind,
}

impl SceneParticleEmitterSettings {
    fn from_node(node: &SceneNode) -> Option<Self> {
        let particle = node.properties.get("particle").and_then(Value::as_object);
        let count = scene_particle_u32(particle, "count")
            .or_else(|| scene_particle_u32(particle, "max_count"))
            .unwrap_or_else(|| {
                let lifetime_seconds = scene_particle_f64(particle, "lifetime")
                    .or_else(|| scene_particle_f64(particle, "lifetime_seconds"))
                    .unwrap_or(SCENE_PARTICLE_DEFAULT_LIFETIME_MS as f64 / 1000.0);
                scene_particle_f64(particle, "rate")
                    .filter(|rate| rate.is_finite() && *rate > 0.0)
                    .map(|rate| (rate * lifetime_seconds).round().max(1.0) as u32)
                    .unwrap_or(SCENE_PARTICLE_DEFAULT_COUNT)
            })
            .clamp(0, SCENE_PARTICLE_MAX_COUNT);
        let lifetime_ms = scene_particle_u64(particle, "lifetime_ms")
            .or_else(|| {
                scene_particle_f64(particle, "lifetime")
                    .or_else(|| scene_particle_f64(particle, "lifetime_seconds"))
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .map(|value| (value * 1000.0).round() as u64)
            })
            .unwrap_or(SCENE_PARTICLE_DEFAULT_LIFETIME_MS)
            .max(1);
        let particle_width = scene_particle_f64(particle, "width")
            .or_else(|| scene_particle_f64(particle, "size"))
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(SCENE_PARTICLE_DEFAULT_SIZE);
        let particle_height = scene_particle_f64(particle, "height")
            .or_else(|| scene_particle_f64(particle, "size"))
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(particle_width);
        let speed = scene_particle_f64(particle, "speed")
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(SCENE_PARTICLE_DEFAULT_SPEED);
        let speed_min = scene_particle_f64(particle, "speed_min")
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(speed);
        let speed_max = scene_particle_f64(particle, "speed_max")
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(speed)
            .max(speed_min);
        let spawn_width = scene_particle_f64(particle, "spawn_width")
            .or_else(|| scene_particle_f64(particle, "emitter_width"))
            .or(node.width)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(0.0);
        let spawn_height = scene_particle_f64(particle, "spawn_height")
            .or_else(|| scene_particle_f64(particle, "emitter_height"))
            .or(node.height)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(0.0);
        let shape = match scene_particle_string(particle, "shape")
            .unwrap_or_else(|| "rectangle".to_owned())
            .to_ascii_lowercase()
            .as_str()
        {
            "ellipse" | "circle" => SceneNodeKind::Ellipse,
            _ => SceneNodeKind::Rectangle,
        };
        Some(Self {
            count,
            seed: scene_particle_u64(particle, "seed")
                .unwrap_or_else(|| scene_particle_seed_from_id(&node.id)),
            lifetime_ms,
            loop_playback: scene_particle_bool(particle, "loop").unwrap_or(true),
            spawn_width,
            spawn_height,
            particle_width,
            particle_height,
            speed_min,
            speed_max,
            direction_deg: scene_particle_f64(particle, "direction_deg").unwrap_or(-90.0),
            spread_deg: scene_particle_f64(particle, "spread_deg").unwrap_or(360.0),
            gravity_x: scene_particle_f64(particle, "gravity_x").unwrap_or(0.0),
            gravity_y: scene_particle_f64(particle, "gravity_y").unwrap_or(0.0),
            fade: scene_particle_bool(particle, "fade").unwrap_or(true),
            color: scene_particle_string(particle, "color")
                .or_else(|| node.color.clone())
                .unwrap_or_else(|| "#ffffff".to_owned()),
            shape,
        })
    }

    fn age_seconds(&self, time_ms: u64, index: u32) -> Option<f64> {
        let phase = scene_particle_unit(self.seed, index, 0);
        let phase_ms = (phase * self.lifetime_ms as f64).round() as u64;
        let local_ms = if self.loop_playback {
            time_ms.wrapping_add(phase_ms) % self.lifetime_ms
        } else {
            let started_at = phase_ms.min(self.lifetime_ms);
            if time_ms < started_at {
                return None;
            }
            (time_ms - started_at).min(self.lifetime_ms)
        };
        Some(local_ms as f64 / 1000.0)
    }

    #[inline]
    fn opacity_and_transform_at(&self, time_ms: u64, index: u32) -> Option<(f64, f64, f64, f64)> {
        let age = self.age_seconds(time_ms, index)?;
        let progress = (age * 1000.0 / self.lifetime_ms as f64).clamp(0.0, 1.0);
        let opacity = if self.fade { 1.0 - progress } else { 1.0 };
        let spawn_x = (scene_particle_unit(self.seed, index, 1) - 0.5) * self.spawn_width;
        let spawn_y = (scene_particle_unit(self.seed, index, 2) - 0.5) * self.spawn_height;
        let speed = self.speed_min
            + (self.speed_max - self.speed_min) * scene_particle_unit(self.seed, index, 3);
        let direction =
            self.direction_deg + (scene_particle_unit(self.seed, index, 4) - 0.5) * self.spread_deg;
        let radians = direction.to_radians();
        let (direction_sin, direction_cos) = radians.sin_cos();
        let x = spawn_x + direction_cos * speed * age + 0.5 * self.gravity_x * age * age;
        let y = spawn_y + direction_sin * speed * age + 0.5 * self.gravity_y * age * age;
        Some((opacity, x, y, direction))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneNodeProvenance {
    #[serde(default)]
    pub source_format: Option<String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub attachment: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub original_type: Option<String>,
    #[serde(default)]
    pub original_path: Option<String>,
    #[serde(default)]
    pub transform: Option<SceneSourceTransform>,
    #[serde(default)]
    pub model: Option<SceneSourceModel>,
    #[serde(default)]
    pub particle: Option<Value>,
    #[serde(default)]
    pub animation_layers: Vec<Value>,
    #[serde(default)]
    pub instance: Option<Value>,
    #[serde(default)]
    pub instance_override: Option<Value>,
}

impl SceneNodeProvenance {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        for (field, value) in [
            ("source_format", self.source_format.as_deref()),
            ("source_id", self.source_id.as_deref()),
            ("parent_id", self.parent_id.as_deref()),
            ("attachment", self.attachment.as_deref()),
            ("original_type", self.original_type.as_deref()),
            ("original_path", self.original_path.as_deref()),
        ] {
            if let Some(value) = value
                && value.trim().is_empty()
            {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} provenance {field} must not be empty"
                )));
            }
        }
        for dependency in &self.dependencies {
            validate_required_text(
                &format!("scene node {node_id:?} provenance dependency"),
                dependency,
            )?;
        }
        if let Some(transform) = &self.transform {
            transform.validate(node_id)?;
        }
        if let Some(model) = &self.model {
            model.validate(node_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneSourceTransform {
    #[serde(default)]
    pub origin: Option<SceneVector3>,
    #[serde(default)]
    pub angles: Option<SceneVector3>,
    #[serde(default)]
    pub scale: Option<SceneVector3>,
    #[serde(default)]
    pub pivot: Option<SceneVector3>,
    #[serde(default)]
    pub size: Option<SceneVector3>,
    #[serde(default)]
    pub alignment: Option<String>,
}

impl SceneSourceTransform {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        for (field, value) in [
            ("origin", self.origin),
            ("angles", self.angles),
            ("scale", self.scale),
            ("pivot", self.pivot),
            ("size", self.size),
        ] {
            if let Some(value) = value {
                value.validate(&format!("scene node {node_id:?} source transform {field}"))?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneSourceModel {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub utility: Option<String>,
    #[serde(default)]
    pub builtin: Option<bool>,
    #[serde(default)]
    pub model_resource: Option<String>,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub material_resource: Option<String>,
    #[serde(default)]
    pub puppet: Option<String>,
    #[serde(default)]
    pub solid_layer: Option<bool>,
    #[serde(default)]
    pub passthrough: Option<bool>,
    #[serde(default)]
    pub textures: Vec<String>,
    #[serde(default)]
    pub texture_resources: Vec<String>,
}

impl SceneSourceModel {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        for (field, value) in [
            ("source", self.source.as_deref()),
            ("utility", self.utility.as_deref()),
            ("model_resource", self.model_resource.as_deref()),
            ("material", self.material.as_deref()),
            ("material_resource", self.material_resource.as_deref()),
            ("puppet", self.puppet.as_deref()),
        ] {
            if let Some(value) = value
                && value.trim().is_empty()
            {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} source model {field} must not be empty"
                )));
            }
        }
        for texture in &self.textures {
            validate_required_text(
                &format!("scene node {node_id:?} source model texture"),
                texture,
            )?;
        }
        for texture_resource in &self.texture_resources {
            validate_required_text(
                &format!("scene node {node_id:?} source model texture resource"),
                texture_resource,
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneEffect {
    pub file: String,
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub visible: Option<Value>,
    #[serde(default)]
    pub fbos: Vec<SceneEffectFbo>,
    #[serde(default)]
    pub passes: Vec<SceneEffectPass>,
}

impl SceneEffect {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        validate_required_text(&format!("scene node {node_id:?} effect file"), &self.file)?;
        if let Some(resource) = &self.resource {
            validate_required_text(&format!("scene node {node_id:?} effect resource"), resource)?;
        }
        if let Some(runtime) = &self.runtime {
            validate_required_text(&format!("scene node {node_id:?} effect runtime"), runtime)?;
        }
        for pass in &self.passes {
            pass.validate(node_id, &self.file)?;
        }
        for fbo in &self.fbos {
            fbo.validate(node_id, &self.file)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneEffectFbo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default = "scene_effect_fbo_default_scale")]
    pub scale: f64,
    #[serde(default)]
    pub unique: bool,
}

fn scene_effect_fbo_default_scale() -> f64 {
    1.0
}

impl SceneEffectFbo {
    fn validate(&self, node_id: &str, effect_file: &str) -> Result<(), SceneError> {
        validate_required_text(
            &format!("scene node {node_id:?} effect {effect_file:?} fbo name"),
            &self.name,
        )?;
        if let Some(format) = &self.format {
            validate_required_text(
                &format!("scene node {node_id:?} effect {effect_file:?} fbo format"),
                format,
            )?;
        }
        if !self.scale.is_finite() || self.scale <= 0.0 {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} effect {effect_file:?} fbo {:?} scale must be positive",
                self.name
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneEffectPass {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub binds: BTreeMap<u32, String>,
    #[serde(default)]
    pub shader: Option<String>,
    #[serde(default)]
    pub blending: Option<String>,
    #[serde(default)]
    pub depthtest: Option<String>,
    #[serde(default)]
    pub depthwrite: Option<String>,
    #[serde(default)]
    pub cullmode: Option<String>,
    #[serde(default)]
    pub alphawriting: Option<String>,
    #[serde(default)]
    pub textures: Vec<Option<String>>,
    #[serde(default)]
    pub texture_resources: Vec<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_uv_transform: Option<SceneEffectUvTransform>,
    #[serde(default)]
    pub combos: BTreeMap<String, i64>,
    #[serde(default)]
    pub constant_shader_values: BTreeMap<String, Value>,
    #[serde(default)]
    pub user_textures: Option<Value>,
}

impl SceneEffectPass {
    fn validate(&self, node_id: &str, effect_file: &str) -> Result<(), SceneError> {
        for (field, value) in [
            ("command", self.command.as_deref()),
            ("source", self.source.as_deref()),
            ("target", self.target.as_deref()),
            ("shader", self.shader.as_deref()),
            ("blending", self.blending.as_deref()),
            ("depthtest", self.depthtest.as_deref()),
            ("depthwrite", self.depthwrite.as_deref()),
            ("cullmode", self.cullmode.as_deref()),
            ("alphawriting", self.alphawriting.as_deref()),
        ] {
            if let Some(value) = value
                && value.trim().is_empty()
            {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} effect {effect_file:?} pass {field} must not be empty"
                )));
            }
        }
        for bind in self.binds.values() {
            validate_required_text(
                &format!("scene node {node_id:?} effect {effect_file:?} pass bind"),
                bind,
            )?;
        }
        for texture in self.textures.iter().flatten() {
            if texture.trim().is_empty() {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} effect {effect_file:?} texture reference must not be empty"
                )));
            }
        }
        for texture_resource in self.texture_resources.iter().flatten() {
            validate_required_text(
                &format!("scene node {node_id:?} effect {effect_file:?} texture resource"),
                texture_resource,
            )?;
        }
        if let Some(transform) = self.effect_uv_transform {
            transform.validate(node_id, effect_file)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneEffectUvMapping {
    #[default]
    TextureResolution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneEffectUvExtent {
    pub width: u32,
    pub height: u32,
}

impl SceneEffectUvExtent {
    fn validate(&self, node_id: &str, effect_file: &str, label: &str) -> Result<(), SceneError> {
        if self.width == 0 || self.height == 0 {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} effect {effect_file:?} {label} extent must be non-zero"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneEffectUvTransform {
    #[serde(default)]
    pub mapping: SceneEffectUvMapping,
    #[serde(default)]
    pub source_slot: u32,
    #[serde(default)]
    pub mask_slot: u32,
    #[serde(default = "default_effect_uv_scale")]
    pub scale: [f64; 2],
    #[serde(default)]
    pub offset: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_extent: Option<SceneEffectUvExtent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_extent: Option<SceneEffectUvExtent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_backing_extent: Option<SceneEffectUvExtent>,
}

impl SceneEffectUvTransform {
    fn validate(&self, node_id: &str, effect_file: &str) -> Result<(), SceneError> {
        for (field, values) in [("scale", self.scale), ("offset", self.offset)] {
            for (index, value) in values.iter().enumerate() {
                if !value.is_finite() {
                    return Err(SceneError::invalid(format!(
                        "scene node {node_id:?} effect {effect_file:?} effect UV {field}[{index}] must be finite"
                    )));
                }
            }
        }
        if let Some(extent) = self.input_extent {
            extent.validate(node_id, effect_file, "input")?;
        }
        if let Some(extent) = self.mask_extent {
            extent.validate(node_id, effect_file, "mask")?;
        }
        if let Some(extent) = self.mask_backing_extent {
            extent.validate(node_id, effect_file, "mask backing")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneAudioCue {
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub playback_mode: Option<String>,
    #[serde(default)]
    pub volume: Option<Value>,
    #[serde(default)]
    pub start_silent: Option<bool>,
    #[serde(default)]
    pub active_conditions: Vec<SceneAudioCueCondition>,
}

impl SceneAudioCue {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        if let Some(resource) = &self.resource {
            validate_required_text(&format!("scene node {node_id:?} audio resource"), resource)?;
        }
        if let Some(source) = &self.source {
            validate_required_text(&format!("scene node {node_id:?} audio source"), source)?;
        }
        if self.resource.is_none() && self.source.is_none() {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} audio cue must define resource or source"
            )));
        }
        for condition in &self.active_conditions {
            condition.validate(node_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneAudioCueCondition {
    pub property: String,
    #[serde(default)]
    pub equals: Option<f64>,
}

impl SceneAudioCueCondition {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        validate_required_text(
            &format!("scene node {node_id:?} audio active condition property"),
            &self.property,
        )?;
        validate_optional_finite(
            &format!("scene node {node_id:?} audio active condition equals"),
            self.equals,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneNodeKind {
    Image,
    Video,
    Color,
    Rectangle,
    Ellipse,
    Text,
    Path,
    Group,
    Shader,
    ParticleEmitter,
    AudioResponse,
    Audio,
    Script,
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneTextAlign {
    #[default]
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneBlendMode {
    #[default]
    Alpha,
    Normal,
    Additive,
    Multiply,
    Screen,
    Max,
    /// Wallpaper Engine colorBlendMode 32: `A * (1 + B*a)` - a background-modulated
    /// brighten that vanishes on dark backgrounds. With a premultiplied source (B*a) this
    /// is the fixed-function equation `src*DST_COLOR + dst*ONE`.
    Modulate,
    /// Wallpaper Engine colorBlendMode 28: HSL Color. This is not a fixed-function
    /// blend; WE evaluates it in a framebuffer-sampling passthrough shader.
    HslColor,
    /// Wallpaper Engine material pass `blending:"alphatocoverage"`: shader alpha
    /// becomes the MSAA coverage mask and fixed-function color blending is disabled.
    AlphaToCoverage,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneAlphaTextureMode {
    #[default]
    Multiply,
    Inverse,
    Iris,
    Coverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneTransform {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default = "default_scale")]
    pub scale_x: f64,
    #[serde(default = "default_scale")]
    pub scale_y: f64,
    #[serde(default)]
    pub rotation_deg: f64,
    #[serde(default = "default_anchor")]
    pub anchor_x: f64,
    #[serde(default = "default_anchor")]
    pub anchor_y: f64,
}
