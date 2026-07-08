//! Legacy wallpaper layer lowering from binary `.gscn` records.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use crate::core::scene::binary::{
    SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE, SCENE_BINARY_EFFECT_UV_MAPPING_TEXTURE_RESOLUTION,
    SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE, SCENE_BINARY_NONE_ID,
    SCENE_BINARY_PARAMETER_ROLE_EFFECT_FBO, SCENE_BINARY_PARAMETER_ROLE_PASS_BIND,
    SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO, SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT,
    SCENE_BINARY_PARAMETER_VALUE_BOOL, SCENE_BINARY_PARAMETER_VALUE_FLOAT,
    SCENE_BINARY_PARAMETER_VALUE_INTEGER, SCENE_BINARY_PARAMETER_VALUE_STRING,
    SCENE_BINARY_PARAMETER_VALUE_VEC2, SCENE_BINARY_PARAMETER_VALUE_VEC3,
    SCENE_BINARY_PARAMETER_VALUE_VEC4, SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE, SceneBinaryChunkKind,
    SceneBinaryEffectParameterRecord, SceneBinaryEffectPassRecord,
    SceneBinaryEffectUvTransformRecord, SceneBinaryGeometryRecord, SceneBinaryMaterialPassRecord,
    SceneBinaryNodeRecord, SceneBinaryParticleEmitterRecord, SceneBinaryTextureSlotRecord,
    SceneBinaryTransformTimelineRecord, decode_effect_parameter_record, decode_effect_pass_record,
    decode_effect_uv_transform_record, decode_texture_slot_record,
};
use crate::core::scene::{
    SceneEffectFbo, SceneEffectUvExtent, SceneEffectUvMapping, SceneEffectUvTransform,
    ScenePuppetAttachmentDelta,
};
use crate::core::{
    FitMode, SceneBlendMode, SceneNodeKind, ScenePathFillRule, SceneTextAlign, SceneTextureRegion,
    SceneTransform,
};
use crate::renderer::{
    RendererPlanError, SceneRenderAlphaTextureMode, SceneRenderImageEffectPass, SceneRenderLayer,
    SceneRenderTextureSlot,
};

use super::dynamic_state::{BinarySceneDynamicState, binary_scene_property_number};
use super::facts::{BinarySceneNames, BinarySceneResource, binary_name};
use super::mesh::{binary_scene_mesh, binary_scene_puppet_attachment_deltas};
use super::reader::{BinarySceneReader, binary_scene_cached_record_slice};
use super::schema::{
    BINARY_EFFECT_UV_HAS_INPUT_EXTENT, BINARY_EFFECT_UV_HAS_MASK_BACKING_EXTENT,
    BINARY_EFFECT_UV_HAS_MASK_EXTENT, BINARY_NODE_FLAG_COLOR, BINARY_NODE_FLAG_CORNER_RADIUS,
    BINARY_NODE_FLAG_STROKE_COLOR, BINARY_NODE_FLAG_STROKE_WIDTH, BINARY_NODE_FLAG_VISIBLE,
    BINARY_TEXTURE_ROLE_BASE_COLOR, BINARY_TRANSFORM_FLAG_LOOP,
    BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS, BINARY_TRANSFORM_PROPERTY_DEFAULT,
    BINARY_TRANSFORM_PROPERTY_HEIGHT, BINARY_TRANSFORM_PROPERTY_OPACITY,
    BINARY_TRANSFORM_PROPERTY_ROTATION_DEG, BINARY_TRANSFORM_PROPERTY_SCALE_X,
    BINARY_TRANSFORM_PROPERTY_SCALE_Y, BINARY_TRANSFORM_PROPERTY_WIDTH,
    BINARY_TRANSFORM_PROPERTY_X, BINARY_TRANSFORM_PROPERTY_Y,
};
use super::topology::BinarySceneRetainedTopology;

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

pub(super) fn binary_scene_render_layers(
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
    node: SceneBinaryNodeRecord,
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

#[allow(clippy::too_many_arguments)]
fn binary_scene_render_layer(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    node: SceneBinaryNodeRecord,
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

pub(super) fn binary_scene_material_texture_slots_cached(
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
        SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
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
        alphawriting: binary_scene_material_flag(pass.alpha_write),
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
    node: SceneBinaryNodeRecord,
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
    node: SceneBinaryNodeRecord,
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
    node: SceneBinaryNodeRecord,
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

fn binary_scene_default_transform(record: SceneBinaryTransformTimelineRecord) -> SceneTransform {
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
    record: SceneBinaryTransformTimelineRecord,
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

pub(super) fn binary_resource_by_name(
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
