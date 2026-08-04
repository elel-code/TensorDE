
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

include!("timeline_snapshot_mesh/timeline.rs");

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
        return Some("builtin-opacity-mask".to_owned());
    }
    if file == "effects/iris/effect.json" || file.ends_with("/effects/iris/effect.json") {
        return Some("builtin-iris-mask".to_owned());
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
    pub effect_motion: SceneEffectMotion,
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
    pub effect_motion: SceneEffectMotion,
    pub blend_mode: SceneBlendMode,
    pub tint: [f32; 4],
    pub fit: FitMode,
    pub opacity: f64,
    pub transform: SceneTransform,
    pub puppet_animation_frames: Vec<ScenePuppetAnimationFrameDebug>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneEffectMotion {
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

impl SceneEffectMotion {
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

include!("timeline_snapshot_mesh/mesh_and_puppet.rs");
