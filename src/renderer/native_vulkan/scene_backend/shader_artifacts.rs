//! Scene shader artifact loading for WE shader variants.
//!
//! References:
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `reverse-engineered/shaders/genericimage4.vert`
//! - `reverse-engineered/shaders/genericimage4.frag`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::engine::scene_engine::{SceneEffectPassGraphPlan, WeShaderInterface};

use super::effect_pipeline::{
    NativeVulkanSceneEffectPipelineCacheKey, NativeVulkanSceneEffectPipelineShaders,
};
use super::pipeline_factory::NativeVulkanSceneMeshPipelineShaders;

const SPIRV_MAGIC: u32 = 0x0723_0203;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneShaderArtifactPathPlan {
    pub shader: String,
    pub vertex_path: PathBuf,
    pub fragment_path: PathBuf,
    pub command_order: [&'static str; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectShaderArtifactPathPlan {
    pub shader: String,
    pub vertex_path: PathBuf,
    pub fragment_path: PathBuf,
    pub source_reference: String,
    pub command_order: [&'static str; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneShaderArtifacts {
    pub shader: String,
    pub vertex_spirv: Vec<u32>,
    pub fragment_spirv: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectShaderArtifacts {
    pub shader: String,
    pub vertex_spirv: Vec<u32>,
    pub fragment_spirv: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectShaderArtifactCatalogPlan {
    pub shader_count: usize,
    pub shaders: Vec<String>,
    pub command_order: [&'static str; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectShaderArtifactCatalog {
    shaders: BTreeMap<String, NativeVulkanSceneEffectShaderArtifacts>,
    plan: NativeVulkanSceneEffectShaderArtifactCatalogPlan,
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

impl NativeVulkanSceneEffectShaderArtifacts {
    pub(in crate::renderer::native_vulkan) fn effect_pipeline_shaders(
        &self,
    ) -> NativeVulkanSceneEffectPipelineShaders<'_> {
        NativeVulkanSceneEffectPipelineShaders {
            vertex_spirv: &self.vertex_spirv,
            fragment_spirv: &self.fragment_spirv,
        }
    }
}

impl NativeVulkanSceneEffectShaderArtifactCatalog {
    pub(in crate::renderer::native_vulkan) fn from_effect_pass_graph(
        artifact_root: &Path,
        graph: &SceneEffectPassGraphPlan,
    ) -> Result<Self, String> {
        let shader_names = required_effect_shader_names(graph)?;
        let mut shaders = BTreeMap::new();
        for shader in &shader_names {
            let artifacts =
                native_vulkan_load_scene_effect_shader_artifacts(artifact_root, shader)?;
            shaders.insert(shader.clone(), artifacts);
        }
        Ok(Self {
            plan: NativeVulkanSceneEffectShaderArtifactCatalogPlan {
                shader_count: shader_names.len(),
                shaders: shader_names,
                command_order: [
                    "collect_effect_shader_names_from_pass_graph",
                    "resolve_we_effect_shader_artifact_paths",
                    "read_unique_effect_vertex_spirv",
                    "read_unique_effect_fragment_spirv",
                ],
            },
            shaders,
        })
    }

    pub(in crate::renderer::native_vulkan) fn empty() -> Self {
        Self {
            shaders: BTreeMap::new(),
            plan: NativeVulkanSceneEffectShaderArtifactCatalogPlan {
                shader_count: 0,
                shaders: Vec::new(),
                command_order: [
                    "collect_effect_shader_names_from_pass_graph",
                    "resolve_we_effect_shader_artifact_paths",
                    "read_unique_effect_vertex_spirv",
                    "read_unique_effect_fragment_spirv",
                ],
            },
        }
    }

    pub(in crate::renderer::native_vulkan) fn effect_pipeline_shaders_for_key(
        &self,
        key: &NativeVulkanSceneEffectPipelineCacheKey,
    ) -> Result<NativeVulkanSceneEffectPipelineShaders<'_>, String> {
        self.shaders
            .get(&key.shader)
            .ok_or_else(|| {
                format!(
                    "scene effect shader catalog has no artifact for shader '{}'",
                    key.shader
                )
            })
            .map(|artifacts| artifacts.effect_pipeline_shaders())
    }

    pub(in crate::renderer::native_vulkan) fn shader_count(&self) -> usize {
        self.plan.shader_count
    }

    pub(in crate::renderer::native_vulkan) fn plan(
        &self,
    ) -> &NativeVulkanSceneEffectShaderArtifactCatalogPlan {
        &self.plan
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

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_effect_shader_artifact_path_plan(
    artifact_root: &Path,
    shader: &str,
) -> Result<NativeVulkanSceneEffectShaderArtifactPathPlan, String> {
    if artifact_root.as_os_str().is_empty() {
        return Err("scene effect shader artifact root cannot be empty".to_owned());
    }
    let shader_path = scene_effect_shader_artifact_relative_path(shader)?;
    Ok(NativeVulkanSceneEffectShaderArtifactPathPlan {
        shader: shader.to_owned(),
        vertex_path: artifact_root.join(shader_path.with_extension("vert.spv")),
        fragment_path: artifact_root.join(shader_path.with_extension("frag.spv")),
        source_reference: format!("reverse-engineered/shaders/{}", shader_path.display()),
        command_order: [
            "resolve_we_effect_shader_artifact_paths",
            "require_reverse_engineered_effect_shader_source",
            "read_effect_vertex_spirv",
            "read_effect_fragment_spirv",
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

pub(in crate::renderer::native_vulkan) fn native_vulkan_load_scene_effect_shader_artifacts(
    artifact_root: &Path,
    shader: &str,
) -> Result<NativeVulkanSceneEffectShaderArtifacts, String> {
    let plan = native_vulkan_scene_effect_shader_artifact_path_plan(artifact_root, shader)?;
    let vertex_bytes = std::fs::read(&plan.vertex_path).map_err(|err| {
        format!(
            "read scene effect vertex shader artifact {}: {err}",
            plan.vertex_path.display()
        )
    })?;
    let fragment_bytes = std::fs::read(&plan.fragment_path).map_err(|err| {
        format!(
            "read scene effect fragment shader artifact {}: {err}",
            plan.fragment_path.display()
        )
    })?;
    Ok(NativeVulkanSceneEffectShaderArtifacts {
        shader: plan.shader,
        vertex_spirv: native_vulkan_scene_spirv_words_from_bytes(
            &vertex_bytes,
            "scene effect vertex shader artifact",
        )?,
        fragment_spirv: native_vulkan_scene_spirv_words_from_bytes(
            &fragment_bytes,
            "scene effect fragment shader artifact",
        )?,
    })
}

fn required_effect_shader_names(graph: &SceneEffectPassGraphPlan) -> Result<Vec<String>, String> {
    let mut unique = BTreeSet::new();
    let mut shaders = Vec::new();
    for pass in &graph.passes {
        let shader = pass.shader.as_deref().ok_or_else(|| {
            format!(
                "scene effect pass {} for object {:?} requires a WE shader artifact name",
                pass.pass_index, pass.object
            )
        })?;
        if shader.is_empty() {
            return Err(format!(
                "scene effect pass {} for object {:?} has an empty WE shader artifact name",
                pass.pass_index, pass.object
            ));
        }
        if unique.insert(shader.to_owned()) {
            shaders.push(shader.to_owned());
        }
    }
    Ok(shaders)
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

fn scene_effect_shader_artifact_relative_path(shader: &str) -> Result<PathBuf, String> {
    let Some(normalized) = shader.strip_prefix("effects/") else {
        return Err(format!(
            "scene effect shader artifact name '{shader}' must use the effects/ namespace"
        ));
    };
    if normalized.is_empty()
        || normalized.contains('\\')
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!(
            "scene effect shader artifact name '{shader}' cannot be mapped to a safe artifact path"
        ));
    }
    Ok(PathBuf::from("effects").join(normalized))
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
    fn effect_shader_artifact_plan_maps_effect_shader_to_stage_spirv_paths() {
        let plan = native_vulkan_scene_effect_shader_artifact_path_plan(
            Path::new("artifacts/scene-shaders"),
            "effects/iris",
        )
        .expect("effect artifact path plan");

        assert_eq!(plan.shader, "effects/iris");
        assert_eq!(
            plan.vertex_path,
            PathBuf::from("artifacts/scene-shaders/effects/iris.vert.spv")
        );
        assert_eq!(
            plan.fragment_path,
            PathBuf::from("artifacts/scene-shaders/effects/iris.frag.spv")
        );
        assert_eq!(
            plan.source_reference,
            "reverse-engineered/shaders/effects/iris"
        );
        assert_eq!(
            plan.command_order,
            [
                "resolve_we_effect_shader_artifact_paths",
                "require_reverse_engineered_effect_shader_source",
                "read_effect_vertex_spirv",
                "read_effect_fragment_spirv"
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
        assert!(
            native_vulkan_scene_effect_shader_artifact_path_plan(Path::new("artifacts"), "we/iris")
                .expect_err("wrong namespace")
                .contains("effects/ namespace")
        );
        assert!(
            scene_effect_shader_artifact_relative_path("effects/../iris")
                .expect_err("unsafe effect shader path")
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

    #[test]
    fn loaded_effect_shader_artifacts_borrow_as_pipeline_shader_slices() {
        let artifacts = NativeVulkanSceneEffectShaderArtifacts {
            shader: "effects/iris".to_owned(),
            vertex_spirv: vec![SPIRV_MAGIC, 1],
            fragment_spirv: vec![SPIRV_MAGIC, 2],
        };

        let shaders = artifacts.effect_pipeline_shaders();

        assert_eq!(shaders.vertex_spirv, &[SPIRV_MAGIC, 1]);
        assert_eq!(shaders.fragment_spirv, &[SPIRV_MAGIC, 2]);
    }

    #[test]
    fn effect_shader_catalog_plan_collects_unique_pass_shaders() {
        use std::collections::BTreeMap;

        use crate::engine::scene_engine::{
            SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
            SceneEffectPassGraphMaterialPass, SceneEffectPassGraphOutput, SceneGraphTarget,
            SceneObjectId, we::WeEffectKind,
        };

        let graph = SceneEffectPassGraphPlan {
            material_pass_count: 3,
            passes: vec![
                effect_pass(0, "effects/iris"),
                effect_pass(1, "effects/iris"),
                effect_pass(2, "effects/blur_downsample4"),
            ],
            ..SceneEffectPassGraphPlan::empty()
        };

        let shaders = required_effect_shader_names(&graph).expect("shader names");

        assert_eq!(
            shaders,
            vec![
                "effects/iris".to_owned(),
                "effects/blur_downsample4".to_owned()
            ]
        );

        fn effect_pass(graph_pass_index: usize, shader: &str) -> SceneEffectPassGraphMaterialPass {
            SceneEffectPassGraphMaterialPass {
                graph_command_index: graph_pass_index,
                graph_pass_index,
                object: SceneObjectId(7),
                program_index: 0,
                pass_index: graph_pass_index,
                effect_file: "effects/test/effect.json".to_owned(),
                effect: WeEffectKind::Unknown,
                shader: Some(shader.to_owned()),
                source: None,
                input_bindings: Vec::new(),
                output: SceneEffectPassGraphOutput::GraphTarget(SceneGraphTarget::EffectTarget(0)),
                blend: SceneEffectPassBlend::NormalReplace,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
                alpha_write: SceneAlphaWriteMode::Default,
                texture_resources: Vec::new(),
                combos: BTreeMap::new(),
                constants: BTreeMap::new(),
            }
        }
    }
}
