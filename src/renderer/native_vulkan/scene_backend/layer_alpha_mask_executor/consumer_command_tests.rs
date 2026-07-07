use super::super::consumer_pipeline::native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets;
use super::super::consumer_target::native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_targets;
use super::super::{
    NativeVulkanSceneLayerAlphaMaskDescriptorSource, NativeVulkanSceneLayerAlphaMaskTextureBindRole,
};
use super::*;
use crate::engine::scene_engine::{
    SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorOperation,
    SceneLayerCompositorTarget, SceneObjectId, ScenePuppetId, SceneResourceId,
};
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::consumer_draws::{
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
};
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::consumer_target::NativeVulkanSceneLayerAlphaMaskLayerTargetBinding;
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::{
    NativeVulkanSceneLayerAlphaMaskHeapSliceBinding, NativeVulkanSceneLayerAlphaMaskHeapSliceKey,
};
use vulkanalia::vk;
use vulkanalia::vk::HasBuilder;

#[test]
fn generated_consumer_command_plan_joins_pipeline_heap_target_and_draw_contracts() {
    let consumer_draws = consumer_draws();
    let targets = target_plan(&consumer_draws, vk::Format::B8G8R8A8_UNORM)
        .expect("generated consumer target plan");
    let pipelines =
        native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets(
            &consumer_draws,
            &targets,
        )
        .expect("generated consumer pipeline plan");

    let plan =
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan::from_draws_targets_pipelines_and_heap(
            &consumer_draws,
            &targets,
            &pipelines,
            |heap_bind_index| {
                assert_eq!(heap_bind_index, 4);
                Ok(bind_info(
                    NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget,
                ))
            },
            1,
        )
        .expect("generated consumer command plan");

    assert_eq!(plan.command_count, 1);
    assert_eq!(plan.warmed_pipeline_count, 1);
    assert_eq!(plan.descriptor_heap_bind_count, 1);
    assert_eq!(plan.target_scope_count, 1);
    assert_eq!(plan.pipeline_bind_count, 1);
    assert_eq!(plan.resource_heap_bind_count, 1);
    assert_eq!(plan.rt_method_8_indexed_draw_count, 1);
    let command = &plan.commands[0];
    assert_eq!(command.command_index, 7);
    assert_eq!(command.shader, "we/genericimage4");
    assert_eq!(
        command.shader_combo_values,
        vec!["CLIPPINGTARGET=1".to_owned(), "CLIPPINGUVS=1".to_owned()]
    );
    assert_eq!(command.source_mask, SceneGraphTarget::FullAlphaMask);
    assert_eq!(
        command.draw_receiver,
        SceneLayerCompositorTarget::LayerTarget490
    );
    assert_eq!(
        command.color_target,
        SceneGraphTarget::ObjectFinal(SceneObjectId(77))
    );
    assert_eq!(command.target_format_label, "B8G8R8A8_UNORM");
    assert_eq!(command.heap_bind_index, 4);
    assert_eq!(command.heap_slice_index, 4);
    assert_eq!(
        command.material_source,
        "local generated material variant +0x428"
    );
    assert_eq!(
        command.blend_byte_source,
        "subdraw+0x40 -> generated material +0x1f0"
    );
    assert_eq!(
        command.effective_alpha_formula,
        "src.a * FullAlphaMask.r with translucent src-alpha/inv-src-alpha blend"
    );
    assert_eq!(command.draw_call, "[layer+0x490].vtable+0x40");
    assert_eq!(
        plan.command_order,
        [
            "require_warmed_genericimage4_clippingtarget_pipelines",
            "resolve_generated_clippingtarget_heap_binds",
            "join_generated_draw_target_pipeline_contracts",
            "preserve_token1_effective_alpha_formula",
            "build_generated_consumer_command_plan",
            "defer_geometry_and_uniform_recording_to_rt_method_8_recorder"
        ]
    );
}

#[test]
fn generated_consumer_command_plan_rejects_wrong_heap_role() {
    let consumer_draws = consumer_draws();
    let targets = target_plan(&consumer_draws, vk::Format::B8G8R8A8_UNORM)
        .expect("generated consumer target plan");
    let pipelines =
        native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets(
            &consumer_draws,
            &targets,
        )
        .expect("generated consumer pipeline plan");

    let err =
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan::from_draws_targets_pipelines_and_heap(
            &consumer_draws,
            &targets,
            &pipelines,
            |_| Ok(bind_info(NativeVulkanSceneLayerAlphaMaskTextureBindRole::FlatTextureCopyBack)),
            1,
        )
        .expect_err("generated consumer requires its own heap role");

    assert!(err.contains("GeneratedClippingTarget heap bind"));
}

