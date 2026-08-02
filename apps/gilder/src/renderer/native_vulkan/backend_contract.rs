// Native Vulkan backend and wallpaper-type capability contract.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanWallpaperType {
    StaticImage,
    Video,
    Web,
    Scene,
    Shader,
    Playlist,
}

pub const WALLPAPER_TYPE_CONTRACT: &[NativeVulkanWallpaperType] = &[
    NativeVulkanWallpaperType::StaticImage,
    NativeVulkanWallpaperType::Video,
    NativeVulkanWallpaperType::Web,
    NativeVulkanWallpaperType::Scene,
    NativeVulkanWallpaperType::Shader,
    NativeVulkanWallpaperType::Playlist,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanWallpaperTypeSupport {
    pub wallpaper_type: NativeVulkanWallpaperType,
    pub current_vulkan_item: bool,
    pub current_renderer_status: &'static str,
    pub target_vulkan_path: &'static str,
}

pub fn wallpaper_type_support_matrix() -> Vec<NativeVulkanWallpaperTypeSupport> {
    vec![
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::StaticImage,
            current_vulkan_item: false,
            current_renderer_status: "no native static-image runtime is exposed while its old raw Vulkan route is removed; requests fail explicitly",
            target_vulkan_path: "cold image decode -> renderer-owned retained sampled image -> typed scene/image presentation plan",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Video,
            current_vulkan_item: true,
            current_renderer_status: "--run-video and scene VideoFrame use renderer-owned FFmpeg Vulkan decode, typed plane leases, descriptor heaps, and retained presentation transactions",
            target_vulkan_path: "typed media source/clock policy -> renderer-owned FFmpeg Vulkan decode -> retained AV_PIX_FMT_VULKAN/AVVkFrame plane leases -> descriptor-heap Y/UV sampling -> Wayland present",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Web,
            current_vulkan_item: false,
            current_renderer_status: "helper contract only; unsupported web presentation fails explicitly",
            target_vulkan_path: "Web helper -> DMABuf/EGLImage/shared-frame handoff -> Vulkan composite",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Scene,
            current_vulkan_item: true,
            current_renderer_status: "typed scene graph executes through renderer-owned descriptor heaps, retained offscreen SceneColor, native Slang pipelines, and FIFO-latest-ready terminal presentation",
            target_vulkan_path: "typed scene graph -> renderer-owned resources/commands -> offscreen SceneColor -> terminal swapchain pass",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Shader,
            current_vulkan_item: false,
            current_renderer_status: "shader contract only; unsupported shader wallpapers fail explicitly",
            target_vulkan_path: "fullscreen triangle -> native Slang 2026.14.1 O2 SPIR-V -> typed time/property uniforms",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Playlist,
            current_vulkan_item: false,
            current_renderer_status: "playlist selection remains in core render sync; no native playlist presentation route is exposed",
            target_vulkan_path: "core playlist decision -> selected child item -> same Vulkan runtime path",
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanBackendContract {
    pub backend_name: &'static str,
    pub default_renderer_candidate: bool,
    pub wallpaper_types: &'static [NativeVulkanWallpaperType],
    pub wallpaper_type_support: Vec<NativeVulkanWallpaperTypeSupport>,
    pub layer_shell_host: &'static str,
    pub render_plan_boundary: &'static str,
    pub lifecycle_boundary: &'static str,
    pub resource_telemetry_boundary: &'static str,
    pub required_instance_extensions: Vec<&'static str>,
    pub required_device_extensions: Vec<&'static str>,
    pub video_pipeline: pipeline::NativeVulkanVideoPipelineContract,
    pub video_flow: video_flow::NativeVulkanVideoFlowContract,
    pub video_interop: NativeVulkanVideoInteropContract,
    pub web_interop: NativeVulkanWebInteropContract,
    pub renderer: NativeVulkanRendererContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanRendererContract {
    pub required_api_version: &'static str,
    pub profile_name: &'static str,
    pub profile_revision: u32,
    pub descriptor_model: &'static str,
    pub present_mode: &'static str,
    pub required_instance_extensions: Vec<&'static str>,
    pub required_device_extensions: Vec<&'static str>,
}

pub fn backend_contract() -> NativeVulkanBackendContract {
    let renderer = native_vulkan_renderer_contract();
    let required_instance_extensions = renderer.required_instance_extensions.clone();
    let required_device_extensions = renderer.required_device_extensions.clone();
    NativeVulkanBackendContract {
        backend_name: "native-vulkan",
        default_renderer_candidate: false,
        wallpaper_types: WALLPAPER_TYPE_CONTRACT,
        wallpaper_type_support: wallpaper_type_support_matrix(),
        layer_shell_host: "NativeWaylandHost provides one Wayland surface handle to the renderer-owned presentation bootstrap",
        render_plan_boundary: "consume existing renderer plans; do not introduce Vulkan-only manifest semantics",
        lifecycle_boundary: "pause-dynamic, hidden/fullscreen/session release, resize, and output selection stay backend-neutral",
        resource_telemetry_boundary: "report CPU/RSS/PSS/private_dirty/GPU resource counts through stable renderer telemetry",
        required_instance_extensions,
        required_device_extensions,
        video_pipeline: pipeline::native_vulkan_video_pipeline_contract(),
        video_flow: video_flow::native_vulkan_video_flow_contract(),
        video_interop: video_interop_contract(),
        web_interop: web_interop_contract(),
        renderer,
    }
}

pub fn required_instance_extensions() -> Vec<&'static str> {
    native_vulkan_renderer_contract().required_instance_extensions
}

pub fn required_device_extensions() -> Vec<&'static str> {
    native_vulkan_renderer_contract().required_device_extensions
}

fn native_vulkan_renderer_contract() -> NativeVulkanRendererContract {
    NativeVulkanRendererContract {
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
        let contract = backend_contract();

        assert_eq!(contract.backend_name, "native-vulkan");
        assert_eq!(contract.renderer.required_api_version, "1.4.328");
        assert_eq!(contract.renderer.profile_name, "VP_KHR_roadmap_2026");
        assert_eq!(contract.renderer.profile_revision, 11);
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
        assert_eq!(contract.video_pipeline.reference, "FFmpeg packet/frame/clock model");
        let decode_owner = contract
            .video_flow
            .invariants
            .iter()
            .find(|invariant| invariant.contains("vulkan-renderer owns"))
            .expect("renderer-owned decode invariant");
        assert!(decode_owner.contains("FFmpeg demux/parser/packet send"));
        assert!(decode_owner.contains("Vulkan hw decode"));
        let shader_target = contract
            .wallpaper_type_support
            .iter()
            .find(|support| support.wallpaper_type == NativeVulkanWallpaperType::Shader)
            .expect("shader target contract")
            .target_vulkan_path;
        assert!(shader_target.contains("native Slang 2026.14.1 O2"));
        assert!(!shader_target.contains("GLSL"));
    }
}
