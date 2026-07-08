//! Parameter-level WE RT method [8] geometry source contract.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;

use super::rt_method8::{
    LAYER_490_RT_METHOD8_GEOMETRY_CREATION_SITE, LAYER_490_RT_METHOD8_RECEIVER_LABEL,
    LAYER_490_RT_METHOD8_RECEIVER_VTABLE, LAYER_490_RT_METHOD8_VMA,
};
use super::rt_method8_payload::{
    NativeVulkanSceneLayerAlphaMaskRtMethod8PayloadPlan,
    native_vulkan_scene_layer_alpha_mask_rt_method8_payload_plan,
};

pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_WRAPPER_CREATE_VMA: &str =
    "0x14009a880";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_ACTIVE_MATERIAL_CREATION_SITE:
    &str = "0x14020b1e8";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_FALLBACK_IMAGE_CREATION_SITE:
    &str = "0x14020a4ff";
pub(in crate::renderer::native_vulkan) const LAYER_490_RT_METHOD8_RUNTIME_BINDING_GAP: &str =
    "retained Vulkan buffer binding for [layer+0x490] MDLV entry geometry and aux+0x298 lowering";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8GeometrySourcePlan
{
    pub creation_site: &'static str,
    pub stored_receiver_field: &'static str,
    pub wrapper_create_method_vma: &'static str,
    pub created_rt_vtable: &'static str,
    pub rt_draw_method_vma: &'static str,
    pub wrapper_argument_count: usize,
    pub wrapper_arguments: [NativeVulkanSceneLayerAlphaMaskRtMethod8WrapperArgumentSource; 9],
    pub created_field_count: usize,
    pub created_fields: [NativeVulkanSceneLayerAlphaMaskRtMethod8CreatedField; 7],
    pub usage_selector: NativeVulkanSceneLayerAlphaMaskRtMethod8UsageSelector,
    pub payload_plan: NativeVulkanSceneLayerAlphaMaskRtMethod8PayloadPlan,
    pub sibling_creation_site_count: usize,
    pub sibling_creation_sites: [NativeVulkanSceneLayerAlphaMaskRtMethod8SiblingCreationSite; 2],
    pub remaining_runtime_gap: &'static str,
    pub remaining_runtime_fact: &'static str,
    pub reference_points: [&'static str; 4],
    pub command_order: [&'static str; 7],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8WrapperArgumentSource
{
    pub argument: &'static str,
    pub value_source: &'static str,
    pub semantic: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8CreatedField {
    pub field: &'static str,
    pub semantic: &'static str,
    pub source: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8UsageSelector
{
    pub stack_argument: &'static str,
    pub source: &'static str,
    pub bit_zero_semantic: &'static str,
    pub bit_one_semantic: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRtMethod8SiblingCreationSite
{
    pub creation_site: &'static str,
    pub stored_target: &'static str,
    pub source: &'static str,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_layer_alpha_mask_rt_method8_geometry_source_plan()
-> NativeVulkanSceneLayerAlphaMaskRtMethod8GeometrySourcePlan {
    NativeVulkanSceneLayerAlphaMaskRtMethod8GeometrySourcePlan {
        creation_site: LAYER_490_RT_METHOD8_GEOMETRY_CREATION_SITE,
        stored_receiver_field: LAYER_490_RT_METHOD8_RECEIVER_LABEL,
        wrapper_create_method_vma: LAYER_490_RT_METHOD8_WRAPPER_CREATE_VMA,
        created_rt_vtable: LAYER_490_RT_METHOD8_RECEIVER_VTABLE,
        rt_draw_method_vma: LAYER_490_RT_METHOD8_VMA,
        wrapper_argument_count: 9,
        wrapper_arguments: [
            NativeVulkanSceneLayerAlphaMaskRtMethod8WrapperArgumentSource {
                argument: "rcx",
                value_source: "r11 = [[layer+0xc8]+0x1518]",
                semantic: "render-state wrapper receiver for target creation",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8WrapperArgumentSource {
                argument: "edx",
                value_source: "[[layer+0x4b8]+0x18]+0x38",
                semantic: "MDLV entry vertex layout/format key; 0x1400ea5b0(edx) computes stride",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8WrapperArgumentSource {
                argument: "r8",
                value_source: "[entry+0x48] or temporary scaled copy from 0x14020af79..0x14020b102",
                semantic: "MDLV vertex-data pointer",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8WrapperArgumentSource {
                argument: "r9d",
                value_source: "[entry+0x40] / [entry+0x3c]",
                semantic: "MDLV vertex count",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8WrapperArgumentSource {
                argument: "stack arg 5",
                value_source: "[entry+0x58]",
                semantic: "MDLV u16 index-data pointer",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8WrapperArgumentSource {
                argument: "stack arg 6",
                value_source: "[entry+0x50] / 2",
                semantic: "u16 index count; positive value becomes target draw count +0x2c",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8WrapperArgumentSource {
                argument: "stack arg 7",
                value_source: "0",
                semantic: "index width selector; 0 maps to DXGI_FORMAT_R16_UINT",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8WrapperArgumentSource {
                argument: "stack arg 8",
                value_source: "0",
                semantic: "primitive topology selector; 0 maps to triangle list",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8WrapperArgumentSource {
                argument: "stack arg 9",
                value_source: "((entry+0x18 >> 3) & 1) * 2",
                semantic: "buffer usage flags; bit 1 controls dynamic index-buffer creation",
            },
        ],
        created_field_count: 7,
        created_fields: [
            NativeVulkanSceneLayerAlphaMaskRtMethod8CreatedField {
                field: "+0x10",
                semantic: "vertex buffer",
                source: "wrapper [8] vertex-buffer creation from r8/stride*vertex_count",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8CreatedField {
                field: "+0x18",
                semantic: "index buffer",
                source: "wrapper [8] index-buffer creation from stack arg 5/index byte count",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8CreatedField {
                field: "+0x20",
                semantic: "index format",
                source: "stack arg 7: 0 -> 0x39 R16, nonzero -> 0x2a R32",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8CreatedField {
                field: "+0x24",
                semantic: "vertex layout key",
                source: "edx",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8CreatedField {
                field: "+0x28",
                semantic: "vertex stride",
                source: "0x1400ea5b0(edx)",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8CreatedField {
                field: "+0x2c",
                semantic: "draw/index count",
                source: "stack arg 6 when positive, otherwise r9d vertex count",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8CreatedField {
                field: "+0x30",
                semantic: "primitive topology",
                source: "stack arg 8: 0 -> 4 triangle list, 1 -> 2 line list, else 1 point list",
            },
        ],
        usage_selector: NativeVulkanSceneLayerAlphaMaskRtMethod8UsageSelector {
            stack_argument: "stack arg 9",
            source: "((entry+0x18 >> 3) & 1) * 2",
            bit_zero_semantic: "dynamic vertex-buffer creation",
            bit_one_semantic: "dynamic index-buffer creation",
        },
        payload_plan: native_vulkan_scene_layer_alpha_mask_rt_method8_payload_plan(),
        sibling_creation_site_count: 2,
        sibling_creation_sites: [
            NativeVulkanSceneLayerAlphaMaskRtMethod8SiblingCreationSite {
                creation_site: LAYER_490_RT_METHOD8_ACTIVE_MATERIAL_CREATION_SITE,
                stored_target: "[layer+0x4b8]+0x3f8",
                source: "active material entry: layout +0x38, vertex +0x48/+0x40/+0x3c, index +0x58/+0x50",
            },
            NativeVulkanSceneLayerAlphaMaskRtMethod8SiblingCreationSite {
                creation_site: LAYER_490_RT_METHOD8_FALLBACK_IMAGE_CREATION_SITE,
                stored_target: "[layer+0x4b8]+0x400",
                source: "fallback image entry: generated layout key, vertex +0x98/+0x90, index +0x58/+0x50",
            },
        ],
        remaining_runtime_gap: LAYER_490_RT_METHOD8_RUNTIME_BINDING_GAP,
        remaining_runtime_fact: "WE payload source is closed; Gilder still must retain/bind the MDLV vertex/index buffers and lower aux+0x298 records before recording draw commands",
        reference_points: [
            "reverse-engineered/docs/exe/blend-and-render.md: wrapper [8] arguments, MDLV entry geometry, and aux+0x298 payload",
            "reverse-engineered/docs/exe/d3d11-context-calls.md: 0x14009a880 create indexed RT/draw-target object",
            "reverse-engineered/docs/exe/composelayer-and-effecttarget.md: [layer+0x490] method [8] draw receiver",
            "references/godot/servers/rendering/rendering_device_graph.cpp: draw resources are modeled before command recording",
        ],
        command_order: [
            "classify_0x14009a880_as_indexed_rt_target_factory",
            "map_0x14020b15e_call_registers_and_stack_arguments",
            "attach_0x14020ae00_mdlv_payload_contract",
            "store_created_target_at_layer_0x490",
            "preserve_wrapper_created_vertex_index_buffer_fields",
            "separate_layer_0x490_from_0x3f8_and_0x400_sibling_targets",
            "defer_only_retained_buffer_binding_and_aux_payload_lowering",
        ],
    }
}

#[cfg(test)]
#[path = "rt_method8_geometry_tests.rs"]
mod tests;
