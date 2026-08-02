//! Explicit cold diagnostic projection of the retained semantic and draw state.

use serde::Serialize;

use crate::engine::scene::{
    INVALID_OBJECT_ID, SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan,
    SceneScriptTarget, SceneStorage,
};
use super::SemanticFrameResolver;
use super::draw_recording::{SceneGpuDrawCommand, SceneGpuScissor};
use super::material_uniform::resolved_standard_material_color;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneSemanticDiagnosticsSnapshot {
    pub descriptor_heap: NativeVulkanSceneDescriptorHeapSnapshot,
    pub retained_script_deltas: Vec<NativeVulkanSceneScriptDeltaSnapshot>,
    pub resolved_objects: Vec<NativeVulkanSceneResolvedObjectSnapshot>,
    pub draws: Vec<NativeVulkanSceneDrawActivationSnapshot>,
    pub enabled_draw_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneDescriptorHeapSnapshot {
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub reference_phase_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneScriptDeltaSnapshot {
    pub object_handle: u32,
    pub object_id: u32,
    pub object_name: String,
    pub target: SceneScriptTarget,
    pub selector: u32,
    pub numeric: [f32; 4],
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneResolvedObjectSnapshot {
    pub object_handle: u32,
    pub object_id: u32,
    pub object_name: String,
    pub parent_handle: Option<u32>,
    pub parent_id: Option<u32>,
    pub authored_visible: bool,
    pub self_visible: bool,
    pub resolved_visible: bool,
    pub authored_origin: [f32; 3],
    pub authored_angles: [f32; 3],
    pub authored_scale: [f32; 3],
    pub authored_alpha: f32,
    pub self_alpha: f32,
    pub resolved_alpha: f32,
    pub local_matrix: [f32; 16],
    pub world_matrix: [f32; 16],
    pub render_world_matrix: [f32; 16],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneDrawActivationSnapshot {
    pub draw_index: u32,
    pub object_handle: Option<u32>,
    pub shader_key: String,
    pub primitive: SceneRenderingDeviceDrawPrimitive,
    pub enabled: bool,
    pub pipeline_index: u32,
    pub resource_descriptor_base: usize,
    pub material_resource_descriptor: Option<usize>,
    pub skinning_resource_descriptor: Option<usize>,
    pub scene_owned_uniform_descriptor_base: usize,
    pub sampled_resource_descriptor_base: usize,
    pub input_attachment_resource_descriptor_base: usize,
    pub sampler_descriptor_base: usize,
    pub vertex_buffer_byte_offset: Option<u64>,
    pub native_descriptor_push_words: Vec<u32>,
    pub standard_material_color: Option<[f32; 4]>,
    pub scissor: Option<NativeVulkanSceneScissorSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneScissorSnapshot {
    pub offset: [i32; 2],
    pub extent: [u32; 2],
}

pub(super) fn scene_semantic_diagnostics_snapshot(
    storage: &SceneStorage,
    resolver: &SemanticFrameResolver,
    graph: &SceneRenderingDeviceGraphPlan,
    draws: &[SceneGpuDrawCommand],
    descriptor_heap: NativeVulkanSceneDescriptorHeapSnapshot,
) -> Result<NativeVulkanSceneSemanticDiagnosticsSnapshot, String> {
    if graph.mesh_draws.len() != draws.len() {
        return Err(format!(
            "scene diagnostic graph draw count {} does not match command count {}",
            graph.mesh_draws.len(),
            draws.len()
        ));
    }
    let retained_script_deltas = resolver
        .retained_script_deltas()
        .iter()
        .map(|delta| {
            let (object_id, object_name) = object_identity(storage, delta.object.0)?;
            Ok(NativeVulkanSceneScriptDeltaSnapshot {
                object_handle: delta.object.0,
                object_id,
                object_name,
                target: delta.target,
                selector: delta.selector,
                numeric: delta.numeric,
                text: delta.text.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let resolved_objects = resolver
        .resolved_frame()
        .objects
        .iter()
        .map(|resolved| {
            let object = storage
                .objects()
                .get(resolved.object.0 as usize)
                .ok_or_else(|| {
                    format!(
                        "resolved object {} is outside scene storage",
                        resolved.object.0
                    )
                })?;
            if object.id != resolved.object {
                return Err(format!(
                    "resolved object handle {} does not match storage handle {}",
                    resolved.object.0, object.id.0
                ));
            }
            let (_, object_name) = object_identity(storage, resolved.object.0)?;
            let parent = (resolved.parent.0 != INVALID_OBJECT_ID).then_some(resolved.parent.0);
            let parent_id = parent
                .map(|handle| object_identity(storage, handle).map(|identity| identity.0))
                .transpose()?;
            Ok(NativeVulkanSceneResolvedObjectSnapshot {
                object_handle: resolved.object.0,
                object_id: object.we_id,
                object_name,
                parent_handle: parent,
                parent_id,
                authored_visible: object.visible,
                self_visible: resolved.self_visible,
                resolved_visible: resolved.resolved_visible,
                authored_origin: [object.origin.x, object.origin.y, object.origin.z],
                authored_angles: [object.angles.x, object.angles.y, object.angles.z],
                authored_scale: [object.scale.x, object.scale.y, object.scale.z],
                authored_alpha: object.alpha,
                self_alpha: resolved.self_alpha,
                resolved_alpha: resolved.resolved_alpha,
                local_matrix: resolved.local_matrix,
                world_matrix: resolved.world_matrix,
                render_world_matrix: resolved.render_world_matrix,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let draws = graph
        .mesh_draws
        .iter()
        .zip(draws)
        .enumerate()
        .map(|(draw_index, (planned, command))| {
            let shader_key = storage
                .string(planned.shader_key)
                .ok_or_else(|| format!("scene draw {draw_index} has no shader key"))?;
            Ok(NativeVulkanSceneDrawActivationSnapshot {
                draw_index: u32::try_from(draw_index)
                    .map_err(|_| "scene draw diagnostic index exceeds u32".to_owned())?,
                object_handle: (planned.object.0 != INVALID_OBJECT_ID).then_some(planned.object.0),
                shader_key: shader_key.to_owned(),
                primitive: command.primitive,
                enabled: command.enabled,
                pipeline_index: command.pipeline_index,
                resource_descriptor_base: command.resource_descriptor_base,
                material_resource_descriptor: command.material_resource_descriptor,
                skinning_resource_descriptor: command.skinning_resource_descriptor,
                scene_owned_uniform_descriptor_base: command.scene_owned_uniform_descriptor_base,
                sampled_resource_descriptor_base: command.sampled_resource_descriptor_base,
                input_attachment_resource_descriptor_base: command
                    .input_attachment_resource_descriptor_base,
                sampler_descriptor_base: command.sampler_descriptor_base,
                vertex_buffer_byte_offset: command.vertex_buffer_byte_offset,
                native_descriptor_push_words: command
                    .active_native_descriptor_push()
                    .map(|push| {
                        push.bytes()
                            .chunks_exact(4)
                            .map(|word| {
                                u32::from_le_bytes(word.try_into().expect("four-byte push word"))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                standard_material_color: resolved_standard_material_color(storage, planned),
                scissor: command.scissor.map(scissor_snapshot),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let enabled_draw_count = draws.iter().filter(|draw| draw.enabled).count();
    Ok(NativeVulkanSceneSemanticDiagnosticsSnapshot {
        descriptor_heap,
        retained_script_deltas,
        resolved_objects,
        draws,
        enabled_draw_count,
    })
}

fn object_identity(storage: &SceneStorage, handle: u32) -> Result<(u32, String), String> {
    let object = storage
        .objects()
        .get(handle as usize)
        .filter(|object| object.id.0 == handle)
        .ok_or_else(|| format!("scene diagnostic references missing object {handle}"))?;
    let name = if object.name.is_some() {
        storage
            .string(object.name)
            .ok_or_else(|| format!("scene object {handle} has an invalid name string"))?
    } else {
        ""
    };
    Ok((object.we_id, name.to_owned()))
}

fn scissor_snapshot(scissor: SceneGpuScissor) -> NativeVulkanSceneScissorSnapshot {
    NativeVulkanSceneScissorSnapshot {
        offset: scissor.offset,
        extent: scissor.extent,
    }
}
