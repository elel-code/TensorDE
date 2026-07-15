//! Dynamic semantic-frame planning and host-visible GPU buffer updates.

use std::fmt::Debug;

use vulkanalia::prelude::v1_4::*;

use crate::engine::scene::rendering_device_graph::scene_clip_transform;
use crate::engine::scene::semantic_world::ResolvedSemanticFrame;
use crate::engine::scene::{
    SceneMaterialHandle, SceneObjectHandle, SceneRenderingDeviceDrawPrimitive,
    SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMaterialSampledBinding,
    SceneRenderingDeviceMeshDraw, SceneRenderingDevicePassNode, SceneRenderingDeviceSampledBinding,
    SceneRenderingDeviceTargetAllocation, SceneSemanticWorld, SceneStorage,
};
use crate::renderer::native_vulkan::{
    NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES, NativeVulkanVulkanaliaBuffer,
    native_vulkan_vulkanalia_write_host_buffer,
};

use super::composite_scissor::update_scene_composite_scissors;
use super::draw_recording::SceneGpuDrawCommand;
use super::draw_uniform::pack_scene_draw_uniforms;
use super::material_uniform::pack_scene_material_uniforms;
use super::scene_color_clear::{
    SceneGpuSceneColorClear, resolve_scene_color_attachment_clear,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SceneFrameBufferUpdate {
    pub transform_uniform_updated: bool,
    pub material_uniform_updated: bool,
    pub skinning_storage_updated: bool,
    pub scene_color_attachment_clear: Option<SceneGpuSceneColorClear>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SceneFrameTopology {
    graph: SceneRenderingDeviceGraphPlan,
    pass_nodes: Vec<SceneRenderingDevicePassNode>,
    target_allocations: Vec<SceneRenderingDeviceTargetAllocation>,
    sampled_bindings: Vec<SceneRenderingDeviceSampledBinding>,
    material_sampled_bindings: Vec<SceneRenderingDeviceMaterialSampledBinding>,
    draws: Vec<SceneFrameDrawTopology>,
    palettes: Vec<SceneFramePuppetPaletteTopology>,
    bones: Vec<SceneFramePuppetBoneTopology>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneFrameDrawTopology {
    primitive: SceneRenderingDeviceDrawPrimitive,
    mesh_index: u32,
    resolved_object_index: u32,
    skinning_palette_start: u32,
    skinning_palette_count: u32,
    object: SceneObjectHandle,
    material: SceneMaterialHandle,
    vertex_start: u32,
    vertex_count: u32,
    index_start: u32,
    index_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneFramePuppetPaletteTopology {
    object: SceneObjectHandle,
    puppet_index: u32,
    bone_matrix_start: u32,
    bone_matrix_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneFramePuppetBoneTopology {
    puppet_index: u32,
    bone_index: u32,
    parent_index: i32,
}

impl SceneFrameTopology {
    pub(super) fn from_graph(graph: &SceneRenderingDeviceGraphPlan) -> Self {
        Self {
            graph: graph.clone(),
            pass_nodes: graph.pass_nodes.clone(),
            target_allocations: graph.target_allocations.clone(),
            sampled_bindings: graph.sampled_bindings.clone(),
            material_sampled_bindings: graph.material_sampled_bindings.clone(),
            draws: graph.mesh_draws.iter().map(draw_topology).collect(),
            palettes: graph
                .puppet_bone_palettes
                .iter()
                .map(|palette| SceneFramePuppetPaletteTopology {
                    object: palette.object,
                    puppet_index: palette.puppet_index,
                    bone_matrix_start: palette.bone_matrix_start,
                    bone_matrix_count: palette.bone_matrix_count,
                })
                .collect(),
            bones: graph
                .puppet_bone_matrices
                .iter()
                .map(|bone| SceneFramePuppetBoneTopology {
                    puppet_index: bone.puppet_index,
                    bone_index: bone.bone_index,
                    parent_index: bone.parent_index,
                })
                .collect(),
        }
    }

    fn update_dynamic_graph(
        &mut self,
        storage: &SceneStorage,
        semantic_frame: &ResolvedSemanticFrame,
        scene_time_seconds: f32,
    ) -> Result<&SceneRenderingDeviceGraphPlan, String> {
        validate_dynamic_counts(&self.graph, semantic_frame, scene_time_seconds)?;
        for draw in &mut self.graph.mesh_draws {
            if draw.object.0 == crate::engine::scene::INVALID_OBJECT_ID {
                continue;
            }
            let object = semantic_frame.object(draw.object).ok_or_else(|| {
                format!(
                    "scene draw object {} disappeared at {scene_time_seconds:.6}s",
                    draw.object.0
                )
            })?;
            if !object.resolved_visible {
                return Err(format!(
                    "scene draw object {} became hidden at {scene_time_seconds:.6}s; live draw topology mutation is not supported",
                    draw.object.0
                ));
            }
            draw.resolved_object_index = object.object_index;
            draw.clip_transform = scene_clip_transform(storage.project(), object.world_matrix);
            draw.resolved_color = object.resolved_color;
            draw.resolved_alpha = object.resolved_alpha;
        }
        update_puppet_palettes(&mut self.graph, semantic_frame, scene_time_seconds)?;
        self.graph.resolved_object_count = semantic_frame.objects.len();
        self.graph.resolved_visible_object_count = semantic_frame.visible_object_count;
        self.graph.resolved_attachment_link_count = semantic_frame.attachment_links.len();
        self.graph.resolved_visible_effect_instance_count =
            semantic_frame.visible_effect_instance_count;
        self.graph.resolved_visible_effect_pass_count = semantic_frame.visible_effect_pass_count;
        self.graph.resolved_visible_effect_fbo_count = semantic_frame.visible_effect_fbo_count;
        Ok(&self.graph)
    }

    fn validate(
        &self,
        graph: &SceneRenderingDeviceGraphPlan,
        scene_time_seconds: f32,
    ) -> Result<(), String> {
        validate_topology_slice(
            "render-pass",
            &self.pass_nodes,
            &graph.pass_nodes,
            scene_time_seconds,
        )?;
        validate_topology_slice(
            "render-target allocation",
            &self.target_allocations,
            &graph.target_allocations,
            scene_time_seconds,
        )?;
        validate_topology_slice(
            "sampled binding",
            &self.sampled_bindings,
            &graph.sampled_bindings,
            scene_time_seconds,
        )?;
        validate_topology_slice(
            "material sampled binding",
            &self.material_sampled_bindings,
            &graph.material_sampled_bindings,
            scene_time_seconds,
        )?;
        let draws = graph
            .mesh_draws
            .iter()
            .map(draw_topology)
            .collect::<Vec<_>>();
        validate_topology_slice("mesh draw", &self.draws, &draws, scene_time_seconds)?;
        let palettes = graph
            .puppet_bone_palettes
            .iter()
            .map(|palette| SceneFramePuppetPaletteTopology {
                object: palette.object,
                puppet_index: palette.puppet_index,
                bone_matrix_start: palette.bone_matrix_start,
                bone_matrix_count: palette.bone_matrix_count,
            })
            .collect::<Vec<_>>();
        validate_topology_slice(
            "puppet palette",
            &self.palettes,
            &palettes,
            scene_time_seconds,
        )?;
        let bones = graph
            .puppet_bone_matrices
            .iter()
            .map(|bone| SceneFramePuppetBoneTopology {
                puppet_index: bone.puppet_index,
                bone_index: bone.bone_index,
                parent_index: bone.parent_index,
            })
            .collect::<Vec<_>>();
        validate_topology_slice("puppet bone", &self.bones, &bones, scene_time_seconds)
    }
}

fn validate_dynamic_counts(
    graph: &SceneRenderingDeviceGraphPlan,
    frame: &ResolvedSemanticFrame,
    scene_time_seconds: f32,
) -> Result<(), String> {
    let expected = [
        graph.resolved_object_count,
        graph.resolved_visible_object_count,
        graph.resolved_attachment_link_count,
        graph.resolved_visible_effect_instance_count,
        graph.resolved_visible_effect_pass_count,
        graph.resolved_visible_effect_fbo_count,
    ];
    let actual = [
        frame.objects.len(),
        frame.visible_object_count,
        frame.attachment_links.len(),
        frame.visible_effect_instance_count,
        frame.visible_effect_pass_count,
        frame.visible_effect_fbo_count,
    ];
    if expected != actual {
        return Err(format!(
            "scene semantic topology changed at {scene_time_seconds:.6}s: setup {expected:?}, frame {actual:?}; live topology mutation is not supported"
        ));
    }
    Ok(())
}

fn update_puppet_palettes(
    graph: &mut SceneRenderingDeviceGraphPlan,
    frame: &ResolvedSemanticFrame,
    scene_time_seconds: f32,
) -> Result<(), String> {
    if graph.puppet_bone_palettes.len() != frame.puppet_bone_palettes.len()
        || graph.puppet_bone_matrices.len() != frame.puppet_bone_matrices.len()
    {
        return Err(format!(
            "scene puppet topology changed at {scene_time_seconds:.6}s: setup palettes/bones {}/{}, frame {}/{}",
            graph.puppet_bone_palettes.len(),
            graph.puppet_bone_matrices.len(),
            frame.puppet_bone_palettes.len(),
            frame.puppet_bone_matrices.len()
        ));
    }
    for (target, source) in graph
        .puppet_bone_palettes
        .iter_mut()
        .zip(&frame.puppet_bone_palettes)
    {
        if (
            target.object,
            target.puppet_index,
            target.bone_matrix_start,
            target.bone_matrix_count,
        ) != (
            source.object,
            source.puppet_index,
            source.bone_start,
            source.bone_count,
        ) {
            return Err(format!(
                "scene puppet palette topology changed at {scene_time_seconds:.6}s for object {}",
                source.object.0
            ));
        }
        target.resolved_visible = source.resolved_visible;
    }
    for (target, source) in graph
        .puppet_bone_matrices
        .iter_mut()
        .zip(&frame.puppet_bone_matrices)
    {
        if (target.puppet_index, target.bone_index, target.parent_index)
            != (source.puppet_index, source.bone_index, source.parent_index)
        {
            return Err(format!(
                "scene puppet bone topology changed at {scene_time_seconds:.6}s for puppet {} bone {}",
                source.puppet_index, source.bone_index
            ));
        }
        target.matrix = rows_from_column_major(source.matrix);
        target.alpha = source.alpha;
    }
    Ok(())
}

fn rows_from_column_major(matrix: [f32; 16]) -> [[f32; 4]; 4] {
    [
        [matrix[0], matrix[4], matrix[8], matrix[12]],
        [matrix[1], matrix[5], matrix[9], matrix[13]],
        [matrix[2], matrix[6], matrix[10], matrix[14]],
        [matrix[3], matrix[7], matrix[11], matrix[15]],
    ]
}

pub(super) fn write_scene_frame_buffers(
    device: &Device,
    storage: &SceneStorage,
    semantic_world: &SceneSemanticWorld<'_>,
    topology: &mut SceneFrameTopology,
    draw_commands: &mut [SceneGpuDrawCommand],
    transform_buffer: &NativeVulkanVulkanaliaBuffer,
    material_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    skinning_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    dynamic_effect_uniforms: bool,
    graph_execution_order: &[u32],
    scene_color_attachment_clear_enabled: bool,
    scene_time_seconds: f32,
    output_extent: [u32; 2],
) -> Result<SceneFrameBufferUpdate, String> {
    let semantic_frame = semantic_world
        .resolve_frame_at(scene_time_seconds)
        .map_err(|err| {
            format!(
                "resolve scene semantic frame at {scene_time_seconds:.6}s for Vulkan buffer update: {err}"
            )
        })?;
    let graph = topology.update_dynamic_graph(storage, &semantic_frame, scene_time_seconds)?;

    let transform_payload = pack_scene_draw_uniforms(
        storage,
        &graph.mesh_draws,
        scene_time_seconds,
        output_extent,
    );
    write_exact_frame_payload(device, transform_buffer, &transform_payload)?;

    let material_uniform_updated = if dynamic_effect_uniforms {
        let material_buffer = material_buffer.ok_or_else(|| {
            "scene has dynamic effect uniforms but no material uniform buffer".to_owned()
        })?;
        let material_payload =
            pack_scene_material_uniforms(storage, &graph.mesh_draws, scene_time_seconds);
        write_exact_frame_payload(device, material_buffer, &material_payload)?;
        true
    } else {
        false
    };

    let skinning_storage_updated = if let Some(skinning_buffer) = skinning_buffer {
        let skinning_payload = pack_scene_skinning_palette(&graph);
        write_exact_frame_payload(device, skinning_buffer, &skinning_payload)?;
        true
    } else {
        false
    };

    update_scene_composite_scissors(storage, graph, output_extent, draw_commands)?;
    let scene_color_attachment_clear = resolve_scene_color_attachment_clear(
        storage,
        graph,
        graph_execution_order,
        output_extent,
        scene_color_attachment_clear_enabled,
    );

    Ok(SceneFrameBufferUpdate {
        transform_uniform_updated: true,
        material_uniform_updated,
        skinning_storage_updated,
        scene_color_attachment_clear,
    })
}

pub(super) fn pack_scene_skinning_palette(graph: &SceneRenderingDeviceGraphPlan) -> Vec<u8> {
    let mut payload = Vec::with_capacity(
        graph
            .puppet_bone_matrices
            .len()
            .saturating_add(1)
            .saturating_mul(NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES),
    );
    push_scene_puppet_bone(
        &mut payload,
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        1.0,
    );
    for bone in &graph.puppet_bone_matrices {
        push_scene_puppet_bone(&mut payload, bone.matrix, bone.alpha);
    }
    payload
}

fn draw_topology(draw: &SceneRenderingDeviceMeshDraw) -> SceneFrameDrawTopology {
    SceneFrameDrawTopology {
        primitive: draw.primitive,
        mesh_index: draw.mesh_index,
        resolved_object_index: draw.resolved_object_index,
        skinning_palette_start: draw.skinning_palette_start,
        skinning_palette_count: draw.skinning_palette_count,
        object: draw.object,
        material: draw.material,
        vertex_start: draw.vertex_start,
        vertex_count: draw.vertex_count,
        index_start: draw.index_start,
        index_count: draw.index_count,
    }
}

fn validate_topology_slice<T: Debug + PartialEq>(
    role: &str,
    expected: &[T],
    actual: &[T],
    scene_time_seconds: f32,
) -> Result<(), String> {
    if expected.len() != actual.len() {
        return Err(format!(
            "scene {role} topology changed at {scene_time_seconds:.6}s: setup count {}, frame count {}; live topology mutation is not supported by the current Vulkan resource allocation",
            expected.len(),
            actual.len()
        ));
    }
    if let Some((index, (expected, actual))) = expected
        .iter()
        .zip(actual)
        .enumerate()
        .find(|(_, (expected, actual))| expected != actual)
    {
        return Err(format!(
            "scene {role} topology changed at {scene_time_seconds:.6}s at index {index}: setup {expected:?}, frame {actual:?}; live topology mutation is not supported by the current Vulkan resource allocation"
        ));
    }
    Ok(())
}

fn write_exact_frame_payload(
    device: &Device,
    buffer: &NativeVulkanVulkanaliaBuffer,
    payload: &[u8],
) -> Result<(), String> {
    if payload.len() as u64 != buffer.snapshot.requested_bytes {
        return Err(format!(
            "{} frame payload has {} bytes, but the setup allocation has {} bytes",
            buffer.snapshot.role,
            payload.len(),
            buffer.snapshot.requested_bytes
        ));
    }
    native_vulkan_vulkanalia_write_host_buffer(device, buffer, payload)
}

fn push_scene_puppet_bone(payload: &mut Vec<u8>, matrix: [[f32; 4]; 4], alpha: f32) {
    for value in matrix.into_iter().flatten() {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    for value in [alpha, 0.0, 0.0, 0.0] {
        payload.extend_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::semantic_world::{
        ResolvedPuppetBoneMatrix, ResolvedPuppetBonePalette,
    };
    use crate::engine::scene::{
        SceneRenderingDevicePuppetBoneMatrix, SceneRenderingDevicePuppetBonePalette,
    };

    #[test]
    fn skinning_payload_prefixes_identity_and_packs_alpha_in_std430_entry() {
        let graph = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
            puppet_index: 0,
            bone_index: 41,
            parent_index: -1,
            matrix: [
                [1.0, 2.0, 3.0, 4.0],
                [5.0, 6.0, 7.0, 8.0],
                [9.0, 10.0, 11.0, 12.0],
                [13.0, 14.0, 15.0, 16.0],
            ],
            alpha: 0.375,
        });

        let payload = pack_scene_skinning_palette(&graph);

        assert_eq!(
            payload.len(),
            2 * NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES
        );
        assert_eq!(payload_f32(&payload, 0), 1.0);
        assert_eq!(payload_f32(&payload, 60), 1.0);
        assert_eq!(payload_f32(&payload, 64), 1.0);
        assert_eq!(
            payload_f32(
                &payload,
                NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES
            ),
            1.0
        );
        assert_eq!(
            payload_f32(
                &payload,
                NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES + 60
            ),
            16.0
        );
        assert_eq!(
            payload_f32(
                &payload,
                NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES + 64
            ),
            0.375
        );
    }

    #[test]
    fn topology_ignores_dynamic_matrix_and_alpha_values() {
        let setup = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
            puppet_index: 0,
            bone_index: 41,
            parent_index: -1,
            matrix: [[0.0; 4]; 4],
            alpha: 1.0,
        });
        let mut frame = setup.clone();
        frame.puppet_bone_matrices[0].matrix = [[2.0; 4]; 4];
        frame.puppet_bone_matrices[0].alpha = 0.25;

        SceneFrameTopology::from_graph(&setup)
            .validate(&frame, 1.0)
            .expect("dynamic bone values preserve topology");
    }

    #[test]
    fn topology_rejects_dynamic_bone_reordering() {
        let setup = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
            puppet_index: 0,
            bone_index: 41,
            parent_index: -1,
            matrix: [[0.0; 4]; 4],
            alpha: 1.0,
        });
        let mut frame = setup.clone();
        frame.puppet_bone_matrices[0].bone_index = 42;

        let error = SceneFrameTopology::from_graph(&setup)
            .validate(&frame, 1.0)
            .unwrap_err();
        assert!(error.contains("puppet bone topology changed"));
        assert!(error.contains("index 0"));
    }

    #[test]
    fn retained_graph_updates_dynamic_palette_matrix_and_alpha_in_place() {
        let mut graph = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
            puppet_index: 0,
            bone_index: 41,
            parent_index: -1,
            matrix: [[0.0; 4]; 4],
            alpha: 1.0,
        });
        let matrix = [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        ];
        let frame = ResolvedSemanticFrame {
            objects: Vec::new(),
            object_effects: Vec::new(),
            attachment_links: Vec::new(),
            puppet_bone_palettes: vec![ResolvedPuppetBonePalette {
                object: SceneObjectHandle(0),
                puppet_index: 0,
                bone_start: 0,
                bone_count: 1,
                resolved_visible: true,
            }],
            puppet_bone_matrices: vec![ResolvedPuppetBoneMatrix {
                puppet_index: 0,
                bone_index: 41,
                parent_index: -1,
                matrix,
                alpha: 0.25,
            }],
            visible_object_count: 0,
            visible_mesh_binding_count: 0,
            visible_effect_instance_count: 0,
            visible_effect_pass_count: 0,
            visible_effect_fbo_count: 0,
            visible_puppet_binding_count: 0,
            visible_puppet_bone_matrix_count: 1,
        };

        update_puppet_palettes(&mut graph, &frame, 2.0).expect("stable palette topology");

        assert_eq!(
            graph.puppet_bone_matrices[0].matrix,
            [
                [1.0, 5.0, 9.0, 13.0],
                [2.0, 6.0, 10.0, 14.0],
                [3.0, 7.0, 11.0, 15.0],
                [4.0, 8.0, 12.0, 16.0],
            ]
        );
        assert_eq!(graph.puppet_bone_matrices[0].alpha, 0.25);
    }

    fn graph_with_bone(
        bone: SceneRenderingDevicePuppetBoneMatrix,
    ) -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            pass_nodes: Vec::new(),
            target_allocations: Vec::new(),
            effect_batches: Vec::new(),
            effect_batch_instances: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            mesh_draws: Vec::new(),
            puppet_bone_palettes: vec![SceneRenderingDevicePuppetBonePalette {
                object: SceneObjectHandle(0),
                puppet_index: 0,
                bone_matrix_start: 0,
                bone_matrix_count: 1,
                resolved_visible: true,
            }],
            puppet_bone_matrices: vec![bone],
            resolved_object_count: 1,
            resolved_visible_object_count: 1,
            resolved_attachment_link_count: 0,
            resolved_visible_effect_instance_count: 0,
            resolved_visible_effect_pass_count: 0,
            resolved_visible_effect_fbo_count: 0,
            descriptor_heap_required: true,
            descriptor_heap_resource_count: 1,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 0,
            descriptor_heap_storage_buffer_count: 1,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 0,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        }
    }

    fn payload_f32(payload: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
    }
}
