//! RenderingDevice graph plan for scene storage.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/exe/blend-and-render.md`
//! - `references/tensor-wallpaper/godot/servers/rendering/rendering_device_graph.*`
//! - `references/tensor-wallpaper/godot/servers/rendering/renderer_scene_render.*`

use serde::{Deserialize, Serialize};

use super::abi::*;
use super::semantic_world::{ResolvedObjectState, ResolvedSemanticFrame};
use super::server::RendererSceneRenderPlan;
use super::storage::SceneStorage;

mod draw_support;
mod effect_batch;
mod projection;
mod queries;
mod target_extent;
mod types;

use draw_support::{
    image_binding_access, material_sampled_bindings, pass_draw_material, rows_from_column_major,
    sampled_binding, skinning_palette_count, skinning_palette_start,
};

pub use effect_batch::{
    SceneRenderingDeviceEffectBatch, SceneRenderingDeviceEffectBatchFamily,
    SceneRenderingDeviceEffectBatchInstance,
};
use projection::pass_projection_domain;
#[cfg(test)]
use projection::{authored_texture_clip_transform, scene_clip_transform};
use target_extent::{authored_texture_space_target_extent, image_target, target_extent_domain};
pub use types::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceGraphPlan {
    pub pass_nodes: Vec<SceneRenderingDevicePassNode>,
    pub target_allocations: Vec<SceneRenderingDeviceTargetAllocation>,
    pub effect_batches: Vec<SceneRenderingDeviceEffectBatch>,
    pub effect_batch_instances: Vec<SceneRenderingDeviceEffectBatchInstance>,
    pub sampled_bindings: Vec<SceneRenderingDeviceSampledBinding>,
    pub material_sampled_bindings: Vec<SceneRenderingDeviceMaterialSampledBinding>,
    pub mesh_draws: Vec<SceneRenderingDeviceMeshDraw>,
    pub puppet_bone_palettes: Vec<SceneRenderingDevicePuppetBonePalette>,
    pub puppet_bone_matrices: Vec<SceneRenderingDevicePuppetBoneMatrix>,
    pub particle_gpu_emitters: Vec<SceneParticleGpuEmitterPlan>,
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
        let mut sampled_bindings = Vec::new();
        let mut mesh_draws = Vec::new();
        let particle_gpu_emitters = particle_gpu_emitter_plans(storage.particles());
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
                alpha: matrix.alpha,
            })
            .collect::<Vec<_>>();
        for (graph_index, graph) in storage.render_graphs().iter().enumerate() {
            for (local_pass_index, pass) in storage.render_graph_passes(graph).iter().enumerate() {
                let pass_node_index = pass_nodes.len() as u32;
                let mesh_draw_start = mesh_draws.len() as u32;
                let effect_visibility_mask = resolved_effect_visibility_mask(semantic_frame, pass);
                let pass_object_state = pass_object(semantic_frame, pass);
                if let (true, Some(pass_object_state)) =
                    (pass_draws_object_mesh(pass), pass_object_state)
                {
                    let resolved_object_index = pass_object_state.object_index;
                    for (mesh_index, mesh) in storage.meshes().iter().enumerate() {
                        if mesh.object == pass.object {
                            let Some((index_start, index_count)) =
                                pass_mesh_index_range(storage, pass, mesh_index as u32, mesh)
                            else {
                                continue;
                            };
                            let projection_domain =
                                pass_projection_domain(storage, graph_index as u32, pass);
                            let clip_transform = projection_domain.clip_transform(
                                storage,
                                semantic_frame,
                                pass_object_state.render_world_matrix,
                            );
                            mesh_draws.push(SceneRenderingDeviceMeshDraw {
                                primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
                                particle_index: INVALID_PARTICLE_INDEX,
                                projection_domain,
                                shader_key: pass.shader_key,
                                mesh_index: mesh_index as u32,
                                resolved_object_index,
                                render_world_matrix: rows_from_column_major(
                                    pass_object_state.render_world_matrix,
                                ),
                                clip_transform,
                                effect_model_view_projection_matrix: clip_transform,
                                authored_source_extent: authored_source_extent(
                                    storage,
                                    pass.object,
                                ),
                                uv_inset_texels: if pass.draw_primitive
                                    == SceneRenderPassDrawPrimitive::ObjectCompositeMesh
                                {
                                    SCENE_OBJECT_COMPOSITE_UV_INSET_TEXELS
                                } else {
                                    0.0
                                },
                                skinning_palette_start: skinning_palette_start(
                                    &puppet_bone_palettes,
                                    mesh.object,
                                ),
                                skinning_palette_count: skinning_palette_count(
                                    &puppet_bone_palettes,
                                    mesh.object,
                                ),
                                resolved_color: pass_object_state.resolved_color,
                                resolved_alpha: pass_object_state.resolved_alpha,
                                apply_resolved_visual: pass_applies_resolved_visual(pass),
                                effect_batch_atlas_tile: INVALID_OBJECT_ID,
                                effect_batch_atlas_grid: [0; 2],
                                effect_binding_start: pass.effect_binding_start,
                                effect_binding_count: pass.effect_binding_count,
                                effect_visibility_policy: pass.effect_visibility_policy,
                                resolved_effect_visibility_mask: effect_visibility_mask,
                                object: mesh.object,
                                material: pass_draw_material(pass, mesh.material),
                                vertex_start: mesh.vertex_start,
                                vertex_count: mesh.vertex_count,
                                index_start,
                                index_count,
                                instance_count: 1,
                            });
                        }
                    }
                }
                if mesh_draws.len() == mesh_draw_start as usize
                    && let Some(primitive) =
                        pass_utility_primitive(storage, pass, pass_object_state)
                {
                    mesh_draws.push(utility_primitive_draw(
                        storage,
                        semantic_frame,
                        graph_index as u32,
                        pass,
                        pass_object_state,
                        primitive,
                        effect_visibility_mask,
                    ));
                }
                let mesh_draw_count = mesh_draws.len() as u32 - mesh_draw_start;
                sampled_bindings.extend(storage.render_pass_bindings(pass).iter().filter_map(
                    |binding| {
                        sampled_binding(
                            storage,
                            graph_index as u32,
                            pass_node_index,
                            mesh_draw_start,
                            mesh_draw_count,
                            binding,
                        )
                        .map(|mut lowered| {
                            lowered.access = image_binding_access(storage, pass, binding.slot);
                            lowered
                        })
                    },
                ));
                pass_nodes.push(SceneRenderingDevicePassNode {
                    graph_index: graph_index as u32,
                    graph_activation_policy: graph.activation_policy,
                    pass_record_index: graph.pass_start + local_pass_index as u32,
                    pass_id: pass.id,
                    role: pass.role,
                    target: pass.target,
                    target_name: pass.target_name,
                    binding_start: pass.binding_start,
                    binding_count: pass.binding_count,
                    effect_binding_start: pass.effect_binding_start,
                    effect_binding_count: pass.effect_binding_count,
                    effect_visibility_policy: pass.effect_visibility_policy,
                    mesh_draw_start,
                    mesh_draw_count,
                });
            }
        }
        let target_allocations = graph_target_allocations(storage);
        let (effect_batches, effect_batch_instances) = effect_batch::build_scene_effect_batches(
            storage,
            &pass_nodes,
            &target_allocations,
            &mut mesh_draws,
        );
        let material_sampled_bindings = material_sampled_bindings(storage, &mesh_draws);
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
            effect_batches,
            effect_batch_instances,
            sampled_bindings,
            material_sampled_bindings,
            mesh_draws,
            puppet_bone_palettes,
            puppet_bone_matrices,
            particle_gpu_emitters,
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

