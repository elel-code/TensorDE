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
            current_vulkan_item: true,
            current_renderer_status: "--run-static lowers static images into a single scene sampled-image layer, then uses Vulkanalia sampled-image dynamic rendering; ash static session and staging-copy runtime are removed",
            target_vulkan_path: "decode image once -> retained sampled Vulkan image -> fit-aware dynamic-rendering pass shared with scene/image layers",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Video,
            current_vulkan_item: true,
            current_renderer_status: "--run-video uses the sole FFmpeg Vulkan HW decode route; native parser and direct Vulkan Video submit code has been removed",
            target_vulkan_path: "FFmpeg demux/parser/avcodec Vulkan hwaccel -> AV_PIX_FMT_VULKAN/AVVkFrame -> VK_EXT_descriptor_heap Y/UV sampling -> Wayland present",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Web,
            current_vulkan_item: false,
            current_renderer_status: "helper contract only; current render plan may fall back to static image",
            target_vulkan_path: "Web helper -> DMABuf/EGLImage/shared-frame handoff -> Vulkan composite",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Scene,
            current_vulkan_item: true,
            current_renderer_status: "deterministic scene snapshot layers carried by Vulkan render item; static images lower into single-image scene layers; native draw-pass plan, fast-clear color path, color/rectangle quads and sampled-image geometry exist, text/path rasterization remains pending",
            target_vulkan_path: "deterministic scene snapshot -> Vulkan shape/image/text passes",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Shader,
            current_vulkan_item: false,
            current_renderer_status: "shader contract only; current render plan may fall back to static image",
            target_vulkan_path: "fullscreen triangle -> GLSL/WGSL-derived SPIR-V -> time/property uniforms",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Playlist,
            current_vulkan_item: false,
            current_renderer_status: "playlist selection remains in core render sync; selected child maps to Vulkan item",
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
    #[cfg(feature = "native-vulkan-video")]
    pub ffmpeg_hw_decode: NativeVulkanFfmpegHwDecodeBackendContract,
    pub video_interop: NativeVulkanVideoInteropContract,
    pub web_interop: NativeVulkanWebInteropContract,
    pub vulkan_backend: NativeVulkanBackendPlan,
}

pub fn backend_contract() -> NativeVulkanBackendContract {
    let vulkan_backend = native_vulkan_backend_plan();
    let required_instance_extensions = vulkan_backend.required_instance_extensions.to_vec();
    let required_device_extensions = required_device_extensions_from_plan(&vulkan_backend);
    NativeVulkanBackendContract {
        backend_name: "native-vulkan",
        default_renderer_candidate: false,
        wallpaper_types: WALLPAPER_TYPE_CONTRACT,
        wallpaper_type_support: wallpaper_type_support_matrix(),
        layer_shell_host: "reuse NativeWaylandHost raw wl_display/wl_surface first, then move ownership here",
        render_plan_boundary: "consume existing renderer plans; do not introduce Vulkan-only manifest semantics",
        lifecycle_boundary: "pause-dynamic, hidden/fullscreen/session release, resize, and output selection stay backend-neutral",
        resource_telemetry_boundary: "report CPU/RSS/PSS/private_dirty/GPU resource counts through stable renderer telemetry",
        required_instance_extensions,
        required_device_extensions,
        video_pipeline: pipeline::native_vulkan_video_pipeline_contract(),
        video_flow: video_flow::native_vulkan_video_flow_contract(),
        #[cfg(feature = "native-vulkan-video")]
        ffmpeg_hw_decode: ffmpeg_hw::native_vulkan_ffmpeg_hw_decode_backend_contract(),
        video_interop: video_interop_contract(),
        web_interop: web_interop_contract(),
        vulkan_backend,
    }
}

pub fn required_instance_extensions() -> Vec<&'static str> {
    native_vulkan_backend_plan()
        .required_instance_extensions
        .to_vec()
}

pub fn required_device_extensions() -> Vec<&'static str> {
    required_device_extensions_from_plan(&native_vulkan_backend_plan())
}

fn required_device_extensions_from_plan(
    plan: &NativeVulkanBackendPlan,
) -> Vec<&'static str> {
    let mut required = Vec::new();
    for extension in plan
        .required_profile_device_extensions
        .iter()
        .chain(plan.required_scene_device_extensions)
        .chain(plan.required_video_route_device_extensions)
    {
        if !required.contains(extension) {
            required.push(*extension);
        }
    }
    required
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_contract_has_one_decode_owner() {
        let contract = backend_contract();

        assert_eq!(contract.backend_name, "native-vulkan");
        assert_eq!(contract.vulkan_backend.required_api_version, "1.4.328");
        assert_eq!(contract.vulkan_backend.profile_name, "VP_KHR_roadmap_2026");
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
        assert!(contract.video_flow.invariants.iter().any(|invariant| {
            invariant.contains("FFmpeg owns demux/parser/packet send and Vulkan hw decode")
        }));
    }
}
