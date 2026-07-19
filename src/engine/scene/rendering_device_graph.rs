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

mod draw_support;
mod effect_batch;

use draw_support::{
    material_sampled_bindings, pass_draw_material, render_texture_producer_graphs,
    rows_from_column_major, sampled_binding, skinning_palette_count, skinning_palette_start,
};

pub use effect_batch::{
    SceneRenderingDeviceEffectBatch, SceneRenderingDeviceEffectBatchFamily,
    SceneRenderingDeviceEffectBatchInstance,
};

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
        let render_texture_producers = render_texture_producer_graphs(storage);

        for (graph_index, graph) in storage.render_graphs().iter().enumerate() {
            for (local_pass_index, pass) in storage.render_graph_passes(graph).iter().enumerate() {
                let pass_node_index = pass_nodes.len() as u32;
                let mesh_draw_start = mesh_draws.len() as u32;
                let pass_object_state = visible_pass_object(
                    semantic_frame,
                    pass,
                    render_texture_producers.contains(&(graph_index as u32))
                        && pass.target != SceneRenderTargetKind::SceneColor,
                );
                if let (true, Some(pass_object_state)) =
                    (pass_draws_object_mesh(storage, pass), pass_object_state)
                {
                    let resolved_object_index = pass_object_state.object_index;
                    for (mesh_index, mesh) in storage.meshes().iter().enumerate() {
                        if mesh.object == pass.object {
                            let Some((index_start, index_count)) =
                                pass_mesh_index_range(storage, pass, mesh_index as u32, mesh)
                            else {
                                continue;
                            };
                            mesh_draws.push(SceneRenderingDeviceMeshDraw {
                                primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
                                shader_key: pass.shader_key,
                                mesh_index: mesh_index as u32,
                                resolved_object_index,
                                clip_transform: scene_clip_transform(
                                    storage.project(),
                                    pass_object_state.render_world_matrix,
                                ),
                                authored_source_extent: authored_source_extent(
                                    storage,
                                    pass.object,
                                ),
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
                        pass,
                        pass_object_state,
                        primitive,
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
                    },
                ));
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

    pub fn fullscreen_utility_draw_count(&self) -> usize {
        self.mesh_draws
            .iter()
            .filter(|draw| draw.primitive == SceneRenderingDeviceDrawPrimitive::FullscreenTriangle)
            .count()
    }

    pub fn uses_fullscreen_utility_primitive(&self) -> bool {
        self.fullscreen_utility_draw_count() != 0
    }

    pub fn effect_batch_atlas_tile(
        &self,
        graph_index: u32,
        target: SceneRenderTargetKind,
        target_name: SceneStringId,
    ) -> Option<u32> {
        self.effect_batch_instances
            .iter()
            .find(|instance| {
                instance.graph_index == graph_index
                    && instance.target == target
                    && instance.target_name == target_name
            })
            .map(|instance| instance.atlas_tile)
    }

    pub fn effect_batch_field_count(&self, physical_slot: u32) -> u32 {
        self.effect_batches
            .iter()
            .find(|batch| batch.physical_slot == physical_slot)
            .map_or(1, |batch| batch.layer_count.max(1))
    }

    pub fn effect_batch_atlas_grid(&self, physical_slot: u32) -> [u32; 2] {
        self.effect_batches
            .iter()
            .find(|batch| batch.physical_slot == physical_slot)
            .map_or([1, 1], |batch| {
                [batch.atlas_columns.max(1), batch.atlas_rows.max(1)]
            })
    }

    pub fn effect_batch_field_extent_divisor(&self, physical_slot: u32) -> u32 {
        self.effect_batches
            .iter()
            .find(|batch| batch.physical_slot == physical_slot)
            .map_or(1, |batch| batch.field_extent_divisor.max(1))
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
    pub graph_index: u32,
    pub target: SceneRenderTargetKind,
    pub target_name: SceneStringId,
    pub first_write_pass_id: u32,
    pub last_use_pass_id: u32,
    pub physical_slot: u32,
    /// Non-zero dimensions select a graph-local authored-texture target.
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceSampledBinding {
    pub pass_node_index: u32,
    pub graph_index: u32,
    pub mesh_draw_start: u32,
    pub mesh_draw_count: u32,
    pub kind: SceneRenderBindingKind,
    pub slot: u32,
    pub target: SceneRenderTargetKind,
    pub target_name: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceMaterialSampledBinding {
    pub draw_index: u32,
    pub slot: u32,
    pub resource: SceneResourceId,
}

impl SceneRenderingDeviceSampledBinding {
    pub fn logical_target(self) -> Option<(u32, SceneRenderTargetKind, SceneStringId)> {
        match self.kind {
            SceneRenderBindingKind::PreviousGraphTarget
            | SceneRenderBindingKind::GraphTarget
            | SceneRenderBindingKind::NamedFboBind
            | SceneRenderBindingKind::EffectTarget => {
                Some((self.graph_index, self.target, self.target_name))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceMeshDraw {
    pub primitive: SceneRenderingDeviceDrawPrimitive,
    /// Shader selected by the actual render pass. Synthetic composites can differ from
    /// the authored material's first pass.
    pub shader_key: SceneStringId,
    pub mesh_index: u32,
    pub resolved_object_index: u32,
    pub clip_transform: [[f32; 4]; 4],
    pub authored_source_extent: [f32; 2],
    pub skinning_palette_start: u32,
    pub skinning_palette_count: u32,
    pub resolved_color: SceneVec3,
    pub resolved_alpha: f32,
    pub apply_resolved_visual: bool,
    /// Layer in a scene-level GPU effect batch, or `INVALID_OBJECT_ID` for ordinary draws.
    pub effect_batch_atlas_tile: u32,
    /// Column/row count of the scene-level 2D effect atlas; `[0, 0]` means no batch.
    pub effect_batch_atlas_grid: [u32; 2],
    pub object: SceneObjectHandle,
    pub material: SceneMaterialHandle,
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
    pub instance_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneRenderingDeviceDrawPrimitive {
    ObjectMesh,
    FullscreenTriangle,
    ObjectUvSupportQuad,
    ParticleBillboard,
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
    pub alpha: f32,
}

fn pass_draws_object_mesh(storage: &SceneStorage, pass: &SceneRenderPassRecord) -> bool {
    pass.object.0 != INVALID_OBJECT_ID
        && (pass.role == SceneRenderPassKind::BaseMaterial
            || matches!(
                pass.role,
                SceneRenderPassKind::MeshVisiblePrefix
                    | SceneRenderPassKind::MeshClippingMask
                    | SceneRenderPassKind::MeshClippedTarget
                    | SceneRenderPassKind::MeshVisibleRemainder
            )
            || (pass.role == SceneRenderPassKind::SceneComposite
                && !storage.string(pass.shader_key).is_some_and(|key| {
                    key.eq_ignore_ascii_case("we/objectcomposite")
                        || key.eq_ignore_ascii_case("we/objectcomposite-screen-group")
                        || key.eq_ignore_ascii_case("we/flat-rounded-mask-composite")
                        || key.eq_ignore_ascii_case("we/flat-rounded-hsl-source")
                        || key.eq_ignore_ascii_case("we/flat-rounded-opacity-final")
                        || key.eq_ignore_ascii_case("we/framebuffer-water-final")
                        || key.eq_ignore_ascii_case("we/framebuffer-water-post-final")
                        || key.eq_ignore_ascii_case("we/framebuffer-lut16-final")
                        || key.eq_ignore_ascii_case("we/framebuffer-lut64-final")
                        || key.eq_ignore_ascii_case("we/framebuffer-lightning-screen-final")
                        || key.eq_ignore_ascii_case("we/framebuffer-lightning-add-final")
                })))
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
                && (!matches!(
                    role,
                    SceneMeshClippingSliceRole::MaskProducer
                        | SceneMeshClippingSliceRole::ClippedTarget
                ) || slice.subdraw == pass.pass_index)
        })
        .map(|slice| (slice.index_start, slice.index_count))
}

fn visible_pass_object<'frame>(
    semantic_frame: &'frame ResolvedSemanticFrame,
    pass: &SceneRenderPassRecord,
    allow_hidden_render_texture: bool,
) -> Option<&'frame ResolvedObjectState> {
    semantic_frame
        .object(pass.object)
        .filter(|object| object.resolved_visible || allow_hidden_render_texture)
}

fn pass_utility_primitive(
    storage: &SceneStorage,
    pass: &SceneRenderPassRecord,
    pass_object_state: Option<&ResolvedObjectState>,
) -> Option<SceneRenderingDeviceDrawPrimitive> {
    if pass.role == SceneRenderPassKind::Particle {
        if pass_object_state.is_none() {
            return None;
        }
        return storage
            .particle_for_object(pass.object)
            .filter(|particle| {
                matches!(
                    particle.simulation,
                    SceneParticleSimulationKind::FallingLeaves
                        | SceneParticleSimulationKind::AmbientSparkles
                        | SceneParticleSimulationKind::FloralOscillation
                ) && particle.max_count != 0
            })
            .map(|_| SceneRenderingDeviceDrawPrimitive::ParticleBillboard);
    }
    if !matches!(
        pass.role,
        SceneRenderPassKind::BaseMaterial
            | SceneRenderPassKind::EffectMaterial
            | SceneRenderPassKind::ColorBlendPassthrough
            | SceneRenderPassKind::SceneComposite
    ) {
        return None;
    }
    if pass.object.0 != INVALID_OBJECT_ID && pass_object_state.is_none() {
        return None;
    }
    storage
        .string(pass.shader_key)
        .and_then(shader_utility_primitive)
}

fn shader_utility_primitive(shader_key: &str) -> Option<SceneRenderingDeviceDrawPrimitive> {
    let key = shader_key.to_ascii_lowercase();
    if matches!(
        key.as_str(),
        "we/flat-rounded-mask-composite"
            | "we/flat-rounded-opacity-final"
            | "we/flat-rounded-hsl-source"
    ) {
        return Some(SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad);
    }
    if matches!(
        key.as_str(),
        "we/framebuffer-water-final"
            | "we/framebuffer-water-post-final"
            | "we/framebuffer-lut16-final"
            | "we/framebuffer-lut64-final"
            | "we/framebuffer-lightning-screen-final"
            | "we/framebuffer-lightning-add-final"
    ) {
        return Some(SceneRenderingDeviceDrawPrimitive::FullscreenTriangle);
    }
    (key.starts_with("effects/")
        || key.starts_with("workshop/")
        || key == "we/image-ripple-source"
        || key.starts_with("we/effect-waterwaves-direct")
        || key.starts_with("we/waterwaves-uv-")
        || key == "minimalalpha"
        || key.starts_with("minimalalpha__")
        || key == "passthrough"
        || key.starts_with("passthrough__")
        || key.contains("composelayer")
        || key.contains("objectcomposite")
        || key.contains("utilitycomposite")
        || key.contains("flattexture"))
    .then_some(SceneRenderingDeviceDrawPrimitive::FullscreenTriangle)
}

fn utility_primitive_draw(
    storage: &SceneStorage,
    pass: &SceneRenderPassRecord,
    pass_object_state: Option<&ResolvedObjectState>,
    primitive: SceneRenderingDeviceDrawPrimitive,
) -> SceneRenderingDeviceMeshDraw {
    let vertex_count = match primitive {
        SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad => 6,
        SceneRenderingDeviceDrawPrimitive::ParticleBillboard => 4,
        SceneRenderingDeviceDrawPrimitive::ObjectMesh
        | SceneRenderingDeviceDrawPrimitive::FullscreenTriangle => 3,
    };
    SceneRenderingDeviceMeshDraw {
        primitive,
        shader_key: pass.shader_key,
        mesh_index: INVALID_OBJECT_ID,
        resolved_object_index: pass_object_state
            .map(|object| object.object_index)
            .unwrap_or(INVALID_OBJECT_ID),
        clip_transform: pass_object_state.map_or_else(identity_clip_transform, |object| {
            scene_clip_transform(storage.project(), object.render_world_matrix)
        }),
        authored_source_extent: authored_source_extent(storage, pass.object),
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
        object: pass.object,
        material: pass.material,
        vertex_start: 0,
        vertex_count,
        index_start: 0,
        index_count: vertex_count,
        instance_count: if primitive == SceneRenderingDeviceDrawPrimitive::ParticleBillboard {
            storage
                .particle_for_object(pass.object)
                .map_or(0, procedural_particle_instance_capacity)
        } else {
            1
        },
    }
}

fn procedural_particle_instance_capacity(particle: &SceneParticleSystemRecord) -> u32 {
    let emitted_during_longest_lifetime = (particle.rate * particle.lifetime_max)
        .ceil()
        .clamp(0.0, u32::MAX as f32) as u32;
    particle.max_count.min(emitted_during_longest_lifetime)
}

fn particle_gpu_emitter_plans(
    particles: &[SceneParticleSystemRecord],
) -> Vec<SceneParticleGpuEmitterPlan> {
    particles
        .iter()
        .enumerate()
        .filter(|(_, particle)| particle.max_count != 0)
        .map(|(particle_index, particle)| SceneParticleGpuEmitterPlan {
            object: particle.object,
            particle_index: particle_index as u32,
            profile: match particle.simulation {
                SceneParticleSimulationKind::FallingLeaves
                | SceneParticleSimulationKind::AmbientSparkles
                | SceneParticleSimulationKind::FloralOscillation => {
                    SceneParticleGpuProfile::AnalyticBillboard
                }
                SceneParticleSimulationKind::Unsupported => SceneParticleGpuProfile::RetainedState,
            },
            state_index: particle_index as u32,
            capacity: procedural_particle_instance_capacity(particle),
            indirect_draw_index: particle_index as u32,
        })
        .collect()
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

fn source_texture_for_material<'storage>(
    storage: &'storage SceneStorage,
    material: SceneMaterialHandle,
) -> Option<&'storage SceneTextureRecord> {
    let material = storage.material(material)?;
    storage
        .material_passes(material)
        .iter()
        .flat_map(|pass| storage.material_pass_textures(pass))
        .find(|binding| binding.slot == 0)
        .and_then(|binding| storage.texture(binding.resource))
}

pub(crate) fn scene_clip_transform(
    project: &SceneProjectRecord,
    world_matrix: [f32; 16],
) -> [[f32; 4]; 4] {
    let width = project.logical_width.max(1) as f32;
    let height = project.logical_height.max(1) as f32;
    [
        [
            2.0 * world_matrix[0] / width - world_matrix[3],
            2.0 * world_matrix[4] / width - world_matrix[7],
            2.0 * world_matrix[8] / width - world_matrix[11],
            2.0 * world_matrix[12] / width - world_matrix[15],
        ],
        [
            -2.0 * world_matrix[1] / height + world_matrix[3],
            -2.0 * world_matrix[5] / height + world_matrix[7],
            -2.0 * world_matrix[9] / height + world_matrix[11],
            -2.0 * world_matrix[13] / height + world_matrix[15],
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
    authored_width: u32,
    authored_height: u32,
    authored_texture_space: bool,
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
                width: compatibility.authored_width,
                height: compatibility.authored_height,
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
        && left.authored_texture_space == right.authored_texture_space
        && (left.authored_width, left.authored_height)
            == (right.authored_width, right.authored_height)
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
    let image_layer_composite = state.target == SceneRenderTargetKind::FirstClassEffectTarget
        && storage
            .string(state.target_name)
            .is_some_and(|name| name.starts_with("_rt_imageLayerComposite_"));
    let authored_texture_space = matches!(
        state.target,
        SceneRenderTargetKind::ImageLocalMain | SceneRenderTargetKind::ImageLocalSub
    ) || image_layer_composite;
    let authored_extent = if image_layer_composite {
        storage
            .render_graphs()
            .get(state.graph_index as usize)
            .map(|graph| authored_source_extent(storage, graph.object))
            .filter(|[width, height]| {
                width.is_finite() && height.is_finite() && *width >= 1.0 && *height >= 1.0
            })
            .map(|[width, height]| [width.round() as u32, height.round() as u32])
    } else if authored_texture_space {
        authored_graph_extent(storage, state.graph_index)
    } else {
        None
    }
    .unwrap_or([0, 0]);
    storage
        .document()
        .image_targets
        .iter()
        .find(|target| target.role == state.target && target.name == state.target_name)
        .map(|target| TargetAllocationCompatibility {
            format: target.format,
            width_divisor_milli: target.width_divisor_milli.max(1),
            height_divisor_milli: target.height_divisor_milli.max(1),
            authored_width: authored_extent[0],
            authored_height: authored_extent[1],
            authored_texture_space: authored_extent != [0, 0],
        })
        .unwrap_or(TargetAllocationCompatibility {
            format: SceneStringId::NONE,
            width_divisor_milli: 1_000,
            height_divisor_milli: 1_000,
            authored_width: authored_extent[0],
            authored_height: authored_extent[1],
            authored_texture_space: authored_extent != [0, 0],
        })
}

fn authored_graph_extent(storage: &SceneStorage, graph_index: u32) -> Option<[u32; 2]> {
    let graph = storage.render_graphs().get(graph_index as usize)?;
    let [width, height] = authored_source_extent(storage, graph.object);
    (width.is_finite() && height.is_finite() && width >= 1.0 && height >= 1.0)
        .then_some([width.round() as u32, height.round() as u32])
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
mod tests;