fn pass_draws_object_mesh(pass: &SceneRenderPassRecord) -> bool {
    pass.object.0 != INVALID_OBJECT_ID
        && matches!(
            pass.draw_primitive,
            SceneRenderPassDrawPrimitive::ObjectMesh
                | SceneRenderPassDrawPrimitive::ObjectCompositeMesh
        )
}

fn pass_mesh_index_range(
    storage: &SceneStorage,
    pass: &SceneRenderPassRecord,
    mesh_index: u32,
    mesh: &SceneMeshRecord,
) -> Option<(u32, u32)> {
    let role = match pass.role {
        SceneRenderPassKind::MeshVisiblePrefix => SceneMeshClippingSliceRole::VisiblePrefix,
        SceneRenderPassKind::MeshClippingMask => SceneMeshClippingSliceRole::MaskProducer,
        SceneRenderPassKind::MeshClippedTarget => SceneMeshClippingSliceRole::ClippedTarget,
        SceneRenderPassKind::MeshVisibleRemainder => SceneMeshClippingSliceRole::VisibleRemainder,
        _ => return Some((mesh.index_start, mesh.index_count)),
    };
    storage
        .mesh_clipping_slices(mesh_index)
        .find(|slice| {
            slice.role == role
                && (role == SceneMeshClippingSliceRole::VisiblePrefix
                    || slice.subdraw == pass.pass_index)
        })
        .map(|slice| (slice.index_start, slice.index_count))
}

