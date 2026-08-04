//! Package-owned Slang program for renderer-owned direct video present.

mod runtime;

pub use runtime::{
    RenderingDeviceSharedVideoPresentOptions, RenderingDeviceSharedVideoPresentSnapshot,
    run_rendering_device_shared_video_present,
};

use vulkan_renderer::DecodedVideoSurfaceTerminalProgram;

include!(concat!(
    env!("OUT_DIR"),
    "/tensor_wallpaper_shared_video_present_shaders.rs"
));

pub(super) fn shared_video_present_program() -> DecodedVideoSurfaceTerminalProgram<'static> {
    DecodedVideoSurfaceTerminalProgram {
        vertex_spirv: SHARED_VIDEO_PRESENT_VERTEX_SPIRV,
        fragment_spirv: SHARED_VIDEO_PRESENT_FRAGMENT_SPIRV,
        descriptor_push_bytes: SHARED_VIDEO_PRESENT_PUSH_BYTES,
    }
}
