// Wallpaper presentation capability and renderer-requirement contract.

use serde::Serialize;

use crate::core::WallpaperKind;

use super::interop::{
    RenderingDeviceVideoInteropContract, RenderingDeviceWebInteropContract, video_interop_contract,
    web_interop_contract,
};
use super::pipeline;
use super::video::flow as video_flow;

pub const WALLPAPER_KINDS: &[WallpaperKind] = &[
    WallpaperKind::StaticImage,
    WallpaperKind::Video,
    WallpaperKind::Slideshow,
    WallpaperKind::Web,
    WallpaperKind::Scene,
    WallpaperKind::Shader,
    WallpaperKind::Playlist,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WallpaperKindSupport {
    pub wallpaper_kind: WallpaperKind,
    pub available: bool,
    pub status: &'static str,
    pub presentation_path: &'static str,
}

pub fn wallpaper_kind_support_matrix() -> Vec<WallpaperKindSupport> {
    vec![
        WallpaperKindSupport {
            wallpaper_kind: WallpaperKind::StaticImage,
            available: false,
            status: "no static-image presentation path is exposed; requests fail explicitly",
            presentation_path: "cold image decode -> renderer-owned retained sampled image -> typed scene/image presentation plan",
        },
        WallpaperKindSupport {
            wallpaper_kind: WallpaperKind::Video,
            available: true,
            status: "--run-video and scene VideoFrame use renderer-owned FFmpeg GPU decode, typed plane leases, descriptor heaps, and retained presentation transactions",
            presentation_path: "typed media source/clock policy -> renderer-owned FFmpeg GPU decode -> retained AV_PIX_FMT_VULKAN/AVVkFrame plane leases -> descriptor-heap Y/UV sampling -> Wayland present",
        },
        WallpaperKindSupport {
            wallpaper_kind: WallpaperKind::Slideshow,
            available: false,
            status: "slideshow selection remains in core render sync; no slideshow presentation path is exposed",
            presentation_path: "core slideshow decision -> selected image -> image presentation path",
        },
        WallpaperKindSupport {
            wallpaper_kind: WallpaperKind::Web,
            available: false,
            status: "helper contract only; unsupported web presentation fails explicitly",
            presentation_path: "Web helper -> DMA-BUF/EGLImage/shared-frame handoff -> renderer composite",
        },
        WallpaperKindSupport {
            wallpaper_kind: WallpaperKind::Scene,
            available: true,
            status: "typed scene graph executes through renderer-owned descriptor heaps, retained offscreen SceneColor, Slang pipelines, and FIFO-latest-ready terminal presentation",
            presentation_path: "typed scene graph -> renderer-owned resources/commands -> offscreen SceneColor -> terminal swapchain pass",
        },
        WallpaperKindSupport {
            wallpaper_kind: WallpaperKind::Shader,
            available: false,
            status: "shader contract only; unsupported shader wallpapers fail explicitly",
            presentation_path: "fullscreen triangle -> Slang 2026.14.1 O2 SPIR-V -> typed time/property uniforms",
        },
        WallpaperKindSupport {
            wallpaper_kind: WallpaperKind::Playlist,
            available: false,
            status: "playlist selection remains in core render sync; no playlist presentation path is exposed",
            presentation_path: "core playlist decision -> selected child item -> selected presentation path",
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WallpaperPresentationContract {
    pub component: &'static str,
    pub wallpaper_kinds: &'static [WallpaperKind],
    pub wallpaper_kind_support: Vec<WallpaperKindSupport>,
    pub layer_shell_host: &'static str,
    pub render_plan_boundary: &'static str,
    pub lifecycle_boundary: &'static str,
    pub resource_telemetry_boundary: &'static str,
    pub required_instance_extensions: Vec<&'static str>,
    pub required_device_extensions: Vec<&'static str>,
    pub video_pipeline: pipeline::RenderingDeviceVideoPipelineContract,
    pub video_flow: video_flow::RenderingDeviceVideoFlowContract,
    pub video_interop: RenderingDeviceVideoInteropContract,
    pub web_interop: RenderingDeviceWebInteropContract,
    pub renderer_requirements: RendererRequirements,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RendererRequirements {
    pub required_api_version: &'static str,
    pub profile_name: &'static str,
    pub profile_revision: u32,
    pub descriptor_model: &'static str,
    pub present_mode: &'static str,
    pub required_instance_extensions: Vec<&'static str>,
    pub required_device_extensions: Vec<&'static str>,
}

pub fn wallpaper_presentation_contract() -> WallpaperPresentationContract {
    let renderer_requirements = renderer_requirements();
    let required_instance_extensions = renderer_requirements.required_instance_extensions.clone();
    let required_device_extensions = renderer_requirements.required_device_extensions.clone();
    WallpaperPresentationContract {
        component: "wallpaper-presentation",
        wallpaper_kinds: WALLPAPER_KINDS,
        wallpaper_kind_support: wallpaper_kind_support_matrix(),
        layer_shell_host: "WaylandHost provides one Wayland surface handle to the renderer-owned presentation bootstrap",
        render_plan_boundary: "consume existing renderer plans; do not introduce Vulkan-only manifest semantics",
        lifecycle_boundary: "pause-dynamic, hidden/fullscreen/session release, resize, and output selection stay presentation-path-neutral",
        resource_telemetry_boundary: "report CPU/RSS/PSS/private_dirty/GPU resource counts through stable renderer telemetry",
        required_instance_extensions,
        required_device_extensions,
        video_pipeline: pipeline::rendering_device_video_pipeline_contract(),
        video_flow: video_flow::rendering_device_video_flow_contract(),
        video_interop: video_interop_contract(),
        web_interop: web_interop_contract(),
        renderer_requirements,
    }
}

pub fn required_instance_extensions() -> Vec<&'static str> {
    renderer_requirements().required_instance_extensions
}

pub fn required_device_extensions() -> Vec<&'static str> {
    renderer_requirements().required_device_extensions
}

fn renderer_requirements() -> RendererRequirements {
    RendererRequirements {
        required_api_version: "1.4.328",
        profile_name: vulkan_renderer::ROADMAP_2026_PROFILE_NAME,
        profile_revision: vulkan_renderer::ROADMAP_2026_PROFILE_REVISION,
        descriptor_model: "VK_EXT_descriptor_heap",
        present_mode: "fifo-latest-ready",
        required_instance_extensions: vulkan_renderer::ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS
            .to_vec(),
        required_device_extensions: vulkan_renderer::ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS
            .to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_contract_has_one_decode_owner() {
        let contract = wallpaper_presentation_contract();

        assert_eq!(contract.component, "wallpaper-presentation");
        assert_eq!(
            contract.renderer_requirements.required_api_version,
            "1.4.328"
        );
        assert_eq!(
            contract.renderer_requirements.profile_name,
            "VP_KHR_roadmap_2026"
        );
        assert_eq!(contract.renderer_requirements.profile_revision, 11);
        assert!(
            contract
                .required_instance_extensions
                .contains(&"VK_KHR_surface_maintenance1")
        );
        assert!(
            contract
                .required_device_extensions
                .contains(&"VK_KHR_cooperative_matrix")
        );
        assert!(contract.video_interop.avoids_default_rgba_upload);
        assert_eq!(
            contract.video_pipeline.reference,
            "FFmpeg packet/frame/clock model"
        );
        let decode_owner = contract
            .video_flow
            .invariants
            .iter()
            .find(|invariant| invariant.contains("vulkan-renderer owns"))
            .expect("renderer-owned decode invariant");
        assert!(decode_owner.contains("FFmpeg demux/parser/packet send"));
        assert!(decode_owner.contains("Vulkan hw decode"));
        let shader_target = contract
            .wallpaper_kind_support
            .iter()
            .find(|support| support.wallpaper_kind == WallpaperKind::Shader)
            .expect("shader target contract")
            .presentation_path;
        assert!(shader_target.contains("Slang 2026.14.1 O2"));
        assert!(!shader_target.contains("GLSL"));
    }
}
