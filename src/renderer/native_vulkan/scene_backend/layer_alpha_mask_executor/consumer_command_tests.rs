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
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::rt_method8::{
    LAYER_490_RT_METHOD8_DRAW_CALL, LAYER_490_RT_METHOD8_GEOMETRY_CREATION_SITE,
    LAYER_490_RT_METHOD8_GEOMETRY_SOURCE, LAYER_490_RT_METHOD8_INDEX,
    LAYER_490_RT_METHOD8_INDEX_BUFFER_USAGE_FLAG, LAYER_490_RT_METHOD8_OFFSET,
    LAYER_490_RT_METHOD8_RECEIVER_LABEL, LAYER_490_RT_METHOD8_RECEIVER_VTABLE,
    LAYER_490_RT_METHOD8_VMA, NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge,
    NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan,
    NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose,
};
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::{
    NativeVulkanSceneLayerAlphaMaskHeapSliceBinding, NativeVulkanSceneLayerAlphaMaskHeapSliceKey,
    NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding,
};
use crate::renderer::native_vulkan::scene_backend::material_uniforms::NativeVulkanSceneMaterialUniformKey;
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
    let rt_method8_bridges = rt_method8_bridges();

    let plan =
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan::from_draws_targets_pipelines_and_heap(
            &consumer_draws,
            &targets,
            &pipelines,
            &rt_method8_bridges,
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
    assert_eq!(plan.rt_method_8_bridge_count, 1);
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
    assert_eq!(command.resource_descriptor_count, 3);
    assert_eq!(command.texture_count, 2);
    assert_eq!(command.material_uniform_buffer_handle, 0x4204);
    assert_eq!(command.material_uniform_device_address, 0x4284);
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
    assert_eq!(command.rt_method8_bridge_index, 0);
    assert_eq!(command.rt_method8_call_site, "0x14020908c");
    assert_eq!(command.rt_method8_method_vma, "0x1400eacd0");
    assert_eq!(command.draw_call, "[layer+0x490].vtable+0x40");
    assert_eq!(
        plan.command_order,
        [
            "require_warmed_genericimage4_clippingtarget_pipelines",
            "resolve_generated_clippingtarget_heap_binds",
            "join_generated_draw_target_pipeline_contracts",
            "join_rt_method_8_bridge_plan",
            "preserve_token1_effective_alpha_formula",
            "build_generated_consumer_command_plan"
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
    let rt_method8_bridges = rt_method8_bridges();

    let err =
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan::from_draws_targets_pipelines_and_heap(
            &consumer_draws,
            &targets,
            &pipelines,
            &rt_method8_bridges,
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
    let rt_method8_bridges = rt_method8_bridges();

    let err =
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan::from_draws_targets_pipelines_and_heap(
            &consumer_draws,
            &targets,
            &pipelines,
            &rt_method8_bridges,
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
        resource_descriptor_count: 3,
        texture_count: 2,
        material_uniform_buffer_handle: 0x4204,
        material_uniform_device_address: 0x4284,
        material_uniform_bytes: 48,
        material_uniform_payload_hash: 0x1238,
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

fn rt_method8_bridges() -> NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan {
    NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan {
        command_count: 8,
        bridge_count: 1,
        producer_bridge_count: 0,
        generated_consumer_bridge_count: 1,
        indexed_vector_draw_bridge_count: 1,
        raw_shader_resource_bind_bridge_count: 0,
        closed_call_site_count: 1,
        geometry_creation_site: LAYER_490_RT_METHOD8_GEOMETRY_CREATION_SITE,
        geometry_source: LAYER_490_RT_METHOD8_GEOMETRY_SOURCE,
        index_buffer_usage_flag: LAYER_490_RT_METHOD8_INDEX_BUFFER_USAGE_FLAG,
        geometry_source_plan:
            super::super::rt_method8_geometry::native_vulkan_scene_layer_alpha_mask_rt_method8_geometry_source_plan(),
        bridges: vec![NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge {
            bridge_index: 0,
            command_index: 7,
            object: SceneObjectId(77),
            entry: crate::engine::scene_engine::SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53,
            operation: SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
            condition: crate::engine::scene_engine::SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
            purpose: NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::GeneratedClippingTargetConsumer,
            producer_draw_index: None,
            generated_consumer_draw_index: Some(0),
            receiver: SceneLayerCompositorTarget::LayerTarget490,
            receiver_field: LAYER_490_RT_METHOD8_RECEIVER_LABEL,
            receiver_vtable: LAYER_490_RT_METHOD8_RECEIVER_VTABLE,
            method_index: LAYER_490_RT_METHOD8_INDEX,
            method_offset: LAYER_490_RT_METHOD8_OFFSET,
            method_vma: LAYER_490_RT_METHOD8_VMA,
            draw_call: LAYER_490_RT_METHOD8_DRAW_CALL,
            call_site: "0x14020908c",
            call_site_role: "vtable [53] token 1/2 generated CLIPPINGTARGET draw",
            draw_index_argument: "edx is the generated subdraw/draw index selector, not a raw shader resource",
            geometry_creation_site: LAYER_490_RT_METHOD8_GEOMETRY_CREATION_SITE,
            geometry_source: LAYER_490_RT_METHOD8_GEOMETRY_SOURCE,
            index_buffer_usage_flag: LAYER_490_RT_METHOD8_INDEX_BUFFER_USAGE_FLAG,
            is_indexed_vector_draw: true,
            is_raw_shader_resource_bind: false,
            reference_points: [
                "reverse-engineered/docs/exe/blend-and-render.md: [layer+0x490] call sites and 0x14020b15e geometry source",
                "reverse-engineered/docs/exe/composelayer-and-effecttarget.md: 0x14020d83e RT method [8] draw",
                "reverse-engineered/docs/exe/d3d11-context-calls.md: offset +0x40 is RT method [8] 0x1400eacd0",
                "reverse-engineered/tools/audit_opacity_final_alpha_path.py: token [52]/[53] generated draw call sites",
                "references/godot/servers/rendering/rendering_device_graph.cpp: graph tracks draw resource usage before command recording",
            ],
            command_order: [
                "read_layer_0x490_runtime_command",
                "classify_rt_vtable_0x140486f38_method_8",
                "preserve_closed_call_site",
                "preserve_indexed_vector_draw_bridge",
                "reject_raw_shader_resource_bind_interpretation",
                "preserve_0x14020b15e_wrapper_argument_contract",
                "require_retained_mdlv_geometry_buffer_plan",
                "feed_rt_method_8_recorder_requirements",
            ],
        }],
        command_order: [
            "read_clippingmaskimage4_and_generated_draw_intents",
            "classify_layer_0x490_receiver_once",
            "map_producer_to_0x14020d83e",
            "map_generated_consumer_to_vtable_52_53_call_site",
            "preserve_rt_method_8_indexed_draw_identity",
            "preserve_0x14020b15e_wrapper_argument_contract",
            "expose_single_bridge_plan_to_recorder",
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
        material: (role == NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget)
            .then(|| NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding {
                key: NativeVulkanSceneMaterialUniformKey {
                    object: SceneObjectId(77),
                    shader: "we/genericimage4".to_owned(),
                },
                buffer_handle: 0x4204,
                device_address: 0x4284,
                record_index: 4,
                bytes: 48,
                payload_hash: 0x1238,
            }),
        base_resource_descriptor_index: 8,
        base_sampler_descriptor_index: 24,
        resource_descriptor_count: if role
            == NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget
        {
            3
        } else {
            2
        },
        texture_count: 2,
        shader_mappings: shader_mappings(),
        resource_bind: vk::BindHeapInfoEXT::builder().build(),
        sampler_bind: vk::BindHeapInfoEXT::builder().build(),
    }
}

fn shader_mappings() -> Vec<String> {
    vec![
        "WE PSSetConstantBuffers(slot=3) -> alpha-mask-heap-slice-offset0".to_owned(),
        "we.texture_slot0.g_Texture0 -> alpha-mask-heap-slice-offset1".to_owned(),
        "we.texture_slot8.g_Texture8 -> alpha-mask-heap-slice-offset2".to_owned(),
    ]
}
