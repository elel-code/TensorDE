use super::*;
use crate::engine::scene_engine::{SceneGraphPipelineClass, SceneGraphTarget, SceneObjectId};
use crate::renderer::native_vulkan::scene_backend::layer_aux_material_draws::NativeVulkanSceneLayerAuxMaterialDrawReceiverKind;
use crate::renderer::native_vulkan::scene_backend::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
use crate::renderer::native_vulkan::scene_backend::pipeline::{
    NativeVulkanScenePipelineResourceHeapClass, NativeVulkanScenePipelineVertexLayout,
};
use crate::renderer::native_vulkan::vulkan::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot;
use vulkanalia::vk;
use vulkanalia::vk::Handle;

#[test]
fn aux_material_resource_heap_empty_plan_keeps_empty_ready_heap() {
    let plan = NativeVulkanSceneLayerAuxMaterialResourceHeapFramePlan::from_pipeline_plan(
        &NativeVulkanSceneLayerAuxMaterialPipelineFramePlan::empty(),
        descriptor_heap_properties(),
        |_| unreachable!("empty plan must not resolve source target"),
        |_| unreachable!("empty plan must not resolve source image"),
    )
    .expect("empty aux material heap plan");

    assert_eq!(plan.clear_bind_count, 0);
    assert_eq!(plan.heap_slice_count, 0);
    assert_eq!(plan.resource_descriptor_count, 0);
    assert_eq!(plan.sampler_descriptor_count, 0);
    assert!(plan.entries.is_empty());
    assert_eq!(
        plan.command_order,
        [
            "collect_aux_fullscreenlayer_sampled_texture",
            "dedupe_aux_material_heap_slices",
            "pack_aux_material_descriptor_heap_slices",
            "bind_aux_material_heap_slice",
            "record_aux_0x410_to_aux_0x3f0_draw"
        ]
    );
}

#[test]
fn aux_material_resource_heap_rejects_swapchain_full_frame_source() {
    let err = NativeVulkanSceneLayerAuxMaterialResourceHeapFramePlan::from_pipeline_plan(
        &pipeline_plan(SceneObjectId(77)),
        descriptor_heap_properties(),
        |_| Ok(SceneGraphTarget::Swapchain),
        |_| unreachable!("swapchain source must be rejected before image resolution"),
    )
    .expect_err("swapchain source is not sampleable");

    assert!(err.contains("_rt_FullFrameBuffer"));
    assert!(err.contains("Swapchain"));
}

#[test]
fn aux_material_resource_heap_plan_packs_sampled_source_target() {
    let object = SceneObjectId(77);
    let source_target = SceneGraphTarget::ImageLayerCompositeA(object);
    let plan = NativeVulkanSceneLayerAuxMaterialResourceHeapFramePlan::from_pipeline_plan(
        &pipeline_plan(object),
        descriptor_heap_properties(),
        |_| Ok(source_target),
        |target| Ok(source_binding(target)),
    )
    .expect("aux material heap plan");

    assert_eq!(plan.clear_bind_count, 1);
    assert_eq!(plan.heap_slice_count, 1);
    assert_eq!(plan.resource_descriptor_count, 1);
    assert_eq!(plan.sampler_descriptor_count, 1);
    assert_eq!(plan.entries[0].object, object);
    assert_eq!(plan.entries[0].source, "_rt_FullFrameBuffer");
    assert_eq!(plan.entries[0].source_target, source_target);
    assert_eq!(plan.entries[0].slot, 0);
    assert_eq!(plan.entries[0].image_handle, 0x1100);
    assert_eq!(plan.entries[0].view_handle, 0x1200);
    assert_eq!(plan.entries[0].sampler_handle, 0x1300);
    assert_eq!(
        plan.clear_bindings[0].shader_mappings,
        vec!["we.texture_slot0.g_Texture0 -> aux-material-heap-slice-offset0".to_owned()]
    );
}

fn pipeline_plan(object: SceneObjectId) -> NativeVulkanSceneLayerAuxMaterialPipelineFramePlan {
    let mut plan = NativeVulkanSceneLayerAuxMaterialPipelineFramePlan::empty();
    plan.active_command_count = 1;
    plan.clear_pipeline_count = 1;
    plan.cache_key_count = 1;
    plan.clear_keys
        .push(NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan {
            command_index: 0,
            block_index: 4,
            object,
            material: WE_AUX_FULLSCREEN_LAYER_MATERIAL,
            shader: WE_AUX_FULLSCREEN_LAYER_SHADER,
            source: WE_AUX_FULLSCREEN_LAYER_TEXTURE_SOURCE,
            target: SceneGraphTarget::LayerAuxClear(object),
            target_format: "R8G8B8A8_UNORM",
            texture_slot: WE_AUX_FULLSCREEN_LAYER_TEXTURE_SLOT,
            texture_slot_mask: 1,
            pipeline_class: SceneGraphPipelineClass::LayerUtilityIndexed,
            vertex_layout: NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv,
            resource_heap: NativeVulkanScenePipelineResourceHeapClass::LayerAuxMaterial,
            draw_receiver:
                NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f0ClearMaterialNonIndexed,
            command_order: [
                "read_materials_util_fullscreenlayer_json",
                "select_util_passthrough_shader",
                "bind_rt_full_frame_buffer_as_g_texture0",
                "select_aux_0x3e8_color_target_format",
                "select_position_uv_triangle_receiver_aux_0x3f0",
                "derive_resource_heap_scoped_pipeline_key",
            ],
        });
    plan
}

fn source_binding(target: SceneGraphTarget) -> NativeVulkanSceneOffscreenTargetBinding {
    NativeVulkanSceneOffscreenTargetBinding {
        target,
        image: vk::Image::from_raw(0x1100),
        view: vk::ImageView::from_raw(0x1200),
        sampler: vk::Sampler::from_raw(0x1300),
        format: vk::Format::R8G8B8A8_UNORM,
        width: 3840,
        height: 2160,
        current_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    }
}

fn descriptor_heap_properties() -> NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
        resource_heap_alignment: 64,
        sampler_heap_alignment: 32,
        max_resource_heap_size: 4096,
        min_resource_heap_reserved_range: 96,
        max_sampler_heap_size: 4096,
        min_sampler_heap_reserved_range: 48,
        image_descriptor_size: 24,
        image_descriptor_alignment: 32,
        sampler_descriptor_size: 12,
        sampler_descriptor_alignment: 16,
        ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
    }
}
