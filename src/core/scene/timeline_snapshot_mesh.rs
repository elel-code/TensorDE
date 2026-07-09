
impl Default for SceneTransform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: 0.0,
            anchor_x: 0.5,
            anchor_y: 0.5,
        }
    }
}

impl SceneTransform {
    fn validate(self, node_id: &str) -> Result<(), SceneError> {
        for (field, value) in [
            ("x", self.x),
            ("y", self.y),
            ("scale_x", self.scale_x),
            ("scale_y", self.scale_y),
            ("rotation_deg", self.rotation_deg),
            ("anchor_x", self.anchor_x),
            ("anchor_y", self.anchor_y),
        ] {
            if !value.is_finite() {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} transform {field} must be finite"
                )));
            }
        }
        if self.scale_x <= 0.0 || self.scale_y <= 0.0 {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} transform scale values must be greater than 0"
            )));
        }
        Ok(())
    }

    fn compose(self, child: Self) -> Self {
        let rotation = self.rotation_deg.to_radians();
        let child_x = child.x * self.scale_x;
        let child_y = child.y * self.scale_y;
        let rotated_child_x = child_x.mul_add(rotation.cos(), -child_y * rotation.sin());
        let rotated_child_y = child_x.mul_add(rotation.sin(), child_y * rotation.cos());
        Self {
            x: self.x + rotated_child_x,
            y: self.y + rotated_child_y,
            scale_x: self.scale_x * child.scale_x,
            scale_y: self.scale_y * child.scale_y,
            rotation_deg: self.rotation_deg + child.rotation_deg,
            anchor_x: child.anchor_x,
            anchor_y: child.anchor_y,
        }
    }
}

