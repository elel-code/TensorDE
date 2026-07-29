//! Typed coverage for scene-color snapshot copies.
//!
//! A bounded copy is valid only when the retained shader catalog proves that
//! the sole consumer fetches its snapshot at the current fragment coordinate.
//! The per-frame draw scissor then conservatively contains every texel the
//! consumer can observe; ordinary sampled consumers retain a full copy.

use vulkanalia::vk::{self, HasBuilder};

use crate::engine::scene::{
    SceneRenderingDeviceGraphPlan, SceneRenderingDeviceImageAccess, SceneStorage,
};
use crate::renderer::native_vulkan::scene::native_vulkan_scene_shader_for_key;

use super::super::draw_recording::SceneGpuDrawCommand;
use super::{
    LogicalEffectTargetKey, SceneEffectTargetCommand, SceneEffectTargetCommandKind,
    SceneEffectTargetCommandSource,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum SceneColorCopyCoverage {
    #[default]
    FullTarget,
    ConsumerDrawScissors { draw_start: u32, draw_count: u32 },
}

pub(super) fn scene_color_copy_coverage(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    copy_pass_node_index: usize,
    target: LogicalEffectTargetKey,
) -> SceneColorCopyCoverage {
    let target_key = (target.graph_index, target.target, target.name);
    let mut consumers = graph
        .sampled_bindings
        .iter()
        .filter(|binding| binding.logical_target() == Some(target_key));
    let Some(binding) = consumers.next() else {
        return SceneColorCopyCoverage::FullTarget;
    };
    if consumers.next().is_some()
        || binding.access != SceneRenderingDeviceImageAccess::SampledImage
        || binding.mesh_draw_count == 0
    {
        return SceneColorCopyCoverage::FullTarget;
    }
    let consumer_index = binding.pass_node_index as usize;
    let Some(consumer) = graph.pass_nodes.get(consumer_index) else {
        return SceneColorCopyCoverage::FullTarget;
    };
    if consumer_index <= copy_pass_node_index
        || consumer.graph_index != target.graph_index
        || (consumer.mesh_draw_start, consumer.mesh_draw_count)
            != (binding.mesh_draw_start, binding.mesh_draw_count)
        || graph
            .pass_nodes
            .get(copy_pass_node_index + 1..consumer_index)
            .is_none_or(|passes| {
                passes.iter().any(|pass| {
                    LogicalEffectTargetKey::from_pass_target(pass) == Some(target)
                })
            })
    {
        return SceneColorCopyCoverage::FullTarget;
    }
    let Some(record) = storage
        .document()
        .render_passes
        .get(consumer.pass_record_index as usize)
    else {
        return SceneColorCopyCoverage::FullTarget;
    };
    let Some(shader_key) = storage.string(record.shader_key) else {
        return SceneColorCopyCoverage::FullTarget;
    };
    let Some(shader) = native_vulkan_scene_shader_for_key(shader_key) else {
        return SceneColorCopyCoverage::FullTarget;
    };
    let Some(slot_bit) = 1u32.checked_shl(binding.slot) else {
        return SceneColorCopyCoverage::FullTarget;
    };
    if shader.fragment_coordinate_fetch_slot_mask & slot_bit == 0 {
        return SceneColorCopyCoverage::FullTarget;
    }
    SceneColorCopyCoverage::ConsumerDrawScissors {
        draw_start: binding.mesh_draw_start,
        draw_count: binding.mesh_draw_count,
    }
}

pub(super) fn scene_color_copy_region(
    scene_extent: vk::Extent2D,
    target_extent: vk::Extent2D,
    coverage: SceneColorCopyCoverage,
    draws: &[SceneGpuDrawCommand],
) -> Result<Option<vk::ImageCopy>, String> {
    let extent = vk::Extent2D {
        width: scene_extent.width.min(target_extent.width),
        height: scene_extent.height.min(target_extent.height),
    };
    if extent.width == 0 || extent.height == 0 {
        return Err("scene-color snapshot copy has an empty image extent".to_owned());
    }
    let SceneColorCopyCoverage::ConsumerDrawScissors {
        draw_start,
        draw_count,
    } = coverage
    else {
        return Ok(Some(image_copy([0, 0], [extent.width, extent.height])));
    };
    let start = draw_start as usize;
    let end = start
        .checked_add(draw_count as usize)
        .ok_or_else(|| "scene-color snapshot consumer draw range overflows usize".to_owned())?;
    let consumer_draws = draws.get(start..end).ok_or_else(|| {
        format!(
            "scene-color snapshot consumer draw range {start}..{end} exceeds {} draws",
            draws.len()
        )
    })?;
    let mut union = None::<[u32; 4]>;
    for draw in consumer_draws.iter().filter(|draw| draw.enabled) {
        let Some(scissor) = draw.scissor else {
            return Ok(Some(image_copy([0, 0], [extent.width, extent.height])));
        };
        let min_x = scissor.offset[0].max(0) as u32;
        let min_y = scissor.offset[1].max(0) as u32;
        let max_x = min_x.saturating_add(scissor.extent[0]).min(extent.width);
        let max_y = min_y.saturating_add(scissor.extent[1]).min(extent.height);
        if max_x <= min_x || max_y <= min_y {
            continue;
        }
        if let Some(bounds) = &mut union {
            bounds[0] = bounds[0].min(min_x);
            bounds[1] = bounds[1].min(min_y);
            bounds[2] = bounds[2].max(max_x);
            bounds[3] = bounds[3].max(max_y);
        } else {
            union = Some([min_x, min_y, max_x, max_y]);
        }
    }
    Ok(union.map(|[min_x, min_y, max_x, max_y]| {
        image_copy([min_x, min_y], [max_x - min_x, max_y - min_y])
    }))
}

fn image_copy(offset: [u32; 2], extent: [u32; 2]) -> vk::ImageCopy {
    vk::ImageCopy::builder()
        .src_subresource(super::color_subresource_layers())
        .src_offset(vk::Offset3D {
            x: offset[0] as i32,
            y: offset[1] as i32,
            z: 0,
        })
        .dst_subresource(super::color_subresource_layers())
        .dst_offset(vk::Offset3D {
            x: offset[0] as i32,
            y: offset[1] as i32,
            z: 0,
        })
        .extent(vk::Extent3D {
            width: extent[0],
            height: extent[1],
            depth: 1,
        })
        .build()
}

pub(in crate::renderer::native_vulkan) fn graph_copies_scene_color(
    commands: &[SceneEffectTargetCommand],
    graph_index: u32,
) -> bool {
    commands.iter().any(|command| {
        command.target.graph_index == graph_index
            && command.kind == SceneEffectTargetCommandKind::Copy
            && command.source == Some(SceneEffectTargetCommandSource::SceneColor)
    })
}

pub(in crate::renderer::native_vulkan) fn graph_requires_effect_target_execution(
    commands: &[SceneEffectTargetCommand],
    graph_index: u32,
) -> bool {
    commands.iter().any(|command| {
        command.target.graph_index == graph_index && command.batch_atlas_tile.is_none()
    })
}

pub(in crate::renderer::native_vulkan) fn graph_uses_direct_scene_color_snapshot(
    commands: &[SceneEffectTargetCommand],
    graph_index: u32,
) -> bool {
    commands.iter().any(|command| {
        command.target.graph_index == graph_index && command.direct_scene_color_snapshot
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        INVALID_MATERIAL_ID, INVALID_OBJECT_ID, SceneBinaryDocument, SceneColorWriteMask,
        SceneCompositeBlend, SceneCullMode, SceneDepthTest, SceneMaterialHandle, SceneObjectHandle,
        ScenePipelineBlend, SceneRenderEffectVisibilityPolicy, SceneRenderPassDrawPrimitive,
        SceneRenderPassKind, SceneRenderPassRecord, SceneRenderTargetKind,
        SceneRenderingDevicePassNode, SceneRenderingDeviceSampledBinding, SceneStringId,
    };
    use crate::renderer::native_vulkan::vulkan::scene::runtime::draw_recording::SceneGpuScissor;

    #[test]
    fn exact_fragment_coordinate_consumer_selects_its_draw_scissors() {
        let (storage, graph, target) = snapshot_graph("we/flat-rounded-hsl-source");

        assert_eq!(
            scene_color_copy_coverage(&storage, &graph, 0, target),
            SceneColorCopyCoverage::ConsumerDrawScissors {
                draw_start: 4,
                draw_count: 1,
            }
        );
    }

    #[test]
    fn ordinary_sampled_consumer_requires_the_full_snapshot() {
        let (storage, graph, target) = snapshot_graph("we/passthrough");

        assert_eq!(
            scene_color_copy_coverage(&storage, &graph, 0, target),
            SceneColorCopyCoverage::FullTarget
        );
    }

    #[test]
    fn bounded_region_matches_enabled_consumer_scissors() {
        let draws = vec![draw_command(None, true), draw_command(Some(([120, 40], [300, 500])), true)];
        let region = scene_color_copy_region(
            vk::Extent2D { width: 3840, height: 2160 },
            vk::Extent2D { width: 3840, height: 2160 },
            SceneColorCopyCoverage::ConsumerDrawScissors { draw_start: 1, draw_count: 1 },
            &draws,
        )
        .expect("bounded region")
        .expect("enabled consumer");

        assert_eq!((region.src_offset.x, region.src_offset.y), (120, 40));
        assert_eq!((region.dst_offset.x, region.dst_offset.y), (120, 40));
        assert_eq!((region.extent.width, region.extent.height), (300, 500));
    }

    #[test]
    fn missing_scissor_keeps_full_copy_and_disabled_consumer_skips_it() {
        let extent = vk::Extent2D { width: 640, height: 360 };
        let full = scene_color_copy_region(
            extent,
            extent,
            SceneColorCopyCoverage::ConsumerDrawScissors { draw_start: 0, draw_count: 1 },
            &[draw_command(None, true)],
        )
        .expect("full region")
        .expect("enabled consumer");
        assert_eq!((full.extent.width, full.extent.height), (640, 360));
        assert!(scene_color_copy_region(
            extent,
            extent,
            SceneColorCopyCoverage::ConsumerDrawScissors { draw_start: 0, draw_count: 1 },
            &[draw_command(Some(([0, 0], [64, 64])), false)],
        )
        .expect("disabled consumer")
        .is_none());
    }

    fn snapshot_graph(
        shader: &str,
    ) -> (SceneStorage, SceneRenderingDeviceGraphPlan, LogicalEffectTargetKey) {
        let snapshot_name = SceneStringId(0);
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["snapshot".to_owned(), shader.to_owned()],
            render_passes: vec![pass_record(
                SceneRenderPassKind::CopyTarget,
                SceneStringId::NONE,
                SceneRenderTargetKind::FirstClassEffectTarget,
                snapshot_name,
            ), pass_record(
                SceneRenderPassKind::SceneComposite,
                SceneStringId(1),
                SceneRenderTargetKind::SceneColor,
                SceneStringId::NONE,
            )],
            ..SceneBinaryDocument::default()
        })
        .expect("snapshot storage");
        let target = LogicalEffectTargetKey {
            graph_index: 0,
            target: SceneRenderTargetKind::FirstClassEffectTarget,
            name: snapshot_name,
        };
        let graph = SceneRenderingDeviceGraphPlan {
            pass_nodes: vec![pass_node(0, 0, target.target, target.name, 0, 0), pass_node(
                1,
                1,
                SceneRenderTargetKind::SceneColor,
                SceneStringId::NONE,
                4,
                1,
            )],
            sampled_bindings: vec![SceneRenderingDeviceSampledBinding {
                pass_node_index: 1,
                graph_index: 0,
                mesh_draw_start: 4,
                mesh_draw_count: 1,
                kind: crate::engine::scene::SceneRenderBindingKind::EffectTarget,
                slot: 0,
                target: target.target,
                target_name: target.name,
                access: SceneRenderingDeviceImageAccess::SampledImage,
            }],
            ..empty_graph_plan()
        };
        (storage, graph, target)
    }

    fn pass_record(
        role: SceneRenderPassKind,
        shader_key: SceneStringId,
        target: SceneRenderTargetKind,
        target_name: SceneStringId,
    ) -> SceneRenderPassRecord {
        SceneRenderPassRecord {
            id: 0,
            role,
            draw_primitive: SceneRenderPassDrawPrimitive::ObjectUvSupportQuad,
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            pass_index: 0,
            shader_key,
            target,
            target_name,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            pipeline_blend: ScenePipelineBlend::Normal,
            scene_blend: SceneCompositeBlend::Alpha,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            color_write_mask: SceneColorWriteMask::Rgba,
            clear_target: false,
        }
    }

    fn pass_node(
        pass_id: u32,
        pass_record_index: u32,
        target: SceneRenderTargetKind,
        target_name: SceneStringId,
        mesh_draw_start: u32,
        mesh_draw_count: u32,
    ) -> SceneRenderingDevicePassNode {
        SceneRenderingDevicePassNode {
            graph_index: 0,
            graph_activation_policy: crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
            pass_record_index,
            pass_id,
            role: if pass_id == 0 { SceneRenderPassKind::CopyTarget } else { SceneRenderPassKind::SceneComposite },
            target,
            target_name,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
            mesh_draw_start,
            mesh_draw_count,
        }
    }

    fn draw_command(scissor: Option<([i32; 2], [u32; 2])>, enabled: bool) -> SceneGpuDrawCommand {
        SceneGpuDrawCommand {
            enabled,
            primitive: crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad,
            pipeline_index: 0,
            authored_pipeline_index: 0,
            disabled_pipeline_index: None,
            first_index: 0,
            index_count: 6,
            vertex_offset: 0,
            vertex_count: 6,
            instance_count: 1,
            instance_capacity: 1,
            first_instance: 0,
            dynamic_text: false,
            particle_indirect_index: None,
            resource_descriptor_base: 0,
            material_resource_descriptor: None,
            skinning_resource_descriptor: None,
            sampled_resource_descriptor_base: 0,
            input_attachment_resource_descriptor_base: 0,
            sampler_descriptor_base: 0,
            skinning_byte_offset: 0,
            skinning_byte_count: 0,
            scissor: scissor.map(|(offset, extent)| SceneGpuScissor { offset, extent }),
        }
    }

    fn empty_graph_plan() -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            pass_nodes: Vec::new(),
            target_allocations: Vec::new(),
            effect_batches: Vec::new(),
            effect_batch_instances: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            mesh_draws: Vec::new(),
            puppet_bone_palettes: Vec::new(),
            puppet_bone_matrices: Vec::new(),
            particle_gpu_emitters: Vec::new(),
            resolved_object_count: 0,
            resolved_visible_object_count: 0,
            resolved_attachment_link_count: 0,
            resolved_visible_effect_instance_count: 0,
            resolved_visible_effect_pass_count: 0,
            resolved_visible_effect_fbo_count: 0,
            descriptor_heap_required: true,
            descriptor_heap_resource_count: 0,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 0,
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 0,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        }
    }
}
