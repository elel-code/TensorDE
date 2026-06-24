//! Native Vulkan external interop contracts.
//!
//! This module owns the stable policy surface for decoded-video memory handoff
//! and future Web/helper texture handoff. Low-level import implementations can
//! stay beside Vulkan code, but route selection and zero-copy claims should
//! point at this boundary.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoInteropContract {
    pub target_memory_flow: &'static str,
    pub current_baseline: &'static str,
    pub vulkan_binding_policy: &'static str,
    pub vulkanalia_replacement_policy: &'static str,
    pub vulkan_1_4_value: &'static str,
    pub ash_mainline_value: &'static str,
    pub target_sampling: &'static str,
    pub avoids_default_rgba_upload: bool,
    pub decoder_policy: &'static str,
    pub audio_strategy: &'static str,
    pub known_blockers: &'static [&'static str],
}

pub fn video_interop_contract() -> NativeVulkanVideoInteropContract {
    NativeVulkanVideoInteropContract {
        target_memory_flow: "decoder GPU memory -> importable DMABuf/EGLImage/Vulkan image -> Vulkan YUV sampling",
        current_baseline: "retired native-wgpu evidence: GStreamer CUDAMemory -> CUDA copy -> external Vulkan image planes -> wgpu present",
        vulkan_binding_policy: "vulkanalia is the prioritized replacement binding; ash remains the current stable runtime baseline until a Vulkan 1.4/video/present parity spike wins evidence; zero-copy evidence still comes from device extension/capability/import telemetry, not from the binding choice alone",
        vulkanalia_replacement_policy: "start the vulkanalia backend early behind a feature flag, then move instance/device capability probing, Vulkan Video submit helpers and present telemetry across one boundary at a time",
        vulkan_1_4_value: "Vulkan 1.4 is valuable for dynamic-rendering-local-read, push descriptors, maintenance5/6, scalar block layout, synchronization2 and stronger portable limits; it does not by itself prove Vulkan Video or zero-copy support",
        ash_mainline_value: "ash remains useful as a working comparison backend while vulkanalia proves Vulkan 1.4, Vulkan Video and external-memory parity",
        target_sampling: "NV12/P010/YUV planes sampled directly in Vulkan before RGB composition",
        avoids_default_rgba_upload: true,
        decoder_policy: "prefer GStreamer for codec/audio coverage; allow Vulkan Video or libavcodec import paths when they win evidence",
        audio_strategy: "keep audio pipeline separate from the video texture path so decoder choice does not block playback support",
        known_blockers: &[
            "direct gst_cuda_memory_export fd import returned zero Vulkan memory_type_bits on NVIDIA",
            "GLMemory DMABuf export may require libnvrtc on nvcodec systems",
            "native Vulkan import/decode must be judged against the retired 4K/240 native-wgpu evidence",
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanWebInteropContract {
    pub helper_boundary: &'static str,
    pub accepted_frame_sources: &'static [&'static str],
    pub blocked_designs: &'static [&'static str],
}

pub fn web_interop_contract() -> NativeVulkanWebInteropContract {
    NativeVulkanWebInteropContract {
        helper_boundary: "WebKitGTK or browser code stays in a helper; native Vulkan receives frames or importable textures",
        accepted_frame_sources: &[
            "DMABuf texture handoff",
            "EGLImage/exportable GL texture handoff",
            "shared-memory frame stream only as a fallback",
        ],
        blocked_designs: &[
            "making GTK/WebKitGTK the native Vulkan renderer host",
            "adding Web-specific daemon or manifest branches",
        ],
    }
}
