//! RenderingDevice graph plan for scene storage.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`
//! - `references/godot/servers/rendering/renderer_scene_render.*`

use serde::{Deserialize, Serialize};

use super::abi::*;
use super::semantic_world::{ResolvedObjectState, ResolvedSemanticFrame};
use super::server::RendererSceneRenderPlan;
use super::storage::SceneStorage;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceGraphPlan {
    pub pass_nodes: Vec<SceneRenderingDevicePassNode>,
    pub target_allocations: Vec<SceneRenderingDeviceTargetAllocation>,
    pub mesh_draws: Vec<SceneRenderingDeviceMeshDraw>,
    pub puppet_bone_palettes: Vec<SceneRenderingDevicePuppetBonePalette>,
    pub puppet_bone_matrices: Vec<SceneRenderingDevicePuppetBoneMatrix>,
    pub resolved_object_count: usize,
    pub resolved_visible_object_count: usize,
    pub resolved_attachment_link_count: usize,
    pub resolved_visible_effect_instance_count: usize,
    pub resolved_visible_effect_pass_count: usize,
    pub resolved_visible_effect_fbo_count: usize,
    pub descriptor_heap_required: bool,
    pub descriptor_heap_resource_count: u32,
    pub descriptor_heap_sampled_image_count: u32,
    pub descriptor_heap_uniform_buffer_count: u32,
    pub descriptor_heap_storage_buffer_count: u32,
    pub descriptor_heap_sampler_count: u32,
    pub graph_physical_target_count: u32,
    pub graph_aliased_target_count: u32,
    pub fifo_latest_ready_present_required: bool,
}

