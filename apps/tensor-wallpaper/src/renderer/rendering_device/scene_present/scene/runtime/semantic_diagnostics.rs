//! Explicit cold diagnostic projection of the retained semantic and draw state.

use serde::Serialize;

use super::SemanticFrameResolver;
use super::draw_recording::{SceneGpuDrawCommand, SceneGpuScissor};
use super::material_uniform::{SCENE_MATERIAL_UNIFORM_BYTES, resolved_standard_material_color};
use super::scene_owned_uniform::SceneOwnedUniformArenaPlan;
use crate::engine::scene::{
    INVALID_OBJECT_ID, SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan,
    SceneScriptTarget, SceneStorage,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderingDeviceSceneSemanticDiagnosticsSnapshot {
    pub descriptor_heap: RenderingDeviceSceneDescriptorHeapSnapshot,
    pub retained_script_deltas: Vec<RenderingDeviceSceneScriptDeltaSnapshot>,
    pub resolved_objects: Vec<RenderingDeviceSceneResolvedObjectSnapshot>,
    pub puppet_bone_palettes: Vec<RenderingDeviceScenePuppetBonePaletteSnapshot>,
    pub draws: Vec<RenderingDeviceSceneDrawActivationSnapshot>,
    pub enabled_draw_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceSceneDescriptorHeapSnapshot {
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub reference_phase_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderingDeviceSceneScriptDeltaSnapshot {
    pub object_handle: u32,
    pub object_id: u32,
    pub object_name: String,
    pub target: SceneScriptTarget,
    pub selector: u32,
    pub numeric: [f32; 4],
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderingDeviceSceneResolvedObjectSnapshot {
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
pub struct RenderingDeviceScenePuppetBonePaletteSnapshot {
    pub object_handle: u32,
    pub object_id: u32,
    pub object_name: String,
    pub puppet_index: u32,
    pub resolved_visible: bool,
    pub bones: Vec<RenderingDeviceScenePuppetBoneMatrixSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RenderingDeviceScenePuppetBoneMatrixSnapshot {
    pub bone_index: u32,
    pub parent_index: i32,
    pub matrix: [[f32; 4]; 4],
    pub alpha: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderingDeviceSceneDrawActivationSnapshot {
    pub draw_index: u32,
    pub object_handle: Option<u32>,
    pub shader_key: String,
    pub primitive: SceneRenderingDeviceDrawPrimitive,
    pub enabled: bool,
    pub pipeline_index: u32,
    pub resource_descriptor_base: usize,
    pub material_resource_descriptor: Option<usize>,
    pub skinning_resource_descriptor: Option<usize>,
    pub particle_resource_descriptor: Option<usize>,
    pub scene_owned_uniform_descriptor_base: usize,
    pub sampled_resource_descriptor_base: usize,
    pub input_attachment_resource_descriptor_base: usize,
    pub sampler_descriptor_base: usize,
    pub vertex_buffer_byte_offset: Option<u64>,
    pub descriptor_push_words: Vec<u32>,
    pub material_uniform_words: Vec<u32>,
    pub scene_owned_uniform_words: Vec<Vec<u32>>,
    pub standard_material_color: Option<[f32; 4]>,
    pub scissor: Option<RenderingDeviceSceneScissorSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceSceneScissorSnapshot {
    pub offset: [i32; 2],
    pub extent: [u32; 2],
}

pub(super) fn scene_semantic_diagnostics_snapshot(
    storage: &SceneStorage,
    resolver: &SemanticFrameResolver,
    graph: &SceneRenderingDeviceGraphPlan,
    draws: &[SceneGpuDrawCommand],
    material_uniform_payload: Option<&[u8]>,
    scene_owned_uniform_plan: &SceneOwnedUniformArenaPlan,
    scene_owned_uniform_payload: &[u8],
    descriptor_heap: RenderingDeviceSceneDescriptorHeapSnapshot,
) -> Result<RenderingDeviceSceneSemanticDiagnosticsSnapshot, String> {
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
            Ok(RenderingDeviceSceneScriptDeltaSnapshot {
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
            Ok(RenderingDeviceSceneResolvedObjectSnapshot {
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
    let puppet_bone_palettes = graph
        .puppet_bone_palettes
        .iter()
        .map(|palette| {
            let (object_id, object_name) = object_identity(storage, palette.object.0)?;
            let start = usize::try_from(palette.bone_matrix_start)
                .map_err(|_| "puppet bone palette start exceeds usize".to_owned())?;
            let count = usize::try_from(palette.bone_matrix_count)
                .map_err(|_| "puppet bone palette count exceeds usize".to_owned())?;
            let end = start
                .checked_add(count)
                .ok_or_else(|| "puppet bone palette range overflows usize".to_owned())?;
            let matrices = graph.puppet_bone_matrices.get(start..end).ok_or_else(|| {
                format!(
                    "puppet {} palette range {start}..{end} exceeds {} matrices",
                    palette.puppet_index,
                    graph.puppet_bone_matrices.len()
                )
            })?;
            let bones = matrices
                .iter()
                .map(|matrix| {
                    if matrix.puppet_index != palette.puppet_index {
                        return Err(format!(
                            "puppet {} palette contains matrix for puppet {}",
                            palette.puppet_index, matrix.puppet_index
                        ));
                    }
                    Ok(RenderingDeviceScenePuppetBoneMatrixSnapshot {
                        bone_index: matrix.bone_index,
                        parent_index: matrix.parent_index,
                        matrix: matrix.matrix,
                        alpha: matrix.alpha,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(RenderingDeviceScenePuppetBonePaletteSnapshot {
                object_handle: palette.object.0,
                object_id,
                object_name,
                puppet_index: palette.puppet_index,
                resolved_visible: palette.resolved_visible,
                bones,
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
            Ok(RenderingDeviceSceneDrawActivationSnapshot {
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
                particle_resource_descriptor: command.particle_resource_descriptor,
                scene_owned_uniform_descriptor_base: command.scene_owned_uniform_descriptor_base,
                sampled_resource_descriptor_base: command.sampled_resource_descriptor_base,
                input_attachment_resource_descriptor_base: command
                    .input_attachment_resource_descriptor_base,
                sampler_descriptor_base: command.sampler_descriptor_base,
                vertex_buffer_byte_offset: command.vertex_buffer_byte_offset,
                descriptor_push_words: command
                    .active_descriptor_push()
                    .map(|push| {
                        push.bytes()
                            .chunks_exact(4)
                            .map(|word| {
                                u32::from_le_bytes(word.try_into().expect("four-byte push word"))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                material_uniform_words: material_uniform_payload
                    .map(|payload| material_uniform_words_for_draw(draw_index, payload))
                    .transpose()?
                    .unwrap_or_default(),
                scene_owned_uniform_words: scene_owned_uniform_plan
                    .payload_slices_for_draw(draw_index, scene_owned_uniform_payload)?
                    .into_iter()
                    .map(|slice| {
                        slice
                            .chunks_exact(4)
                            .map(|word| {
                                u32::from_le_bytes(word.try_into().expect("four-byte uniform word"))
                            })
                            .collect()
                    })
                    .collect(),
                standard_material_color: resolved_standard_material_color(storage, planned),
                scissor: command.scissor.map(scissor_snapshot),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let enabled_draw_count = draws.iter().filter(|draw| draw.enabled).count();
    Ok(RenderingDeviceSceneSemanticDiagnosticsSnapshot {
        descriptor_heap,
        retained_script_deltas,
        resolved_objects,
        puppet_bone_palettes,
        draws,
        enabled_draw_count,
    })
}

fn material_uniform_words_for_draw(draw_index: usize, payload: &[u8]) -> Result<Vec<u32>, String> {
    let stride = usize::try_from(SCENE_MATERIAL_UNIFORM_BYTES)
        .map_err(|_| "scene material uniform stride exceeds usize".to_owned())?;
    let start = draw_index
        .checked_mul(stride)
        .ok_or_else(|| "scene material uniform draw offset overflows usize".to_owned())?;
    let end = start
        .checked_add(stride)
        .ok_or_else(|| "scene material uniform draw range overflows usize".to_owned())?;
    let bytes = payload.get(start..end).ok_or_else(|| {
        format!(
            "scene material uniform draw {draw_index} range {start}..{end} exceeds {} bytes",
            payload.len()
        )
    })?;
    Ok(bytes
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().expect("four-byte material uniform word")))
        .collect())
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

fn scissor_snapshot(scissor: SceneGpuScissor) -> RenderingDeviceSceneScissorSnapshot {
    RenderingDeviceSceneScissorSnapshot {
        offset: scissor.offset,
        extent: scissor.extent,
    }
}
