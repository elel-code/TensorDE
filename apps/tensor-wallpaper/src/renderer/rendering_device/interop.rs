//! Vulkan external interop contracts.
//!
//! This module owns the stable policy surface for decoded-video memory handoff
//! and future Web/helper texture handoff. Low-level import implementations can
//! stay beside Vulkan code, but route selection and zero-copy claims should
//! point at this boundary.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceVideoInteropContract {
    pub target_memory_flow: &'static str,
    pub current_baseline: &'static str,
    pub vulkan_binding_policy: &'static str,
    pub renderer_ownership_policy: &'static str,
    pub vulkan_1_4_value: &'static str,
    pub target_sampling: &'static str,
    pub avoids_default_rgba_upload: bool,
    pub decoder_policy: &'static str,
    pub audio_strategy: &'static str,
    pub known_blockers: &'static [&'static str],
}

pub fn video_interop_contract() -> RenderingDeviceVideoInteropContract {
    RenderingDeviceVideoInteropContract {
        target_memory_flow: "renderer-owned FFmpeg AVVkFrame image -> opaque retained Y/UV plane leases -> descriptor heap sampled composition/present",
        current_baseline: "vulkan-renderer owns FFmpeg Vulkan decode, AV_PIX_FMT_VULKAN validation, plane views, synchronization, descriptor heaps and presentation; Tensor Wallpaper never borrows FFmpeg or Vulkan handles",
        vulkan_binding_policy: "vulkan-renderer contains Vulkanalia behind typed ownership; zero-copy evidence comes from exact plane leases, synchronization and command scope rather than binding spelling",
        renderer_ownership_policy: "vulkan-renderer owns the surface/device, FFmpeg device integration, AVVkFrame plane leases, descriptor sampling and presentation transaction",
        vulkan_1_4_value: "Vulkan 1.4.328 plus VP_KHR_roadmap_2026 revision 11 is the mandatory device baseline for every Vulkan route",
        target_sampling: "NV12/P010/YUV planes sampled directly in Vulkan before RGB composition",
        avoids_default_rgba_upload: true,
        decoder_policy: "renderer-owned FFmpeg Vulkan decode is the sole H.264/H.265/AV1 runtime and rejects non-AV_PIX_FMT_VULKAN output",
        audio_strategy: "scene audio spectrum remains independent; the removed standalone raw audio clock cannot hold video-frame ownership",
        known_blockers: &[
            "renderer-owned FFmpeg Vulkan decode must keep 4K240 FPS stable while retained plane leases and descriptor heaps remain bounded",
            "typed audio policy must be added independently before standalone video audio output is reintroduced",
            "descriptor heap must remain the only shader resource binding model",
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceWebInteropContract {
    pub helper_boundary: &'static str,
    pub accepted_frame_sources: &'static [&'static str],
    pub blocked_designs: &'static [&'static str],
}

pub fn web_interop_contract() -> RenderingDeviceWebInteropContract {
    RenderingDeviceWebInteropContract {
        helper_boundary: "browser helper code stays out of daemon/core; Vulkan receives frames or importable textures",
        accepted_frame_sources: &[
            "DMABuf texture handoff",
            "EGLImage/exportable GL texture handoff",
            "shared-memory frame stream only as a fallback",
        ],
        blocked_designs: &[
            "making a browser toolkit the Vulkan renderer host",
            "adding Web-specific daemon or manifest branches",
        ],
    }
}
