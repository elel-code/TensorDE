//! WE `[layer+0x490]` render-target method [8] draw bridge.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/tools/audit_opacity_final_alpha_path.py`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneLayerCompositorCondition, SceneLayerCompositorEntry, SceneLayerCompositorOperation,
    SceneLayerCompositorTarget, SceneObjectId,
};

use super::NativeVulkanSceneLayerAlphaMaskRuntimePlan;
use super::consumer_draws::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan;
use super::producer_draws::NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan;
use super::rt_method8_geometry::{
    NativeVulkanSceneLayerAlphaMaskRtMethod8GeometrySourcePlan,
    native_vulkan_scene_layer_alpha_mask_rt_method8_geometry_source_plan,
};

pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_RECEIVER_LABEL: &str =
    "[layer+0x490]";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_RECEIVER_VTABLE: &str =
    "0x140486f38";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_OFFSET: &str = "0x40";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_INDEX: u32 = 8;
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_VMA: &str = "0x1400eacd0";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_DRAW_CALL: &str =
    "[layer+0x490].vtable+0x40";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_GEOMETRY_CREATION_SITE: &str =
    "0x14020b15e";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_GEOMETRY_SOURCE: &str =
    "0x14020b15e first/current MDLV entry-owner geometry for [layer+0x490] RT method [8]";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_INDEX_BUFFER_USAGE_FLAG: &str = "((entry+0x18 >> 3) & 1) * 2 selects wrapper [8] stack arg 9 bit1 dynamic index-buffer creation";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan {
    pub command_count: usize,
    pub bridge_count: usize,
    pub producer_bridge_count: usize,
    pub generated_consumer_bridge_count: usize,
    pub indexed_vector_draw_bridge_count: usize,
    pub raw_shader_resource_bind_bridge_count: usize,
    pub closed_call_site_count: usize,
    pub geometry_creation_site: &'static str,
    pub geometry_source: &'static str,
    pub index_buffer_usage_flag: &'static str,
    pub geometry_source_plan: NativeVulkanSceneLayerAlphaMaskRtMethod8GeometrySourcePlan,
    pub bridges: Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge>,
    pub command_order: [&'static str; 7],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge {
    pub bridge_index: usize,
    pub command_index: usize,
    pub object: SceneObjectId,
    pub entry: SceneLayerCompositorEntry,
    pub operation: SceneLayerCompositorOperation,
    pub condition: SceneLayerCompositorCondition,
    pub purpose: NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose,
    pub producer_draw_index: Option<usize>,
    pub generated_consumer_draw_index: Option<usize>,
    pub receiver: SceneLayerCompositorTarget,
    pub receiver_field: &'static str,
    pub receiver_vtable: &'static str,
    pub method_index: u32,
    pub method_offset: &'static str,
    pub method_vma: &'static str,
    pub draw_call: &'static str,
    pub call_site: &'static str,
    pub call_site_role: &'static str,
    pub draw_index_argument: &'static str,
    pub geometry_creation_site: &'static str,
    pub geometry_source: &'static str,
    pub index_buffer_usage_flag: &'static str,
    pub is_indexed_vector_draw: bool,
    pub is_raw_shader_resource_bind: bool,
    pub reference_points: [&'static str; 5],
    pub command_order: [&'static str; 8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose {
    ClippingMaskImage4Producer,
    GeneratedClippingTargetConsumer,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_rt_method8_bridges(
    runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
    producer_draws: &NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
    generated_consumer_draws: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan, String> {
    if runtime.tokenized_layer_count == 0 {
        if producer_draws.producer_draw_count != 0
            || generated_consumer_draws.consumer_draw_count != 0
        {
            return Err(
                "scene layer alpha-mask RT method [8] bridge received draws for an empty runtime"
                    .to_owned(),
            );
        }
        return Ok(NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan::empty());
    }

    let mut bridges = Vec::with_capacity(
        producer_draws.producer_draw_count + generated_consumer_draws.consumer_draw_count,
    );
    for producer in &producer_draws.draws {
        let command = runtime.commands.get(producer.command_index).ok_or_else(|| {
            format!(
                "scene layer alpha-mask RT method [8] producer draw {} references missing command {}",
                producer.producer_draw_index, producer.command_index
            )
        })?;
        validate_producer_command(producer.command_index, command)?;
        bridges.push(rt_method8_bridge(
            producer.command_index,
            command.object,
            command.entry,
            command.operation,
            command.condition,
            NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::ClippingMaskImage4Producer,
            Some(producer.producer_draw_index),
            None,
            "0x14020d83e",
            "0x14020d6a0 clippingmaskimage4 producer draw",
            "edx is the subdraw/index selector passed to [layer+0x490].vtable+0x40",
        ));
    }

    for consumer in &generated_consumer_draws.bindings {
        let command = runtime.commands.get(consumer.command_index).ok_or_else(|| {
            format!(
                "scene layer alpha-mask RT method [8] generated consumer draw {} references missing command {}",
                consumer.consumer_draw_index, consumer.command_index
            )
        })?;
        validate_generated_consumer_command(consumer.command_index, command)?;
        validate_generated_consumer_draw_identity(consumer.command_index, consumer)?;
        let (call_site, call_site_role) =
            generated_consumer_call_site(consumer.command_index, command.entry)?;
        bridges.push(rt_method8_bridge(
            consumer.command_index,
            command.object,
            command.entry,
            command.operation,
            command.condition,
            NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::GeneratedClippingTargetConsumer,
            None,
            Some(consumer.consumer_draw_index),
            call_site,
            call_site_role,
            "edx is the generated subdraw/draw index selector, not a raw shader resource",
        ));
    }

    bridges.sort_by_key(|bridge| (bridge.command_index, bridge.purpose_order()));
    for (bridge_index, bridge) in bridges.iter_mut().enumerate() {
        bridge.bridge_index = bridge_index;
    }

    Ok(
        NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan::from_bridges(
            runtime.command_count,
            bridges,
        ),
    )
}

impl NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan {
    fn empty() -> Self {
        Self {
            command_count: 0,
            bridge_count: 0,
            producer_bridge_count: 0,
            generated_consumer_bridge_count: 0,
            indexed_vector_draw_bridge_count: 0,
            raw_shader_resource_bind_bridge_count: 0,
            closed_call_site_count: 0,
            geometry_creation_site: LAYER_490_RT_METHOD8_GEOMETRY_CREATION_SITE,
            geometry_source: LAYER_490_RT_METHOD8_GEOMETRY_SOURCE,
            index_buffer_usage_flag: LAYER_490_RT_METHOD8_INDEX_BUFFER_USAGE_FLAG,
            geometry_source_plan:
                native_vulkan_scene_layer_alpha_mask_rt_method8_geometry_source_plan(),
            bridges: Vec::new(),
            command_order: rt_method8_bridge_command_order(),
        }
    }

    fn from_bridges(
        command_count: usize,
        bridges: Vec<NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge>,
    ) -> Self {
        Self {
            command_count,
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
            command_order: rt_method8_bridge_command_order(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn bridge_for_producer_draw(
        &self,
        producer_draw_index: usize,
    ) -> Option<&NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge> {
        self.bridges.iter().find(|bridge| {
            bridge.producer_draw_index == Some(producer_draw_index)
                && bridge.purpose
                    == NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::ClippingMaskImage4Producer
        })
    }

    pub(in crate::renderer::native_vulkan) fn bridge_for_generated_consumer_draw(
        &self,
        consumer_draw_index: usize,
    ) -> Option<&NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge> {
        self.bridges.iter().find(|bridge| {
            bridge.generated_consumer_draw_index == Some(consumer_draw_index)
                && bridge.purpose
                    == NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::GeneratedClippingTargetConsumer
        })
    }
}

impl NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge {
    fn purpose_order(&self) -> u8 {
        match self.purpose {
            NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::ClippingMaskImage4Producer => 0,
            NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::GeneratedClippingTargetConsumer => 1,
        }
    }
}

fn rt_method8_bridge(
    command_index: usize,
    object: SceneObjectId,
    entry: SceneLayerCompositorEntry,
    operation: SceneLayerCompositorOperation,
    condition: SceneLayerCompositorCondition,
    purpose: NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose,
    producer_draw_index: Option<usize>,
    generated_consumer_draw_index: Option<usize>,
    call_site: &'static str,
    call_site_role: &'static str,
    draw_index_argument: &'static str,
) -> NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge {
    NativeVulkanSceneLayerAlphaMaskRtMethod8Bridge {
        bridge_index: 0,
        command_index,
        object,
        entry,
        operation,
        condition,
        purpose,
        producer_draw_index,
        generated_consumer_draw_index,
        receiver: SceneLayerCompositorTarget::LayerTarget490,
        receiver_field: LAYER_490_RT_METHOD8_RECEIVER_LABEL,
        receiver_vtable: LAYER_490_RT_METHOD8_RECEIVER_VTABLE,
        method_index: LAYER_490_RT_METHOD8_INDEX,
        method_offset: LAYER_490_RT_METHOD8_OFFSET,
        method_vma: LAYER_490_RT_METHOD8_VMA,
        draw_call: LAYER_490_RT_METHOD8_DRAW_CALL,
        call_site,
        call_site_role,
        draw_index_argument,
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

fn validate_producer_command(
    command_index: usize,
    command: &super::NativeVulkanSceneLayerAlphaMaskCommandPlan,
) -> Result<(), String> {
    if command.entry != SceneLayerCompositorEntry::AlphaMaskHelper20d6a0
        || command.operation != SceneLayerCompositorOperation::DrawClippingMask
        || command.source.is_some()
    {
        return Err(format!(
            "scene layer alpha-mask RT method [8] producer command {command_index} must be 0x14020d6a0 DrawClippingMask"
        ));
    }
    Ok(())
}

fn validate_generated_consumer_command(
    command_index: usize,
    command: &super::NativeVulkanSceneLayerAlphaMaskCommandPlan,
) -> Result<(), String> {
    if command.operation != SceneLayerCompositorOperation::DrawGeneratedClippingTarget
        || command.source != Some(SceneLayerCompositorTarget::FullAlphaMask)
        || command.target != SceneLayerCompositorTarget::LayerTarget490
    {
        return Err(format!(
            "scene layer alpha-mask RT method [8] generated consumer command {command_index} must be FullAlphaMask -> LayerTarget490 generated draw"
        ));
    }
    Ok(())
}

fn validate_generated_consumer_draw_identity(
    command_index: usize,
    consumer: &super::consumer_draws::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan,
) -> Result<(), String> {
    if consumer.target != SceneLayerCompositorTarget::LayerTarget490
        || consumer.target_receiver != LAYER_490_RT_METHOD8_RECEIVER_LABEL
        || consumer.draw_receiver_vtable_offset != LAYER_490_RT_METHOD8_OFFSET
    {
        return Err(format!(
            "scene layer alpha-mask RT method [8] generated consumer command {command_index} lost layer+0x490 receiver identity"
        ));
    }
    Ok(())
}

fn generated_consumer_call_site(
    command_index: usize,
    entry: SceneLayerCompositorEntry,
) -> Result<(&'static str, &'static str), String> {
    match entry {
        SceneLayerCompositorEntry::TokenizedCompositeEntry52 => Ok((
            "0x140208bbb",
            "vtable [52] token 1/2 generated CLIPPINGTARGET draw",
        )),
        SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53 => Ok((
            "0x14020908c",
            "vtable [53] token 1/2 generated CLIPPINGTARGET draw",
        )),
        entry => Err(format!(
            "scene layer alpha-mask RT method [8] generated consumer command {command_index} has unsupported entry {entry:?}"
        )),
    }
}

fn rt_method8_bridge_command_order() -> [&'static str; 7] {
    [
        "read_clippingmaskimage4_and_generated_draw_intents",
        "classify_layer_0x490_receiver_once",
        "map_producer_to_0x14020d83e",
        "map_generated_consumer_to_vtable_52_53_call_site",
        "preserve_rt_method_8_indexed_draw_identity",
        "preserve_0x14020b15e_wrapper_argument_contract",
        "expose_single_bridge_plan_to_recorder",
    ]
}

#[cfg(test)]
#[path = "rt_method8_tests.rs"]
mod tests;