#[inline]
fn scene_compose_particle_transform(
    parent: SceneTransform,
    parent_sin: f64,
    parent_cos: f64,
    x: f64,
    y: f64,
    rotation_deg: f64,
) -> SceneTransform {
    let child_x = x * parent.scale_x;
    let child_y = y * parent.scale_y;
    let rotated_child_x = child_x.mul_add(parent_cos, -child_y * parent_sin);
    let rotated_child_y = child_x.mul_add(parent_sin, child_y * parent_cos);
    SceneTransform {
        x: parent.x + rotated_child_x,
        y: parent.y + rotated_child_y,
        scale_x: parent.scale_x,
        scale_y: parent.scale_y,
        rotation_deg: parent.rotation_deg + rotation_deg,
        anchor_x: 0.5,
        anchor_y: 0.5,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneTimeline {
    pub id: String,
    #[serde(default)]
    pub target_node: Option<String>,
    #[serde(default)]
    pub channels: Vec<SceneTimelineChannel>,
}

impl SceneTimeline {
    fn validate(&self, node_ids: &BTreeSet<String>) -> Result<(), SceneError> {
        validate_required_text("scene timeline id", &self.id)?;
        if let Some(target_node) = &self.target_node
            && !node_ids.contains(target_node)
        {
            return Err(SceneError::invalid(format!(
                "scene timeline {:?} references unknown target node {:?}",
                self.id, target_node
            )));
        }
        for channel in &self.channels {
            channel.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneTimelineChannel {
    pub property: SceneAnimatedProperty,
    #[serde(rename = "loop", default)]
    pub loop_playback: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub time_offset_ms: u64,
    #[serde(default)]
    pub keyframes: Vec<SceneKeyframe>,
}

impl SceneTimelineChannel {
    fn validate(&self) -> Result<(), SceneError> {
        for keyframe in &self.keyframes {
            keyframe.validate(self.property)?;
        }
        Ok(())
    }

    fn value_at(&self, time_ms: u64) -> f64 {
        let Some(first) = self.keyframes.first() else {
            return 0.0;
        };
        if self.keyframes.len() == 1 {
            return first.value;
        }
        let last_time = self
            .keyframes
            .last()
            .map(|keyframe| keyframe.time_ms)
            .unwrap_or_default();
        let time_ms = time_ms.saturating_add(self.time_offset_ms);
        let time_ms = if self.loop_playback && last_time > 0 {
            time_ms % last_time
        } else {
            time_ms
        };
        if time_ms <= first.time_ms {
            return first.value;
        }
        for pair in self.keyframes.windows(2) {
            let start = &pair[0];
            let end = &pair[1];
            if time_ms <= end.time_ms {
                let span = (end.time_ms - start.time_ms) as f64;
                let progress = if span > 0.0 {
                    (time_ms - start.time_ms) as f64 / span
                } else {
                    1.0
                };
                let eased = end.curve.ease(progress.clamp(0.0, 1.0));
                return start.value + (end.value - start.value) * eased;
            }
        }
        self.keyframes
            .last()
            .map(|keyframe| keyframe.value)
            .unwrap_or(first.value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneAnimatedProperty {
    Opacity,
    X,
    Y,
    ScaleX,
    ScaleY,
    RotationDeg,
    Width,
    Height,
    CornerRadius,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneKeyframe {
    pub time_ms: u64,
    pub value: f64,
    #[serde(default)]
    pub curve: SceneCurve,
}

impl SceneKeyframe {
    fn validate(self, property: SceneAnimatedProperty) -> Result<(), SceneError> {
        if !self.value.is_finite() {
            return Err(SceneError::invalid(format!(
                "scene timeline {property:?} keyframe value must be finite"
            )));
        }
        if property == SceneAnimatedProperty::Opacity {
            validate_opacity(self.value, "timeline")?;
        }
        if matches!(
            property,
            SceneAnimatedProperty::ScaleX | SceneAnimatedProperty::ScaleY
        ) && self.value <= 0.0
        {
            return Err(SceneError::invalid(format!(
                "scene timeline {property:?} scale value must be greater than 0"
            )));
        }
        if matches!(
            property,
            SceneAnimatedProperty::Width
                | SceneAnimatedProperty::Height
                | SceneAnimatedProperty::CornerRadius
        ) && self.value < 0.0
        {
            return Err(SceneError::invalid(format!(
                "scene timeline {property:?} geometry value must be non-negative"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneCurve {
    #[default]
    Linear,
    Step,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl SceneCurve {
    fn ease(self, value: f64) -> f64 {
        match self {
            Self::Linear => value,
            Self::Step => {
                if value >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
            Self::EaseIn => value * value,
            Self::EaseOut => 1.0 - (1.0 - value) * (1.0 - value),
            Self::EaseInOut => {
                if value < 0.5 {
                    2.0 * value * value
                } else {
                    1.0 - (-2.0 * value + 2.0).powi(2) / 2.0
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenePropertyBinding {
    pub property: String,
    #[serde(default)]
    pub target_node: Option<String>,
    pub target: SceneAnimatedProperty,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub offset: Option<f64>,
}

impl ScenePropertyBinding {
    fn validate(&self, node_ids: &BTreeSet<String>) -> Result<(), SceneError> {
        validate_required_text("scene property binding property", &self.property)?;
        if let Some(target_node) = &self.target_node
            && !node_ids.contains(target_node)
        {
            return Err(SceneError::invalid(format!(
                "scene property binding {:?} references unknown target node {:?}",
                self.property, target_node
            )));
        }
        for (field, value) in [("scale", self.scale), ("offset", self.offset)] {
            if let Some(value) = value
                && !value.is_finite()
            {
                return Err(SceneError::invalid(format!(
                    "scene property binding {field} must be finite"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneSystems {
    #[serde(default)]
    pub scenescript: SceneSystemStatus,
    #[serde(default)]
    pub shader_material_graph: SceneSystemStatus,
    #[serde(default)]
    pub particles: SceneSystemStatus,
    #[serde(default)]
    pub parallax: SceneSystemStatus,
    #[serde(default)]
    pub audio_response: SceneSystemStatus,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneSystemStatus {
    Ready,
    Detected,
    #[default]
    Absent,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneNativeLowering {
    #[serde(default)]
    pub target_runtime: Option<String>,
    #[serde(default)]
    pub current_runtime: Option<String>,
    #[serde(default)]
    pub progress_estimate_percent: Option<u8>,
    #[serde(default)]
    pub full_scene_complete: bool,
    #[serde(default)]
    pub completed_boundaries: Vec<String>,
    #[serde(default)]
    pub pending_boundaries: Vec<String>,
    #[serde(default)]
    pub unsupported_boundaries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneUnsupportedFeature {
    pub feature: String,
    pub reason: String,
    #[serde(default)]
    pub source_path: Option<String>,
}

impl SceneUnsupportedFeature {
    fn validate(&self) -> Result<(), SceneError> {
        validate_required_text("scene unsupported feature", &self.feature)?;
        validate_required_text("scene unsupported reason", &self.reason)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSnapshot {
    pub time_ms: u64,
    pub layers: Vec<SceneSnapshotLayer>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct SceneParallaxOffset {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SceneSnapshotVisibility {
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SceneSnapshotBuildOptions {
    compact_particle_ids: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SceneSnapshotSampledImageBuildIndex {
    resources_by_id: BTreeMap<String, usize>,
    timelines_by_node: BTreeMap<String, Vec<usize>>,
    global_property_bindings: Vec<usize>,
    property_bindings_by_node: BTreeMap<String, Vec<usize>>,
}

impl SceneSnapshotSampledImageBuildIndex {
    fn from_document(document: &SceneDocument) -> Self {
        let mut resources_by_id = BTreeMap::new();
        for (index, resource) in document.resources.iter().enumerate() {
            resources_by_id.insert(resource.id.clone(), index);
        }

        let mut timelines_by_node: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, timeline) in document.timelines.iter().enumerate() {
            if let Some(target_node) = timeline.target_node.as_deref() {
                timelines_by_node
                    .entry(target_node.to_owned())
                    .or_default()
                    .push(index);
            }
        }

        let mut global_property_bindings = Vec::new();
        let mut property_bindings_by_node: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, binding) in document.property_bindings.iter().enumerate() {
            if let Some(target_node) = binding.target_node.as_deref() {
                property_bindings_by_node
                    .entry(target_node.to_owned())
                    .or_default()
                    .push(index);
            } else {
                global_property_bindings.push(index);
            }
        }

        Self {
            resources_by_id,
            timelines_by_node,
            global_property_bindings,
            property_bindings_by_node,
        }
    }

    fn resource<'a>(
        &self,
        resources: &'a [SceneResource],
        resource_id: &str,
    ) -> Option<&'a SceneResource> {
        let resource = resources.get(*self.resources_by_id.get(resource_id)?)?;
        (resource.id == resource_id).then_some(resource)
    }

    fn timeline_indices_for_node(&self, node_id: &str) -> &[usize] {
        self.timelines_by_node
            .get(node_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn global_property_binding_indices(&self) -> &[usize] {
        &self.global_property_bindings
    }

    fn property_binding_indices_for_node(&self, node_id: &str) -> &[usize] {
        self.property_bindings_by_node
            .get(node_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

fn scene_texture_slots_for_node<'a>(
    base_resource: Option<&'a SceneResource>,
    effects: &[SceneEffect],
    mut resolve_resource: impl FnMut(&str) -> Option<&'a SceneResource>,
) -> (Vec<SceneTextureSlot>, Option<u32>, SceneAlphaTextureMode) {
    let mut slots = Vec::new();
    if let Some(resource) = base_resource {
        scene_push_texture_slot(&mut slots, 0, resource);
    }

    let mut alpha_texture_slot = None;
    let mut alpha_texture_mode = SceneAlphaTextureMode::Multiply;
    for effect in effects {
        let Some(effect_mode) = scene_effect_alpha_texture_mode(effect) else {
            continue;
        };
        for pass in &effect.passes {
            for (slot, resource_id) in pass.texture_resources.iter().enumerate().skip(1) {
                let Some(resource_id) = resource_id.as_deref() else {
                    continue;
                };
                let Some(resource) = resolve_resource(resource_id) else {
                    continue;
                };
                let Ok(slot) = u32::try_from(slot) else {
                    continue;
                };
                scene_push_texture_slot(&mut slots, slot, resource);
                if alpha_texture_slot.is_none() {
                    alpha_texture_slot = Some(slot);
                    alpha_texture_mode = effect_mode;
                }
            }
        }
    }

    (slots, alpha_texture_slot, alpha_texture_mode)
}

fn scene_image_effect_passes_for_node<'a>(
    effects: &[SceneEffect],
    mut resolve_resource: impl FnMut(&str) -> Option<&'a SceneResource>,
) -> Vec<SceneImageEffectPass> {
    let mut passes = Vec::new();
    for effect in effects {
        if effect
            .visible
            .as_ref()
            .and_then(scene_runtime_visibility_value_bool)
            .is_some_and(|visible| !visible)
        {
            continue;
        }
        for (pass_index, pass) in effect.passes.iter().enumerate() {
            let mut texture_slots = Vec::new();
            for (slot, resource_id) in pass.texture_resources.iter().enumerate() {
                let Some(resource_id) = resource_id.as_deref() else {
                    continue;
                };
                let Some(resource) = resolve_resource(resource_id) else {
                    continue;
                };
                let Ok(slot) = u32::try_from(slot) else {
                    continue;
                };
                scene_push_texture_slot(&mut texture_slots, slot, resource);
            }
            passes.push(SceneImageEffectPass {
                effect_file: effect.file.clone(),
                runtime: scene_image_effect_pass_runtime(effect),
                pass_index,
                command: pass.command.clone(),
                source: pass.source.clone(),
                target: pass.target.clone(),
                binds: pass.binds.clone(),
                fbos: effect.fbos.clone(),
                shader: pass.shader.clone(),
                blending: pass.blending.clone(),
                depthtest: pass.depthtest.clone(),
                depthwrite: pass.depthwrite.clone(),
                cullmode: pass.cullmode.clone(),
                alphawriting: pass.alphawriting.clone(),
                texture_slots,
                effect_uv_transform: pass.effect_uv_transform,
                combos: pass.combos.clone(),
                constant_shader_values: pass.constant_shader_values.clone(),
            });
        }
    }
    passes
}

fn scene_push_texture_slot(slots: &mut Vec<SceneTextureSlot>, slot: u32, resource: &SceneResource) {
    let _ = scene_push_texture_slot_value(
        slots,
        SceneTextureSlot {
            slot,
            source: resource.source.clone(),
            width: resource.width,
            height: resource.height,
        },
    );
}

fn scene_push_texture_slot_value(
    slots: &mut Vec<SceneTextureSlot>,
    texture_slot: SceneTextureSlot,
) -> bool {
    if slots.iter().any(|existing| {
        existing.slot == texture_slot.slot && existing.source == texture_slot.source
    }) {
        return true;
    }
    if slots
        .iter()
        .any(|existing| existing.slot == texture_slot.slot)
    {
        return false;
    }
    slots.push(texture_slot);
    slots.sort_by_key(|slot| slot.slot);
    true
}

fn scene_effect_alpha_texture_mode(effect: &SceneEffect) -> Option<SceneAlphaTextureMode> {
    let file = effect.file.replace('\\', "/").to_ascii_lowercase();
    if file == "effects/opacity/effect.json" || file.ends_with("/effects/opacity/effect.json") {
        return Some(SceneAlphaTextureMode::Multiply);
    }
    None
}

fn scene_image_effect_pass_runtime(effect: &SceneEffect) -> Option<String> {
    let file = effect.file.replace('\\', "/").to_ascii_lowercase();
    if file == "effects/opacity/effect.json" || file.ends_with("/effects/opacity/effect.json") {
        return Some("native-opacity-mask".to_owned());
    }
    if file == "effects/iris/effect.json" || file.ends_with("/effects/iris/effect.json") {
        return Some("native-iris-mask".to_owned());
    }
    effect.runtime.clone()
}

impl SceneSnapshotVisibility {
    fn from_size(size: Option<SceneSize>) -> Option<Self> {
        let size = size?;
        if size.width == 0 || size.height == 0 {
            return None;
        }
        Some(Self {
            width: f64::from(size.width),
            height: f64::from(size.height),
        })
    }

    fn intersects(self, bounds: SceneSnapshotBounds) -> bool {
        bounds.max_x >= 0.0
            && bounds.max_y >= 0.0
            && bounds.min_x <= self.width
            && bounds.min_y <= self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SceneSnapshotBounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl SceneSnapshotBounds {
    fn empty() -> Self {
        Self {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }

    fn include(&mut self, x: f64, y: f64) -> bool {
        if !x.is_finite() || !y.is_finite() {
            return false;
        }
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneTextureSlot {
    pub slot: u32,
    pub source: PackagePath,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneImageEffectPass {
    pub effect_file: String,
    pub runtime: Option<String>,
    pub pass_index: usize,
    pub command: Option<String>,
    pub source: Option<String>,
    pub target: Option<String>,
    pub binds: BTreeMap<u32, String>,
    pub fbos: Vec<SceneEffectFbo>,
    pub shader: Option<String>,
    pub blending: Option<String>,
    pub depthtest: Option<String>,
    pub depthwrite: Option<String>,
    pub cullmode: Option<String>,
    pub alphawriting: Option<String>,
    pub texture_slots: Vec<SceneTextureSlot>,
    pub effect_uv_transform: Option<SceneEffectUvTransform>,
    pub combos: BTreeMap<String, i64>,
    pub constant_shader_values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneLayerCompositeKey {
    pub parent_source_id: Option<String>,
    pub puppet_attachment: String,
    pub original_path: String,
    pub base_source: PackagePath,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSnapshotLayer {
    pub id: String,
    pub kind: SceneNodeKind,
    pub source: Option<PackagePath>,
    pub texture_slots: Vec<SceneTextureSlot>,
    pub alpha_texture_slot: Option<u32>,
    pub alpha_texture_mode: SceneAlphaTextureMode,
    pub image_effect_passes: Vec<SceneImageEffectPass>,
    pub composite_key: Option<SceneLayerCompositeKey>,
    pub texture_region: Option<SceneTextureRegion>,
    pub effect_motion: SceneNativeEffectMotion,
    pub blend_mode: SceneBlendMode,
    pub audio: Vec<SceneAudioCue>,
    pub color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<f64>,
    pub corner_radius: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub mesh: Option<Arc<SceneMesh>>,
    pub parallax_depth: Option<f64>,
    pub text: Option<String>,
    pub font_size: Option<f64>,
    pub font_family: Option<String>,
    pub font_source: Option<PackagePath>,
    pub font_weight: Option<String>,
    pub text_align: Option<SceneTextAlign>,
    pub path_data: Option<String>,
    pub path_fill_rule: ScenePathFillRule,
    pub fit: FitMode,
    pub opacity: f64,
    pub transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSnapshotSampledImageLayer {
    pub id: String,
    pub has_source: bool,
    pub texture_slots: Vec<SceneTextureSlot>,
    pub alpha_texture_slot: Option<u32>,
    pub alpha_texture_mode: SceneAlphaTextureMode,
    pub image_effect_passes: Vec<SceneImageEffectPass>,
    pub composite_key: Option<SceneLayerCompositeKey>,
    pub texture_region: Option<SceneTextureRegion>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub mesh: Option<Arc<SceneMesh>>,
    pub effect_motion: SceneNativeEffectMotion,
    pub blend_mode: SceneBlendMode,
    pub tint: [f32; 4],
    pub fit: FitMode,
    pub opacity: f64,
    pub transform: SceneTransform,
    pub puppet_animation_frames: Vec<ScenePuppetAnimationFrameDebug>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneNativeEffectMotion {
    pub wave_x: f64,
    pub wave_y: f64,
    pub wave_direction_x: f64,
    pub wave_direction_y: f64,
    pub wave_spatial_frequency: f64,
    pub wave_phase: f64,
    pub wave_count: u32,
    pub wave2_x: f64,
    pub wave2_y: f64,
    pub wave2_direction_x: f64,
    pub wave2_direction_y: f64,
    pub wave2_spatial_frequency: f64,
    pub wave2_phase: f64,
    pub wave2_count: u32,
    pub sway_amplitude: f64,
    pub sway_direction_x: f64,
    pub sway_direction_y: f64,
    pub sway_spatial_frequency: f64,
    pub sway_phase: f64,
    pub sway_power: f64,
    pub sway_count: u32,
}

impl SceneNativeEffectMotion {
    pub fn is_active(self) -> bool {
        self.wave_count > 0
            || self.wave2_count > 0
            || (self.sway_count > 0 && self.sway_amplitude.abs() > f64::EPSILON)
    }

    fn normalize(&mut self) {
        if self.wave_count > 0 {
            let count = f64::from(self.wave_count);
            self.wave_direction_x /= count;
            self.wave_direction_y /= count;
            self.wave_spatial_frequency /= count;
            self.wave_phase /= count;
        }
        if self.wave2_count > 0 {
            let count = f64::from(self.wave2_count);
            self.wave2_direction_x /= count;
            self.wave2_direction_y /= count;
            self.wave2_spatial_frequency /= count;
            self.wave2_phase /= count;
        }
        if self.sway_count > 0 {
            let count = f64::from(self.sway_count);
            self.sway_direction_x /= count;
            self.sway_direction_y /= count;
            self.sway_spatial_frequency /= count;
            self.sway_phase /= count;
        }
    }
}

fn scene_snapshot_layer_intersects_visibility(
    layer: &SceneSnapshotLayer,
    visibility: Option<SceneSnapshotVisibility>,
) -> bool {
    scene_snapshot_visual_bounds_intersects(
        layer.width,
        layer.height,
        layer.mesh.as_deref(),
        layer.transform,
        visibility,
    )
}

fn scene_sampled_image_snapshot_layer_intersects_visibility(
    layer: &SceneSnapshotSampledImageLayer,
    visibility: Option<SceneSnapshotVisibility>,
) -> bool {
    scene_snapshot_visual_bounds_intersects(
        layer.width,
        layer.height,
        layer.mesh.as_deref(),
        layer.transform,
        visibility,
    )
}

fn scene_snapshot_visual_bounds_intersects(
    width: Option<f64>,
    height: Option<f64>,
    mesh: Option<&SceneMesh>,
    transform: SceneTransform,
    visibility: Option<SceneSnapshotVisibility>,
) -> bool {
    let Some(visibility) = visibility else {
        return true;
    };
    let (Some(width), Some(height)) = (width, height) else {
        return true;
    };
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return true;
    }
    let Some(bounds) = scene_snapshot_visual_bounds(width, height, mesh, transform) else {
        return true;
    };
    visibility.intersects(bounds)
}

fn scene_snapshot_visual_bounds(
    width: f64,
    height: f64,
    mesh: Option<&SceneMesh>,
    transform: SceneTransform,
) -> Option<SceneSnapshotBounds> {
    let rotation = transform.rotation_deg.to_radians();
    let cos = rotation.cos();
    let sin = rotation.sin();
    let mut bounds = SceneSnapshotBounds::empty();
    if let Some(mesh) = mesh {
        let local_offset_x = (0.5 - transform.anchor_x) * width;
        let local_offset_y = (0.5 - transform.anchor_y) * height;
        for vertex in &mesh.vertices {
            let (x, y) = scene_snapshot_transform_point(
                vertex.x + local_offset_x,
                vertex.y + local_offset_y,
                transform,
                cos,
                sin,
            )?;
            if !bounds.include(x, y) {
                return None;
            }
        }
    } else {
        let left = -transform.anchor_x * width;
        let top = -transform.anchor_y * height;
        let right = left + width;
        let bottom = top + height;
        for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
            let (x, y) = scene_snapshot_transform_point(x, y, transform, cos, sin)?;
            if !bounds.include(x, y) {
                return None;
            }
        }
    }
    Some(bounds)
}

fn scene_snapshot_transform_point(
    x: f64,
    y: f64,
    transform: SceneTransform,
    cos: f64,
    sin: f64,
) -> Option<(f64, f64)> {
    let scaled_x = x * transform.scale_x;
    let scaled_y = y * transform.scale_y;
    let scene_x = scaled_x.mul_add(cos, -scaled_y * sin) + transform.x;
    let scene_y = scaled_x.mul_add(sin, scaled_y * cos) + transform.y;
    if !scene_x.is_finite() || !scene_y.is_finite() {
        return None;
    }
    Some((scene_x, scene_y))
}

fn scene_runtime_visibility_condition_matches(
    expected: &Value,
    actual_number: Option<f64>,
    actual_text: Option<&str>,
) -> bool {
    let expected = expected.get("value").unwrap_or(expected);
    if let Some(expected_bool) = scene_runtime_visibility_value_bool(expected) {
        if let Some(actual_number) = actual_number {
            return (actual_number.abs() > f64::EPSILON) == expected_bool;
        }
        return actual_text
            .and_then(scene_runtime_visibility_text_bool)
            .is_some_and(|actual| actual == expected_bool);
    }
    if let Some(expected_number) = scene_runtime_visibility_value_number(expected) {
        if let Some(actual_number) = actual_number {
            return (actual_number - expected_number).abs() <= 0.000_001;
        }
        return actual_text
            .and_then(scene_runtime_visibility_text_number)
            .is_some_and(|actual| (actual - expected_number).abs() <= 0.000_001);
    }
    let Some(expected_text) = scene_runtime_visibility_value_string(expected) else {
        return false;
    };
    if let Some(actual_text) = actual_text
        && scene_runtime_visibility_normalized_text(actual_text)
            == scene_runtime_visibility_normalized_text(&expected_text)
    {
        return true;
    }
    if let Some(expected_number) = scene_runtime_visibility_text_number(&expected_text)
        && let Some(actual_number) = actual_number
    {
        return (actual_number - expected_number).abs() <= 0.000_001;
    }
    false
}

fn scene_runtime_visibility_value_bool(value: &Value) -> Option<bool> {
    match value.get("value").unwrap_or(value) {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => value.as_i64().map(|value| value != 0),
        Value::String(value) => scene_runtime_visibility_text_bool(value),
        _ => None,
    }
}

fn scene_runtime_visibility_value_number(value: &Value) -> Option<f64> {
    let number = match value.get("value").unwrap_or(value) {
        Value::Bool(value) => {
            if *value {
                1.0
            } else {
                0.0
            }
        }
        Value::Number(value) => value.as_f64()?,
        Value::String(value) => scene_runtime_visibility_text_number(value)?,
        _ => return None,
    };
    number.is_finite().then_some(number)
}

fn scene_runtime_visibility_value_string(value: &Value) -> Option<String> {
    match value.get("value").unwrap_or(value) {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn scene_runtime_visibility_text_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" => Some(true),
        "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn scene_runtime_visibility_text_number(value: &str) -> Option<f64> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn scene_runtime_visibility_normalized_text(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneMesh {
    #[serde(default)]
    pub vertices: Vec<SceneMeshVertex>,
    #[serde(default)]
    pub indices: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<SceneMeshSkin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub puppet_clips: Vec<ScenePuppetAnimationClip>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub puppet_clipping_records: Vec<SceneMeshPuppetClippingRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub puppet_clipping_active_sources: Vec<SceneMeshPuppetClippingActiveSource>,
}

impl SceneMesh {
    fn validate(&self, node_id: &str) -> Result<(), SceneError> {
        if self.vertices.len() < 3 {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh must contain at least 3 vertices"
            )));
        }
        if self.indices.len() < 3 || self.indices.len() % 3 != 0 {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh indices must contain complete triangles"
            )));
        }
        for (index, vertex) in self.vertices.iter().enumerate() {
            vertex.validate(node_id, index)?;
        }
        let vertex_count = self.vertices.len();
        for index in &self.indices {
            if usize::try_from(*index).map_or(true, |index| index >= vertex_count) {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh index {index} is outside the vertex array"
                )));
            }
        }
        if let Some(skin) = &self.skin {
            skin.validate(node_id, self.vertices.len())?;
        } else if !self.puppet_clips.is_empty() {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh has puppet clips without skin"
            )));
        }
        if !self.puppet_clips.is_empty() {
            let Some(skin) = &self.skin else {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh has puppet clips without skin"
                )));
            };
            for clip in &self.puppet_clips {
                clip.validate(node_id, skin.bones.len())?;
            }
        }
        if !self.puppet_clipping_records.is_empty() {
            let Some(skin) = &self.skin else {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh has puppet clipping records without skin"
                )));
            };
            for (index, record) in self.puppet_clipping_records.iter().enumerate() {
                record.validate(node_id, index, skin.bones.len())?;
            }
        }
        if !self.puppet_clipping_active_sources.is_empty() {
            if self.skin.is_none() {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh has puppet clipping active sources without skin"
                )));
            }
            for (index, source) in self.puppet_clipping_active_sources.iter().enumerate() {
                source.validate(node_id, index)?;
            }
        }
        Ok(())
    }

    pub(crate) fn sample_puppet_animation(
        &self,
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
    ) -> Option<SceneMesh> {
        let mut vertices = Vec::with_capacity(self.vertices.len());
        self.sample_puppet_animation_vertices_into(layers, time_ms, &mut vertices)?;
        Some(SceneMesh {
            vertices,
            indices: self.indices.clone(),
            skin: self.skin.clone(),
            puppet_clips: Vec::new(),
            puppet_clipping_records: self.puppet_clipping_records.clone(),
            puppet_clipping_active_sources: self.puppet_clipping_active_sources.clone(),
        })
    }

    pub(crate) fn sample_puppet_animation_vertices_into(
        &self,
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
        vertices: &mut Vec<SceneMeshVertex>,
    ) -> Option<()> {
        let inverse_bind_world = self.puppet_inverse_bind_world()?;
        self.sample_puppet_animation_vertices_with_inverse_bind_into(
            layers,
            time_ms,
            &inverse_bind_world,
            vertices,
        )
    }

    pub(crate) fn puppet_inverse_bind_world(&self) -> Option<Vec<[f64; 16]>> {
        let skin = self.skin.as_ref()?;
        Self::puppet_inverse_bind_world_for_skin(skin)
    }

    pub(crate) fn puppet_inverse_bind_world_for_skin(
        skin: &SceneMeshSkin,
    ) -> Option<Vec<[f64; 16]>> {
        let bind_world = scene_puppet_world_matrices(
            skin.bones.iter().map(|bone| bone.parent),
            skin.bones.iter().map(|bone| bone.bind.matrix()),
        )?;
        bind_world
            .iter()
            .map(|matrix| scene_puppet_inverse_affine_matrix(*matrix))
            .collect::<Option<Vec<_>>>()
    }

    pub(crate) fn sample_puppet_animation_vertices_with_inverse_bind_into(
        &self,
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
        inverse_bind_world: &[[f64; 16]],
        vertices: &mut Vec<SceneMeshVertex>,
    ) -> Option<()> {
        self.sample_puppet_animation_vertices_with_clips_and_inverse_bind_into(
            &self.puppet_clips,
            layers,
            time_ms,
            inverse_bind_world,
            vertices,
        )
    }

    pub(crate) fn sample_puppet_animation_vertices_with_clips_and_inverse_bind_into(
        &self,
        clips: &[ScenePuppetAnimationClip],
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
        inverse_bind_world: &[[f64; 16]],
        vertices: &mut Vec<SceneMeshVertex>,
    ) -> Option<()> {
        let matrices = self.sample_puppet_skin_matrices_with_clips_and_inverse_bind(
            clips,
            layers,
            time_ms,
            inverse_bind_world,
        )?;
        self.sample_puppet_animation_vertices_with_skin_matrices_into(&matrices, vertices)
    }

    pub(crate) fn sample_puppet_skin_matrices_with_clips_and_inverse_bind(
        &self,
        clips: &[ScenePuppetAnimationClip],
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
        inverse_bind_world: &[[f64; 16]],
    ) -> Option<ScenePuppetSkinningMatrices> {
        let skin = self.skin.as_ref()?;
        if inverse_bind_world.len() != skin.bones.len() {
            return None;
        }
        let (skin, local_pose) =
            self.sample_puppet_local_pose_with_clips(clips, layers, time_ms)?;
        let pose_world = scene_puppet_world_matrices(
            skin.bones.iter().map(|bone| bone.parent),
            local_pose.iter().map(|transform| transform.matrix()),
        )?;
        let skin_matrices = pose_world
            .iter()
            .zip(inverse_bind_world)
            .map(|(pose, inverse_bind)| scene_puppet_matrix_mul(*pose, *inverse_bind))
            .collect::<Vec<_>>();
        let bone_opacities = local_pose
            .iter()
            .map(|pose| pose.opacity.clamp(0.0, 1.0))
            .collect::<Vec<_>>();
        Some(ScenePuppetSkinningMatrices {
            skin_matrices,
            bone_opacities,
        })
    }

    pub(crate) fn sample_puppet_animation_vertices_with_skin_matrices_into(
        &self,
        matrices: &ScenePuppetSkinningMatrices,
        vertices: &mut Vec<SceneMeshVertex>,
    ) -> Option<()> {
        let skin = self.skin.as_ref()?;
        if skin.vertices.len() != self.vertices.len()
            || matrices.skin_matrices.len() != skin.bones.len()
            || matrices.bone_opacities.len() != skin.bones.len()
        {
            return None;
        }

        vertices.clear();
        vertices.reserve(self.vertices.len().saturating_sub(vertices.capacity()));
        for (vertex, skin_vertex) in self.vertices.iter().zip(&skin.vertices) {
            let mut x = 0.0;
            let mut y = 0.0;
            let mut z = 0.0;
            let mut total_weight = 0.0;
            for slot in 0..4 {
                let weight = skin_vertex.weights[slot];
                if !weight.is_finite() || weight <= f64::EPSILON {
                    continue;
                }
                let bone_index = skin_vertex.bone_indices[slot];
                let point = scene_puppet_transform_point_3d(
                    matrices.skin_matrices[bone_index],
                    vertex.x,
                    vertex.y,
                    0.0,
                );
                x += point[0] * weight;
                y += point[1] * weight;
                z += point[2] * weight;
                total_weight += weight;
            }
            let (sampled_x, sampled_y) = if total_weight > f64::EPSILON {
                (x / total_weight, y / total_weight)
            } else {
                (vertex.x, vertex.y)
            };
            let _ = z;
            vertices.push(SceneMeshVertex {
                x: sampled_x,
                y: sampled_y,
                u: vertex.u,
                v: vertex.v,
                opacity: if total_weight > f64::EPSILON {
                    (0..4)
                        .filter_map(|slot| {
                            let weight = skin_vertex.weights[slot];
                            (weight.is_finite() && weight > f64::EPSILON).then(|| {
                                let bone_index = skin_vertex.bone_indices[slot];
                                matrices
                                    .bone_opacities
                                    .get(bone_index)
                                    .map(|opacity| opacity * weight)
                            })?
                        })
                        .sum::<f64>()
                        / total_weight
                } else {
                    vertex.opacity
                },
            });
        }

        Some(())
    }

    fn puppet_animation_frame_debug(
        &self,
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
    ) -> Vec<ScenePuppetAnimationFrameDebug> {
        let Some(skin) = self.skin.as_ref() else {
            return Vec::new();
        };
        if skin.bones.is_empty() {
            return Vec::new();
        }
        let mut frames = Vec::new();
        for layer in layers {
            if !layer.visible || layer.blend <= 0.0 {
                continue;
            }
            let Some(clip) = self
                .puppet_clips
                .iter()
                .find(|clip| clip.id == layer.clip_id)
            else {
                continue;
            };
            let Some(timing) = clip.sample_timing(layer, time_ms) else {
                continue;
            };
            frames.push(ScenePuppetAnimationFrameDebug {
                clip_id: clip.id,
                clip_name: clip.name.clone(),
                layer_name: layer.name.clone(),
                fps: clip.fps,
                frame_count: clip.frame_count,
                looping: clip.looping,
                rate: layer.rate,
                initial_phase: layer.initial_phase,
                blend: layer.blend,
                additive: layer.additive,
                lock_transforms: layer.lock_transforms,
                frame: timing.frame,
                frame0: timing.frame0,
                frame1: timing.frame1,
                mix: timing.mix,
            });
        }
        frames
    }

    pub(crate) fn sample_puppet_attachment_poses(
        &self,
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
    ) -> Option<BTreeMap<String, ScenePuppetAttachmentPose>> {
        self.sample_puppet_attachment_poses_with_clips(&self.puppet_clips, layers, time_ms)
    }

    pub(crate) fn sample_puppet_attachment_poses_with_clips(
        &self,
        clips: &[ScenePuppetAnimationClip],
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
    ) -> Option<BTreeMap<String, ScenePuppetAttachmentPose>> {
        let (skin, _local_pose, _bind_world, pose_world) =
            self.sample_puppet_pose_world_with_clips(clips, layers, time_ms)?;
        if skin.attachments.is_empty() {
            return None;
        }
        let mut poses = BTreeMap::new();
        for attachment in &skin.attachments {
            let bone_index = attachment.bone_index;
            let pose_point = scene_puppet_transform_point_3d(
                *pose_world.get(bone_index)?,
                attachment.local_position[0],
                attachment.local_position[1],
                attachment.local_position[2],
            );
            let pose_angle = scene_puppet_matrix_rotation_z(*pose_world.get(bone_index)?)?;
            poses.insert(
                attachment.name.clone(),
                ScenePuppetAttachmentPose {
                    x: pose_point[0],
                    y: pose_point[1],
                    rotation_deg: pose_angle.to_degrees(),
                },
            );
        }
        (!poses.is_empty()).then_some(poses)
    }

    fn sample_puppet_pose_world_with_clips(
        &self,
        clips: &[ScenePuppetAnimationClip],
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
    ) -> Option<(
        &SceneMeshSkin,
        Vec<ScenePuppetTransform>,
        Vec<[f64; 16]>,
        Vec<[f64; 16]>,
    )> {
        let (skin, local_pose) =
            self.sample_puppet_local_pose_with_clips(clips, layers, time_ms)?;
        let bind_world = scene_puppet_world_matrices(
            skin.bones.iter().map(|bone| bone.parent),
            skin.bones.iter().map(|bone| bone.bind.matrix()),
        )?;
        let pose_world = scene_puppet_world_matrices(
            skin.bones.iter().map(|bone| bone.parent),
            local_pose.iter().map(|transform| transform.matrix()),
        )?;
        Some((skin, local_pose, bind_world, pose_world))
    }

    fn sample_puppet_local_pose_with_clips(
        &self,
        clips: &[ScenePuppetAnimationClip],
        layers: &[ScenePuppetAnimationLayer],
        time_ms: u64,
    ) -> Option<(&SceneMeshSkin, Vec<ScenePuppetTransform>)> {
        let skin = self.skin.as_ref()?;
        let local_pose = scene_puppet_local_pose_for_skin(skin, clips, layers, time_ms, true)?;
        Some((skin, local_pose))
    }
}

fn scene_puppet_local_pose_for_skin(
    skin: &SceneMeshSkin,
    clips: &[ScenePuppetAnimationClip],
    layers: &[ScenePuppetAnimationLayer],
    time_ms: u64,
    require_active_layer: bool,
) -> Option<Vec<ScenePuppetTransform>> {
    if skin.bones.is_empty() {
        return None;
    }
    let mut local_pose = skin.bones.iter().map(|bone| bone.bind).collect::<Vec<_>>();
    let mut has_layer = false;
    for layer in layers {
        if !layer.visible || layer.blend <= 0.0 {
            continue;
        }
        let clip = clips.iter().find(|clip| clip.id == layer.clip_id)?;
        let sampled = clip.sample(layer, time_ms, skin.bones.len())?;
        let blend = layer.blend.clamp(0.0, 1.0);
        for (bone_index, transform) in sampled.iter().enumerate() {
            let bind = skin.bones.get(bone_index)?.bind;
            if layer.lock_transforms {
                local_pose[bone_index] = local_pose[bone_index].blend_opacity_only(
                    bind,
                    *transform,
                    blend,
                    layer.additive,
                );
            } else if layer.additive {
                local_pose[bone_index] =
                    local_pose[bone_index].additive_blend(bind, *transform, blend);
            } else {
                local_pose[bone_index] = local_pose[bone_index].lerp(*transform, blend);
            }
        }
        has_layer = true;
    }
    (!require_active_layer || has_layer).then_some(local_pose)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshPuppetClippingRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    pub mask: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_resource: Option<String>,
    #[serde(default)]
    pub duration_frames: u32,
    #[serde(default)]
    pub flags: u32,
    #[serde(default)]
    pub bones: Vec<usize>,
    #[serde(default)]
    pub frame_keys: Vec<u32>,
}

impl SceneMeshPuppetClippingRecord {
    fn validate(&self, node_id: &str, index: usize, bone_count: usize) -> Result<(), SceneError> {
        validate_required_text("scene mesh puppet clipping mask", &self.mask)?;
        validate_optional_text("scene mesh puppet clipping source name", &self.source_name)?;
        validate_optional_text(
            "scene mesh puppet clipping mask resource",
            &self.mask_resource,
        )?;
        if self.bones.is_empty() {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh puppet clipping record {index} must reference at least one bone"
            )));
        }
        for bone in &self.bones {
            if *bone >= bone_count {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh puppet clipping record {index} bone {bone} is outside the bone array"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshPuppetClippingActiveSource {
    pub source_name: String,
    pub source_id: u64,
    pub scalar_bits: u32,
    pub source_scale: u32,
    pub flags: u32,
    pub transform_index: u32,
    pub parameter0: f32,
    pub parameter1: f32,
}

impl SceneMeshPuppetClippingActiveSource {
    fn validate(&self, node_id: &str, index: usize) -> Result<(), SceneError> {
        validate_required_text(
            "scene mesh puppet clipping active source name",
            &self.source_name,
        )?;
        if !self.parameter0.is_finite() || !self.parameter1.is_finite() {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh puppet clipping active source {index} parameters must be finite"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshVertex {
    pub x: f64,
    pub y: f64,
    pub u: f64,
    pub v: f64,
    #[serde(
        default = "default_opacity",
        skip_serializing_if = "is_default_opacity"
    )]
    pub opacity: f64,
}

impl Default for SceneMeshVertex {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            u: 0.0,
            v: 0.0,
            opacity: 1.0,
        }
    }
}

impl SceneMeshVertex {
    fn validate(&self, node_id: &str, index: usize) -> Result<(), SceneError> {
        for (field, value) in [("x", self.x), ("y", self.y), ("u", self.u), ("v", self.v)] {
            if !value.is_finite() {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh vertex {index} {field} must be finite"
                )));
            }
        }
        validate_opacity(
            self.opacity,
            &format!("node {node_id:?} mesh vertex {index}"),
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshSkin {
    #[serde(default)]
    pub bones: Vec<SceneMeshSkinBone>,
    #[serde(default)]
    pub vertices: Vec<SceneMeshSkinVertex>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<SceneMeshSkinAttachment>,
}

impl SceneMeshSkin {
    fn validate(&self, node_id: &str, vertex_count: usize) -> Result<(), SceneError> {
        if self.bones.is_empty() {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh skin must contain at least one bone"
            )));
        }
        if self.vertices.len() != vertex_count {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh skin vertex count {} does not match mesh vertex count {vertex_count}",
                self.vertices.len()
            )));
        }
        for (index, bone) in self.bones.iter().enumerate() {
            bone.validate(node_id, index, self.bones.len())?;
        }
        for (index, vertex) in self.vertices.iter().enumerate() {
            vertex.validate(node_id, index, self.bones.len())?;
        }
        for (index, attachment) in self.attachments.iter().enumerate() {
            attachment.validate(node_id, index, self.bones.len())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshSkinAttachment {
    pub name: String,
    pub bone_index: usize,
    #[serde(default)]
    pub local_position: [f64; 3],
    #[serde(default)]
    pub bind_position: [f64; 3],
}

impl SceneMeshSkinAttachment {
    fn validate(&self, node_id: &str, index: usize, bone_count: usize) -> Result<(), SceneError> {
        validate_required_text("scene mesh skin attachment name", &self.name)?;
        if self.bone_index >= bone_count {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh skin attachment {index} bone index {} is outside the bone array",
                self.bone_index
            )));
        }
        for (field, values) in [
            ("local_position", self.local_position),
            ("bind_position", self.bind_position),
        ] {
            for (component, value) in values.iter().enumerate() {
                if !value.is_finite() {
                    return Err(SceneError::invalid(format!(
                        "scene node {node_id:?} mesh skin attachment {index} {field}[{component}] must be finite"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScenePuppetAttachmentPose {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) rotation_deg: f64,
}

impl ScenePuppetAttachmentPose {
    fn transform(self) -> SceneTransform {
        SceneTransform {
            x: self.x,
            y: self.y,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: self.rotation_deg,
            anchor_x: 0.5,
            anchor_y: 0.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScenePuppetSkinningMatrices {
    pub(crate) skin_matrices: Vec<[f64; 16]>,
    pub(crate) bone_opacities: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshSkinBone {
    #[serde(default)]
    pub parent: Option<usize>,
    #[serde(default)]
    pub bind: ScenePuppetTransform,
}

impl SceneMeshSkinBone {
    fn validate(&self, node_id: &str, index: usize, bone_count: usize) -> Result<(), SceneError> {
        if let Some(parent) = self.parent
            && parent >= bone_count
        {
            return Err(SceneError::invalid(format!(
                "scene node {node_id:?} mesh skin bone {index} parent {parent} is outside the bone array"
            )));
        }
        self.bind.validate(node_id, "mesh skin bind transform")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshSkinVertex {
    #[serde(default)]
    pub bone_indices: [usize; 4],
    #[serde(default)]
    pub weights: [f64; 4],
}

impl SceneMeshSkinVertex {
    fn validate(&self, node_id: &str, index: usize, bone_count: usize) -> Result<(), SceneError> {
        for (slot, bone_index) in self.bone_indices.iter().enumerate() {
            if *bone_index >= bone_count {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh skin vertex {index} bone slot {slot} index {bone_index} is outside the bone array"
                )));
            }
        }
        for (slot, weight) in self.weights.iter().enumerate() {
            if !weight.is_finite() || *weight < 0.0 {
                return Err(SceneError::invalid(format!(
                    "scene node {node_id:?} mesh skin vertex {index} weight slot {slot} must be finite and non-negative"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenePuppetAnimationClip {
    pub id: u32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub fps: f64,
    #[serde(default)]
    pub frame_count: u32,
    #[serde(default)]
    pub looping: bool,
    #[serde(default)]
    pub bones: Vec<ScenePuppetAnimationBone>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenePuppetAnimationFrameDebug {
    pub clip_id: u32,
    pub clip_name: Option<String>,
    pub layer_name: Option<String>,
    pub fps: f64,
    pub frame_count: u32,
    pub looping: bool,
    pub rate: f64,
    pub initial_phase: f64,
    pub blend: f64,
    pub additive: bool,
    pub lock_transforms: bool,
    pub frame: f64,
    pub frame0: usize,
    pub frame1: usize,
    pub mix: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScenePuppetAnimationFrameTiming {
    frame: f64,
    frame0: usize,
    frame1: usize,
    mix: f64,
}
