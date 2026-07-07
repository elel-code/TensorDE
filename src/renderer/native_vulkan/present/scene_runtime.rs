use std::path::PathBuf;
use std::time::Duration;

use crate::engine::scene_engine::SceneEnginePlan;

use super::super::{
    NativeVulkanError, NativeVulkanOptions, NativeVulkanVulkanaliaScenePresentOptions,
    NativeVulkanVulkanaliaScenePresentSnapshot, run_native_vulkan_vulkanalia_scene_present,
};

pub fn run_scene(
    options: NativeVulkanOptions,
    duration: Duration,
    shader_artifact_root: PathBuf,
    scene: SceneEnginePlan,
) -> Result<NativeVulkanVulkanaliaScenePresentSnapshot, NativeVulkanError> {
    run_native_vulkan_vulkanalia_scene_present(NativeVulkanVulkanaliaScenePresentOptions {
        host: options.host,
        wait_configure_roundtrips: options.wait_configure_roundtrips,
        duration,
        clear_color: options.clear_color,
        shader_artifact_root,
        scene,
    })
    .map_err(NativeVulkanError::Scene)
}

pub fn default_scene_shader_artifact_root() -> PathBuf {
    PathBuf::from("artifacts/scene-shaders")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_shader_artifact_root_defaults_to_formal_scene_artifacts() {
        assert_eq!(
            default_scene_shader_artifact_root(),
            PathBuf::from("artifacts/scene-shaders")
        );
    }
}
