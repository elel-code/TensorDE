use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;

use crate::core::scene::binary::{
    SCENE_BINARY_EFFECT_UV_MAPPING_TEXTURE_RESOLUTION,
    SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE, SCENE_BINARY_NONE_ID,
    SCENE_BINARY_PARAMETER_ROLE_EFFECT_FBO, SCENE_BINARY_PARAMETER_ROLE_PASS_BIND,
    SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO, SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT,
    SCENE_BINARY_PARAMETER_VALUE_BOOL, SCENE_BINARY_PARAMETER_VALUE_FLOAT,
    SCENE_BINARY_PARAMETER_VALUE_INTEGER, SCENE_BINARY_PARAMETER_VALUE_STRING,
    SCENE_BINARY_PARAMETER_VALUE_VEC2, SCENE_BINARY_PARAMETER_VALUE_VEC3,
    SCENE_BINARY_PARAMETER_VALUE_VEC4, SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE, SceneBinaryChunkKind,
    SceneBinaryEffectParameterRecord, SceneBinaryEffectPassRecord,
    SceneBinaryEffectUvTransformRecord, SceneBinaryError, SceneBinaryGeometryRecord,
    SceneBinaryMaterialPassRecord, SceneBinaryParticleEmitterRecord, SceneBinaryTextureSlotRecord,
    decode_effect_parameter_record, decode_effect_pass_record, decode_effect_uv_transform_record,
    decode_texture_slot_record,
};
use crate::core::scene::{
    SceneEffectFbo, SceneEffectUvExtent, SceneEffectUvMapping, SceneEffectUvTransform,
    ScenePuppetAttachmentDelta,
};
use crate::core::{
    FitMode, SceneBlendMode, SceneNodeKind, ScenePathFillRule, SceneSystems, SceneTextAlign,
    SceneTextureRegion, SceneTransform,
};
use crate::renderer::{
    RendererPlanError, SceneRenderAlphaTextureMode, SceneRenderImageEffectPass, SceneRenderLayer,
    SceneRenderTextureSlot, SceneWallpaperPlan,
};

mod effect_program;
mod engine_plan;
mod facts;
mod mesh;
mod reader;
mod texture;
mod topology;

pub(super) use engine_plan::scene_engine_plan_from_gscn_path_with_properties;
use facts::{
    BinarySceneNames, BinarySceneResource, binary_name, binary_scene_names,
    binary_scene_package_root, binary_scene_puppet_animation_layer_count, binary_scene_resources,
    binary_scene_size, binary_scene_timeline_counts,
};
use mesh::{binary_scene_mesh, binary_scene_puppet_attachment_deltas};
use reader::{BinarySceneReader, binary_scene_cached_record_slice};
use topology::{BinarySceneRetainedTopology, binary_scene_retained_topology};

const BINARY_TRANSFORM_PROPERTY_DEFAULT: u16 = 0;
const BINARY_TRANSFORM_PROPERTY_X: u16 = 1;
const BINARY_TRANSFORM_PROPERTY_Y: u16 = 2;
const BINARY_TRANSFORM_PROPERTY_SCALE_X: u16 = 3;
const BINARY_TRANSFORM_PROPERTY_SCALE_Y: u16 = 4;
const BINARY_TRANSFORM_PROPERTY_OPACITY: u16 = 5;
const BINARY_TRANSFORM_PROPERTY_ROTATION_DEG: u16 = 6;
const BINARY_TRANSFORM_PROPERTY_WIDTH: u16 = 7;
const BINARY_TRANSFORM_PROPERTY_HEIGHT: u16 = 8;
const BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS: u16 = 9;
const BINARY_TRANSFORM_FLAG_LOOP: u16 = 1;
const BINARY_NODE_FLAG_VISIBLE: u16 = 1;
const BINARY_NODE_FLAG_COLOR: u16 = 1 << 7;
const BINARY_NODE_FLAG_STROKE_COLOR: u16 = 1 << 8;
const BINARY_NODE_FLAG_STROKE_WIDTH: u16 = 1 << 9;
const BINARY_NODE_FLAG_CORNER_RADIUS: u16 = 1 << 10;
const BINARY_EFFECT_UV_HAS_INPUT_EXTENT: u16 = 1;
const BINARY_EFFECT_UV_HAS_MASK_EXTENT: u16 = 1 << 1;
const BINARY_EFFECT_UV_HAS_MASK_BACKING_EXTENT: u16 = 1 << 2;
const BINARY_TEXTURE_ROLE_BASE_COLOR: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinarySceneRenderLayerFilter {
    All,
}

impl BinarySceneRenderLayerFilter {
    fn allows_kind(self, _kind: SceneNodeKind) -> bool {
        match self {
            Self::All => true,
        }
    }
}

#[derive(Debug, Clone)]
struct BinarySceneDynamicState {
    nodes: BTreeMap<String, BinarySceneDynamicNode>,
    property_bindings: Vec<BinarySceneDynamicPropertyBinding>,
    properties: BTreeMap<String, Value>,
    bound_properties: Vec<String>,
}

#[derive(Debug, Clone)]
struct BinarySceneDynamicNode {
    visible: bool,
    visibility_condition: Option<Value>,
}

#[derive(Debug, Clone)]
struct BinarySceneDynamicPropertyBinding {
    property: String,
    target_node: Option<String>,
    target: u16,
    scale: f64,
    offset: f64,
}

#[derive(Debug, Deserialize)]
struct BinarySceneRuntimeMetadata {
    #[allow(dead_code)]
    version: Option<u32>,
    #[serde(default)]
    properties: BTreeMap<String, Value>,
    #[serde(default)]
    nodes: Vec<BinarySceneRuntimeMetadataNode>,
    #[serde(default)]
    property_bindings: Vec<BinarySceneRuntimeMetadataPropertyBinding>,
}

#[derive(Debug, Deserialize)]
struct BinarySceneRuntimeMetadataNode {
    id: String,
    #[serde(default = "binary_scene_runtime_metadata_default_visible")]
    visible: bool,
    #[serde(default)]
    visibility_condition: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct BinarySceneRuntimeMetadataPropertyBinding {
    property: String,
    #[serde(default)]
    target_node: Option<String>,
    target: String,
    #[serde(default = "binary_scene_runtime_metadata_default_scale")]
    scale: f64,
    #[serde(default)]
    offset: f64,
}

fn binary_scene_runtime_metadata_default_visible() -> bool {
    true
}

fn binary_scene_runtime_metadata_default_scale() -> f64 {
    1.0
}

fn binary_scene_dynamic_state_from_source_path(
    source_path: &Path,
    render_properties: Option<&BTreeMap<String, Value>>,
) -> Result<Option<BinarySceneDynamicState>, RendererPlanError> {
    let metadata_path = binary_scene_runtime_metadata_path(source_path);
    if !metadata_path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&metadata_path).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to read binary scene runtime metadata {}: {err}",
            metadata_path.display()
        ))
    })?;
    let metadata: BinarySceneRuntimeMetadata = serde_json::from_slice(&bytes).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to parse binary scene runtime metadata {}: {err}",
            metadata_path.display()
        ))
    })?;
    Ok(Some(BinarySceneDynamicState::from_metadata(
        metadata,
        render_properties,
    )))
}

fn binary_scene_runtime_metadata_path(source_path: &Path) -> PathBuf {
    let mut path = source_path.as_os_str().to_os_string();
    path.push(".runtime.json");
    PathBuf::from(path)
}