fn pass_object<'frame>(
    semantic_frame: &'frame ResolvedSemanticFrame,
    pass: &SceneRenderPassRecord,
) -> Option<&'frame ResolvedObjectState> {
    semantic_frame.object(pass.object)
}

fn pass_utility_primitive(
    storage: &SceneStorage,
    pass: &SceneRenderPassRecord,
    pass_object_state: Option<&ResolvedObjectState>,
) -> Option<SceneRenderingDeviceDrawPrimitive> {
    if pass.draw_primitive == SceneRenderPassDrawPrimitive::ParticleBillboard {
        pass_object_state?;
        return storage
            .particle(pass.id)
            .filter(|particle| {
                particle.object == pass.object
                    && (particle.parent_particle_index == INVALID_PARTICLE_INDEX
                        || particle.child_type == SceneParticleChildType::BuiltinDefault)
                    && matches!(
                        particle.simulation,
                        SceneParticleSimulationKind::FallingLeaves
                            | SceneParticleSimulationKind::AmbientSparkles
                            | SceneParticleSimulationKind::FloralOscillation
                            | SceneParticleSimulationKind::ModuleSprite
                    )
                    && particle.max_count != 0
            })
            .map(|_| SceneRenderingDeviceDrawPrimitive::ParticleBillboard);
    }
    if pass.object.0 != INVALID_OBJECT_ID && pass_object_state.is_none() {
        return None;
    }
    match pass.draw_primitive {
        SceneRenderPassDrawPrimitive::FullscreenTriangle => {
            Some(SceneRenderingDeviceDrawPrimitive::FullscreenTriangle)
        }
        SceneRenderPassDrawPrimitive::ObjectUvSupportQuad => {
            Some(SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad)
        }
        SceneRenderPassDrawPrimitive::None
        | SceneRenderPassDrawPrimitive::ObjectMesh
        | SceneRenderPassDrawPrimitive::ObjectCompositeMesh
        | SceneRenderPassDrawPrimitive::ParticleBillboard => None,
    }
}

fn utility_primitive_draw(
    storage: &SceneStorage,
    semantic_frame: &ResolvedSemanticFrame,
    graph_index: u32,
    pass: &SceneRenderPassRecord,
    pass_object_state: Option<&ResolvedObjectState>,
    primitive: SceneRenderingDeviceDrawPrimitive,
    resolved_effect_visibility_mask: u32,
) -> SceneRenderingDeviceMeshDraw {
    let vertex_count = match primitive {
        SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad => 6,
        SceneRenderingDeviceDrawPrimitive::ParticleBillboard => 4,
        SceneRenderingDeviceDrawPrimitive::ObjectMesh
        | SceneRenderingDeviceDrawPrimitive::FullscreenTriangle => 3,
    };
    let projection_domain = pass_projection_domain(storage, graph_index, pass);
    let clip_transform = pass_object_state.map_or_else(identity_clip_transform, |object| {
        projection_domain.clip_transform(storage, semantic_frame, object.render_world_matrix)
    });
    SceneRenderingDeviceMeshDraw {
        primitive,
        particle_index: if primitive == SceneRenderingDeviceDrawPrimitive::ParticleBillboard {
            pass.id
        } else {
            INVALID_PARTICLE_INDEX
        },
        projection_domain,
        shader_key: pass.shader_key,
        mesh_index: INVALID_OBJECT_ID,
        resolved_object_index: pass_object_state
            .map(|object| object.object_index)
            .unwrap_or(INVALID_OBJECT_ID),
        render_world_matrix: pass_object_state.map_or_else(identity_clip_transform, |object| {
            rows_from_column_major(object.render_world_matrix)
        }),
        clip_transform,
        effect_model_view_projection_matrix: clip_transform,
        authored_source_extent: authored_source_extent(storage, pass.object),
        uv_inset_texels: 0.0,
        skinning_palette_start: INVALID_OBJECT_ID,
        skinning_palette_count: 0,
        resolved_color: pass_object_state.map_or(
            SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            |object| object.resolved_color,
        ),
        resolved_alpha: pass_object_state.map_or(1.0, |object| object.resolved_alpha),
        apply_resolved_visual: pass_applies_resolved_visual(pass),
        effect_batch_atlas_tile: INVALID_OBJECT_ID,
        effect_batch_atlas_grid: [0; 2],
        effect_binding_start: pass.effect_binding_start,
        effect_binding_count: pass.effect_binding_count,
        effect_visibility_policy: pass.effect_visibility_policy,
        resolved_effect_visibility_mask,
        object: pass.object,
        material: pass.material,
        vertex_start: 0,
        vertex_count,
        index_start: 0,
        index_count: vertex_count,
        instance_count: if primitive == SceneRenderingDeviceDrawPrimitive::ParticleBillboard {
            storage
                .particle(pass.id)
                .map_or(0, procedural_particle_instance_capacity)
        } else {
            1
        },
    }
}

