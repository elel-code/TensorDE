//! Scene shader artifact loading for WE shader variants.
//!
//! References:
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `reverse-engineered/shaders/genericimage4.vert`
//! - `reverse-engineered/shaders/genericimage4.frag`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::engine::scene_engine::WeShaderInterface;

use super::pipeline_factory::NativeVulkanSceneMeshPipelineShaders;

const SPIRV_MAGIC: u32 = 0x0723_0203;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneShaderArtifactPathPlan {
    pub shader: String,
    pub vertex_path: PathBuf,
    pub fragment_path: PathBuf,
    pub command_order: [&'static str; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneShaderArtifacts {
    pub shader: String,
    pub vertex_spirv: Vec<u32>,
    pub fragment_spirv: Vec<u32>,
}

impl NativeVulkanSceneShaderArtifacts {
    pub(in crate::renderer::native_vulkan) fn mesh_pipeline_shaders(
        &self,
    ) -> NativeVulkanSceneMeshPipelineShaders<'_> {
        NativeVulkanSceneMeshPipelineShaders {
            vertex_spirv: &self.vertex_spirv,
            fragment_spirv: &self.fragment_spirv,
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_shader_artifact_path_plan(
    artifact_root: &Path,
    shader: &str,
) -> Result<NativeVulkanSceneShaderArtifactPathPlan, String> {
    if artifact_root.as_os_str().is_empty() {
        return Err("scene shader artifact root cannot be empty".to_owned());
    }
    WeShaderInterface::for_shader(shader).ok_or_else(|| {
        format!("scene shader artifact plan references unknown WE shader '{shader}'")
    })?;
    let shader_path = scene_shader_artifact_relative_path(shader)?;
    Ok(NativeVulkanSceneShaderArtifactPathPlan {
        shader: shader.to_owned(),
        vertex_path: artifact_root.join(shader_path.with_extension("vert.spv")),
        fragment_path: artifact_root.join(shader_path.with_extension("frag.spv")),
        command_order: [
            "resolve_we_shader_artifact_paths",
            "read_vertex_spirv",
            "read_fragment_spirv",
        ],
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_load_scene_shader_artifacts(
    artifact_root: &Path,
    shader: &str,
) -> Result<NativeVulkanSceneShaderArtifacts, String> {
    let plan = native_vulkan_scene_shader_artifact_path_plan(artifact_root, shader)?;
    let vertex_bytes = std::fs::read(&plan.vertex_path).map_err(|err| {
        format!(
            "read scene vertex shader artifact {}: {err}",
            plan.vertex_path.display()
        )
    })?;
    let fragment_bytes = std::fs::read(&plan.fragment_path).map_err(|err| {
        format!(
            "read scene fragment shader artifact {}: {err}",
            plan.fragment_path.display()
        )
    })?;
    Ok(NativeVulkanSceneShaderArtifacts {
        shader: plan.shader,
        vertex_spirv: native_vulkan_scene_spirv_words_from_bytes(
            &vertex_bytes,
            "scene vertex shader artifact",
        )?,
        fragment_spirv: native_vulkan_scene_spirv_words_from_bytes(
            &fragment_bytes,
            "scene fragment shader artifact",
        )?,
    })
}

fn scene_shader_artifact_relative_path(shader: &str) -> Result<PathBuf, String> {
    let normalized = shader.strip_prefix("we/").unwrap_or(shader);
    if normalized.is_empty()
        || normalized.contains('\\')
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!(
            "scene shader artifact name '{shader}' cannot be mapped to a safe artifact path"
        ));
    }
    Ok(PathBuf::from("we").join(normalized))
}

fn native_vulkan_scene_spirv_words_from_bytes(
    bytes: &[u8],
    label: &'static str,
) -> Result<Vec<u32>, String> {
    if bytes.len() < 4 || bytes.len() % 4 != 0 {
        return Err(format!(
            "{label} is not valid SPIR-V: byte length {} is not a non-empty multiple of 4",
            bytes.len()
        ));
    }
    let mut words = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        words.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    if words.first().copied() != Some(SPIRV_MAGIC) {
        return Err(format!(
            "{label} is not valid SPIR-V: missing magic 0x{SPIRV_MAGIC:08x}"
        ));
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_artifact_plan_maps_we_shader_to_stage_spirv_paths() {
        let plan = native_vulkan_scene_shader_artifact_path_plan(
            Path::new("artifacts/scene-shaders"),
            "we/genericimage4",
        )
        .expect("artifact path plan");

        assert_eq!(plan.shader, "we/genericimage4");
        assert_eq!(
            plan.vertex_path,
            PathBuf::from("artifacts/scene-shaders/we/genericimage4.vert.spv")
        );
        assert_eq!(
            plan.fragment_path,
            PathBuf::from("artifacts/scene-shaders/we/genericimage4.frag.spv")
        );
        assert_eq!(
            plan.command_order,
            [
                "resolve_we_shader_artifact_paths",
                "read_vertex_spirv",
                "read_fragment_spirv"
            ]
        );
    }

    #[test]
    fn shader_artifact_plan_rejects_unknown_or_unsafe_shader_names() {
        assert!(
            native_vulkan_scene_shader_artifact_path_plan(Path::new("artifacts"), "we/notreal")
                .expect_err("unknown shader")
                .contains("unknown WE shader")
        );
        assert!(
            scene_shader_artifact_relative_path("../genericimage4")
                .expect_err("unsafe shader path")
                .contains("safe artifact path")
        );
    }

    #[test]
    fn spirv_words_are_decoded_little_endian_and_validated() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SPIRV_MAGIC.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());

        let words = native_vulkan_scene_spirv_words_from_bytes(&bytes, "test spirv")
            .expect("valid spirv words");

        assert_eq!(words, vec![SPIRV_MAGIC, 1]);
        assert!(
            native_vulkan_scene_spirv_words_from_bytes(&[1, 2, 3], "bad spirv")
                .expect_err("bad size")
                .contains("multiple of 4")
        );
        assert!(
            native_vulkan_scene_spirv_words_from_bytes(&[0, 0, 0, 0], "bad spirv")
                .expect_err("bad magic")
                .contains("missing magic")
        );
    }

    #[test]
    fn loaded_shader_artifacts_borrow_as_pipeline_shader_slices() {
        let artifacts = NativeVulkanSceneShaderArtifacts {
            shader: "we/genericimage4".to_owned(),
            vertex_spirv: vec![SPIRV_MAGIC, 1],
            fragment_spirv: vec![SPIRV_MAGIC, 2],
        };

        let shaders = artifacts.mesh_pipeline_shaders();

        assert_eq!(shaders.vertex_spirv, &[SPIRV_MAGIC, 1]);
        assert_eq!(shaders.fragment_spirv, &[SPIRV_MAGIC, 2]);
    }
}
