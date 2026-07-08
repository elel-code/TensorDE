use super::*;
use crate::engine::scene_engine::{
    SceneLayerCompositorCondition, SceneLayerCompositorEntry, SceneLayerCompositorOperation,
    SceneLayerCompositorTarget, SceneObjectId,
};
use crate::renderer::native_vulkan::scene_backend::resource_storage::{
    NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRole,
    NativeVulkanSceneGpuBufferUsage,
};

use super::super::rt_method8::{
    LAYER_490_RT_METHOD8_DRAW_CALL, LAYER_490_RT_METHOD8_GEOMETRY_CREATION_SITE,
    LAYER_490_RT_METHOD8_GEOMETRY_SOURCE, LAYER_490_RT_METHOD8_INDEX,
    LAYER_490_RT_METHOD8_INDEX_BUFFER_USAGE_FLAG, LAYER_490_RT_METHOD8_OFFSET,
    LAYER_490_RT_METHOD8_RECEIVER_LABEL, LAYER_490_RT_METHOD8_RECEIVER_VTABLE,
    LAYER_490_RT_METHOD8_VMA, NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge,
    NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan,
    NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose,
};
use super::super::rt_method8_geometry::native_vulkan_scene_layer_alpha_mask_rt_method8_geometry_source_plan;

#[test]
fn rt_method8_mdlv_geometry_buffers_are_retained_outside_mesh_geometry() {
    let bridge_plan = bridges(vec![
        bridge(
            0,
            0,
            SceneObjectId(7),
            NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::ClippingMaskImage4Producer,
        ),
        bridge(
            1,
            1,
            SceneObjectId(7),
            NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::GeneratedClippingTargetConsumer,
        ),
        bridge(
            2,
            2,
            SceneObjectId(9),
            NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::GeneratedClippingTargetConsumer,
        ),
    ]);

    let plan =
        native_vulkan_plan_scene_layer_alpha_mask_rt_method8_mdlv_geometry_buffers(&bridge_plan)
            .expect("RT method [8] MDLV geometry buffer plan");

    assert_eq!(plan.command_count, 3);
    assert_eq!(plan.bridge_count, 3);
    assert_eq!(plan.geometry_count, 2);
    assert_eq!(plan.vertex_requirement_count, 2);
    assert_eq!(plan.index_requirement_count, 2);
    assert_eq!(
        plan.entry_owner_source,
        LAYER_490_RT_METHOD8_ENTRY_OWNER_SOURCE
    );

    let (object7_index, object7) = plan
        .requirement_for_object(SceneObjectId(7))
        .expect("object 7 geometry");
    assert_eq!(object7_index, 0);
    assert_eq!(object7.requirement_index, 0);
    assert_eq!(object7.entry_owner_index, 0);
    assert_eq!(
        object7.owner,
        NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(object7.geometry)
    );
    assert_eq!(
        object7.vertex_role,
        NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvVertex
    );
    assert_eq!(
        object7.index_role,
        NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvIndex
    );
    assert_eq!(
        object7.vertex_usage,
        NativeVulkanSceneGpuBufferUsage::Vertex
    );
    assert_eq!(object7.index_usage, NativeVulkanSceneGpuBufferUsage::Index);
    assert!(
        object7
            .reference_points
            .iter()
            .any(|reference| reference.contains("0x14020b15e"))
    );

    let (_, object9) = plan
        .requirement_for_object(SceneObjectId(9))
        .expect("object 9 geometry");
    assert_eq!(object9.requirement_index, 1);
}

#[test]
fn rt_method8_mdlv_geometry_buffers_reject_shader_resource_interpretation() {
    let mut invalid = bridge(
        0,
        0,
        SceneObjectId(7),
        NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::ClippingMaskImage4Producer,
    );
    invalid.is_raw_shader_resource_bind = true;
    let bridge_plan = bridges(vec![invalid]);

    let err =
        native_vulkan_plan_scene_layer_alpha_mask_rt_method8_mdlv_geometry_buffers(&bridge_plan)
            .expect_err("raw shader resource interpretation must fail");

    assert!(err.contains("shader resource bind"));
}

fn bridges(
    bridges: Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge>,
) -> NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan {
    NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan {
        command_count: bridges.len(),
        bridge_count: bridges.len(),
        producer_bridge_count: bridges
            .iter()
            .filter(|bridge| {
                bridge.purpose
                    == NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::ClippingMaskImage4Producer
            })
            .count(),
        generated_consumer_bridge_count: bridges
            .iter()
            .filter(|bridge| {
                bridge.purpose
                    == NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::GeneratedClippingTargetConsumer
            })
            .count(),
        indexed_vector_draw_bridge_count: bridges
            .iter()
            .filter(|bridge| bridge.is_indexed_vector_draw)
            .count(),
        raw_shader_resource_bind_bridge_count: bridges
            .iter()
            .filter(|bridge| bridge.is_raw_shader_resource_bind)
            .count(),
        closed_call_site_count: bridges
            .iter()
            .filter(|bridge| !bridge.call_site.is_empty())
            .count(),
        geometry_creation_site: LAYER_490_RT_METHOD8_GEOMETRY_CREATION_SITE,
        geometry_source: LAYER_490_RT_METHOD8_GEOMETRY_SOURCE,
        index_buffer_usage_flag: LAYER_490_RT_METHOD8_INDEX_BUFFER_USAGE_FLAG,
        geometry_source_plan:
            native_vulkan_scene_layer_alpha_mask_rt_method8_geometry_source_plan(),
        bridges,
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

fn bridge(
    bridge_index: usize,
    command_index: usize,
    object: SceneObjectId,
    purpose: NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose,
) -> NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge {
    NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge {
        bridge_index,
        command_index,
        object,
        entry: match purpose {
            NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::ClippingMaskImage4Producer => {
                SceneLayerCompositorEntry::AlphaMaskHelper20d6a0
            }
            NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::GeneratedClippingTargetConsumer => {
                SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53
            }
        },
        operation: match purpose {
            NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::ClippingMaskImage4Producer => {
                SceneLayerCompositorOperation::DrawClippingMask
            }
            NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::GeneratedClippingTargetConsumer => {
                SceneLayerCompositorOperation::DrawGeneratedClippingTarget
            }
        },
        condition: SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
        purpose,
        producer_draw_index: (purpose
            == NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::ClippingMaskImage4Producer)
            .then_some(bridge_index),
        generated_consumer_draw_index: (purpose
            == NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::GeneratedClippingTargetConsumer)
            .then_some(bridge_index),
        receiver: SceneLayerCompositorTarget::LayerTarget490,
        receiver_field: LAYER_490_RT_METHOD8_RECEIVER_LABEL,
        receiver_vtable: LAYER_490_RT_METHOD8_RECEIVER_VTABLE,
        method_index: LAYER_490_RT_METHOD8_INDEX,
        method_offset: LAYER_490_RT_METHOD8_OFFSET,
        method_vma: LAYER_490_RT_METHOD8_VMA,
        draw_call: LAYER_490_RT_METHOD8_DRAW_CALL,
        call_site: "0x14020d83e",
        call_site_role: "test closed call site",
        draw_index_argument: "edx is the subdraw/index selector",
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
    }
}