fn resolved_effect_visibility_mask(
    frame: &ResolvedSemanticFrame,
    pass: &SceneRenderPassRecord,
) -> u32 {
    (0..pass.effect_binding_count.min(32)).fold(0u32, |mask, local_index| {
        let binding_index = pass.effect_binding_start.saturating_add(local_index);
        if frame
            .object_effect(binding_index)
            .is_some_and(|effect| effect.resolved_visible)
        {
            mask | (1 << local_index)
        } else {
            mask
        }
    })
}

fn procedural_particle_instance_capacity(particle: &SceneParticleSystemRecord) -> u32 {
    particle.procedural_instance_capacity()
}

fn particle_gpu_emitter_plans(
    particles: &[SceneParticleSystemRecord],
) -> Vec<SceneParticleGpuEmitterPlan> {
    let mut particle_state_offset = 0u32;
    let mut plans = Vec::new();
    for (particle_index, particle) in particles.iter().enumerate() {
        if particle.max_count == 0 {
            continue;
        }
        let capacity = procedural_particle_instance_capacity(particle);
        let profile = particle.gpu_profile();
        let emitter_index = plans.len() as u32;
        plans.push(SceneParticleGpuEmitterPlan {
            object: particle.object,
            particle_index: particle_index as u32,
            profile,
            state_index: emitter_index,
            capacity,
            particle_state_offset,
            indirect_draw_index: emitter_index,
        });
        particle_state_offset = particle_state_offset.saturating_add(capacity);
    }
    plans
}

fn pass_applies_resolved_visual(pass: &SceneRenderPassRecord) -> bool {
    matches!(
        pass.target,
        SceneRenderTargetKind::SceneColor | SceneRenderTargetKind::Swapchain
    )
}

fn authored_source_extent(storage: &SceneStorage, object: SceneObjectHandle) -> [f32; 2] {
    let Some(object_record) = storage.objects().get(object.0 as usize) else {
        return [0.0; 2];
    };
    let texture_extent = storage
        .texture(object_record.resource)
        .or_else(|| source_texture_for_material(storage, object_record.material))
        .or_else(|| {
            storage
                .meshes()
                .iter()
                .filter(|mesh| mesh.object == object)
                .find_map(|mesh| source_texture_for_material(storage, mesh.material))
        })
        .map(|texture| [texture.width.max(1) as f32, texture.height.max(1) as f32]);
    texture_extent
        .or_else(|| {
            storage
                .meshes()
                .iter()
                .find(|mesh| mesh.object == object)
                .map(|mesh| [mesh.width, mesh.height])
        })
        .unwrap_or([0.0; 2])
}

fn source_texture_for_material(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
) -> Option<&SceneTextureRecord> {
    let material = storage.material(material)?;
    storage
        .material_passes(material)
        .iter()
        .flat_map(|pass| storage.material_pass_textures(pass))
        .find(|binding| binding.slot == 0)
        .and_then(|binding| storage.texture(binding.resource))
}