impl BinarySceneDynamicState {
    fn from_metadata(
        metadata: BinarySceneRuntimeMetadata,
        render_properties: Option<&BTreeMap<String, Value>>,
    ) -> Self {
        let mut properties = binary_scene_runtime_default_properties(&metadata.properties);
        if let Some(render_properties) = render_properties {
            for (property, value) in render_properties {
                let value = binary_scene_coerce_runtime_property_override(
                    properties.get(property),
                    value.clone(),
                );
                properties.insert(property.clone(), value);
            }
        }
        let nodes = metadata
            .nodes
            .into_iter()
            .map(|node| {
                (
                    node.id,
                    BinarySceneDynamicNode {
                        visible: node.visible,
                        visibility_condition: node.visibility_condition,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let property_bindings = metadata
            .property_bindings
            .into_iter()
            .filter_map(BinarySceneDynamicPropertyBinding::from_metadata)
            .collect::<Vec<_>>();
        let mut bound_properties = Vec::new();
        for node in nodes.values() {
            if let Some(property) = node
                .visibility_condition
                .as_ref()
                .and_then(Value::as_object)
                .and_then(|condition| condition.get("property"))
                .and_then(Value::as_str)
            {
                binary_scene_push_unique_property(&mut bound_properties, property);
            }
        }
        for binding in &property_bindings {
            binary_scene_push_unique_property(&mut bound_properties, &binding.property);
        }
        Self {
            nodes,
            property_bindings,
            properties,
            bound_properties,
        }
    }

    fn property_number(&self, property: &str) -> Option<f64> {
        binary_scene_property_number(self.properties.get(property)?)
    }

    fn property_value(&self, property: &str) -> Option<&Value> {
        self.properties.get(property)
    }

    fn property_text(&self, property: &str) -> Option<String> {
        binary_scene_property_text(self.properties.get(property)?).map(str::to_owned)
    }

    fn node_visible(&self, node_id: &str) -> Option<bool> {
        let node = self.nodes.get(node_id)?;
        if !node.visible {
            return Some(false);
        }
        let Some(condition) = node.visibility_condition.as_ref() else {
            return Some(true);
        };
        Some(binary_scene_dynamic_visibility_condition_matches(
            condition,
            |property| self.property_number(property),
            |property| self.property_text(property),
        ))
    }
}

impl BinarySceneDynamicPropertyBinding {
    fn from_metadata(binding: BinarySceneRuntimeMetadataPropertyBinding) -> Option<Self> {
        Some(Self {
            property: binding.property,
            target_node: binding.target_node,
            target: binary_scene_dynamic_property_target(&binding.target)?,
            scale: binding.scale,
            offset: binding.offset,
        })
    }
}

fn binary_scene_coerce_runtime_property_override(default: Option<&Value>, value: Value) -> Value {
    if default.is_some_and(Value::is_string) && !value.is_string() {
        return Value::String(
            binary_scene_value_string(&value).unwrap_or_else(|| value.to_string()),
        );
    }
    if default.is_some_and(Value::is_boolean)
        && let Some(value) = binary_scene_value_bool(&value)
    {
        return Value::Bool(value);
    }
    value
}

fn binary_scene_runtime_default_properties(
    properties: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    properties
        .iter()
        .filter_map(|(name, spec)| {
            let value = spec
                .as_object()
                .and_then(|spec| spec.get("default"))
                .cloned()
                .unwrap_or_else(|| spec.clone());
            (!value.is_null()).then(|| (name.clone(), value))
        })
        .collect()
}

fn binary_scene_push_unique_property(properties: &mut Vec<String>, property: &str) {
    if !properties.iter().any(|existing| existing == property) {
        properties.push(property.to_owned());
    }
}

fn binary_scene_dynamic_property_target(target: &str) -> Option<u16> {
    match target {
        "x" => Some(BINARY_TRANSFORM_PROPERTY_X),
        "y" => Some(BINARY_TRANSFORM_PROPERTY_Y),
        "scale_x" | "scaleX" | "scalex" => Some(BINARY_TRANSFORM_PROPERTY_SCALE_X),
        "scale_y" | "scaleY" | "scaley" => Some(BINARY_TRANSFORM_PROPERTY_SCALE_Y),
        "opacity" | "alpha" => Some(BINARY_TRANSFORM_PROPERTY_OPACITY),
        "rotation" | "rotation_deg" | "angle" => Some(BINARY_TRANSFORM_PROPERTY_ROTATION_DEG),
        "width" => Some(BINARY_TRANSFORM_PROPERTY_WIDTH),
        "height" => Some(BINARY_TRANSFORM_PROPERTY_HEIGHT),
        "corner_radius" | "cornerRadius" => Some(BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS),
        _ => None,
    }
}

fn binary_scene_property_number(value: &Value) -> Option<f64> {
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    if let Some(value) = value.as_bool() {
        return Some(if value { 1.0 } else { 0.0 });
    }
    None
}

fn binary_scene_property_text(value: &Value) -> Option<&str> {
    value.as_str()
}

fn binary_scene_dynamic_visibility_condition_matches<N, T>(
    condition: &Value,
    resolve_number: N,
    resolve_text: T,
) -> bool
where
    N: Fn(&str) -> Option<f64>,
    T: Fn(&str) -> Option<String>,
{
    let Some(condition) = condition.as_object() else {
        return true;
    };
    if condition
        .get("runtime")
        .and_then(Value::as_str)
        .is_some_and(|runtime| runtime != "wallpaper-engine-user-condition")
    {
        return true;
    }
    let default_visible = condition
        .get("default_visible")
        .and_then(binary_scene_value_bool)
        .unwrap_or_else(|| {
            condition
                .get("authored_value")
                .and_then(binary_scene_value_bool)
                .unwrap_or(true)
        });
    let Some(property) = condition
        .get("property")
        .and_then(binary_scene_value_string)
    else {
        return default_visible;
    };
    let Some(expected) = condition.get("condition") else {
        return default_visible;
    };
    let actual_number = resolve_number(&property);
    let actual_text = resolve_text(&property);
    if actual_number.is_none() && actual_text.is_none() {
        return default_visible;
    }
    binary_scene_dynamic_expected_matches(expected, actual_number, actual_text.as_deref())
}

fn binary_scene_dynamic_expected_matches(
    expected: &Value,
    actual_number: Option<f64>,
    actual_text: Option<&str>,
) -> bool {
    let expected = expected.get("value").unwrap_or(expected);
    if let Some(expected_bool) = binary_scene_value_bool(expected) {
        if let Some(actual_number) = actual_number {
            return (actual_number.abs() > f64::EPSILON) == expected_bool;
        }
        return actual_text
            .and_then(binary_scene_text_bool)
            .is_some_and(|actual| actual == expected_bool);
    }
    if let Some(expected_number) = binary_scene_value_number(expected) {
        if let Some(actual_number) = actual_number {
            return (actual_number - expected_number).abs() <= 0.000_001;
        }
        return actual_text
            .and_then(binary_scene_text_number)
            .is_some_and(|actual| (actual - expected_number).abs() <= 0.000_001);
    }
    let Some(expected_text) = binary_scene_value_string(expected) else {
        return false;
    };
    if let Some(actual_text) = actual_text
        && binary_scene_normalized_text(actual_text) == binary_scene_normalized_text(&expected_text)
    {
        return true;
    }
    if let Some(expected_number) = binary_scene_text_number(&expected_text)
        && let Some(actual_number) = actual_number
    {
        return (actual_number - expected_number).abs() <= 0.000_001;
    }
    false
}

fn binary_scene_value_bool(value: &Value) -> Option<bool> {
    value
        .as_bool()
        .or_else(|| binary_scene_text_bool(value.as_str()?))
}

fn binary_scene_text_bool(value: &str) -> Option<bool> {
    match binary_scene_normalized_text(value).as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn binary_scene_value_number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| binary_scene_text_number(value.as_str()?))
}

fn binary_scene_text_number(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok()
}

fn binary_scene_value_string(value: &Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        return Some(value.to_owned());
    }
    if let Some(value) = value.as_bool() {
        return Some(if value { "1" } else { "0" }.to_owned());
    }
    if let Some(value) = value.as_i64() {
        return Some(value.to_string());
    }
    if let Some(value) = value.as_u64() {
        return Some(value.to_string());
    }
    if let Some(value) = value.as_f64() {
        if value.is_finite() && (value.fract()).abs() <= f64::EPSILON {
            return Some(format!("{value:.0}"));
        }
        return Some(value.to_string());
    }
    None
}

fn binary_scene_normalized_text(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(super) fn scene_wallpaper_plan_from_gscn_path(
    output_name: String,
    source_path: PathBuf,
    target_max_fps: Option<u32>,
    snapshot_time_ms: u64,
    fit_override: Option<FitMode>,
) -> Result<SceneWallpaperPlan, RendererPlanError> {
    scene_wallpaper_plan_from_gscn_path_with_properties(
        output_name,
        source_path,
        target_max_fps,
        snapshot_time_ms,
        fit_override,
        None,
    )
}

pub(super) fn scene_wallpaper_plan_from_gscn_path_with_properties(
    output_name: String,
    source_path: PathBuf,
    target_max_fps: Option<u32>,
    snapshot_time_ms: u64,
    fit_override: Option<FitMode>,
    render_properties: Option<&BTreeMap<String, Value>>,
) -> Result<SceneWallpaperPlan, RendererPlanError> {
    let mut reader = BinarySceneReader::open(&source_path)?;
    let names = binary_scene_names(&mut reader)?;
    let package_root = binary_scene_package_root(&source_path);
    let resources = binary_scene_resources(&mut reader, &names, &package_root)?;
    let topology = binary_scene_retained_topology(&mut reader, &resources)?;
    let scene_size = binary_scene_size(&mut reader)?;
    let dynamic_state =
        binary_scene_dynamic_state_from_source_path(&source_path, render_properties)?;
    let layers = binary_scene_render_layers(
        &mut reader,
        &names,
        &resources,
        &topology,
        snapshot_time_ms,
        dynamic_state.as_ref(),
    )?;
    let (timeline_animation_count, timeline_animated_layer_count) =
        binary_scene_timeline_counts(&mut reader)?;
    let puppet_animation_layer_count = binary_scene_puppet_animation_layer_count(&mut reader)?;
    let particle_emitter_count = reader.chunk_count(SceneBinaryChunkKind::ParticleEmitter);
    let scene_systems = SceneSystems {
        particles: if particle_emitter_count > 0 {
            crate::core::SceneSystemStatus::Ready
        } else {
            crate::core::SceneSystemStatus::Absent
        },
        ..Default::default()
    };

    Ok(SceneWallpaperPlan {
        output_name,
        source: Some(source_path),
        manifest_max_fps: None,
        target_max_fps,
        snapshot_time_ms,
        scene_size,
        scene_fit: fit_override.unwrap_or(FitMode::Stretch),
        scene_systems,
        audio_cue_count: 0,
        bound_properties: dynamic_state
            .as_ref()
            .map(|state| state.bound_properties.clone())
            .unwrap_or_default(),
        timeline_animation_count,
        timeline_animated_layer_count,
        puppet_animation_layer_count,
        property_binding_count: dynamic_state
            .as_ref()
            .map(|state| state.property_bindings.len())
            .unwrap_or_default(),
        cursor_parallax_input_ready: false,
        scene_input_properties: dynamic_state
            .as_ref()
            .map(|state| state.properties.clone())
            .unwrap_or_default(),
        scene_scenescript_binding_count: 0,
        scene_material_graph_count: reader.chunk_count(SceneBinaryChunkKind::MaterialPass),
        scene_material_graph_resource_count: resources.len(),
        scene_effect_graph_count: reader.chunk_count(SceneBinaryChunkKind::EffectPass),
        scene_audio_response_binding_count: 0,
        unsupported_scene_features: Vec::new(),
        display: None,
        layers,
    })
}

fn binary_scene_render_layers(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    topology: &BinarySceneRetainedTopology,
    snapshot_time_ms: u64,
    dynamic_state: Option<&BinarySceneDynamicState>,
) -> Result<Vec<SceneRenderLayer>, RendererPlanError> {
    let mut layers = Vec::new();
    binary_scene_render_layers_into(
        reader,
        names,
        resources,
        topology,
        snapshot_time_ms,
        dynamic_state,
        BinarySceneRenderLayerFilter::All,
        &mut layers,
    )?;
    Ok(layers)
}

fn binary_scene_render_layers_into(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    topology: &BinarySceneRetainedTopology,
    snapshot_time_ms: u64,
    dynamic_state: Option<&BinarySceneDynamicState>,
    filter: BinarySceneRenderLayerFilter,
    layers: &mut Vec<SceneRenderLayer>,
) -> Result<(), RendererPlanError> {
    layers.clear();
    reader.puppet_attachment_delta_cache.clear();
    let node_states =
        binary_scene_effective_node_states(reader, names, snapshot_time_ms, dynamic_state, true)?;
    layers.reserve(topology.renderables.len());
    for renderable in &topology.renderables {
        if !filter.allows_kind(renderable.render_layer_kind()) {
            continue;
        }
        let Some(effective_state) = node_states.get(renderable.node_index) else {
            return Err(RendererPlanError::PackageLoad(format!(
                "binary scene retained topology node index {} is out of range",
                renderable.node_index
            )));
        };
        let mut node_state = effective_state.state;
        if !effective_state.visible {
            node_state.opacity = 0.0;
        }
        if let Some(particle) = renderable.particle {
            if let Some(layer) = binary_scene_particle_render_layer(
                reader,
                names,
                resources,
                renderable.node,
                particle,
                renderable.material_index,
                renderable.material,
                node_state,
            )? {
                layers.push(layer);
            }
            continue;
        }
        let layer = binary_scene_render_layer(
            reader,
            names,
            resources,
            renderable.node,
            renderable.geometry,
            renderable.material_index,
            renderable.material,
            renderable.kind,
            node_state,
        )?;
        layers.push(layer);
    }
    Ok(())
}

fn binary_scene_effective_node_states(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    snapshot_time_ms: u64,
    dynamic_state: Option<&BinarySceneDynamicState>,
    include_sampled_pose_dynamics: bool,
) -> Result<Vec<BinarySceneEffectiveNodeState>, RendererPlanError> {
    let node_records = reader.node_records_cached()?;
    let mut node_states = Vec::with_capacity(node_records.len());
    for node in node_records.iter().copied() {
        let geometry = if node.geometry_index == SCENE_BINARY_NONE_ID {
            None
        } else {
            Some(reader.geometry_record_cached(node.geometry_index)?)
        };
        let mut local_state = binary_scene_node_state(reader, node, geometry, snapshot_time_ms)?;
        let node_id = binary_name(names, node.id_name);
        let local_sampled_pose_dynamic = include_sampled_pose_dynamics
            && (binary_scene_node_has_timed_transform(reader, node)?
                || node.puppet_attachment_name != SCENE_BINARY_NONE_ID
                || binary_scene_node_has_dynamic_property_binding(node_id, dynamic_state));
        if let Some(dynamic_state) = dynamic_state
            && let Some(node_id) = node_id
        {
            binary_scene_apply_dynamic_property_bindings(&mut local_state, node_id, dynamic_state);
        }
        let parent_state = binary_scene_parent_node_state(&node_states, node.parent_index)?;
        binary_scene_apply_puppet_attachment_delta(
            names,
            &mut local_state.transform,
            node.puppet_attachment_name,
            parent_state.and_then(|state| state.puppet_attachment_deltas.as_ref()),
        );
        let mut effective_state = binary_scene_effective_node_state(
            names,
            node,
            local_state,
            parent_state,
            dynamic_state,
            local_sampled_pose_dynamic,
        );
        effective_state.puppet_attachment_deltas = binary_scene_puppet_attachment_deltas(
            reader,
            names,
            node.puppet_index,
            snapshot_time_ms,
        )?;
        node_states.push(effective_state);
    }
    Ok(node_states)
}

#[allow(clippy::too_many_arguments)]
fn binary_scene_particle_render_layer(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    node: crate::core::scene::binary::SceneBinaryNodeRecord,
    particle: SceneBinaryParticleEmitterRecord,
    material_index: u32,
    material: Option<SceneBinaryMaterialPassRecord>,
    node_state: BinarySceneNodeState,
) -> Result<Option<SceneRenderLayer>, RendererPlanError> {
    let particle_count = particle.particle_count();
    if particle_count == 0 {
        return Ok(None);
    }

    let node_resource = binary_resource_by_name(resources, node.resource_name);
    let material_texture_slots = if let Some(material) = material {
        let slots = binary_scene_material_texture_slots_cached(
            reader,
            material_index,
            material,
            resources,
        )?;
        if slots.is_empty() {
            binary_scene_particle_base_texture_slot(node_resource)
        } else {
            slots
        }
    } else {
        binary_scene_particle_base_texture_slot(node_resource)
    };
    let source = node_resource
        .and_then(|resource| resource.source.clone())
        .or_else(|| {
            material_texture_slots
                .iter()
                .find(|slot| slot.slot == 0)
                .map(|slot| slot.source.clone())
        });
    let Some(source) = source else {
        return Ok(None);
    };
    let particle_width = f64::from(particle.particle_width);
    let particle_height = f64::from(particle.particle_height);
    if !particle_width.is_finite()
        || !particle_height.is_finite()
        || particle_width <= 0.0
        || particle_height <= 0.0
    {
        return Ok(None);
    }
    // The particle base image is already represented by SceneRenderLayer::source.
    // Retaining slot 0 here clones thousands of identical paths per frame.
    let texture_slots = material_texture_slots
        .iter()
        .filter(|slot| slot.slot != 0)
        .cloned()
        .collect::<Vec<_>>();
    let blend_mode = material
        .map(|material| binary_scene_blend_mode(material.blend_mode))
        .unwrap_or_default();
    let mut transform = node_state.transform;
    transform.anchor_x = 0.5;
    transform.anchor_y = 0.5;
    Ok(Some(SceneRenderLayer {
        id: binary_name(names, node.id_name)
            .unwrap_or("binary-particle-emitter")
            .to_owned(),
        kind: SceneNodeKind::Image,
        source: Some(source),
        texture_slots,
        alpha_texture_slot: None,
        alpha_texture_mode: SceneRenderAlphaTextureMode::Multiply,
        image_effect_passes: Vec::new(),
        composite_key: None,
        texture_region: None::<SceneTextureRegion>,
        effect_motion: Default::default(),
        blend_mode,
        audio: Vec::new(),
        color: Some(binary_scene_rgba_hex(particle.color_rgba)),
        stroke_color: None,
        stroke_width: None,
        corner_radius: None,
        width: Some(particle_width.max(1.0)),
        height: Some(particle_height.max(1.0)),
        mesh: None,
        text: None,
        font_size: None,
        font_family: None,
        font_source: None,
        font_weight: None,
        text_align: None,
        path_data: None,
        path_fill_rule: ScenePathFillRule::default(),
        fit: binary_scene_fit(node.fit),
        opacity: node_state.opacity.clamp(0.0, 1.0),
        transform,
    }))
}

fn binary_scene_particle_base_texture_slot(
    resource: Option<&BinarySceneResource>,
) -> Vec<SceneRenderTextureSlot> {
    let Some(resource) = resource else {
        return Vec::new();
    };
    let Some(source) = resource.source.clone() else {
        return Vec::new();
    };
    vec![SceneRenderTextureSlot {
        slot: 0,
        source,
        width: resource.width,
        height: resource.height,
    }]
}

fn binary_scene_render_layer(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    node: crate::core::scene::binary::SceneBinaryNodeRecord,
    geometry: SceneBinaryGeometryRecord,
    material_index: u32,
    material: Option<SceneBinaryMaterialPassRecord>,
    kind: SceneNodeKind,
    node_state: BinarySceneNodeState,
) -> Result<SceneRenderLayer, RendererPlanError> {
    let material_texture_slots = if let Some(material) = material {
        binary_scene_material_texture_slots_cached(reader, material_index, material, resources)?
    } else {
        Vec::new()
    };
    let image_effect_passes = if let Some(material) = material {
        binary_scene_image_effect_passes_cached(reader, names, material_index, material, resources)?
    } else {
        Vec::new()
    };
    let node_resource = binary_resource_by_name(resources, node.resource_name);
    let source = node_resource
        .and_then(|resource| resource.source.clone())
        .or_else(|| {
            material_texture_slots
                .iter()
                .find(|slot| slot.slot == 0)
                .map(|slot| slot.source.clone())
        });
    let blend_mode = material
        .map(|material| binary_scene_blend_mode(material.blend_mode))
        .unwrap_or_default();
    let layer_id = binary_name(names, node.id_name)
        .unwrap_or("binary-node")
        .to_owned();
    Ok(SceneRenderLayer {
        id: layer_id.clone(),
        kind,
        source,
        texture_slots: material_texture_slots,
        alpha_texture_slot: material.and_then(binary_scene_alpha_texture_slot),
        alpha_texture_mode: material
            .map(binary_scene_alpha_texture_mode)
            .unwrap_or_default(),
        image_effect_passes,
        composite_key: None,
        texture_region: None::<SceneTextureRegion>,
        effect_motion: Default::default(),
        blend_mode,
        audio: Vec::new(),
        color: binary_scene_flagged_color(node.flags, BINARY_NODE_FLAG_COLOR, node.color_rgba),
        stroke_color: binary_scene_flagged_color(
            node.flags,
            BINARY_NODE_FLAG_STROKE_COLOR,
            node.stroke_color_rgba,
        ),
        stroke_width: (node.flags & BINARY_NODE_FLAG_STROKE_WIDTH != 0)
            .then_some(f64::from(node.stroke_width)),
        corner_radius: node_state.corner_radius,
        width: node_state.width,
        height: node_state.height,
        mesh: binary_scene_mesh(
            reader,
            names,
            node.geometry_index,
            geometry,
            node.puppet_index,
        )?,
        text: binary_name(names, node.text_name).map(str::to_owned),
        font_size: (node.font_size > 0.0).then_some(f64::from(node.font_size)),
        font_family: binary_name(names, node.font_family_name).map(str::to_owned),
        font_source: binary_resource_by_name(resources, node.font_resource_name)
            .and_then(|resource| resource.source.clone()),
        font_weight: binary_name(names, node.font_weight_name).map(str::to_owned),
        text_align: binary_scene_text_align(node.text_align),
        path_data: None,
        path_fill_rule: ScenePathFillRule::default(),
        fit: binary_scene_fit(node.fit),
        opacity: node_state.opacity,
        transform: node_state.transform,
    })
}

fn binary_scene_material_texture_slots_cached(
    reader: &mut BinarySceneReader,
    material_index: u32,
    material: SceneBinaryMaterialPassRecord,
    resources: &[BinarySceneResource],
) -> Result<Vec<SceneRenderTextureSlot>, RendererPlanError> {
    if let Some(slots) = reader.material_texture_slots_cache.get(&material_index) {
        return Ok((**slots).clone());
    }
    let slots = Arc::new(binary_scene_material_texture_slots(
        reader, material, resources,
    )?);
    reader
        .material_texture_slots_cache
        .insert(material_index, Arc::clone(&slots));
    Ok((*slots).clone())
}

fn binary_scene_material_texture_slots(
    reader: &mut BinarySceneReader,
    material: SceneBinaryMaterialPassRecord,
    resources: &[BinarySceneResource],
) -> Result<Vec<SceneRenderTextureSlot>, RendererPlanError> {
    let slots = reader.record_range(
        SceneBinaryChunkKind::TextureSlots,
        SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
        material.first_texture_slot,
        material.texture_slot_count,
        decode_texture_slot_record,
    )?;
    binary_scene_texture_slots(slots, resources, |slot| {
        slot.role_flags & BINARY_TEXTURE_ROLE_BASE_COLOR != 0
    })
}

fn binary_scene_image_effect_passes_cached(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    material_index: u32,
    material: SceneBinaryMaterialPassRecord,
    resources: &[BinarySceneResource],
) -> Result<Vec<SceneRenderImageEffectPass>, RendererPlanError> {
    if let Some(passes) = reader.material_effect_passes_cache.get(&material_index) {
        return Ok((**passes).clone());
    }
    let passes = Arc::new(binary_scene_image_effect_passes(
        reader, names, material, resources,
    )?);
    reader
        .material_effect_passes_cache
        .insert(material_index, Arc::clone(&passes));
    Ok((*passes).clone())
}

fn binary_scene_image_effect_passes(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    material: SceneBinaryMaterialPassRecord,
    resources: &[BinarySceneResource],
) -> Result<Vec<SceneRenderImageEffectPass>, RendererPlanError> {
    let passes = reader.record_range(
        SceneBinaryChunkKind::EffectPass,
        reader.layout_record_size(SceneBinaryChunkKind::EffectPass)?,
        material.first_effect_pass,
        material.effect_pass_count,
        decode_effect_pass_record,
    )?;
    let mut output = Vec::with_capacity(passes.len());
    for pass in passes {
        output.push(binary_scene_image_effect_pass(
            reader, names, resources, pass,
        )?);
    }
    Ok(output)
}

fn binary_scene_image_effect_pass(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    pass: SceneBinaryEffectPassRecord,
) -> Result<SceneRenderImageEffectPass, RendererPlanError> {
    let texture_slots = reader.record_range(
        SceneBinaryChunkKind::TextureSlots,
        SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
        pass.first_texture_slot,
        pass.texture_slot_count,
        decode_texture_slot_record,
    )?;
    let transforms = reader.record_range(
        SceneBinaryChunkKind::EffectUvTransform,
        SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE,
        pass.first_effect_uv_transform,
        pass.effect_uv_transform_count,
        decode_effect_uv_transform_record,
    )?;
    let parameters = reader.record_range(
        SceneBinaryChunkKind::EffectParameter,
        crate::core::scene::binary::SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
        pass.first_parameter,
        pass.parameter_count,
        decode_effect_parameter_record,
    )?;
    let effect_file = binary_name(names, pass.effect_name)
        .unwrap_or("")
        .to_owned();
    let shader = binary_name(names, pass.shader_name).map(str::to_owned);
    let blending = binary_name(names, pass.blending_name).map(str::to_owned);
    let command = binary_name(names, pass.command_name).map(str::to_owned);
    let source = binary_name(names, pass.source_name).map(str::to_owned);
    let target = binary_name(names, pass.target_name).map(str::to_owned);
    let (binds, fbos, combos, constant_shader_values) =
        binary_scene_image_effect_parameters(names, parameters);
    Ok(SceneRenderImageEffectPass {
        effect_file: effect_file.clone(),
        runtime: binary_scene_effect_runtime(pass.kind, &effect_file),
        pass_index: pass.pass_index as usize,
        command,
        source,
        target,
        binds,
        fbos,
        shader,
        blending,
        depthtest: binary_scene_material_flag(pass.depth_test),
        depthwrite: binary_scene_material_flag(pass.depth_write),
        cullmode: binary_scene_cull_mode(pass.cull_mode),
        texture_slots: binary_scene_texture_slots(texture_slots, resources, |_| true)?,
        effect_uv_transform: transforms
            .into_iter()
            .next()
            .map(binary_scene_effect_uv_transform),
        combos,
        constant_shader_values,
    })
}

fn binary_scene_image_effect_parameters(
    names: &BinarySceneNames,
    parameters: Vec<SceneBinaryEffectParameterRecord>,
) -> (
    BTreeMap<u32, String>,
    Vec<SceneEffectFbo>,
    BTreeMap<String, i64>,
    BTreeMap<String, Value>,
) {
    let mut binds = BTreeMap::new();
    let mut fbos = Vec::new();
    let mut combos = BTreeMap::new();
    let mut constants = BTreeMap::new();
    for parameter in parameters {
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_EFFECT_FBO != 0 {
            if let Some(name) = binary_name(names, parameter.parameter_name) {
                fbos.push(SceneEffectFbo {
                    name: name.to_owned(),
                    format: binary_name(names, parameter.value_name).map(str::to_owned),
                    scale: if parameter.value0.is_finite() && parameter.value0 > 0.0 {
                        parameter.value0 as f64
                    } else {
                        1.0
                    },
                    unique: parameter.integer_value != 0,
                });
            }
            continue;
        }
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_BIND != 0 {
            let slot = u32::try_from(parameter.integer_value)
                .ok()
                .or_else(|| {
                    binary_name(names, parameter.parameter_name).and_then(|name| name.parse().ok())
                })
                .unwrap_or(0);
            if let Some(name) = binary_name(names, parameter.value_name) {
                binds.insert(slot, name.to_owned());
            }
            continue;
        }
        let Some(name) = binary_name(names, parameter.parameter_name) else {
            continue;
        };
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO != 0 {
            combos.insert(name.to_owned(), parameter.integer_value);
            continue;
        }
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT != 0
            && let Some(value) = binary_scene_effect_parameter_value(names, parameter)
        {
            constants.insert(name.to_owned(), value);
        }
    }
    (binds, fbos, combos, constants)
}

fn binary_scene_effect_parameter_value(
    names: &BinarySceneNames,
    parameter: SceneBinaryEffectParameterRecord,
) -> Option<Value> {
    match parameter.value_kind {
        SCENE_BINARY_PARAMETER_VALUE_BOOL => Some(Value::Bool(parameter.integer_value != 0)),
        SCENE_BINARY_PARAMETER_VALUE_FLOAT => {
            serde_json::Number::from_f64(parameter.value0 as f64).map(Value::Number)
        }
        SCENE_BINARY_PARAMETER_VALUE_INTEGER => Some(Value::Number(serde_json::Number::from(
            parameter.integer_value,
        ))),
        SCENE_BINARY_PARAMETER_VALUE_STRING => binary_name(names, parameter.value_name)
            .map(str::to_owned)
            .map(Value::String),
        SCENE_BINARY_PARAMETER_VALUE_VEC2 => Some(Value::Array(vec![
            Value::from(parameter.value0 as f64),
            Value::from(parameter.value1 as f64),
        ])),
        SCENE_BINARY_PARAMETER_VALUE_VEC3 => Some(Value::Array(vec![
            Value::from(parameter.value0 as f64),
            Value::from(parameter.value1 as f64),
            Value::from(parameter.value2 as f64),
        ])),
        SCENE_BINARY_PARAMETER_VALUE_VEC4 => Some(Value::Array(vec![
            Value::from(parameter.value0 as f64),
            Value::from(parameter.value1 as f64),
            Value::from(parameter.value2 as f64),
            Value::from(parameter.value3 as f64),
        ])),
        _ => None,
    }
}

fn binary_scene_texture_slots(
    slots: Vec<SceneBinaryTextureSlotRecord>,
    resources: &[BinarySceneResource],
    keep: impl Fn(&SceneBinaryTextureSlotRecord) -> bool,
) -> Result<Vec<SceneRenderTextureSlot>, RendererPlanError> {
    let mut output = Vec::with_capacity(slots.len());
    for slot in slots {
        if !keep(&slot) {
            continue;
        }
        let Some(resource) = resources.get(slot.resource_index as usize) else {
            continue;
        };
        let Some(source) = resource.source.clone() else {
            continue;
        };
        output.push(SceneRenderTextureSlot {
            slot: slot.slot,
            source,
            width: resource.width.or((slot.width > 0).then_some(slot.width)),
            height: resource.height.or((slot.height > 0).then_some(slot.height)),
        });
    }
    Ok(output)
}

#[derive(Debug, Clone, Copy)]
struct BinarySceneNodeState {
    transform: SceneTransform,
    opacity: f64,
    width: Option<f64>,
    height: Option<f64>,
    corner_radius: Option<f64>,
}

#[derive(Debug, Clone)]
struct BinarySceneEffectiveNodeState {
    visible: bool,
    state: BinarySceneNodeState,
    puppet_attachment_deltas: Option<BTreeMap<String, ScenePuppetAttachmentDelta>>,
    sampled_pose_dynamic: bool,
}

fn binary_scene_node_state(
    reader: &mut BinarySceneReader,
    node: crate::core::scene::binary::SceneBinaryNodeRecord,
    geometry: Option<SceneBinaryGeometryRecord>,
    snapshot_time_ms: u64,
) -> Result<BinarySceneNodeState, RendererPlanError> {
    let mut state = BinarySceneNodeState {
        transform: SceneTransform::default(),
        opacity: f64::from(node.opacity),
        width: geometry
            .and_then(|geometry| (geometry.width > 0.0).then_some(f64::from(geometry.width))),
        height: geometry
            .and_then(|geometry| (geometry.height > 0.0).then_some(f64::from(geometry.height))),
        corner_radius: (node.flags & BINARY_NODE_FLAG_CORNER_RADIUS != 0)
            .then_some(f64::from(node.corner_radius)),
    };
    let timeline_record_count = reader.chunk_count(SceneBinaryChunkKind::TransformTimeline);
    let timeline_records = reader.transform_timeline_records_cached()?;
    let records = binary_scene_cached_record_slice(
        &timeline_records,
        SceneBinaryChunkKind::TransformTimeline,
        node.first_transform,
        node.transform_count,
        timeline_record_count,
    )?;
    for record in records.iter().copied() {
        if record.property == BINARY_TRANSFORM_PROPERTY_DEFAULT {
            state.transform = binary_scene_default_transform(record);
            continue;
        }
        if record.keyframe_count == 0 {
            continue;
        }
        let Some(value) = binary_scene_transform_timeline_value(reader, record, snapshot_time_ms)?
        else {
            continue;
        };
        binary_scene_apply_timeline_value(&mut state, record.property, value);
    }
    Ok(state)
}

fn binary_scene_parent_node_state(
    states: &[BinarySceneEffectiveNodeState],
    parent_index: u32,
) -> Result<Option<&BinarySceneEffectiveNodeState>, RendererPlanError> {
    if parent_index == SCENE_BINARY_NONE_ID {
        return Ok(None);
    }
    let Some(state) = states.get(parent_index as usize) else {
        return Err(RendererPlanError::PackageLoad(format!(
            "binary scene node parent index {parent_index} is not before its child"
        )));
    };
    Ok(Some(state))
}

fn binary_scene_effective_node_state(
    names: &BinarySceneNames,
    node: crate::core::scene::binary::SceneBinaryNodeRecord,
    local: BinarySceneNodeState,
    parent: Option<&BinarySceneEffectiveNodeState>,
    dynamic_state: Option<&BinarySceneDynamicState>,
    local_sampled_pose_dynamic: bool,
) -> BinarySceneEffectiveNodeState {
    let local_visible = dynamic_state
        .and_then(|state| {
            binary_name(names, node.id_name).and_then(|node_id| state.node_visible(node_id))
        })
        .unwrap_or(node.flags & BINARY_NODE_FLAG_VISIBLE != 0);
    let visible = local_visible && parent.is_none_or(|parent| parent.visible);
    let Some(parent) = parent else {
        return BinarySceneEffectiveNodeState {
            visible,
            state: local,
            puppet_attachment_deltas: None,
            sampled_pose_dynamic: local_sampled_pose_dynamic,
        };
    };
    BinarySceneEffectiveNodeState {
        visible,
        state: BinarySceneNodeState {
            transform: binary_scene_compose_transform(parent.state.transform, local.transform),
            opacity: (parent.state.opacity * local.opacity).clamp(0.0, 1.0),
            width: local.width,
            height: local.height,
            corner_radius: local.corner_radius,
        },
        puppet_attachment_deltas: None,
        sampled_pose_dynamic: parent.sampled_pose_dynamic || local_sampled_pose_dynamic,
    }
}

fn binary_scene_node_has_timed_transform(
    reader: &mut BinarySceneReader,
    node: crate::core::scene::binary::SceneBinaryNodeRecord,
) -> Result<bool, RendererPlanError> {
    let timeline_record_count = reader.chunk_count(SceneBinaryChunkKind::TransformTimeline);
    let timeline_records = reader.transform_timeline_records_cached()?;
    let records = binary_scene_cached_record_slice(
        &timeline_records,
        SceneBinaryChunkKind::TransformTimeline,
        node.first_transform,
        node.transform_count,
        timeline_record_count,
    )?;
    Ok(records.iter().any(|record| {
        record.property != BINARY_TRANSFORM_PROPERTY_DEFAULT && record.keyframe_count > 0
    }))
}

fn binary_scene_node_has_dynamic_property_binding(
    node_id: Option<&str>,
    dynamic_state: Option<&BinarySceneDynamicState>,
) -> bool {
    let Some(dynamic_state) = dynamic_state else {
        return false;
    };
    dynamic_state.property_bindings.iter().any(|binding| {
        binding
            .target_node
            .as_deref()
            .is_none_or(|target| node_id.is_some_and(|node_id| target == node_id))
            && dynamic_state.property_value(&binding.property).is_some()
    })
}

fn binary_scene_apply_dynamic_property_bindings(
    state: &mut BinarySceneNodeState,
    node_id: &str,
    dynamic_state: &BinarySceneDynamicState,
) {
    for binding in &dynamic_state.property_bindings {
        if binding
            .target_node
            .as_deref()
            .is_some_and(|target| target != node_id)
        {
            continue;
        }
        let Some(raw_value) = dynamic_state.property_value(&binding.property) else {
            continue;
        };
        if binding.target == BINARY_TRANSFORM_PROPERTY_OPACITY
            && let Some(visible) = raw_value.as_bool()
        {
            state.opacity *= if visible { 1.0 } else { 0.0 };
            continue;
        }
        let Some(raw_value) = binary_scene_property_number(raw_value) else {
            continue;
        };
        let value = raw_value * binding.scale + binding.offset;
        if value.is_finite() {
            binary_scene_apply_timeline_value(state, binding.target, value);
        }
    }
}

fn binary_scene_apply_puppet_attachment_delta(
    names: &BinarySceneNames,
    transform: &mut SceneTransform,
    puppet_attachment_name: u32,
    parent_puppet_attachment_deltas: Option<&BTreeMap<String, ScenePuppetAttachmentDelta>>,
) {
    let Some(attachment) = binary_name(names, puppet_attachment_name) else {
        return;
    };
    let Some(delta) = parent_puppet_attachment_deltas.and_then(|deltas| deltas.get(attachment))
    else {
        return;
    };
    transform.x += delta.x;
    transform.y += delta.y;
    transform.rotation_deg += delta.rotation_deg;
}

fn binary_scene_compose_transform(parent: SceneTransform, child: SceneTransform) -> SceneTransform {
    let rotation = parent.rotation_deg.to_radians();
    let child_x = child.x * parent.scale_x;
    let child_y = child.y * parent.scale_y;
    let rotated_child_x = child_x.mul_add(rotation.cos(), -child_y * rotation.sin());
    let rotated_child_y = child_x.mul_add(rotation.sin(), child_y * rotation.cos());
    SceneTransform {
        x: parent.x + rotated_child_x,
        y: parent.y + rotated_child_y,
        scale_x: parent.scale_x * child.scale_x,
        scale_y: parent.scale_y * child.scale_y,
        rotation_deg: parent.rotation_deg + child.rotation_deg,
        anchor_x: child.anchor_x,
        anchor_y: child.anchor_y,
    }
}

fn binary_scene_default_transform(
    record: crate::core::scene::binary::SceneBinaryTransformTimelineRecord,
) -> SceneTransform {
    SceneTransform {
        x: f64::from(record.value0),
        y: f64::from(record.value1),
        scale_x: f64::from(record.value2),
        scale_y: f64::from(record.value3),
        rotation_deg: f64::from(record.value4),
        anchor_x: f64::from(record.value5),
        anchor_y: f64::from(record.value6),
    }
}

fn binary_scene_transform_timeline_value(
    reader: &mut BinarySceneReader,
    record: crate::core::scene::binary::SceneBinaryTransformTimelineRecord,
    snapshot_time_ms: u64,
) -> Result<Option<f64>, RendererPlanError> {
    let keyframe_record_count = reader.chunk_count(SceneBinaryChunkKind::TransformKeyframes);
    let keyframe_records = reader.transform_keyframe_records_cached()?;
    let keyframes = binary_scene_cached_record_slice(
        &keyframe_records,
        SceneBinaryChunkKind::TransformKeyframes,
        record.first_keyframe,
        record.keyframe_count,
        keyframe_record_count,
    )?;
    let mut keyframes = keyframes.iter().copied();
    let Some(first) = keyframes.next() else {
        return Ok(None);
    };
    if record.keyframe_count == 1 {
        return Ok(Some(f64::from(first.value)));
    }
    let mut time_ms = snapshot_time_ms.saturating_add(record.time_offset_ms);
    if record.flags & BINARY_TRANSFORM_FLAG_LOOP != 0 && record.last_time_ms > 0 {
        time_ms %= record.last_time_ms;
    }
    if time_ms <= first.time_ms {
        return Ok(Some(f64::from(first.value)));
    }
    let mut previous = first;
    for next in keyframes {
        if time_ms <= next.time_ms {
            let span = next.time_ms.saturating_sub(previous.time_ms) as f64;
            let progress = if span > 0.0 {
                (time_ms.saturating_sub(previous.time_ms) as f64 / span).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let eased = binary_scene_curve_ease(next.curve, progress);
            return Ok(Some(
                f64::from(previous.value)
                    + (f64::from(next.value) - f64::from(previous.value)) * eased,
            ));
        }
        previous = next;
    }
    Ok(Some(f64::from(previous.value)))
}

fn binary_scene_apply_timeline_value(state: &mut BinarySceneNodeState, property: u16, value: f64) {
    match property {
        BINARY_TRANSFORM_PROPERTY_X => state.transform.x = value,
        BINARY_TRANSFORM_PROPERTY_Y => state.transform.y = value,
        BINARY_TRANSFORM_PROPERTY_SCALE_X if value > 0.0 => state.transform.scale_x = value,
        BINARY_TRANSFORM_PROPERTY_SCALE_Y if value > 0.0 => state.transform.scale_y = value,
        BINARY_TRANSFORM_PROPERTY_OPACITY => state.opacity = value.clamp(0.0, 1.0),
        BINARY_TRANSFORM_PROPERTY_ROTATION_DEG => state.transform.rotation_deg = value,
        BINARY_TRANSFORM_PROPERTY_WIDTH => state.width = Some(value.max(0.0)),
        BINARY_TRANSFORM_PROPERTY_HEIGHT => state.height = Some(value.max(0.0)),
        BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS => state.corner_radius = Some(value.max(0.0)),
        _ => {}
    }
}

fn binary_scene_curve_ease(code: u16, value: f64) -> f64 {
    match code {
        2 => {
            if value >= 1.0 {
                1.0
            } else {
                0.0
            }
        }
        3 => value * value,
        4 => 1.0 - (1.0 - value) * (1.0 - value),
        5 => {
            if value < 0.5 {
                2.0 * value * value
            } else {
                1.0 - (-2.0 * value + 2.0).powi(2) / 2.0
            }
        }
        _ => value,
    }
}

fn binary_scene_effect_uv_transform(
    record: SceneBinaryEffectUvTransformRecord,
) -> SceneEffectUvTransform {
    SceneEffectUvTransform {
        mapping: match record.mapping {
            SCENE_BINARY_EFFECT_UV_MAPPING_TEXTURE_RESOLUTION => {
                SceneEffectUvMapping::TextureResolution
            }
            _ => SceneEffectUvMapping::TextureResolution,
        },
        source_slot: record.source_slot,
        mask_slot: record.mask_slot,
        scale: [f64::from(record.scale_u), f64::from(record.scale_v)],
        offset: [f64::from(record.offset_u), f64::from(record.offset_v)],
        input_extent: (record.flags & BINARY_EFFECT_UV_HAS_INPUT_EXTENT != 0)
            .then(|| binary_scene_effect_uv_extent(record.input_width, record.input_height))
            .flatten(),
        mask_extent: (record.flags & BINARY_EFFECT_UV_HAS_MASK_EXTENT != 0)
            .then(|| binary_scene_effect_uv_extent(record.mask_width, record.mask_height))
            .flatten(),
        mask_backing_extent: (record.flags & BINARY_EFFECT_UV_HAS_MASK_BACKING_EXTENT != 0)
            .then(|| binary_scene_effect_uv_extent(record.backing_width, record.backing_height))
            .flatten(),
    }
}

fn binary_scene_effect_uv_extent(width: u32, height: u32) -> Option<SceneEffectUvExtent> {
    (width > 0 && height > 0).then_some(SceneEffectUvExtent { width, height })
}

fn binary_resource_by_name(
    resources: &[BinarySceneResource],
    id_name: u32,
) -> Option<&BinarySceneResource> {
    (id_name != SCENE_BINARY_NONE_ID)
        .then(|| {
            resources
                .iter()
                .find(|resource| resource.id_name == id_name)
        })
        .flatten()
}

fn binary_scene_alpha_texture_slot(material: SceneBinaryMaterialPassRecord) -> Option<u32> {
    (material.alpha_texture_slot != SCENE_BINARY_NONE_ID).then_some(material.alpha_texture_slot)
}

fn binary_scene_alpha_texture_mode(
    material: SceneBinaryMaterialPassRecord,
) -> SceneRenderAlphaTextureMode {
    match material.alpha_texture_mode {
        2 => SceneRenderAlphaTextureMode::Inverse,
        3 => SceneRenderAlphaTextureMode::Iris,
        4 => SceneRenderAlphaTextureMode::Coverage,
        _ => SceneRenderAlphaTextureMode::Multiply,
    }
}

fn binary_scene_blend_mode(code: u16) -> SceneBlendMode {
    match code {
        2 => SceneBlendMode::Additive,
        3 => SceneBlendMode::Multiply,
        4 => SceneBlendMode::Screen,
        5 => SceneBlendMode::Max,
        6 => SceneBlendMode::Normal,
        7 => SceneBlendMode::Modulate,
        8 => SceneBlendMode::HslColor,
        9 => SceneBlendMode::AlphaToCoverage,
        _ => SceneBlendMode::Alpha,
    }
}

fn binary_scene_fit(code: u16) -> FitMode {
    match code {
        2 => FitMode::Contain,
        3 => FitMode::Stretch,
        4 => FitMode::Tile,
        5 => FitMode::Center,
        _ => FitMode::Cover,
    }
}

fn binary_scene_text_align(code: u16) -> Option<SceneTextAlign> {
    match code {
        2 => Some(SceneTextAlign::Middle),
        3 => Some(SceneTextAlign::End),
        1 => Some(SceneTextAlign::Start),
        _ => None,
    }
}

fn binary_scene_effect_runtime(kind: u16, effect_file: &str) -> Option<String> {
    let normalized = effect_file.replace('\\', "/").to_ascii_lowercase();
    let runtime = match kind {
        1 => "native-opacity-mask",
        2 => "native-iris-mask",
        3..=5 => return None,
        7 if normalized.contains("foliagesway")
            || normalized.contains("foliage_sway")
            || normalized.contains("auto_sway")
            || normalized.contains("autosway") =>
        {
            return None;
        }
        7..=9 => "native-effect-motion",
        6 => "native-water-caustics",
        _ if normalized.ends_with("effects/opacity/effect.json") => "native-opacity-mask",
        _ if normalized.ends_with("effects/iris/effect.json") => "native-iris-mask",
        _ if normalized.contains("waterripple")
            || normalized.contains("waterwaves")
            || normalized.contains("waterflow") =>
        {
            return None;
        }
        _ if normalized.contains("foliagesway")
            || normalized.contains("foliage_sway")
            || normalized.contains("auto_sway")
            || normalized.contains("autosway") =>
        {
            return None;
        }
        _ if normalized.contains("sway")
            || normalized.contains("shake")
            || normalized.contains("flutter")
            || normalized.contains("drift") =>
        {
            "native-effect-motion"
        }
        _ => return None,
    };
    Some(runtime.to_owned())
}

fn binary_scene_material_flag(code: u16) -> Option<String> {
    match code {
        1 => Some("enabled".to_owned()),
        2 => Some("disabled".to_owned()),
        _ => None,
    }
}

fn binary_scene_cull_mode(code: u16) -> Option<String> {
    match code {
        1 => Some("disabled".to_owned()),
        2 => Some("back".to_owned()),
        3 => Some("front".to_owned()),
        4 => Some("frontandback".to_owned()),
        5 => Some("unknown".to_owned()),
        _ => None,
    }
}

fn binary_scene_flagged_color(flags: u16, flag: u16, rgba: u32) -> Option<String> {
    (flags & flag != 0).then(|| binary_scene_rgba_hex(rgba))
}

fn binary_scene_rgba_hex(rgba: u32) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        rgba >> 24,
        (rgba >> 16) & 0xff,
        (rgba >> 8) & 0xff
    )
}

fn binary_plan_error(err: SceneBinaryError) -> RendererPlanError {
    RendererPlanError::PackageLoad(format!("failed to read binary scene: {err}"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::*;
    use crate::core::SceneSize;
    use crate::core::scene::SceneDocument;
    use crate::core::scene::binary::encode_scene_binary_document;
    use crate::engine::scene_engine::{
        SceneEffectCommand, SceneEffectConstantValue, SceneEffectFboFormat, SceneEffectImageRef,
        SceneEffectPassBlend, SceneGraphTarget, SceneObjectId, SceneResource, SceneTextureFormat,
    };

    #[test]
    fn gscn_direct_ingest_defaults_to_we_projection_stretch_fit() {
        let document: SceneDocument = serde_json::from_value(json!({
            "size": { "width": 3840, "height": 2160 },
            "nodes": [
                {
                    "id": "background",
                    "type": "rectangle",
                    "width": 3840.0,
                    "height": 2160.0
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-scene-fit");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path.clone(),
            None,
            0,
            None,
        )
        .expect("binary scene plan");
        let cover_plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path,
            None,
            0,
            Some(FitMode::Cover),
        )
        .expect("binary scene plan with override");
        fs::remove_dir_all(root).expect("remove test dir");

        assert_eq!(
            plan.scene_size,
            Some(SceneSize {
                width: 3840,
                height: 2160
            })
        );
        assert_eq!(plan.scene_fit, FitMode::Stretch);
        assert_eq!(cover_plan.scene_fit, FitMode::Cover);
    }

    #[test]
    fn gscn_direct_ingest_preserves_hsl_color_blend_from_binary_payload() {
        let document: SceneDocument = serde_json::from_value(json!({
            "size": { "width": 3840, "height": 2160 },
            "nodes": [
                {
                    "id": "hsl-color-bar",
                    "type": "rectangle",
                    "width": 550.0,
                    "height": 3300.0,
                    "color": "#003ca4",
                    "properties": {
                        "wallpaper_engine_blend": { "colorBlendMode": 28 }
                    }
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-hsl-color-blend");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path.clone(),
            None,
            0,
            None,
        )
        .expect("binary scene plan");
        fs::remove_dir_all(root).expect("remove test dir");

        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0].blend_mode, SceneBlendMode::HslColor);
    }

    #[test]
    fn gscn_direct_ingest_emits_meshless_retained_particle_marker_from_binary_payload() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "spark", "type": "image", "source": "assets/spark.gtex", "width": 16, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "parent",
                    "type": "group",
                    "opacity": 0.5,
                    "transform": { "x": 100.0, "y": 50.0 },
                    "children": [
                        {
                            "id": "spark-emitter",
                            "type": "particle-emitter",
                            "resource": "spark",
                            "opacity": 0.8,
                            "transform": { "x": 10.0, "y": 20.0 },
                            "properties": {
                                "particle": {
                                    "count": 3,
                                    "seed": 1,
                                    "lifetime_ms": 1000,
                                    "loop": true,
                                    "spawn_width": 0.0,
                                    "spawn_height": 0.0,
                                    "width": 6.0,
                                    "height": 8.0,
                                    "speed": 0.0,
                                    "spread_deg": 0.0,
                                    "gravity_x": 0.0,
                                    "gravity_y": 0.0,
                                    "fade": false,
                                    "color": "#aabbcc"
                                }
                            }
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-particle-plan");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan =
            scene_wallpaper_plan_from_gscn_path("HDMI-A-1".to_owned(), scene_path, None, 250, None)
                .expect("binary scene plan");
        fs::remove_dir_all(root).expect("remove test dir");

        assert_eq!(plan.layers.len(), 1);
        let layer = &plan.layers[0];
        assert_eq!(layer.id, "spark-emitter");
        assert_eq!(layer.kind, SceneNodeKind::Image);
        assert_eq!(layer.texture_slots.len(), 0);
        assert_eq!(layer.color.as_deref(), Some("#aabbcc"));
        assert_eq!(layer.width, Some(6.0));
        assert_eq!(layer.height, Some(8.0));
        assert!((layer.opacity - 0.4).abs() < 1e-6);
        assert!((layer.transform.x - 110.0).abs() < f64::EPSILON);
        assert!((layer.transform.y - 70.0).abs() < f64::EPSILON);
        assert!(
            layer.mesh.is_none(),
            "textured retained particle marker should not carry CPU mesh vertices"
        );
    }

    #[test]
    fn gscn_direct_ingest_preserves_effect_graph_pass_fields_from_binary_payload() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "base", "type": "image", "source": "assets/base.gtex", "width": 320, "height": 180 },
                { "id": "normal", "type": "image", "source": "assets/normal.gtex", "width": 64, "height": 64 }
            ],
            "nodes": [
                {
                    "id": "water-carrier",
                    "type": "image",
                    "resource": "base",
                    "width": 320.0,
                    "height": 180.0,
                    "effects": [
                        {
                            "file": "effects/custom/effect.json",
                            "fbos": [
                                { "name": "_rt_Custom", "format": "rgba8888", "scale": 0.5, "unique": true }
                            ],
                            "passes": [
                                {
                                    "command": "draw",
                                    "source": "previous",
                                    "target": "_rt_Custom",
                                    "binds": { "0": "previous", "2": "_rt_CustomNormal" },
                                    "shader": "effects/custom",
                                    "blending": "normal",
                                    "texture_resources": ["base", null, "normal"],
                                    "combos": { "MASK": 0 },
                                    "constant_shader_values": { "strength": 0.5 }
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-effect-graph-plan");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path.clone(),
            None,
            0,
            None,
        )
        .expect("binary scene plan");
        fs::remove_dir_all(root).expect("remove test dir");

        assert_eq!(plan.layers.len(), 1);
        let pass = &plan.layers[0].image_effect_passes[0];
        assert_eq!(pass.command.as_deref(), Some("draw"));
        assert_eq!(pass.source.as_deref(), Some("previous"));
        assert_eq!(pass.target.as_deref(), Some("_rt_Custom"));
        assert_eq!(pass.binds.get(&0).map(String::as_str), Some("previous"));
        assert_eq!(
            pass.binds.get(&2).map(String::as_str),
            Some("_rt_CustomNormal")
        );
        assert_eq!(pass.fbos.len(), 1);
        assert_eq!(pass.fbos[0].name, "_rt_Custom");
        assert_eq!(pass.fbos[0].format.as_deref(), Some("rgba8888"));
        assert!((pass.fbos[0].scale - 0.5).abs() < f64::EPSILON);
        assert!(pass.fbos[0].unique);
        assert_eq!(pass.combos.get("MASK"), Some(&0));
        assert_eq!(
            pass.constant_shader_values
                .get("strength")
                .and_then(|value| value.as_f64()),
            Some(0.5)
        );
    }

    #[test]
    fn gscn_engine_plan_preserves_typed_effect_program_fbos_copy_and_swaps() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "base", "type": "image", "source": "assets/base.gtex", "width": 320, "height": 180 },
                { "id": "normal", "type": "image", "source": "assets/normal.gtex", "width": 64, "height": 64 }
            ],
            "nodes": [
                {
                    "id": "water-carrier",
                    "type": "image",
                    "resource": "base",
                    "width": 320.0,
                    "height": 180.0,
                    "effects": [
                        {
                            "file": "effects/custom/effect.json",
                            "fbos": [
                                { "name": "_rt_Custom", "format": "rgba8888", "scale": 0.5, "unique": true },
                                { "name": "_rt_CustomPrev", "format": "rgba8888", "scale": 0.5, "unique": true }
                            ],
                            "passes": [
                                {
                                    "command": "draw",
                                    "source": "previous",
                                    "target": "_rt_Custom",
                                    "binds": { "0": "previous", "2": "_rt_CustomNormal" },
                                    "shader": "effects/custom",
                                    "blending": "normal",
                                    "texture_resources": ["base", null, "normal"],
                                    "combos": { "MASK": 0 },
                                    "constant_shader_values": { "strength": 0.5 }
                                },
                                {
                                    "command": "copy",
                                    "source": "_rt_Custom",
                                    "target": "_rt_CustomPrev"
                                },
                                {
                                    "command": "swap",
                                    "source": "_rt_Custom",
                                    "target": "_rt_CustomPrev"
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-engine-effect-program");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_engine_plan_from_gscn_path_with_properties(scene_path, 0, None)
            .expect("scene engine plan");
        fs::remove_dir_all(root).expect("remove test dir");

        assert_eq!(plan.effects.len(), 1);
        assert_eq!(plan.effects[0].object, SceneObjectId(0));
        let program = &plan.effects[0].program;
        assert_eq!(program.effect_file, "effects/custom/effect.json");
        assert_eq!(program.fbos.len(), 2);
        assert_eq!(program.fbos[0].name, "_rt_Custom");
        assert_eq!(program.fbos[0].target, SceneGraphTarget::NamedFbo(0));
        assert_eq!(
            program.fbos[0].format,
            Some(SceneEffectFboFormat::Rgba8Unorm)
        );
        assert!((program.fbos[0].scale - 0.5).abs() < f32::EPSILON);
        assert!(program.fbos[0].unique);
        assert_eq!(program.fbos[1].name, "_rt_CustomPrev");
        assert_eq!(program.fbos[1].target, SceneGraphTarget::NamedFbo(1));
        assert_eq!(program.material_pass_count(), 1);
        assert_eq!(program.copy_command_count(), 1);
        assert_eq!(program.swap_command_count(), 1);
        let SceneEffectCommand::MaterialPass(pass) = &program.commands[0] else {
            panic!("expected material pass");
        };
        assert_eq!(pass.pass_index, 0);
        assert_eq!(pass.shader.as_deref(), Some("effects/custom"));
        assert_eq!(pass.source, Some(SceneEffectImageRef::PreviousFramebuffer));
        assert_eq!(
            pass.target,
            Some(SceneEffectImageRef::NamedFbo("_rt_Custom".to_owned()))
        );
        assert_eq!(pass.blend, SceneEffectPassBlend::NormalReplace);
        assert_eq!(
            pass.binds.get(&0),
            Some(&SceneEffectImageRef::PreviousFramebuffer)
        );
        assert_eq!(
            pass.binds.get(&2),
            Some(&SceneEffectImageRef::NamedFbo(
                "_rt_CustomNormal".to_owned()
            ))
        );
        assert_eq!(pass.texture_resources.len(), 2);
        assert_eq!(pass.texture_resources[0].slot, 0);
        assert_eq!(pass.texture_resources[1].slot, 2);
        assert_eq!(pass.combos.get("MASK"), Some(&0));
        assert_eq!(
            pass.constants.get("strength"),
            Some(&SceneEffectConstantValue::Float(0.5))
        );
        let SceneEffectCommand::Copy(copy) = &program.commands[1] else {
            panic!("expected copy command");
        };
        assert_eq!(copy.pass_index, 1);
        assert_eq!(
            copy.source,
            SceneEffectImageRef::NamedFbo("_rt_Custom".to_owned())
        );
        assert_eq!(
            copy.target,
            SceneEffectImageRef::NamedFbo("_rt_CustomPrev".to_owned())
        );
        let SceneEffectCommand::Swap(swap) = &program.commands[2] else {
            panic!("expected swap command");
        };
        assert_eq!(swap.pass_index, 2);
        assert_eq!(
            swap.a,
            SceneEffectImageRef::NamedFbo("_rt_Custom".to_owned())
        );
        assert_eq!(
            swap.b,
            SceneEffectImageRef::NamedFbo("_rt_CustomPrev".to_owned())
        );
    }

    #[test]
    fn gscn_binary_runtime_topology_keeps_initially_hidden_layers() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "hero", "type": "image", "source": "assets/hero.gtex", "width": 16, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "hidden-hero",
                    "type": "image",
                    "resource": "hero",
                    "visible": false,
                    "width": 16.0,
                    "height": 16.0,
                    "transform": { "x": 10.0, "y": 20.0 }
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-retained-hidden");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path.clone(),
            None,
            0,
            None,
        )
        .expect("binary scene plan");

        assert_eq!(plan.layers.len(), 1);
        let layer = &plan.layers[0];
        assert_eq!(layer.id, "hidden-hero");
        assert_eq!(layer.kind, SceneNodeKind::Image);
        assert_eq!(layer.opacity, 0.0);
        assert_eq!(layer.width, Some(16.0));
        assert_eq!(layer.height, Some(16.0));

        fs::remove_dir_all(root).expect("remove test dir");
    }

    #[test]
    fn gscn_engine_plan_preserves_native_gtex_metadata() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "eye", "type": "image", "source": "assets/eye.gtex", "width": 32, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "eye-node",
                    "type": "image",
                    "resource": "eye",
                    "width": 32,
                    "height": 16
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-gtex-metadata");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        write_test_gtex_header(&assets.join("eye.gtex"), 663, 230, 7, 1, 155_520);
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_engine_plan_from_gscn_path_with_properties(scene_path, 0, None)
            .expect("scene engine plan");

        fs::remove_dir_all(root).expect("remove test dir");

        let SceneResource::Texture {
            width,
            height,
            format,
            mip_count,
            payload_bytes,
            ..
        } = &plan.resources[0]
        else {
            panic!("expected texture resource");
        };
        assert_eq!(*width, Some(663));
        assert_eq!(*height, Some(230));
        assert_eq!(*format, Some(SceneTextureFormat::Bc7UnormBlock));
        assert_eq!(*mip_count, Some(1));
        assert_eq!(*payload_bytes, Some(155_520));
    }

    #[test]
    fn gscn_direct_ingest_preserves_puppet_clipping_records() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "eye", "type": "image", "source": "assets/eye.gtex", "width": 32, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "eye-node",
                    "type": "image",
                    "resource": "eye",
                    "width": 32,
                    "height": 16,
                    "mesh": {
                        "vertices": [
                            { "x": 0.0, "y": 0.0, "u": 0.0, "v": 0.0 },
                            { "x": 1.0, "y": 0.0, "u": 1.0, "v": 0.0 },
                            { "x": 0.0, "y": 1.0, "u": 0.0, "v": 1.0 }
                        ],
                        "indices": [0, 1, 2],
                        "skin": {
                            "bones": [
                                { "bind": { "translation": [0.0, 0.0, 0.0] } },
                                { "parent": 0, "bind": { "translation": [1.0, 0.0, 0.0] } }
                            ],
                            "vertices": [
                                { "bone_indices": [1, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [1, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [0, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] }
                            ]
                        },
                        "puppet_clipping_records": [
                            {
                                "source_name": "eye-right",
                                "mask": "masks/clipping_mask_eye",
                                "duration_frames": 1680,
                                "flags": 1,
                                "bones": [1],
                                "frame_keys": [0, 1, 2]
                            }
                        ],
                        "puppet_clipping_active_sources": [
                            {
                                "source_name": "eye-right",
                                "scalar_bits": 1065353216,
                                "source_scale": 6,
                                "flags": 2,
                                "transform_index": 4,
                                "parameter0": -1.0,
                                "parameter1": 0.5
                            }
                        ]
                    }
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-puppet-clipping");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan = scene_wallpaper_plan_from_gscn_path(
            "HDMI-A-1".to_owned(),
            scene_path.clone(),
            None,
            0,
            None,
        )
        .expect("binary scene plan");

        let mesh = plan.layers[0].mesh.as_ref().expect("mesh");
        assert_eq!(mesh.puppet_clipping_records.len(), 1);
        assert_eq!(
            mesh.puppet_clipping_records[0].source_name.as_deref(),
            Some("eye-right")
        );
        assert_eq!(
            mesh.puppet_clipping_records[0].mask,
            "masks/clipping_mask_eye"
        );
        assert_eq!(mesh.puppet_clipping_records[0].duration_frames, 1680);
        assert_eq!(mesh.puppet_clipping_records[0].flags, 1);
        assert_eq!(mesh.puppet_clipping_records[0].bones, vec![1]);
        assert_eq!(mesh.puppet_clipping_records[0].frame_keys, vec![0, 1, 2]);
        assert_eq!(mesh.puppet_clipping_active_sources.len(), 1);
        assert_eq!(
            mesh.puppet_clipping_active_sources[0].source_name,
            "eye-right"
        );

        let engine_plan = scene_engine_plan_from_gscn_path_with_properties(scene_path, 0, None)
            .expect("scene engine plan");
        fs::remove_dir_all(root).expect("remove test dir");
        let puppet = engine_plan
            .resources
            .iter()
            .find_map(|resource| match resource {
                SceneResource::PuppetRig { clipping, .. } => Some(clipping),
                _ => None,
            })
            .expect("puppet rig");
        assert_eq!(puppet.active_sources.len(), 1);
        assert_eq!(puppet.records[0].active_source_index, Some(0));
    }

    #[test]
    fn gscn_direct_ingest_resolves_puppet_clipping_mask_resource_paths() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "eye", "type": "image", "source": "assets/eye.gtex", "width": 32, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "eye-node",
                    "type": "image",
                    "resource": "eye",
                    "width": 32,
                    "height": 16,
                    "mesh": {
                        "vertices": [
                            { "x": 0.0, "y": 0.0, "u": 0.0, "v": 0.0 },
                            { "x": 1.0, "y": 0.0, "u": 1.0, "v": 0.0 },
                            { "x": 0.0, "y": 1.0, "u": 0.0, "v": 1.0 }
                        ],
                        "indices": [0, 1, 2],
                        "skin": {
                            "bones": [
                                { "bind": { "translation": [0.0, 0.0, 0.0] } },
                                { "parent": 0, "bind": { "translation": [1.0, 0.0, 0.0] } }
                            ],
                            "vertices": [
                                { "bone_indices": [1, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [1, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [0, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] }
                            ]
                        },
                        "puppet_clipping_records": [
                            {
                                "mask": "masks/clipping_mask_eye",
                                "mask_resource": "assets/clipping-mask.gtex",
                                "duration_frames": 1680,
                                "bones": [1],
                                "frame_keys": [0, 1, 2]
                            }
                        ]
                    }
                }
            ]
        }))
        .expect("scene document");
        let bytes = encode_scene_binary_document(0, &document).expect("binary scene");
        let root = unique_test_dir("gilder-binary-puppet-clipping-resource");
        let assets = root.join("assets");
        fs::create_dir_all(&assets).expect("assets dir");
        let scene_path = assets.join("scene.gscn");
        fs::write(&scene_path, bytes).expect("write gscn");

        let plan =
            scene_wallpaper_plan_from_gscn_path("HDMI-A-1".to_owned(), scene_path, None, 0, None)
                .expect("binary scene plan");
        let expected_mask_resource = root
            .join("assets/clipping-mask.gtex")
            .to_string_lossy()
            .into_owned();
        fs::remove_dir_all(root).expect("remove test dir");

        let mesh = plan.layers[0].mesh.as_ref().expect("mesh");
        assert_eq!(mesh.puppet_clipping_records.len(), 1);
        assert_eq!(
            mesh.puppet_clipping_records[0].mask,
            "assets/clipping-mask.gtex"
        );
        assert_eq!(
            mesh.puppet_clipping_records[0].mask_resource.as_deref(),
            Some(expected_mask_resource.as_str())
        );
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }

    fn write_test_gtex_header(
        path: &Path,
        width: u32,
        height: u32,
        format: u32,
        mip_count: u32,
        payload_bytes: u64,
    ) {
        let mut bytes = [0u8; 32];
        bytes[0..8].copy_from_slice(b"GDTEX002");
        bytes[8..12].copy_from_slice(&width.to_le_bytes());
        bytes[12..16].copy_from_slice(&height.to_le_bytes());
        bytes[16..20].copy_from_slice(&format.to_le_bytes());
        bytes[20..24].copy_from_slice(&mip_count.to_le_bytes());
        bytes[24..32].copy_from_slice(&payload_bytes.to_le_bytes());
        fs::write(path, bytes).expect("write test gtex");
    }
}