#[test]
fn generated_consumer_command_plan_rejects_target_pipeline_format_drift() {
    let consumer_draws = consumer_draws();
    let targets = target_plan(&consumer_draws, vk::Format::B8G8R8A8_UNORM)
        .expect("generated consumer target plan");
    let mismatched_targets = target_plan(&consumer_draws, vk::Format::R16G16B16A16_SFLOAT)
        .expect("generated consumer target plan");
    let pipelines =
        native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets(
            &consumer_draws,
            &mismatched_targets,
        )
        .expect("generated consumer pipeline plan");

    let err =
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan::from_draws_targets_pipelines_and_heap(
            &consumer_draws,
            &targets,
            &pipelines,
            |_| Ok(bind_info(NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget)),
            1,
        )
        .expect_err("target/pipeline drift must fail");

    assert!(err.contains("target and pipeline variants disagree"));
}

fn target_plan(
    consumer_draws: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
    format: vk::Format,
) -> Result<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan, String> {
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_targets(
        consumer_draws,
        |object, target| {
            Ok(NativeVulkanSceneLayerAlphaMaskLayerTargetBinding {
                object,
                layer_target: target,
                color_target: SceneGraphTarget::ObjectFinal(object),
                format,
                width: 3840,
                height: 2160,
                pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
            })
        },
    )
}

fn consumer_draws() -> NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan {
    let bindings = vec![consumer_binding()];
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan {
        command_count: bindings.len(),
        consumer_draw_count: bindings.len(),
        heap_binding_count: bindings.len(),
        texture_slot_mask: bindings
            .iter()
            .fold(0u32, |mask, binding| mask | binding.texture_slot_mask),
        bindings,
        command_order: [
            "read_generated_clippingtarget_schedule_steps",
            "resolve_single_generated_clippingtarget_heap_bind",
            "validate_genericimage4_clippingtarget_slots_0_8",
            "preserve_generated_material_0x428",
            "preserve_subdraw_blend_byte_to_material_0x1f0",
            "preserve_layer_0x490_generated_draw_receiver",
        ],
    }
}

fn consumer_binding() -> NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan {
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan {
        consumer_draw_index: 0,
        command_index: 7,
        object: SceneObjectId(77),
        operation: SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
        source_mask: SceneGraphTarget::FullAlphaMask,
        target: SceneLayerCompositorTarget::LayerTarget490,
        target_receiver: "[layer+0x490]",
        draw_receiver_vtable_offset: "0x40",
        shader: "we/genericimage4",
        texture_slot_mask: (1u32 << 0) | (1u32 << 8),
        required_texture_slots: [0, 8],
        heap_bind_index: 4,
        heap_slice_index: 4,
        base_resource_descriptor_index: 8,
        base_sampler_descriptor_index: 24,
        resource_descriptor_count: 2,
        texture_count: 2,
        blend_byte_source: "subdraw+0x40 -> generated material +0x1f0",
        generated_material_source: "local generated material variant +0x428",
        shader_mappings: shader_mappings(),
        command_order: [
            "read_generated_clippingtarget_token_step",
            "match_single_generated_clippingtarget_heap_bind",
            "validate_slot0_source_and_slot8_full_alpha_mask",
            "preserve_subdraw_blend_byte_to_generated_material_0x1f0",
            "preserve_layer_0x490_rt_method_8_draw_receiver",
            "defer_uniform_and_geometry_lowering_to_generated_consumer_recorder",
        ],
    }
}

fn bind_info(
    role: NativeVulkanSceneLayerAlphaMaskTextureBindRole,
) -> NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
    NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
        heap_bind_index: 4,
        object: SceneObjectId(77),
        puppet: ScenePuppetId(5),
        shader: "we/genericimage4".to_owned(),
        role,
        heap_slice_index: 4,
        heap_slice: NativeVulkanSceneLayerAlphaMaskHeapSliceKey {
            shader: "we/genericimage4".to_owned(),
            bindings: vec![
                NativeVulkanSceneLayerAlphaMaskHeapSliceBinding {
                    slot: 0,
                    source: NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                        SceneResourceId(9),
                    ),
                },
                NativeVulkanSceneLayerAlphaMaskHeapSliceBinding {
                    slot: 8,
                    source: NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                        SceneGraphTarget::FullAlphaMask,
                    ),
                },
            ],
        },
        base_resource_descriptor_index: 8,
        base_sampler_descriptor_index: 24,
        resource_descriptor_count: 2,
        texture_count: 2,
        shader_mappings: shader_mappings(),
        resource_bind: vk::BindHeapInfoEXT::builder().build(),
        sampler_bind: vk::BindHeapInfoEXT::builder().build(),
    }
}

fn shader_mappings() -> Vec<String> {
    vec![
        "we.texture_slot0.g_Texture0 -> alpha-mask-heap-slice-offset0".to_owned(),
        "we.texture_slot8.g_Texture8 -> alpha-mask-heap-slice-offset1".to_owned(),
    ]
}