fn identity_clip_transform() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TargetAllocationState {
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
    first_write_pass_id: u32,
    last_use_pass_id: u32,
    first_write_order: u32,
    last_use_order: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TargetAllocationCompatibility {
    format: SceneStringId,
    width_divisor_milli: u32,
    height_divisor_milli: u32,
    target_width: u32,
    target_height: u32,
    extent_domain: SceneTargetExtentDomain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PhysicalTargetState {
    last_use_order: u32,
    compatibility: TargetAllocationCompatibility,
    reusable: bool,
}

fn graph_target_allocations(storage: &SceneStorage) -> Vec<SceneRenderingDeviceTargetAllocation> {
    let mut states = Vec::<TargetAllocationState>::new();
    let mut pass_order = 0u32;
    for (graph_index, graph) in storage.render_graphs().iter().enumerate() {
        for pass in storage.render_graph_passes(graph) {
            if graph_target_is_allocatable(pass.target) {
                record_target_write(
                    &mut states,
                    graph_index as u32,
                    pass.target,
                    pass.target_name,
                    pass.id,
                    pass_order,
                );
            }
            for binding in storage.render_pass_bindings(pass) {
                if let Some((target, name)) = binding_target_read(binding) {
                    record_target_read(
                        &mut states,
                        graph_index as u32,
                        target,
                        name,
                        pass.id,
                        pass_order,
                    );
                }
            }
            pass_order = pass_order.saturating_add(1);
        }
    }
    states.sort_by(|left, right| {
        left.first_write_order
            .cmp(&right.first_write_order)
            .then_with(|| left.target.to_u32().cmp(&right.target.to_u32()))
            .then_with(|| left.graph_index.cmp(&right.graph_index))
            .then_with(|| left.target_name.0.cmp(&right.target_name.0))
    });
    let mut physical_targets = Vec::<PhysicalTargetState>::new();
    states
        .into_iter()
        .map(|state| {
            let compatibility = target_allocation_compatibility(storage, state);
            let reusable = target_is_transient_aliasable(state.target);
            let slot = physical_targets
                .iter()
                .position(|physical| {
                    reusable
                        && physical.reusable
                        && physical.last_use_order < state.first_write_order
                        && target_allocations_are_compatible(physical.compatibility, compatibility)
                })
                .unwrap_or_else(|| {
                    physical_targets.push(PhysicalTargetState {
                        last_use_order: 0,
                        compatibility,
                        reusable,
                    });
                    physical_targets.len() - 1
                });
            physical_targets[slot].last_use_order = state.last_use_order;
            SceneRenderingDeviceTargetAllocation {
                graph_index: state.graph_index,
                target: state.target,
                target_name: state.target_name,
                first_write_pass_id: state.first_write_pass_id,
                last_use_pass_id: state.last_use_pass_id,
                physical_slot: slot as u32,
                width: compatibility.target_width,
                height: compatibility.target_height,
                extent_domain: compatibility.extent_domain,
            }
        })
        .collect()
}

fn target_allocations_are_compatible(
    left: TargetAllocationCompatibility,
    right: TargetAllocationCompatibility,
) -> bool {
    left.format == right.format
        && left.width_divisor_milli == right.width_divisor_milli
        && left.height_divisor_milli == right.height_divisor_milli
        && left.extent_domain == right.extent_domain
        && (left.target_width, left.target_height) == (right.target_width, right.target_height)
}

fn target_is_transient_aliasable(target: SceneRenderTargetKind) -> bool {
    matches!(
        target,
        SceneRenderTargetKind::ImageLocalMain
            | SceneRenderTargetKind::ImageLocalSub
            | SceneRenderTargetKind::Temporary
    )
}

fn target_allocation_compatibility(
    storage: &SceneStorage,
    state: TargetAllocationState,
) -> TargetAllocationCompatibility {
    let authored_extent = authored_texture_space_target_extent(
        storage,
        state.graph_index,
        state.target,
        state.target_name,
    )
    .unwrap_or([0, 0]);
    let image_target = image_target(storage, state.target, state.target_name);
    let extent_domain =
        target_extent_domain(storage, state.graph_index, state.target, state.target_name);
    image_target
        .map(|target| TargetAllocationCompatibility {
            format: target.format,
            width_divisor_milli: target.width_divisor_milli.max(1),
            height_divisor_milli: target.height_divisor_milli.max(1),
            target_width: authored_extent[0],
            target_height: authored_extent[1],
            extent_domain,
        })
        .unwrap_or(TargetAllocationCompatibility {
            format: SceneStringId::NONE,
            width_divisor_milli: 1_000,
            height_divisor_milli: 1_000,
            target_width: authored_extent[0],
            target_height: authored_extent[1],
            extent_domain,
        })
}

fn record_target_write(
    states: &mut Vec<TargetAllocationState>,
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
    pass_id: u32,
    pass_order: u32,
) {
    if let Some(state) = states.iter_mut().find(|state| {
        state.graph_index == graph_index
            && state.target == target
            && state.target_name == target_name
    }) {
        state.first_write_pass_id = state.first_write_pass_id.min(pass_id);
        state.last_use_pass_id = state.last_use_pass_id.max(pass_id);
        state.first_write_order = state.first_write_order.min(pass_order);
        state.last_use_order = state.last_use_order.max(pass_order);
    } else {
        states.push(TargetAllocationState {
            graph_index,
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
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
    pass_id: u32,
    pass_order: u32,
) {
    if let Some(state) = states.iter_mut().find(|state| {
        state.graph_index == graph_index
            && state.target == target
            && state.target_name == target_name
    }) {
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
        | SceneRenderBindingKind::EffectTarget
        | SceneRenderBindingKind::PreviousGraphTarget => Some((binding.target, binding.name)),
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
mod camera_tests;
#[cfg(test)]
mod projection_tests;
#[cfg(test)]
mod tests;
