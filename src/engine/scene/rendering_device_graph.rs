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

mod effect_batch;

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
                let pass_object_state = visible_pass_object(semantic_frame, pass);
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
                                mesh_index: mesh_index as u32,
                                resolved_object_index,
                                clip_transform: scene_clip_transform(
                                    storage.project(),
                                    pass_object_state.world_matrix,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneRenderingDeviceDrawPrimitive {
    ObjectMesh,
    FullscreenTriangle,
    ObjectUvSupportQuad,
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
                        || key.eq_ignore_ascii_case("we/flat-rounded-mask-composite")
                        || key.eq_ignore_ascii_case("we/flat-rounded-opacity-final")
                        || key.eq_ignore_ascii_case("we/framebuffer-water-final")
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
) -> Option<&'frame ResolvedObjectState> {
    semantic_frame
        .object(pass.object)
        .filter(|object| object.resolved_visible)
}

fn pass_utility_primitive(
    storage: &SceneStorage,
    pass: &SceneRenderPassRecord,
    pass_object_state: Option<&ResolvedObjectState>,
) -> Option<SceneRenderingDeviceDrawPrimitive> {
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
        "we/flat-rounded-mask-composite" | "we/flat-rounded-opacity-final"
    ) {
        return Some(SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad);
    }
    if key == "we/framebuffer-water-final" {
        return Some(SceneRenderingDeviceDrawPrimitive::FullscreenTriangle);
    }
    (key.starts_with("effects/")
        || key.starts_with("workshop/")
        || key == "we/image-ripple-source"
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
        SceneRenderingDeviceDrawPrimitive::ObjectMesh
        | SceneRenderingDeviceDrawPrimitive::FullscreenTriangle => 3,
    };
    SceneRenderingDeviceMeshDraw {
        primitive,
        mesh_index: INVALID_OBJECT_ID,
        resolved_object_index: pass_object_state
            .map(|object| object.object_index)
            .unwrap_or(INVALID_OBJECT_ID),
        clip_transform: pass_object_state.map_or_else(identity_clip_transform, |object| {
            scene_clip_transform(storage.project(), object.world_matrix)
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
    }
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

fn material_sampled_bindings(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
) -> Vec<SceneRenderingDeviceMaterialSampledBinding> {
    let mut bindings = Vec::new();
    for (draw_index, draw) in draws.iter().enumerate() {
        let Some(material) = storage.material(draw.material) else {
            continue;
        };
        let Some(pass) = storage.material_passes(material).first() else {
            continue;
        };
        bindings.extend(
            storage
                .material_pass_textures(pass)
                .iter()
                .filter(|texture| storage.texture(texture.resource).is_some())
                .map(|texture| SceneRenderingDeviceMaterialSampledBinding {
                    draw_index: draw_index as u32,
                    slot: texture.slot,
                    resource: texture.resource,
                }),
        );
    }
    bindings
}

fn sampled_binding(
    graph_index: u32,
    pass_node_index: u32,
    mesh_draw_start: u32,
    mesh_draw_count: u32,
    binding: &SceneRenderBindingRecord,
) -> Option<SceneRenderingDeviceSampledBinding> {
    matches!(
        binding.kind,
        SceneRenderBindingKind::SourceTexture
            | SceneRenderBindingKind::TextureSlot
            | SceneRenderBindingKind::AlphaTextureSlot
            | SceneRenderBindingKind::PreviousGraphTarget
            | SceneRenderBindingKind::GraphTarget
            | SceneRenderBindingKind::NamedFboBind
            | SceneRenderBindingKind::EffectTarget
            | SceneRenderBindingKind::VideoFrame
    )
    .then_some(SceneRenderingDeviceSampledBinding {
        pass_node_index,
        graph_index,
        mesh_draw_start,
        mesh_draw_count,
        kind: binding.kind,
        slot: binding.slot,
        target: binding.target,
        target_name: binding.name,
    })
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
    let authored_extent = matches!(
        state.target,
        SceneRenderTargetKind::ImageLocalMain | SceneRenderTargetKind::ImageLocalSub
    )
    .then(|| puppet_effect_graph_extent(storage, state.graph_index))
    .flatten()
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

fn puppet_effect_graph_extent(storage: &SceneStorage, graph_index: u32) -> Option<[u32; 2]> {
    let graph = storage.render_graphs().get(graph_index as usize)?;
    let uses_authored_texture_space = storage.render_graph_passes(graph).iter().any(|pass| {
        storage.string(pass.shader_key).is_some_and(|shader| {
            shader.eq_ignore_ascii_case("we/image-effect-source")
                || shader.eq_ignore_ascii_case("we/puppet-effect-source")
                || shader.eq_ignore_ascii_case("we/image-ripple-source")
        })
    });
    if !uses_authored_texture_space {
        return None;
    }
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
mod tests {
    use super::*;
    use crate::engine::scene::{RenderingServer, SceneStorage};
    use crate::engine::scene::{
        SceneBinaryDocument, SceneMaterialHandle, SceneMaterialRecord, SceneMeshRecord,
        SceneMeshVertexRecord, SceneObjectHandle, SceneObjectKind, SceneObjectRecord,
        ScenePuppetBoneRecord, ScenePuppetRecord, SceneRenderBindingKind, SceneRenderBindingRecord,
        SceneRenderGraphRecord, SceneRenderPassRecord, SceneResourceId, SceneResourceKind,
        SceneResourceRecord, SceneShaderContractRecord, SceneStringId, SceneTextureFormat,
        SceneTextureRecord, SceneVec3,
    };

    #[test]
    fn authored_targets_only_alias_images_with_identical_extents() {
        let base = TargetAllocationCompatibility {
            format: SceneStringId(3),
            width_divisor_milli: 1_000,
            height_divisor_milli: 1_000,
            authored_width: 1_571,
            authored_height: 2_621,
            authored_texture_space: true,
        };
        assert!(target_allocations_are_compatible(base, base));
        assert!(!target_allocations_are_compatible(
            base,
            TargetAllocationCompatibility {
                authored_width: 2_318,
                authored_height: 1_794,
                ..base
            }
        ));
    }

    #[test]
    fn scene_projection_maps_authored_bounds_to_vulkan_ndc() {
        let mut project = SceneBinaryDocument::default().project;
        project.logical_width = 3840;
        project.logical_height = 2160;
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];

        let transform = scene_clip_transform(&project, identity);

        assert_eq!(transform[0], [2.0 / 3840.0, 0.0, 0.0, -1.0]);
        assert_eq!(transform[1], [0.0, -2.0 / 2160.0, 0.0, 1.0]);
        assert_eq!(transform[3], [0.0, 0.0, 0.0, 1.0]);
    }

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
                    color: SceneVec3 {
                        x: 1.0,
                        y: 1.0,
                        z: 1.0,
                    },
                    alpha: 1.0,
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
                    color: SceneVec3 {
                        x: 1.0,
                        y: 1.0,
                        z: 1.0,
                    },
                    alpha: 1.0,
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
                    blend_indices: [0; 4],
                    blend_weights: [0.0; 4],
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
                name: SceneStringId::NONE,
                simulation_type: 0,
                parent_index: -1,
                local_bind_matrix: [
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ],
                simulation_json: SceneStringId::NONE,
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
                    scene_blend: SceneCompositeBlend::Alpha,
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
                    scene_blend: SceneCompositeBlend::Alpha,
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
                named_fbo_binding(SceneStringId(0), 0),
                named_fbo_binding(SceneStringId(1), 2),
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
        assert_eq!(graph.sampled_bindings.len(), 2);
        assert_eq!(graph.sampled_bindings[0].pass_node_index, 1);
        assert_eq!(graph.sampled_bindings[0].slot, 0);
        assert_eq!(graph.sampled_bindings[1].pass_node_index, 2);
        assert_eq!(graph.sampled_bindings[1].slot, 2);
        assert_eq!(
            graph.sampled_bindings[1].logical_target(),
            Some((0, SceneRenderTargetKind::NamedFbo, SceneStringId(1)))
        );
    }

    #[test]
    fn rendering_device_graph_does_not_alias_incompatible_effect_target_images() {
        let document = SceneBinaryDocument {
            strings: vec![
                "fbo_a".to_owned(),
                "fbo_b".to_owned(),
                "rgba8".to_owned(),
                "rgba16f".to_owned(),
            ],
            render_graphs: vec![SceneRenderGraphRecord {
                object: SceneObjectHandle(INVALID_OBJECT_ID),
                pass_start: 0,
                pass_count: 4,
                unsupported_start: 0,
                unsupported_count: 0,
            }],
            render_passes: vec![
                named_fbo_pass(1, 0, SceneStringId(0), 0, 0),
                scene_color_pass_reading_fbo(2, 0, 1),
                named_fbo_pass(3, 2, SceneStringId(1), 1, 0),
                scene_color_pass_reading_fbo(4, 1, 1),
            ],
            render_bindings: vec![
                named_fbo_binding(SceneStringId(0), 0),
                named_fbo_binding(SceneStringId(1), 0),
            ],
            image_targets: vec![
                SceneImageTargetRecord {
                    name: SceneStringId(0),
                    role: SceneRenderTargetKind::NamedFbo,
                    format: SceneStringId(2),
                    width_divisor_milli: 1_000,
                    height_divisor_milli: 1_000,
                },
                SceneImageTargetRecord {
                    name: SceneStringId(1),
                    role: SceneRenderTargetKind::NamedFbo,
                    format: SceneStringId(3),
                    width_divisor_milli: 2_000,
                    height_divisor_milli: 2_000,
                },
            ],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");
        let graph = RenderingServer::new(&storage).rendering_device_graph_plan();

        assert_eq!(graph.target_allocations.len(), 2);
        assert_eq!(graph.graph_physical_target_count, 2);
        assert_eq!(graph.graph_aliased_target_count, 0);
        assert_eq!(graph.target_allocations[0].physical_slot, 0);
        assert_eq!(graph.target_allocations[1].physical_slot, 1);
    }

    #[test]
    fn same_named_fbo_in_distinct_graphs_keeps_graph_scoped_identity() {
        let document = SceneBinaryDocument {
            strings: vec!["fbo_shared".to_owned()],
            render_graphs: vec![
                SceneRenderGraphRecord {
                    object: SceneObjectHandle(INVALID_OBJECT_ID),
                    pass_start: 0,
                    pass_count: 2,
                    unsupported_start: 0,
                    unsupported_count: 0,
                },
                SceneRenderGraphRecord {
                    object: SceneObjectHandle(INVALID_OBJECT_ID),
                    pass_start: 2,
                    pass_count: 2,
                    unsupported_start: 0,
                    unsupported_count: 0,
                },
            ],
            render_passes: vec![
                named_fbo_pass(1, 0, SceneStringId(0), 0, 0),
                scene_color_pass_reading_fbo(2, 0, 1),
                named_fbo_pass(1, 0, SceneStringId(0), 1, 0),
                scene_color_pass_reading_fbo(2, 1, 1),
            ],
            render_bindings: vec![
                named_fbo_binding(SceneStringId(0), 0),
                named_fbo_binding(SceneStringId(0), 0),
            ],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");
        let graph = RenderingServer::new(&storage).rendering_device_graph_plan();

        assert_eq!(graph.target_allocations.len(), 2);
        assert_eq!(graph.target_allocations[0].graph_index, 0);
        assert_eq!(graph.target_allocations[1].graph_index, 1);
        assert_ne!(
            graph.target_allocations[0].physical_slot,
            graph.target_allocations[1].physical_slot
        );
        assert_eq!(
            graph.sampled_bindings[0].logical_target(),
            Some((0, SceneRenderTargetKind::NamedFbo, SceneStringId(0)))
        );
        assert_eq!(
            graph.sampled_bindings[1].logical_target(),
            Some((1, SceneRenderTargetKind::NamedFbo, SceneStringId(0)))
        );
    }

    #[test]
    fn rendering_device_graph_uses_fullscreen_utility_for_effect_pass_without_object_mesh() {
        let document = SceneBinaryDocument {
            strings: vec!["effects/opacity__SLOTS_1".to_owned(), "fbo_a".to_owned()],
            render_graphs: vec![SceneRenderGraphRecord {
                object: SceneObjectHandle(INVALID_OBJECT_ID),
                pass_start: 0,
                pass_count: 1,
                unsupported_start: 0,
                unsupported_count: 0,
            }],
            render_passes: vec![SceneRenderPassRecord {
                id: 5,
                role: SceneRenderPassKind::EffectMaterial,
                object: SceneObjectHandle(INVALID_OBJECT_ID),
                material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                pass_index: 0,
                shader_key: SceneStringId(0),
                target: SceneRenderTargetKind::NamedFbo,
                target_name: SceneStringId(1),
                binding_start: 0,
                binding_count: 0,
                pipeline_blend: ScenePipelineBlend::Normal,
                scene_blend: SceneCompositeBlend::Alpha,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
            }],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");
        let graph = RenderingServer::new(&storage).rendering_device_graph_plan();

        assert_eq!(graph.pass_nodes[0].mesh_draw_start, 0);
        assert_eq!(graph.pass_nodes[0].mesh_draw_count, 1);
        assert_eq!(graph.mesh_draws.len(), 1);
        assert_eq!(
            graph.mesh_draws[0].primitive,
            SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
        );
        assert_eq!(graph.mesh_draws[0].vertex_count, 3);
        assert_eq!(graph.mesh_draws[0].index_count, 3);
        assert_eq!(
            graph.mesh_draws[0].clip_transform,
            identity_clip_transform()
        );
        assert_eq!(graph.mesh_draws[0].authored_source_extent, [0.0; 2]);
    }

    #[test]
    fn direct_flat_rounded_mask_uses_object_uv_support_quad() {
        assert_eq!(
            shader_utility_primitive("we/flat-rounded-mask-composite"),
            Some(SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad)
        );
    }

    #[test]
    fn object_effect_utility_retains_semantic_transform_and_authored_source_extent() {
        let mut project = SceneBinaryDocument::default().project;
        project.logical_width = 200;
        project.logical_height = 100;
        let source_resource = SceneResourceId(7);
        let document = SceneBinaryDocument {
            project,
            strings: vec!["effects/waterwaves__SLOTS_1".to_owned()],
            resources: vec![SceneResourceRecord {
                id: source_resource,
                kind: SceneResourceKind::TextureTex,
                path: SceneStringId::NONE,
                source: SceneStringId::NONE,
                payload_offset: 0,
                payload_len: 0,
            }],
            textures: vec![SceneTextureRecord {
                resource: source_resource,
                format: SceneTextureFormat::Bc7UnormBlock,
                source_runtime_format: 0,
                payload_format: 0,
                sampler_flags: 0,
                width: 1571,
                height: 2621,
                storage_width: 1572,
                storage_height: 2624,
                mip_start: 0,
                mip_count: 0,
                texv_tag: SceneStringId::NONE,
                texb_tag: SceneStringId::NONE,
                payload_offset: 0,
                payload_len: 0,
                alpha_coverage_rows: [u32::MAX;
                    crate::engine::scene::SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
            }],
            objects: vec![SceneObjectRecord {
                id: SceneObjectHandle(0),
                we_id: 937,
                name: SceneStringId::NONE,
                kind: SceneObjectKind::Puppet,
                resource: SceneResourceId::NONE,
                material: SceneMaterialHandle(0),
                parent_we_id: INVALID_OBJECT_ID,
                attachment: SceneStringId::NONE,
                origin: SceneVec3 {
                    x: 50.0,
                    y: 20.0,
                    z: 0.0,
                },
                angles: SceneVec3::default(),
                scale: SceneVec3::ONE,
                color: SceneVec3::ONE,
                alpha: 1.0,
                visible: true,
                color_blend_mode: 0,
                sort_order: 0,
                effect_start: u32::MAX,
                effect_count: 0,
                render_graph: 0,
            }],
            materials: vec![SceneMaterialRecord {
                id: SceneMaterialHandle(0),
                resource: SceneResourceId::NONE,
                pass_start: 0,
                pass_count: 1,
            }],
            material_passes: vec![SceneMaterialPassRecord {
                material: SceneMaterialHandle(0),
                shader_key: SceneStringId(0),
                target: SceneStringId::NONE,
                texture_start: 0,
                texture_count: 1,
                constant_start: 0,
                constant_count: 0,
                pipeline_blend: ScenePipelineBlend::Normal,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
                alpha_writing: SceneStringId::NONE,
                clear_target: false,
            }],
            material_textures: vec![SceneMaterialTextureRecord {
                slot: 0,
                resource: source_resource,
                path: SceneStringId::NONE,
            }],
            render_graphs: vec![SceneRenderGraphRecord {
                object: SceneObjectHandle(0),
                pass_start: 0,
                pass_count: 1,
                unsupported_start: 0,
                unsupported_count: 0,
            }],
            render_passes: vec![SceneRenderPassRecord {
                id: 1,
                role: SceneRenderPassKind::EffectMaterial,
                object: SceneObjectHandle(0),
                material: SceneMaterialHandle(0),
                pass_index: 0,
                shader_key: SceneStringId(0),
                target: SceneRenderTargetKind::SceneColor,
                target_name: SceneStringId::NONE,
                binding_start: 0,
                binding_count: 0,
                pipeline_blend: ScenePipelineBlend::Normal,
                scene_blend: SceneCompositeBlend::Alpha,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
            }],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");
        let graph = RenderingServer::new(&storage).rendering_device_graph_plan();
        let draw = graph.mesh_draws.first().expect("fullscreen effect draw");

        assert_eq!(
            draw.primitive,
            SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
        );
        assert_eq!(draw.authored_source_extent, [1571.0, 2621.0]);
        assert_eq!(draw.clip_transform[0], [0.01, 0.0, 0.0, -0.5]);
        assert_eq!(draw.clip_transform[1], [0.0, -0.02, 0.0, 0.6]);
    }

    #[test]
    fn textureless_solid_layer_uses_authored_mesh_extent_for_local_effects() {
        let object = SceneObjectHandle(0);
        let document = SceneBinaryDocument {
            objects: vec![SceneObjectRecord {
                id: object,
                we_id: 1416,
                name: SceneStringId::NONE,
                kind: SceneObjectKind::Image,
                resource: SceneResourceId::NONE,
                material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                parent_we_id: INVALID_OBJECT_ID,
                attachment: SceneStringId::NONE,
                origin: SceneVec3::default(),
                angles: SceneVec3::default(),
                scale: SceneVec3::ONE,
                color: SceneVec3::ONE,
                alpha: 1.0,
                visible: true,
                color_blend_mode: 0,
                sort_order: 0,
                effect_start: 0,
                effect_count: 0,
                render_graph: INVALID_OBJECT_ID,
            }],
            meshes: vec![SceneMeshRecord {
                object,
                material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                vertex_start: 0,
                vertex_count: 0,
                index_start: 0,
                index_count: 0,
                width: 550.0,
                height: 3300.0,
                bounds_min: SceneVec3::default(),
                bounds_max: SceneVec3::default(),
            }],
            ..SceneBinaryDocument::default()
        };
        let storage = SceneStorage::from_document(document).expect("storage");

        assert_eq!(authored_source_extent(&storage, object), [550.0, 3300.0]);
    }

    #[test]
    fn only_base_material_pass_draws_authored_object_mesh() {
        let storage =
            SceneStorage::from_document(SceneBinaryDocument::default()).expect("empty storage");
        let mut pass = named_fbo_pass(5, 0, SceneStringId(1), 0, 0);
        pass.object = SceneObjectHandle(7);
        assert!(!pass_draws_object_mesh(&storage, &pass));

        pass.role = SceneRenderPassKind::ColorBlendPassthrough;
        assert!(!pass_draws_object_mesh(&storage, &pass));

        pass.role = SceneRenderPassKind::BaseMaterial;
        assert!(pass_draws_object_mesh(&storage, &pass));
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
            scene_blend: SceneCompositeBlend::Alpha,
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

    fn named_fbo_binding(name: SceneStringId, slot: u32) -> SceneRenderBindingRecord {
        SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::NamedFboBind,
            slot,
            target: SceneRenderTargetKind::NamedFbo,
            name,
        }
    }
}