impl SceneRenderingDeviceGraphPlan {
    pub(crate) fn from_storage_with_semantic_frame(
        storage: &SceneStorage,
        render_plan: RendererSceneRenderPlan,
        semantic_frame: &ResolvedSemanticFrame,
    ) -> Self {
        let mut pass_nodes = Vec::new();
        let mut mesh_draws = Vec::new();
        let puppet_bone_palettes = semantic_frame
            .puppet_bone_palettes
            .iter()
            .map(|palette| SceneRenderingDevicePuppetBonePalette {
                object: palette.object,
                puppet_index: palette.puppet_index,
                bone_matrix_start: palette.bone_start,
                bone_matrix_count: palette.bone_count,
                resolved_visible: palette.resolved_visible,
            })
            .collect::<Vec<_>>();
        let puppet_bone_matrices = semantic_frame
            .puppet_bone_matrices
            .iter()
            .map(|matrix| SceneRenderingDevicePuppetBoneMatrix {
                puppet_index: matrix.puppet_index,
                bone_index: matrix.bone_index,
                parent_index: matrix.parent_index,
                matrix: rows_from_column_major(matrix.matrix),
            })
            .collect::<Vec<_>>();

        for (graph_index, graph) in storage.render_graphs().iter().enumerate() {
            for (local_pass_index, pass) in storage.render_graph_passes(graph).iter().enumerate() {
                let mesh_draw_start = mesh_draws.len() as u32;
                let pass_object_state = visible_pass_object(semantic_frame, pass);
                if let (true, Some(pass_object_state)) =
                    (pass_draws_object_mesh(pass), pass_object_state)
                {
                    let resolved_object_index = pass_object_state.object_index;
                    for (mesh_index, mesh) in storage.meshes().iter().enumerate() {
                        if mesh.object == pass.object {
                            mesh_draws.push(SceneRenderingDeviceMeshDraw {
                                mesh_index: mesh_index as u32,
                                resolved_object_index,
                                clip_transform: scene_clip_transform(
                                    storage.project(),
                                    pass_object_state.world_matrix,
                                ),
                                skinning_palette_start: skinning_palette_start(
                                    &puppet_bone_palettes,
                                    mesh.object,
                                ),
                                skinning_palette_count: skinning_palette_count(
                                    &puppet_bone_palettes,
                                    mesh.object,
                                ),
                                object: mesh.object,
                                material: pass_draw_material(pass, mesh.material),
                                vertex_start: mesh.vertex_start,
                                vertex_count: mesh.vertex_count,
                                index_start: mesh.index_start,
                                index_count: mesh.index_count,
                            });
                        }
                    }
                }
                pass_nodes.push(SceneRenderingDevicePassNode {
                    graph_index: graph_index as u32,
                    pass_record_index: graph.pass_start + local_pass_index as u32,
                    pass_id: pass.id,
                    role: pass.role,
                    target: pass.target,
                    target_name: pass.target_name,
                    binding_start: pass.binding_start,
                    binding_count: pass.binding_count,
                    mesh_draw_start,
                    mesh_draw_count: mesh_draws.len() as u32 - mesh_draw_start,
                });
            }
        }
        let target_allocations = graph_target_allocations(storage);
        let graph_physical_target_count = target_allocations
            .iter()
            .map(|allocation| allocation.physical_slot)
            .max()
            .map(|slot| slot.saturating_add(1))
            .unwrap_or(0);
        let graph_aliased_target_count =
            (target_allocations.len() as u32).saturating_sub(graph_physical_target_count);

        Self {
            pass_nodes,
            target_allocations,
            mesh_draws,
            puppet_bone_palettes,
            puppet_bone_matrices,
            resolved_object_count: semantic_frame.objects.len(),
            resolved_visible_object_count: semantic_frame.visible_object_count,
            resolved_attachment_link_count: semantic_frame.attachment_links.len(),
            resolved_visible_effect_instance_count: semantic_frame.visible_effect_instance_count,
            resolved_visible_effect_pass_count: semantic_frame.visible_effect_pass_count,
            resolved_visible_effect_fbo_count: semantic_frame.visible_effect_fbo_count,
            descriptor_heap_required: render_plan.descriptor_heap_required,
            descriptor_heap_resource_count: render_plan.descriptor_heap_resource_count,
            descriptor_heap_sampled_image_count: render_plan.descriptor_heap_sampled_image_count,
            descriptor_heap_uniform_buffer_count: render_plan.descriptor_heap_uniform_buffer_count,
            descriptor_heap_storage_buffer_count: render_plan.descriptor_heap_storage_buffer_count,
            descriptor_heap_sampler_count: render_plan.descriptor_heap_sampler_count,
            graph_physical_target_count,
            graph_aliased_target_count,
            fifo_latest_ready_present_required: render_plan.fifo_latest_ready_present_required,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDevicePassNode {
    pub graph_index: u32,
    pub pass_record_index: u32,
    pub pass_id: u32,
    pub role: SceneRenderPassKind,
    pub target: SceneRenderTargetKind,
    pub target_name: SceneStringId,
    pub binding_start: u32,
    pub binding_count: u32,
    pub mesh_draw_start: u32,
    pub mesh_draw_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceTargetAllocation {
    pub target: SceneRenderTargetKind,
    pub target_name: SceneStringId,
    pub first_write_pass_id: u32,
    pub last_use_pass_id: u32,
    pub physical_slot: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceMeshDraw {
    pub mesh_index: u32,
    pub resolved_object_index: u32,
    pub clip_transform: [[f32; 4]; 4],
    pub skinning_palette_start: u32,
    pub skinning_palette_count: u32,
    pub object: SceneObjectHandle,
    pub material: SceneMaterialHandle,
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderingDevicePuppetBonePalette {
    pub object: SceneObjectHandle,
    pub puppet_index: u32,
    pub bone_matrix_start: u32,
    pub bone_matrix_count: u32,
    pub resolved_visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderingDevicePuppetBoneMatrix {
    pub puppet_index: u32,
    pub bone_index: u32,
    pub parent_index: i32,
    pub matrix: [[f32; 4]; 4],
}

fn pass_draws_object_mesh(pass: &SceneRenderPassRecord) -> bool {
    pass.object.0 != INVALID_OBJECT_ID
        && matches!(
            pass.role,
            SceneRenderPassKind::BaseMaterial
                | SceneRenderPassKind::EffectMaterial
                | SceneRenderPassKind::ColorBlendPassthrough
        )
}

fn visible_pass_object<'frame>(
    semantic_frame: &'frame ResolvedSemanticFrame,
    pass: &SceneRenderPassRecord,
) -> Option<&'frame ResolvedObjectState> {
    semantic_frame
        .object(pass.object)
        .filter(|object| object.resolved_visible)
}

fn scene_clip_transform(project: &SceneProjectRecord, world_matrix: [f32; 16]) -> [[f32; 4]; 4] {
    let width = project.logical_width.max(1) as f32;
    let height = project.logical_height.max(1) as f32;
    [
        [
            2.0 * world_matrix[0] / width,
            2.0 * world_matrix[4] / width,
            2.0 * world_matrix[8] / width,
            2.0 * world_matrix[12] / width,
        ],
        [
            -2.0 * world_matrix[1] / height,
            -2.0 * world_matrix[5] / height,
            -2.0 * world_matrix[9] / height,
            -2.0 * world_matrix[13] / height,
        ],
        [
            world_matrix[2],
            world_matrix[6],
            world_matrix[10],
            world_matrix[14],
        ],
        [
            world_matrix[3],
            world_matrix[7],
            world_matrix[11],
            world_matrix[15],
        ],
    ]
}

fn rows_from_column_major(matrix: [f32; 16]) -> [[f32; 4]; 4] {
    [
        [matrix[0], matrix[4], matrix[8], matrix[12]],
        [matrix[1], matrix[5], matrix[9], matrix[13]],
        [matrix[2], matrix[6], matrix[10], matrix[14]],
        [matrix[3], matrix[7], matrix[11], matrix[15]],
    ]
}

fn skinning_palette_start(
    palettes: &[SceneRenderingDevicePuppetBonePalette],
    object: SceneObjectHandle,
) -> u32 {
    palettes
        .iter()
        .find(|palette| palette.object == object && palette.resolved_visible)
        .map(|palette| palette.bone_matrix_start)
        .unwrap_or(INVALID_OBJECT_ID)
}

fn skinning_palette_count(
    palettes: &[SceneRenderingDevicePuppetBonePalette],
    object: SceneObjectHandle,
) -> u32 {
    palettes
        .iter()
        .find(|palette| palette.object == object && palette.resolved_visible)
        .map(|palette| palette.bone_matrix_count)
        .unwrap_or(0)
}

fn pass_draw_material(
    pass: &SceneRenderPassRecord,
    mesh_material: SceneMaterialHandle,
) -> SceneMaterialHandle {
    if pass.material.0 == INVALID_MATERIAL_ID {
        mesh_material
    } else {
        pass.material
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TargetAllocationState {
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
    first_write_pass_id: u32,
    last_use_pass_id: u32,
    first_write_order: u32,
    last_use_order: u32,
}

fn graph_target_allocations(storage: &SceneStorage) -> Vec<SceneRenderingDeviceTargetAllocation> {
    let mut states = Vec::<TargetAllocationState>::new();
    let mut pass_order = 0u32;
    for graph in storage.render_graphs() {
        for pass in storage.render_graph_passes(graph) {
            if graph_target_is_allocatable(pass.target) {
                record_target_write(
                    &mut states,
                    pass.target,
                    pass.target_name,
                    pass.id,
                    pass_order,
                );
            }
            for binding in storage.render_pass_bindings(pass) {
                if let Some((target, name)) = binding_target_read(binding) {
                    record_target_read(&mut states, target, name, pass.id, pass_order);
                }
            }
            pass_order = pass_order.saturating_add(1);
        }
    }
    states.sort_by(|left, right| {
        left.first_write_order
            .cmp(&right.first_write_order)
            .then_with(|| left.target.to_u32().cmp(&right.target.to_u32()))
            .then_with(|| left.target_name.0.cmp(&right.target_name.0))
    });
    let mut physical_last_use = Vec::<u32>::new();
    states
        .into_iter()
        .map(|state| {
            let slot = physical_last_use
                .iter()
                .position(|last_use| *last_use < state.first_write_order)
                .unwrap_or_else(|| {
                    physical_last_use.push(0);
                    physical_last_use.len() - 1
                });
            physical_last_use[slot] = state.last_use_order;
            SceneRenderingDeviceTargetAllocation {
                target: state.target,
                target_name: state.target_name,
                first_write_pass_id: state.first_write_pass_id,
                last_use_pass_id: state.last_use_pass_id,
                physical_slot: slot as u32,
            }
        })
        .collect()
}

fn record_target_write(
    states: &mut Vec<TargetAllocationState>,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
    pass_id: u32,
    pass_order: u32,
) {
    if let Some(state) = states
        .iter_mut()
        .find(|state| state.target == target && state.target_name == target_name)
    {
        state.first_write_pass_id = state.first_write_pass_id.min(pass_id);
        state.last_use_pass_id = state.last_use_pass_id.max(pass_id);
        state.first_write_order = state.first_write_order.min(pass_order);
        state.last_use_order = state.last_use_order.max(pass_order);
    } else {
        states.push(TargetAllocationState {
            target,
            target_name,
            first_write_pass_id: pass_id,
            last_use_pass_id: pass_id,
            first_write_order: pass_order,
            last_use_order: pass_order,
        });
    }
}

fn record_target_read(
    states: &mut [TargetAllocationState],
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
    pass_id: u32,
    pass_order: u32,
) {
    if let Some(state) = states
        .iter_mut()
        .find(|state| state.target == target && state.target_name == target_name)
    {
        state.last_use_pass_id = state.last_use_pass_id.max(pass_id);
        state.last_use_order = state.last_use_order.max(pass_order);
    }
}

fn binding_target_read(
    binding: &SceneRenderBindingRecord,
) -> Option<(SceneRenderTargetKind, SceneStringId)> {
    match binding.kind {
        SceneRenderBindingKind::GraphTarget
        | SceneRenderBindingKind::NamedFboBind
        | SceneRenderBindingKind::EffectTarget => Some((binding.target, binding.name)),
        SceneRenderBindingKind::PreviousGraphTarget => {
            Some((SceneRenderTargetKind::ImageLocalMain, SceneStringId::NONE))
        }
        _ => None,
    }
}

fn graph_target_is_allocatable(target: SceneRenderTargetKind) -> bool {
    matches!(
        target,
        SceneRenderTargetKind::ImageLocalMain
            | SceneRenderTargetKind::ImageLocalSub
            | SceneRenderTargetKind::NamedFbo
            | SceneRenderTargetKind::FirstClassEffectTarget
            | SceneRenderTargetKind::Temporary
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{RenderingServer, SceneStorage};
    use crate::engine::scene::{
        SceneBinaryDocument, SceneMaterialHandle, SceneMaterialRecord, SceneMeshRecord,
        SceneMeshVertexRecord, SceneObjectHandle, SceneObjectKind, SceneObjectRecord,
        ScenePuppetBoneRecord, ScenePuppetRecord, SceneRenderBindingKind, SceneRenderBindingRecord,
        SceneRenderGraphRecord, SceneRenderPassRecord, SceneResourceId, SceneShaderContractRecord,
        SceneStringId, SceneVec3,
    };

    #[test]
    fn rendering_device_graph_plans_mesh_draws_and_heap_counts() {
        let document = SceneBinaryDocument {
            strings: vec!["shader".to_owned(), "pipeline".to_owned()],
            objects: vec![
                SceneObjectRecord {
                    id: SceneObjectHandle(0),
                    we_id: 7,
                    name: SceneStringId::NONE,
                    kind: SceneObjectKind::Puppet,
                    resource: SceneResourceId::NONE,
                    material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                    parent_we_id: INVALID_OBJECT_ID,
                    attachment: SceneStringId::NONE,
                    origin: SceneVec3::default(),
                    angles: SceneVec3::default(),
                    scale: SceneVec3 {
                        x: 1.0,
                        y: 1.0,
                        z: 1.0,
                    },
                    visible: true,
                    color_blend_mode: 0,
                    sort_order: 0,
                    effect_start: u32::MAX,
                    effect_count: 0,
                    render_graph: 0,
                },
                SceneObjectRecord {
                    id: SceneObjectHandle(1),
                    we_id: 8,
                    name: SceneStringId::NONE,
                    kind: SceneObjectKind::Image,
                    resource: SceneResourceId::NONE,
                    material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                    parent_we_id: INVALID_OBJECT_ID,
                    attachment: SceneStringId::NONE,
                    origin: SceneVec3::default(),
                    angles: SceneVec3::default(),
                    scale: SceneVec3 {
                        x: 1.0,
                        y: 1.0,
                        z: 1.0,
                    },
                    visible: false,
                    color_blend_mode: 0,
                    sort_order: 0,
                    effect_start: u32::MAX,
                    effect_count: 0,
                    render_graph: 1,
                },
            ],
            meshes: vec![
                SceneMeshRecord {
                    object: SceneObjectHandle(0),
                    material: SceneMaterialHandle(0),
                    vertex_start: 0,
                    vertex_count: 4,
                    index_start: 0,
                    index_count: 6,
                    width: 64.0,
                    height: 32.0,
                    bounds_min: SceneVec3 {
                        x: -32.0,
                        y: -16.0,
                        z: 0.0,
                    },
                    bounds_max: SceneVec3 {
                        x: 32.0,
                        y: 16.0,
                        z: 0.0,
                    },
                },
                SceneMeshRecord {
                    object: SceneObjectHandle(1),
                    material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                    vertex_start: 4,
                    vertex_count: 4,
                    index_start: 6,
                    index_count: 6,
                    width: 64.0,
                    height: 32.0,
                    bounds_min: SceneVec3 {
                        x: -32.0,
                        y: -16.0,
                        z: 0.0,
                    },
                    bounds_max: SceneVec3 {
                        x: 32.0,
                        y: 16.0,
                        z: 0.0,
                    },
                },
            ],
            mesh_vertices: vec![
                SceneMeshVertexRecord {
                    position: SceneVec3 {
                        x: -32.0,
                        y: -16.0,
                        z: 0.0,
                    },
                    uv: [0.0, 1.0],
                };
                8
            ],
            mesh_indices: vec![0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3],
            puppets: vec![ScenePuppetRecord {
                object: SceneObjectHandle(0),
                resource: SceneResourceId::NONE,
                mesh_start: 0,
                mesh_count: 1,
                bone_start: 0,
                bone_count: 1,
                attachment_start: 0,
                attachment_count: 0,
            }],
            puppet_bones: vec![ScenePuppetBoneRecord {
                puppet: 0,
                bone_index: 41,
                flags: 0,
                parent_index: -1,
                local_matrix: [
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ],
                info: SceneStringId::NONE,
            }],
            materials: vec![
                SceneMaterialRecord {
                    id: SceneMaterialHandle(0),
                    resource: SceneResourceId::NONE,
                    pass_start: 0,
                    pass_count: 0,
                },
                SceneMaterialRecord {
                    id: SceneMaterialHandle(1),
                    resource: SceneResourceId::NONE,
                    pass_start: 0,
                    pass_count: 0,
                },
            ],
            render_graphs: vec![
                SceneRenderGraphRecord {
                    object: SceneObjectHandle(0),
                    pass_start: 0,
                    pass_count: 1,
                    unsupported_start: 0,
                    unsupported_count: 0,
                },
                SceneRenderGraphRecord {
                    object: SceneObjectHandle(1),
                    pass_start: 1,
                    pass_count: 1,
                    unsupported_start: 0,
                    unsupported_count: 0,
                },
            ],
            render_passes: vec![
                SceneRenderPassRecord {
                    id: 9,
                    role: SceneRenderPassKind::BaseMaterial,
                    object: SceneObjectHandle(0),
                    material: SceneMaterialHandle(1),
                    pass_index: 0,
                    shader_key: SceneStringId(0),
                    target: SceneRenderTargetKind::SceneColor,
                    target_name: SceneStringId::NONE,
                    binding_start: 0,
                    binding_count: 0,
                    pipeline_blend: ScenePipelineBlend::Normal,
                    depth_test: SceneDepthTest::Disabled,
                    depth_write: false,
                    cull_mode: SceneCullMode::None,
                },
                SceneRenderPassRecord {
                    id: 10,
                    role: SceneRenderPassKind::BaseMaterial,
                    object: SceneObjectHandle(1),
                    material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                    pass_index: 0,
                    shader_key: SceneStringId(0),
                    target: SceneRenderTargetKind::SceneColor,
                    target_name: SceneStringId::NONE,
                    binding_start: 0,
                    binding_count: 0,
                    pipeline_blend: ScenePipelineBlend::Normal,
                    depth_test: SceneDepthTest::Disabled,
                    depth_write: false,
                    cull_mode: SceneCullMode::None,
                },
            ],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(1),
                texture_slot_mask: 0b1,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 2,
                sampler_heap_count: 1,
            }],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");
        let graph = RenderingServer::new(&storage).rendering_device_graph_plan();

        assert_eq!(graph.pass_nodes.len(), 2);
        assert_eq!(graph.pass_nodes[0].pass_id, 9);
        assert_eq!(graph.pass_nodes[0].mesh_draw_count, 1);
        assert_eq!(graph.pass_nodes[1].pass_id, 10);
        assert_eq!(graph.pass_nodes[1].mesh_draw_count, 0);
        assert_eq!(graph.mesh_draws.len(), 1);
        assert_eq!(graph.mesh_draws[0].resolved_object_index, 0);
        assert_eq!(graph.mesh_draws[0].clip_transform[0][0], 2.0);
        assert_eq!(graph.mesh_draws[0].clip_transform[1][1], -2.0);
        assert_eq!(graph.mesh_draws[0].skinning_palette_start, 0);
        assert_eq!(graph.mesh_draws[0].skinning_palette_count, 1);
        assert_eq!(graph.mesh_draws[0].material, SceneMaterialHandle(1));
        assert_eq!(graph.mesh_draws[0].vertex_count, 4);
        assert_eq!(graph.mesh_draws[0].index_count, 6);
        assert_eq!(graph.puppet_bone_palettes.len(), 1);
        assert_eq!(graph.puppet_bone_matrices.len(), 1);
        assert_eq!(graph.puppet_bone_matrices[0].bone_index, 41);
        assert_eq!(graph.resolved_object_count, 2);
        assert_eq!(graph.resolved_visible_object_count, 1);
        assert_eq!(graph.descriptor_heap_resource_count, 3);
        assert_eq!(graph.descriptor_heap_sampled_image_count, 1);
        assert_eq!(graph.descriptor_heap_uniform_buffer_count, 1);
        assert_eq!(graph.descriptor_heap_storage_buffer_count, 1);
        assert!(graph.fifo_latest_ready_present_required);
    }

    #[test]
    fn rendering_device_graph_allocates_named_effect_targets_from_pass_bindings() {
        let document = SceneBinaryDocument {
            strings: vec!["fbo_a".to_owned(), "fbo_b".to_owned()],
            render_graphs: vec![SceneRenderGraphRecord {
                object: SceneObjectHandle(INVALID_OBJECT_ID),
                pass_start: 0,
                pass_count: 3,
                unsupported_start: 0,
                unsupported_count: 0,
            }],
            render_passes: vec![
                named_fbo_pass(1, 0, SceneStringId(0), 0, 0),
                named_fbo_pass(2, 1, SceneStringId(1), 0, 1),
                scene_color_pass_reading_fbo(3, 1, 1),
            ],
            render_bindings: vec![
                named_fbo_binding(SceneStringId(0)),
                named_fbo_binding(SceneStringId(1)),
            ],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");
        let graph = RenderingServer::new(&storage).rendering_device_graph_plan();

        assert_eq!(graph.target_allocations.len(), 2);
        assert_eq!(
            graph.target_allocations[0].target,
            SceneRenderTargetKind::NamedFbo
        );
        assert_eq!(graph.target_allocations[0].target_name, SceneStringId(0));
        assert_eq!(graph.target_allocations[0].last_use_pass_id, 2);
        assert_eq!(graph.target_allocations[1].target_name, SceneStringId(1));
        assert_eq!(graph.target_allocations[1].last_use_pass_id, 3);
        assert_eq!(graph.graph_physical_target_count, 2);
        assert_eq!(graph.graph_aliased_target_count, 0);
    }

    fn named_fbo_pass(
        id: u32,
        pass_index: u32,
        target_name: SceneStringId,
        binding_start: u32,
        binding_count: u32,
    ) -> SceneRenderPassRecord {
        SceneRenderPassRecord {
            id,
            role: SceneRenderPassKind::EffectMaterial,
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            pass_index,
            shader_key: SceneStringId::NONE,
            target: SceneRenderTargetKind::NamedFbo,
            target_name,
            binding_start,
            binding_count,
            pipeline_blend: ScenePipelineBlend::Normal,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
        }
    }

    fn scene_color_pass_reading_fbo(
        id: u32,
        binding_start: u32,
        binding_count: u32,
    ) -> SceneRenderPassRecord {
        SceneRenderPassRecord {
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            ..named_fbo_pass(id, 2, SceneStringId::NONE, binding_start, binding_count)
        }
    }

    fn named_fbo_binding(name: SceneStringId) -> SceneRenderBindingRecord {
        SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::NamedFboBind,
            slot: 0,
            target: SceneRenderTargetKind::NamedFbo,
            name,
        }
    }
}
