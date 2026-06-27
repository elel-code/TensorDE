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
    pub vulkanalia_primary_policy: &'static str,
    pub vulkan_1_4_value: &'static str,
    pub removed_ash_baseline: &'static str,
    pub target_sampling: &'static str,
    pub avoids_default_rgba_upload: bool,
    pub decoder_policy: &'static str,
    pub audio_strategy: &'static str,
    pub known_blockers: &'static [&'static str],
}

pub fn video_interop_contract() -> NativeVulkanVideoInteropContract {
    NativeVulkanVideoInteropContract {
        target_memory_flow: "Vulkan Video decoded image -> descriptor heap sampled YUV planes -> Vulkan composition/present",
        current_baseline: "native Vulkan Video direct decode/render/present with FFmpeg packet frontend",
        vulkan_binding_policy: "vulkanalia is the native Vulkan binding; the ash dependency and runtime baseline are removed, and zero-copy evidence comes from device extension/capability/import telemetry rather than the binding choice alone",
        vulkanalia_primary_policy: "vulkanalia owns the native-vulkan-renderer surface for instance/device ownership, Vulkan Video submit helpers, image/import resources and present telemetry",
        vulkan_1_4_value: "Vulkan 1.4 is valuable for dynamic-rendering-local-read, host image copy, push descriptors, maintenance5/6, scalar block layout, synchronization2 and stronger portable limits; it does not by itself prove Vulkan Video or zero-copy support",
        removed_ash_baseline: "ash is removed; Vulkanalia owns Vulkan 1.4, Vulkan Video and external-memory parity work",
        target_sampling: "NV12/P010/YUV planes sampled directly in Vulkan before RGB composition",
        avoids_default_rgba_upload: true,
        decoder_policy: "prefer native Vulkan Video for H.264/H.265/AV1; FFmpeg remains demux/bitstream-filter frontend only",
        audio_strategy: "keep audio pipeline separate from the video texture path so decoder choice does not block playback support",
        known_blockers: &[
            "native Vulkan decode/render/present must stay under the current 4K/240 Private_Dirty and FPS evidence",
            "packet queues and bitstream rings must remain bounded and FFmpeg-semantics aligned",
            "descriptor heap must remain the only shader resource binding model",
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
        helper_boundary: "browser helper code stays out of daemon/core; native Vulkan receives frames or importable textures",
        accepted_frame_sources: &[
            "DMABuf texture handoff",
            "EGLImage/exportable GL texture handoff",
            "shared-memory frame stream only as a fallback",
        ],
        blocked_designs: &[
            "making a browser toolkit the native Vulkan renderer host",
            "adding Web-specific daemon or manifest branches",
        ],
    }
}
