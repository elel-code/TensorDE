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

use crate::engine::scene_engine::{
    SceneEffectPassGraphPlan, SceneFramePlan, SceneGraph, SceneLayerCompositorOperation,
    SceneLayerCompositorPlan, WeShaderInterface,
};

use super::effect_pipeline::{
    NativeVulkanSceneEffectPipelineCacheKey, NativeVulkanSceneEffectPipelineShaders,
};
use super::layer_alpha_mask_executor::ALPHA_MASK_FLATTEXTURE_SHADER;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneShaderArtifactCatalogPlan {
    pub shader_count: usize,
    pub shaders: Vec<String>,
    pub command_order: [&'static str; 4],
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
    pub command_order: [&'static str; 5],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectShaderArtifactCatalog {
    shaders: BTreeMap<String, NativeVulkanSceneEffectShaderArtifacts>,
    plan: NativeVulkanSceneEffectShaderArtifactCatalogPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneShaderArtifactCatalog {
    shaders: BTreeMap<String, NativeVulkanSceneShaderArtifacts>,
    plan: NativeVulkanSceneShaderArtifactCatalogPlan,
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

impl NativeVulkanSceneShaderArtifactCatalog {
    pub(in crate::renderer::native_vulkan) fn from_scene_frame(
        artifact_root: &Path,
        frame: &SceneFramePlan,
    ) -> Result<Self, String> {
        Self::from_graph_and_layer_compositor(artifact_root, &frame.graph, &frame.layer_compositor)
    }

    pub(in crate::renderer::native_vulkan) fn from_graph_and_layer_compositor(
        artifact_root: &Path,
        graph: &SceneGraph,
        layer_compositor: &SceneLayerCompositorPlan,
    ) -> Result<Self, String> {
        let shader_names = required_scene_shader_names(graph, layer_compositor)?;
        let mut shaders = BTreeMap::new();
        for shader in &shader_names {
            let artifacts = native_vulkan_load_scene_shader_artifacts(artifact_root, shader)?;
            shaders.insert(shader.clone(), artifacts);
        }
        Ok(Self {
            plan: NativeVulkanSceneShaderArtifactCatalogPlan {
                shader_count: shader_names.len(),
                shaders: shader_names,
                command_order: [
                    "collect_scene_graph_draw_shader_names",
                    "append_layer_alpha_mask_shader_names",
                    "resolve_we_scene_shader_artifact_paths",
                    "read_unique_scene_shader_spirv",
                ],
            },
            shaders,
        })
    }

    pub(in crate::renderer::native_vulkan) fn empty() -> Self {
        Self {
            shaders: BTreeMap::new(),
            plan: NativeVulkanSceneShaderArtifactCatalogPlan {
                shader_count: 0,
                shaders: Vec::new(),
                command_order: [
                    "collect_scene_graph_draw_shader_names",
                    "append_layer_alpha_mask_shader_names",
                    "resolve_we_scene_shader_artifact_paths",
                    "read_unique_scene_shader_spirv",
                ],
            },
        }
    }

    pub(in crate::renderer::native_vulkan) fn mesh_pipeline_shaders_for_key(
        &self,
        key: &super::pipeline::NativeVulkanScenePipelineCacheKey,
    ) -> Result<NativeVulkanSceneMeshPipelineShaders<'_>, String> {
        self.shaders
            .get(&key.shader)
            .ok_or_else(|| {
                format!(
                    "scene shader catalog has no artifact for shader '{}'",
                    key.shader
                )
            })
            .map(|artifacts| artifacts.mesh_pipeline_shaders())
    }

    pub(in crate::renderer::native_vulkan) fn shader_count(&self) -> usize {
        self.plan.shader_count
    }

    pub(in crate::renderer::native_vulkan) fn plan(
        &self,
    ) -> &NativeVulkanSceneShaderArtifactCatalogPlan {
        &self.plan
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
        Self::from_shader_names(artifact_root, shader_names)
    }

    pub(in crate::renderer::native_vulkan) fn from_effect_pass_graph_and_layer_compositor(
        artifact_root: &Path,
        graph: &SceneEffectPassGraphPlan,
        layer_compositor: &SceneLayerCompositorPlan,
    ) -> Result<Self, String> {
        let shader_names =
            required_effect_shader_names_with_layer_compositor(graph, Some(layer_compositor))?;
        Self::from_shader_names(artifact_root, shader_names)
    }

    fn from_shader_names(artifact_root: &Path, shader_names: Vec<String>) -> Result<Self, String> {
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
                    "append_layer_alpha_mask_copy_back_shader_names",
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
                    "append_layer_alpha_mask_copy_back_shader_names",
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
    let source_reference = scene_effect_shader_source_reference(shader, &shader_path)?;
    Ok(NativeVulkanSceneEffectShaderArtifactPathPlan {
        shader: shader.to_owned(),
        vertex_path: artifact_root.join(shader_path.with_extension("vert.spv")),
        fragment_path: artifact_root.join(shader_path.with_extension("frag.spv")),
        source_reference,
        command_order: [
            "resolve_we_effect_shader_artifact_paths",
            "resolve_we_shader_source_reference",
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
    required_effect_shader_names_with_layer_compositor(graph, None)
}

fn required_effect_shader_names_with_layer_compositor(
    graph: &SceneEffectPassGraphPlan,
    layer_compositor: Option<&SceneLayerCompositorPlan>,
) -> Result<Vec<String>, String> {
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
    if let Some(layer_compositor) = layer_compositor
        && layer_compositor_uses_flattexture_copy_back(layer_compositor)
        && unique.insert(ALPHA_MASK_FLATTEXTURE_SHADER.to_owned())
    {
        shaders.push(ALPHA_MASK_FLATTEXTURE_SHADER.to_owned());
    }
    Ok(shaders)
}

fn layer_compositor_uses_flattexture_copy_back(
    layer_compositor: &SceneLayerCompositorPlan,
) -> bool {
    layer_compositor.layers.iter().any(|layer| {
        layer.commands.iter().any(|command| {
            command.operation == SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
        })
    })
}

fn required_scene_shader_names(
    graph: &SceneGraph,
    layer_compositor: &SceneLayerCompositorPlan,
) -> Result<Vec<String>, String> {
    let mut unique = BTreeSet::new();
    let mut shaders = Vec::new();
    for pass in &graph.passes {
        for draw in &pass.draws {
            let shader = draw.material.shader.as_str();
            if shader.is_empty() {
                return Err(format!(
                    "scene graph draw for object {:?} has an empty WE shader artifact name",
                    draw.object
                ));
            }
            WeShaderInterface::for_shader(shader).ok_or_else(|| {
                format!(
                    "scene graph draw for object {:?} references unknown WE shader '{}'",
                    draw.object, shader
                )
            })?;
            if unique.insert(shader.to_owned()) {
                shaders.push(shader.to_owned());
            }
        }
    }
    if layer_compositor.tokenized_layer_count > 0
        && unique.insert("we/clippingmaskimage4".to_owned())
    {
        shaders.push("we/clippingmaskimage4".to_owned());
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
    let shader_path = if let Some(normalized) = shader.strip_prefix("effects/") {
        safe_shader_artifact_relative_path(shader, "effects", normalized)?
    } else if let Some(normalized) = shader.strip_prefix("util/") {
        safe_shader_artifact_relative_path(shader, "", normalized)?
    } else {
        return Err(format!(
            "scene effect shader artifact name '{shader}' must use the effects/ or util/ namespace"
        ));
    };
    Ok(shader_path)
}

fn scene_effect_shader_source_reference(
    shader: &str,
    shader_path: &Path,
) -> Result<String, String> {
    if shader == "util/minimalalpha" {
        return Ok(
            "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha"
                .to_owned(),
        );
    }
    Ok(format!(
        "reverse-engineered/shaders/{}",
        shader_path.display()
    ))
}

fn safe_shader_artifact_relative_path(
    shader: &str,
    root: &str,
    normalized: &str,
) -> Result<PathBuf, String> {
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
    let path = if root.is_empty() {
        PathBuf::from(normalized)
    } else {
        PathBuf::from(root).join(normalized)
    };
    Ok(path)
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
    fn shader_artifact_plan_maps_clipping_mask_shader_to_stage_spirv_paths() {
        let plan = native_vulkan_scene_shader_artifact_path_plan(
            Path::new("artifacts/scene-shaders"),
            "we/clippingmaskimage4",
        )
        .expect("clipping mask artifact path plan");

        assert_eq!(plan.shader, "we/clippingmaskimage4");
        assert_eq!(
            plan.vertex_path,
            PathBuf::from("artifacts/scene-shaders/we/clippingmaskimage4.vert.spv")
        );
        assert_eq!(
            plan.fragment_path,
            PathBuf::from("artifacts/scene-shaders/we/clippingmaskimage4.frag.spv")
        );
    }

    #[test]
    fn scene_shader_catalog_names_include_layer_alpha_mask_shader() {
        let graph = scene_graph_with_shader("we/genericimage4");
        let mut layer_compositor = SceneLayerCompositorPlan::empty();
        layer_compositor.tokenized_layer_count = 1;

        let shaders = required_scene_shader_names(&graph, &layer_compositor)
            .expect("scene shader name collection");

        assert_eq!(
            shaders,
            vec![
                "we/genericimage4".to_owned(),
                "we/clippingmaskimage4".to_owned()
            ]
        );
    }

    #[test]
    fn scene_shader_catalog_names_reject_unknown_scene_shader() {
        let err = required_scene_shader_names(
            &scene_graph_with_shader("we/notreal"),
            &SceneLayerCompositorPlan::empty(),
        )
        .expect_err("unknown scene shader must fail");

        assert!(err.contains("unknown WE shader"));
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
                "resolve_we_shader_source_reference",
                "read_effect_vertex_spirv",
                "read_effect_fragment_spirv"
            ]
        );
    }

    #[test]
    fn effect_shader_artifact_plan_maps_util_passthrough_to_top_level_shader() {
        let plan = native_vulkan_scene_effect_shader_artifact_path_plan(
            Path::new("artifacts/scene-shaders"),
            "util/passthrough",
        )
        .expect("util artifact path plan");

        assert_eq!(plan.shader, "util/passthrough");
        assert_eq!(
            plan.vertex_path,
            PathBuf::from("artifacts/scene-shaders/passthrough.vert.spv")
        );
        assert_eq!(
            plan.fragment_path,
            PathBuf::from("artifacts/scene-shaders/passthrough.frag.spv")
        );
        assert_eq!(
            plan.source_reference,
            "reverse-engineered/shaders/passthrough"
        );
    }

    #[test]
    fn effect_shader_artifact_plan_maps_util_minimalalpha_to_top_level_shader() {
        let plan = native_vulkan_scene_effect_shader_artifact_path_plan(
            Path::new("artifacts/scene-shaders"),
            "util/minimalalpha",
        )
        .expect("minimalalpha artifact path plan");

        assert_eq!(plan.shader, "util/minimalalpha");
        assert_eq!(
            plan.vertex_path,
            PathBuf::from("artifacts/scene-shaders/minimalalpha.vert.spv")
        );
        assert_eq!(
            plan.fragment_path,
            PathBuf::from("artifacts/scene-shaders/minimalalpha.frag.spv")
        );
        assert_eq!(
            plan.source_reference,
            "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha"
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
                .contains("effects/ or util/ namespace")
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
        let graph = effect_graph_with_shader_names(&[
            "effects/iris",
            "effects/iris",
            "effects/blur_downsample4",
        ]);

        let shaders = required_effect_shader_names(&graph).expect("shader names");

        assert_eq!(
            shaders,
            vec![
                "effects/iris".to_owned(),
                "effects/blur_downsample4".to_owned()
            ]
        );
    }

    #[test]
    fn effect_shader_catalog_names_append_minimalalpha_for_layer_copy_back() {
        use crate::engine::scene_engine::{
            SceneLayerCompositorBlendKey, SceneLayerCompositorCommand,
            SceneLayerCompositorCondition, SceneLayerCompositorEntry, SceneLayerCompositorLayer,
            SceneLayerCompositorOperation, SceneLayerCompositorRoute, SceneLayerCompositorTarget,
            SceneObjectId,
        };

        let graph = effect_graph_with_shader_names(&["effects/iris"]);
        let mut layer_compositor = SceneLayerCompositorPlan::empty();
        layer_compositor.layer_count = 1;
        layer_compositor.command_count = 1;
        layer_compositor.tokenized_layer_count = 1;
        layer_compositor.layers = vec![SceneLayerCompositorLayer {
            object: SceneObjectId(7),
            route: SceneLayerCompositorRoute::DirectSwapchain,
            uses_tokenized_subdraw: true,
            commands: vec![SceneLayerCompositorCommand {
                entry: SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed,
                operation: SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
                condition: SceneLayerCompositorCondition::Token2AfterIntermediateMask,
                source: Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
                target: SceneLayerCompositorTarget::FullAlphaMask,
                blend_key: SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
            }],
        }];

        let shaders =
            required_effect_shader_names_with_layer_compositor(&graph, Some(&layer_compositor))
                .expect("shader names with alpha-mask copy-back");

        assert_eq!(
            shaders,
            vec!["effects/iris".to_owned(), "util/minimalalpha".to_owned()]
        );
    }

    fn effect_graph_with_shader_names(shaders: &[&str]) -> SceneEffectPassGraphPlan {
        use std::collections::BTreeMap;

        use crate::engine::scene_engine::{
            SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
            SceneEffectPassGraphMaterialPass, SceneEffectPassGraphOutput, SceneGraphTarget,
            SceneObjectId, we::WeEffectKind,
        };

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

        SceneEffectPassGraphPlan {
            material_pass_count: shaders.len(),
            passes: shaders
                .iter()
                .enumerate()
                .map(|(index, shader)| effect_pass(index, shader))
                .collect(),
            ..SceneEffectPassGraphPlan::empty()
        }
    }

    fn scene_graph_with_shader(shader: &str) -> SceneGraph {
        SceneGraph {
            passes: vec![crate::engine::scene_engine::SceneGraphPass {
                name: "scene-main".to_owned(),
                input: None,
                output: crate::engine::scene_engine::SceneGraphTarget::Swapchain,
                draws: vec![crate::engine::scene_engine::SceneGraphDraw {
                    object: crate::engine::scene_engine::SceneObjectId(1),
                    pipeline: crate::engine::scene_engine::SceneGraphPipelineClass::Mesh,
                    material: crate::engine::scene_engine::SceneMaterialKey {
                        shader: shader.to_owned(),
                        blend: crate::engine::scene_engine::SceneBlendContract::TranslucentAlpha,
                        render_state:
                            crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(),
                    },
                    geometry: Some(crate::engine::scene_engine::SceneGeometryId(1)),
                    puppet: None,
                    resources: Vec::new(),
                    index_count: 6,
                }],
            }],
        }
    }
}
