//! Effective node-state sampling for legacy `.gscn` layer lowering.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use std::collections::BTreeMap;

use crate::core::SceneTransform;
use crate::core::scene::ScenePuppetAttachmentDelta;
use crate::core::scene::binary::{SCENE_BINARY_NONE_ID, SceneBinaryNodeRecord};
use crate::renderer::RendererPlanError;

use super::super::dynamic_state::{BinarySceneDynamicState, binary_scene_property_number};
use super::super::facts::{BinarySceneNames, binary_name};
use super::super::mesh::binary_scene_puppet_attachment_deltas;
use super::super::reader::BinarySceneReader;
use super::super::schema::{BINARY_NODE_FLAG_VISIBLE, BINARY_TRANSFORM_PROPERTY_OPACITY};

mod timeline;

use timeline::{
    binary_scene_apply_timeline_value, binary_scene_node_has_timed_transform,
    binary_scene_node_state,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct BinarySceneNodeState {
    pub(super) transform: SceneTransform,
    pub(super) opacity: f64,
    pub(super) width: Option<f64>,
    pub(super) height: Option<f64>,
    pub(super) corner_radius: Option<f64>,
}

#[derive(Debug, Clone)]
pub(super) struct BinarySceneEffectiveNodeState {
    pub(super) visible: bool,
    pub(super) state: BinarySceneNodeState,
    puppet_attachment_deltas: Option<BTreeMap<String, ScenePuppetAttachmentDelta>>,
    sampled_pose_dynamic: bool,
}

pub(super) fn binary_scene_effective_node_states(
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
