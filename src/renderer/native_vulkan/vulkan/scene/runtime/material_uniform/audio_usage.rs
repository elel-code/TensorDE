//! Audio-spectrum demand detection for retained scene draw pipelines.

use super::first_material_pass;
use crate::engine::scene::{SceneMaterialHandle, SceneRenderingDeviceMeshDraw, SceneStorage};
use crate::renderer::native_vulkan::native_vulkan_scene_backend_plan;
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneParameterLayout, native_vulkan_scene_shader_for_key,
};

pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) fn scene_uses_audio_spectrum(
    storage: &SceneStorage,
) -> bool {
    !storage.audio_band_material_bindings().is_empty()
        || native_vulkan_scene_backend_plan(storage)
            .rendering_device_graph
            .mesh_draws
            .iter()
            .any(|draw| draw_uses_audio_spectrum(storage, draw))
}

fn draw_uses_audio_spectrum(storage: &SceneStorage, draw: &SceneRenderingDeviceMeshDraw) -> bool {
    storage
        .string(draw.shader_key)
        .is_some_and(shader_uses_audio_spectrum)
        || material_uses_audio_spectrum(storage, draw.material)
}

pub(super) fn material_uses_audio_spectrum(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
) -> bool {
    let Some(pass) = first_material_pass(storage, material) else {
        return false;
    };
    let Some(shader_key) = storage.string(pass.shader_key) else {
        return false;
    };
    shader_uses_audio_spectrum(shader_key)
}

fn shader_uses_audio_spectrum(shader_key: &str) -> bool {
    native_vulkan_scene_shader_for_key(shader_key).is_some_and(|shader| {
        shader.parameter_layout == BuiltinSceneParameterLayout::AudioBars
            || shader.parameter_layout == BuiltinSceneParameterLayout::Oscilloscope
            || (shader.parameter_layout == BuiltinSceneParameterLayout::FinalEffectProgram
                && shader_key.eq_ignore_ascii_case("we/audio-bars-final"))
    })
}
