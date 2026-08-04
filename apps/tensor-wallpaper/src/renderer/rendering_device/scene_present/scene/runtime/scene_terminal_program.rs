//! Package-owned Slang terminal program for the shared renderer.

use vulkan_renderer::FullscreenSampledSurfaceTerminalProgram;

include!(concat!(env!("OUT_DIR"), "/tensor_wallpaper_scene_present_shaders.rs"));

pub(super) fn scene_terminal_program() -> FullscreenSampledSurfaceTerminalProgram<'static> {
    FullscreenSampledSurfaceTerminalProgram {
        vertex_spirv: SCENE_TERMINAL_PRESENT_VERTEX_SPIRV,
        fragment_spirv: SCENE_TERMINAL_PRESENT_FRAGMENT_SPIRV,
        descriptor_push_bytes: SCENE_TERMINAL_PRESENT_PUSH_BYTES,
    }
}
