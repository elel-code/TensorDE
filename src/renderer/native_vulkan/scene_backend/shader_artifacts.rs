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
    native_vulkan_scene_effect_pass_shader_combo_values,
};
use super::layer_alpha_mask_executor::ALPHA_MASK_FLATTEXTURE_SHADER;
use super::pipeline::{
    NativeVulkanScenePipelineCacheKey, NativeVulkanScenePipelineShaderComboValue,
    native_vulkan_scene_pipeline_shader_combo_values,
};
use super::pipeline_factory::NativeVulkanSceneMeshPipelineShaders;

const SPIRV_MAGIC: u32 = 0x0723_0203;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneShaderArtifactPathPlan {
    pub shader: String,
    pub shader_combo_values: Vec<String>,
    pub vertex_path: PathBuf,
    pub fragment_path: PathBuf,
    pub command_order: [&'static str; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectShaderArtifactPathPlan {
    pub shader: String,
    pub shader_combo_values: Vec<String>,
    pub vertex_path: PathBuf,
    pub fragment_path: PathBuf,
    pub source_reference: String,
    pub command_order: [&'static str; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneShaderArtifacts {
    pub shader: String,
    pub shader_combo_values: Vec<NativeVulkanScenePipelineShaderComboValue>,
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
    pub shader_combo_values: Vec<NativeVulkanScenePipelineShaderComboValue>,
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
    shaders:
        BTreeMap<NativeVulkanSceneEffectShaderArtifactKey, NativeVulkanSceneEffectShaderArtifacts>,
    plan: NativeVulkanSceneEffectShaderArtifactCatalogPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NativeVulkanSceneShaderArtifactKey {
    shader: String,
    shader_combo_values: Vec<NativeVulkanScenePipelineShaderComboValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NativeVulkanSceneEffectShaderArtifactKey {
    shader: String,
    shader_combo_values: Vec<NativeVulkanScenePipelineShaderComboValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneShaderArtifactCatalog {
    shaders: BTreeMap<NativeVulkanSceneShaderArtifactKey, NativeVulkanSceneShaderArtifacts>,
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
        let shader_keys = required_scene_shader_artifact_keys(graph, layer_compositor)?;
        let mut shaders = BTreeMap::new();
        for key in &shader_keys {
            let artifacts = native_vulkan_load_scene_shader_artifacts_for_key(artifact_root, key)?;
            shaders.insert(key.clone(), artifacts);
        }
        let shader_labels = shader_keys
            .iter()
            .map(NativeVulkanSceneShaderArtifactKey::label)
            .collect::<Vec<_>>();
        Ok(Self {
            plan: NativeVulkanSceneShaderArtifactCatalogPlan {
                shader_count: shader_labels.len(),
                shaders: shader_labels,
                command_order: [
                    "collect_scene_graph_draw_shader_names",
                    "append_layer_alpha_mask_shader_variant_keys",
                    "resolve_we_scene_shader_variant_artifact_paths",
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
                    "append_layer_alpha_mask_shader_variant_keys",
                    "resolve_we_scene_shader_variant_artifact_paths",
                    "read_unique_scene_shader_spirv",
                ],
            },
        }
    }

    pub(in crate::renderer::native_vulkan) fn mesh_pipeline_shaders_for_key(
        &self,
        key: &NativeVulkanScenePipelineCacheKey,
    ) -> Result<NativeVulkanSceneMeshPipelineShaders<'_>, String> {
        let artifact_key = NativeVulkanSceneShaderArtifactKey::from_pipeline_cache_key(key);
        self.shaders
            .get(&artifact_key)
            .ok_or_else(|| {
                if key.shader_combo_values.is_empty() {
                    format!(
                        "scene shader catalog has no artifact for shader '{}'",
                        key.shader
                    )
                } else {
                    format!(
                        "scene shader catalog has no WE combo variant artifact for {}",
                        artifact_key.label()
                    )
                }
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
        let shader_keys = required_effect_shader_artifact_keys(graph)?;
        Self::from_shader_keys(artifact_root, shader_keys)
    }

    pub(in crate::renderer::native_vulkan) fn from_effect_pass_graph_and_layer_compositor(
        artifact_root: &Path,
        graph: &SceneEffectPassGraphPlan,
        layer_compositor: &SceneLayerCompositorPlan,
    ) -> Result<Self, String> {
        let shader_keys = required_effect_shader_artifact_keys_with_layer_compositor(
            graph,
            Some(layer_compositor),
        )?;
        Self::from_shader_keys(artifact_root, shader_keys)
    }

    fn from_shader_keys(
        artifact_root: &Path,
        shader_keys: Vec<NativeVulkanSceneEffectShaderArtifactKey>,
    ) -> Result<Self, String> {
        let mut shaders = BTreeMap::new();
        for key in &shader_keys {
            let artifacts =
                native_vulkan_load_scene_effect_shader_artifacts_for_key(artifact_root, key)?;
            shaders.insert(key.clone(), artifacts);
        }
        let shader_labels = shader_keys
            .iter()
            .map(NativeVulkanSceneEffectShaderArtifactKey::label)
            .collect::<Vec<_>>();
        Ok(Self {
            plan: NativeVulkanSceneEffectShaderArtifactCatalogPlan {
                shader_count: shader_labels.len(),
                shaders: shader_labels,
                command_order: [
                    "collect_effect_shader_variant_keys_from_pass_graph",
                    "append_layer_alpha_mask_copy_back_shader_variant_keys",
                    "resolve_we_effect_shader_variant_artifact_paths",
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
                    "collect_effect_shader_variant_keys_from_pass_graph",
                    "append_layer_alpha_mask_copy_back_shader_variant_keys",
                    "resolve_we_effect_shader_variant_artifact_paths",
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
        let artifact_key = NativeVulkanSceneEffectShaderArtifactKey::from_pipeline_cache_key(key);
        self.shaders
            .get(&artifact_key)
            .ok_or_else(|| {
                if key.shader_combo_values.is_empty() {
                    format!(
                        "scene effect shader catalog has no artifact for shader '{}'",
                        key.shader
                    )
                } else {
                    format!(
                        "scene effect shader catalog has no WE combo variant artifact for {}",
                        artifact_key.label()
                    )
                }
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

impl NativeVulkanSceneShaderArtifactKey {
    fn plain(shader: &str) -> Self {
        Self {
            shader: shader.to_owned(),
            shader_combo_values: Vec::new(),
        }
    }

    fn variant(
        shader: &str,
        shader_combo_values: Vec<NativeVulkanScenePipelineShaderComboValue>,
    ) -> Self {
        Self {
            shader: shader.to_owned(),
            shader_combo_values,
        }
    }

    fn from_pipeline_cache_key(key: &NativeVulkanScenePipelineCacheKey) -> Self {
        Self {
            shader: key.shader.clone(),
            shader_combo_values: key.shader_combo_values.clone(),
        }
    }

    fn combo_labels(&self) -> Vec<String> {
        self.shader_combo_values
            .iter()
            .map(|combo| format!("{}={}", combo.name, combo.value))
            .collect()
    }

    fn label(&self) -> String {
        if self.shader_combo_values.is_empty() {
            self.shader.clone()
        } else {
            format!("{}[{}]", self.shader, self.combo_labels().join(","))
        }
    }
}

impl NativeVulkanSceneEffectShaderArtifactKey {
    fn plain(shader: &str) -> Self {
        Self {
            shader: shader.to_owned(),
            shader_combo_values: Vec::new(),
        }
    }

    fn variant(
        shader: &str,
        shader_combo_values: Vec<NativeVulkanScenePipelineShaderComboValue>,
    ) -> Self {
        Self {
            shader: shader.to_owned(),
            shader_combo_values,
        }
    }

    fn from_pipeline_cache_key(key: &NativeVulkanSceneEffectPipelineCacheKey) -> Self {
        Self {
            shader: key.shader.clone(),
            shader_combo_values: key.shader_combo_values.clone(),
        }
    }

    fn combo_labels(&self) -> Vec<String> {
        self.shader_combo_values
            .iter()
            .map(|combo| format!("{}={}", combo.name, combo.value))
            .collect()
    }

    fn label(&self) -> String {
        if self.shader_combo_values.is_empty() {
            self.shader.clone()
        } else {
            format!("{}[{}]", self.shader, self.combo_labels().join(","))
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_shader_artifact_path_plan(
    artifact_root: &Path,
    shader: &str,
) -> Result<NativeVulkanSceneShaderArtifactPathPlan, String> {
    native_vulkan_scene_shader_artifact_path_plan_for_key(
        artifact_root,
        &NativeVulkanSceneShaderArtifactKey::plain(shader),
    )
}

fn native_vulkan_scene_shader_artifact_path_plan_for_key(
    artifact_root: &Path,
    key: &NativeVulkanSceneShaderArtifactKey,
) -> Result<NativeVulkanSceneShaderArtifactPathPlan, String> {
    if artifact_root.as_os_str().is_empty() {
        return Err("scene shader artifact root cannot be empty".to_owned());
    }
    validate_scene_shader_artifact_key(key)?;
    let shader_path = scene_shader_artifact_relative_path_for_key(key)?;
    Ok(NativeVulkanSceneShaderArtifactPathPlan {
        shader: key.shader.clone(),
        shader_combo_values: key.combo_labels(),
        vertex_path: artifact_root.join(shader_path.with_extension("vert.spv")),
        fragment_path: artifact_root.join(shader_path.with_extension("frag.spv")),
        command_order: [
            "resolve_we_shader_artifact_paths",
            "read_vertex_spirv",
            "read_fragment_spirv",
        ],
    })
}

fn validate_scene_shader_artifact_key(
    key: &NativeVulkanSceneShaderArtifactKey,
) -> Result<(), String> {
    let interface = WeShaderInterface::for_shader(&key.shader).ok_or_else(|| {
        format!(
            "scene shader artifact plan references unknown WE shader '{}'",
            key.shader
        )
    })?;
    for combo in &key.shader_combo_values {
        if combo.name.is_empty() {
            return Err(format!(
                "scene shader artifact key for '{}' has empty WE combo override",
                key.shader
            ));
        }
        if !combo
            .name
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        {
            return Err(format!(
                "scene shader artifact key for '{}' has unsafe WE combo name '{}'",
                key.shader, combo.name
            ));
        }
        if !interface
            .combos
            .iter()
            .any(|declared| declared.name == combo.name)
        {
            return Err(format!(
                "scene shader artifact key for '{}' references undeclared WE combo '{}'",
                key.shader, combo.name
            ));
        }
    }
    for pair in key.shader_combo_values.windows(2) {
        if pair[0].name >= pair[1].name {
            return Err(format!(
                "scene shader artifact key for '{}' WE combo overrides must be sorted and unique, got '{}' before '{}'",
                key.shader, pair[0].name, pair[1].name
            ));
        }
    }
    Ok(())
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_effect_shader_artifact_path_plan(
    artifact_root: &Path,
    shader: &str,
) -> Result<NativeVulkanSceneEffectShaderArtifactPathPlan, String> {
    native_vulkan_scene_effect_shader_artifact_path_plan_for_key(
        artifact_root,
        &NativeVulkanSceneEffectShaderArtifactKey::plain(shader),
    )
}

fn native_vulkan_scene_effect_shader_artifact_path_plan_for_key(
    artifact_root: &Path,
    key: &NativeVulkanSceneEffectShaderArtifactKey,
) -> Result<NativeVulkanSceneEffectShaderArtifactPathPlan, String> {
    if artifact_root.as_os_str().is_empty() {
        return Err("scene effect shader artifact root cannot be empty".to_owned());
    }
    validate_scene_effect_shader_artifact_key(key)?;
    let base_shader_path = scene_effect_shader_artifact_relative_path(&key.shader)?;
    let shader_path = scene_effect_shader_artifact_relative_path_for_key(key)?;
    let source_reference = scene_effect_shader_source_reference(&key.shader, &base_shader_path)?;
    Ok(NativeVulkanSceneEffectShaderArtifactPathPlan {
        shader: key.shader.clone(),
        shader_combo_values: key.combo_labels(),
        vertex_path: artifact_root.join(shader_path.with_extension("vert.spv")),
        fragment_path: artifact_root.join(shader_path.with_extension("frag.spv")),
        source_reference,
        command_order: [
            "resolve_we_effect_shader_variant_artifact_paths",
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
    native_vulkan_load_scene_shader_artifacts_for_key(
        artifact_root,
        &NativeVulkanSceneShaderArtifactKey::plain(shader),
    )
}

fn native_vulkan_load_scene_shader_artifacts_for_key(
    artifact_root: &Path,
    key: &NativeVulkanSceneShaderArtifactKey,
) -> Result<NativeVulkanSceneShaderArtifacts, String> {
    let plan = native_vulkan_scene_shader_artifact_path_plan_for_key(artifact_root, key)?;
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
        shader_combo_values: key.shader_combo_values.clone(),
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
    native_vulkan_load_scene_effect_shader_artifacts_for_key(
        artifact_root,
        &NativeVulkanSceneEffectShaderArtifactKey::plain(shader),
    )
}

fn native_vulkan_load_scene_effect_shader_artifacts_for_key(
    artifact_root: &Path,
    key: &NativeVulkanSceneEffectShaderArtifactKey,
) -> Result<NativeVulkanSceneEffectShaderArtifacts, String> {
    let plan = native_vulkan_scene_effect_shader_artifact_path_plan_for_key(artifact_root, key)?;
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
        shader_combo_values: key.shader_combo_values.clone(),
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
    Ok(required_effect_shader_artifact_keys(graph)?
        .into_iter()
        .map(|key| key.shader)
        .collect())
}

fn required_effect_shader_names_with_layer_compositor(
    graph: &SceneEffectPassGraphPlan,
    layer_compositor: Option<&SceneLayerCompositorPlan>,
) -> Result<Vec<String>, String> {
    Ok(
        required_effect_shader_artifact_keys_with_layer_compositor(graph, layer_compositor)?
            .into_iter()
            .map(|key| key.shader)
            .collect(),
    )
}

fn required_effect_shader_artifact_keys(
    graph: &SceneEffectPassGraphPlan,
) -> Result<Vec<NativeVulkanSceneEffectShaderArtifactKey>, String> {
    required_effect_shader_artifact_keys_with_layer_compositor(graph, None)
}

fn required_effect_shader_artifact_keys_with_layer_compositor(
    graph: &SceneEffectPassGraphPlan,
    layer_compositor: Option<&SceneLayerCompositorPlan>,
) -> Result<Vec<NativeVulkanSceneEffectShaderArtifactKey>, String> {
    let mut unique = BTreeSet::new();
    let mut shader_keys = Vec::new();
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
        let key = NativeVulkanSceneEffectShaderArtifactKey::variant(
            shader,
            native_vulkan_scene_effect_pass_shader_combo_values(pass)?,
        );
        validate_scene_effect_shader_artifact_key(&key)?;
        if unique.insert(key.clone()) {
            shader_keys.push(key);
        }
    }
    let copy_back_key =
        NativeVulkanSceneEffectShaderArtifactKey::plain(ALPHA_MASK_FLATTEXTURE_SHADER);
    if let Some(layer_compositor) = layer_compositor
        && layer_compositor_uses_flattexture_copy_back(layer_compositor)
        && unique.insert(copy_back_key.clone())
    {
        shader_keys.push(copy_back_key);
    }
    Ok(shader_keys)
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
    Ok(
        required_scene_shader_artifact_keys(graph, layer_compositor)?
            .into_iter()
            .map(|key| key.shader)
            .collect(),
    )
}

fn required_scene_shader_artifact_keys(
    graph: &SceneGraph,
    layer_compositor: &SceneLayerCompositorPlan,
) -> Result<Vec<NativeVulkanSceneShaderArtifactKey>, String> {
    let mut unique = BTreeSet::new();
    let mut shader_keys = Vec::new();
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
            let key = NativeVulkanSceneShaderArtifactKey::plain(shader);
            if unique.insert(key.clone()) {
                shader_keys.push(key);
            }
        }
    }
    if layer_compositor_uses_generated_clipping_target(layer_compositor) {
        let key = NativeVulkanSceneShaderArtifactKey::variant(
            "we/genericimage4",
            native_vulkan_scene_pipeline_shader_combo_values(&[
                ("CLIPPINGTARGET", 1),
                ("CLIPPINGUVS", 1),
            ]),
        );
        if unique.insert(key.clone()) {
            shader_keys.push(key);
        }
    }
    if layer_compositor.tokenized_layer_count > 0
        && unique.insert(NativeVulkanSceneShaderArtifactKey::plain(
            "we/clippingmaskimage4",
        ))
    {
        shader_keys.push(NativeVulkanSceneShaderArtifactKey::plain(
            "we/clippingmaskimage4",
        ));
    }
    if layer_compositor_uses_flattexture_copy_back(layer_compositor)
        && unique.insert(NativeVulkanSceneShaderArtifactKey::plain(
            ALPHA_MASK_FLATTEXTURE_SHADER,
        ))
    {
        shader_keys.push(NativeVulkanSceneShaderArtifactKey::plain(
            ALPHA_MASK_FLATTEXTURE_SHADER,
        ));
    }
    Ok(shader_keys)
}

fn layer_compositor_uses_generated_clipping_target(
    layer_compositor: &SceneLayerCompositorPlan,
) -> bool {
    layer_compositor.layers.iter().any(|layer| {
        layer.commands.iter().any(|command| {
            command.operation == SceneLayerCompositorOperation::DrawGeneratedClippingTarget
        })
    })
}

fn scene_shader_artifact_relative_path(shader: &str) -> Result<PathBuf, String> {
    if shader == ALPHA_MASK_FLATTEXTURE_SHADER {
        return Ok(PathBuf::from("minimalalpha"));
    }
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

fn scene_shader_artifact_relative_path_for_key(
    key: &NativeVulkanSceneShaderArtifactKey,
) -> Result<PathBuf, String> {
    let base = scene_shader_artifact_relative_path(&key.shader)?;
    if key.shader_combo_values.is_empty() {
        return Ok(base);
    }
    let file_name = base
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "scene shader artifact key '{}' cannot be mapped to a variant path",
                key.label()
            )
        })?;
    let mut variant_name = file_name.to_owned();
    for combo in &key.shader_combo_values {
        variant_name.push_str("__");
        variant_name.push_str(&combo.name);
        variant_name.push('_');
        variant_name.push_str(&combo.value.to_string());
    }
    Ok(base.with_file_name(variant_name))
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

fn scene_effect_shader_artifact_relative_path_for_key(
    key: &NativeVulkanSceneEffectShaderArtifactKey,
) -> Result<PathBuf, String> {
    let base = scene_effect_shader_artifact_relative_path(&key.shader)?;
    if key.shader_combo_values.is_empty() {
        return Ok(base);
    }
    let file_name = base
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "scene effect shader artifact key '{}' cannot be mapped to a variant path",
                key.label()
            )
        })?;
    let mut variant_name = file_name.to_owned();
    for combo in &key.shader_combo_values {
        variant_name.push_str("__");
        variant_name.push_str(&combo.name);
        variant_name.push('_');
        variant_name.push_str(&combo.value.to_string());
    }
    Ok(base.with_file_name(variant_name))
}

fn validate_scene_effect_shader_artifact_key(
    key: &NativeVulkanSceneEffectShaderArtifactKey,
) -> Result<(), String> {
    scene_effect_shader_artifact_relative_path(&key.shader)?;
    for combo in &key.shader_combo_values {
        if combo.name.is_empty() {
            return Err(format!(
                "scene effect shader artifact key for '{}' has empty WE combo override",
                key.shader
            ));
        }
        if !combo
            .name
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        {
            return Err(format!(
                "scene effect shader artifact key for '{}' has unsafe WE combo name '{}'",
                key.shader, combo.name
            ));
        }
    }
    for pair in key.shader_combo_values.windows(2) {
        if pair[0].name >= pair[1].name {
            return Err(format!(
                "scene effect shader artifact key for '{}' WE combo overrides must be sorted and unique, got '{}' before '{}'",
                key.shader, pair[0].name, pair[1].name
            ));
        }
    }
    if let Some(interface) = WeShaderInterface::for_effect_shader(&key.shader) {
        for combo in &key.shader_combo_values {
            if !interface.declares_combo(&combo.name) {
                return Err(format!(
                    "scene effect shader artifact key for '{}' references undeclared WE combo '{}'",
                    key.shader, combo.name
                ));
            }
        }
    }
    Ok(())
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
        assert!(plan.shader_combo_values.is_empty());
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
    fn shader_artifact_plan_maps_we_combo_variant_to_distinct_stage_spirv_paths() {
        let key = NativeVulkanSceneShaderArtifactKey::variant(
            "we/genericimage4",
            native_vulkan_scene_pipeline_shader_combo_values(&[
                ("CLIPPINGTARGET", 1),
                ("CLIPPINGUVS", 1),
            ]),
        );
        let plan = native_vulkan_scene_shader_artifact_path_plan_for_key(
            Path::new("artifacts/scene-shaders"),
            &key,
        )
        .expect("combo variant artifact path plan");

        assert_eq!(plan.shader, "we/genericimage4");
        assert_eq!(
            plan.shader_combo_values,
            vec!["CLIPPINGTARGET=1".to_owned(), "CLIPPINGUVS=1".to_owned()]
        );
        assert_eq!(
            plan.vertex_path,
            PathBuf::from(
                "artifacts/scene-shaders/we/genericimage4__CLIPPINGTARGET_1__CLIPPINGUVS_1.vert.spv"
            )
        );
        assert_eq!(
            plan.fragment_path,
            PathBuf::from(
                "artifacts/scene-shaders/we/genericimage4__CLIPPINGTARGET_1__CLIPPINGUVS_1.frag.spv"
            )
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
    fn shader_artifact_plan_maps_minimalalpha_scene_utility_shader() {
        let plan = native_vulkan_scene_shader_artifact_path_plan(
            Path::new("artifacts/scene-shaders"),
            "util/minimalalpha",
        )
        .expect("minimalalpha scene utility artifact path plan");

        assert_eq!(plan.shader, "util/minimalalpha");
        assert_eq!(
            plan.vertex_path,
            PathBuf::from("artifacts/scene-shaders/minimalalpha.vert.spv")
        );
        assert_eq!(
            plan.fragment_path,
            PathBuf::from("artifacts/scene-shaders/minimalalpha.frag.spv")
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
    fn scene_shader_catalog_names_append_minimalalpha_for_copy_back() {
        use crate::engine::scene_engine::{
            SceneLayerCompositorBlendKey, SceneLayerCompositorCommand,
            SceneLayerCompositorCondition, SceneLayerCompositorEntry, SceneLayerCompositorLayer,
            SceneLayerCompositorOperation, SceneLayerCompositorRoute, SceneLayerCompositorTarget,
            SceneObjectId,
        };

        let graph = scene_graph_with_shader("we/genericimage4");
        let mut layer_compositor = SceneLayerCompositorPlan::empty();
        layer_compositor.layer_count = 1;
        layer_compositor.command_count = 1;
        layer_compositor.tokenized_layer_count = 1;
        layer_compositor.layers = vec![SceneLayerCompositorLayer {
            object: SceneObjectId(7),
            route: SceneLayerCompositorRoute::DirectSwapchain,
            uses_tokenized_subdraw: true,
            has_active_aux_clear_target: false,
            commands: vec![SceneLayerCompositorCommand {
                entry: SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed,
                operation: SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
                condition: SceneLayerCompositorCondition::Token2AfterIntermediateMask,
                source: Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
                target: SceneLayerCompositorTarget::FullAlphaMask,
                blend_key: SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
            }],
        }];

        let shaders = required_scene_shader_names(&graph, &layer_compositor)
            .expect("scene shader name collection with copy-back");

        assert_eq!(
            shaders,
            vec![
                "we/genericimage4".to_owned(),
                "we/clippingmaskimage4".to_owned(),
                "util/minimalalpha".to_owned()
            ]
        );
    }

    #[test]
    fn scene_shader_catalog_keys_append_generated_clippingtarget_combo_variant() {
        use crate::engine::scene_engine::{
            SceneLayerCompositorBlendKey, SceneLayerCompositorCommand,
            SceneLayerCompositorCondition, SceneLayerCompositorEntry, SceneLayerCompositorLayer,
            SceneLayerCompositorOperation, SceneLayerCompositorRoute, SceneLayerCompositorTarget,
            SceneObjectId,
        };

        let graph = scene_graph_with_shader("we/genericimage4");
        let mut layer_compositor = SceneLayerCompositorPlan::empty();
        layer_compositor.layer_count = 1;
        layer_compositor.command_count = 1;
        layer_compositor.tokenized_layer_count = 1;
        layer_compositor.layers = vec![SceneLayerCompositorLayer {
            object: SceneObjectId(7),
            route: SceneLayerCompositorRoute::DirectSwapchain,
            uses_tokenized_subdraw: true,
            has_active_aux_clear_target: false,
            commands: vec![SceneLayerCompositorCommand {
                entry: SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53,
                operation: SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
                condition: SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
                source: Some(SceneLayerCompositorTarget::FullAlphaMask),
                target: SceneLayerCompositorTarget::LayerTarget490,
                blend_key: SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0,
            }],
        }];

        let keys = required_scene_shader_artifact_keys(&graph, &layer_compositor)
            .expect("scene shader variant key collection");
        let labels = keys
            .iter()
            .map(NativeVulkanSceneShaderArtifactKey::label)
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "we/genericimage4".to_owned(),
                "we/genericimage4[CLIPPINGTARGET=1,CLIPPINGUVS=1]".to_owned(),
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
        assert!(plan.shader_combo_values.is_empty());
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
                "resolve_we_effect_shader_variant_artifact_paths",
                "resolve_we_shader_source_reference",
                "read_effect_vertex_spirv",
                "read_effect_fragment_spirv"
            ]
        );
    }

    #[test]
    fn effect_shader_artifact_plan_maps_we_combo_variant_to_distinct_stage_spirv_paths() {
        let key = NativeVulkanSceneEffectShaderArtifactKey::variant(
            "effects/iris",
            native_vulkan_scene_pipeline_shader_combo_values(&[("MASK", 1), ("BACKGROUND", 1)]),
        );
        let plan = native_vulkan_scene_effect_shader_artifact_path_plan_for_key(
            Path::new("artifacts/scene-shaders"),
            &key,
        )
        .expect("effect combo variant artifact path plan");

        assert_eq!(plan.shader, "effects/iris");
        assert_eq!(
            plan.shader_combo_values,
            vec!["BACKGROUND=1".to_owned(), "MASK=1".to_owned()]
        );
        assert_eq!(
            plan.vertex_path,
            PathBuf::from("artifacts/scene-shaders/effects/iris__BACKGROUND_1__MASK_1.vert.spv")
        );
        assert_eq!(
            plan.fragment_path,
            PathBuf::from("artifacts/scene-shaders/effects/iris__BACKGROUND_1__MASK_1.frag.spv")
        );
        assert_eq!(
            plan.source_reference,
            "reverse-engineered/shaders/effects/iris"
        );
    }

    #[test]
    fn effect_shader_artifact_plan_rejects_undeclared_iris_combo_variant() {
        let key = NativeVulkanSceneEffectShaderArtifactKey::variant(
            "effects/iris",
            native_vulkan_scene_pipeline_shader_combo_values(&[("BLUR", 1)]),
        );

        let err = native_vulkan_scene_effect_shader_artifact_path_plan_for_key(
            Path::new("artifacts/scene-shaders"),
            &key,
        )
        .expect_err("undeclared iris combo variant must fail");

        assert!(err.contains("undeclared WE combo 'BLUR'"));
    }

    #[test]
    fn effect_shader_artifact_plan_keeps_unknown_effect_interfaces_path_checked_only() {
        let key = NativeVulkanSceneEffectShaderArtifactKey::variant(
            "effects/blur_downsample4",
            native_vulkan_scene_pipeline_shader_combo_values(&[("KERNEL", 1)]),
        );

        let plan = native_vulkan_scene_effect_shader_artifact_path_plan_for_key(
            Path::new("artifacts/scene-shaders"),
            &key,
        )
        .expect("unknown effect shader interface remains path validated");

        assert_eq!(plan.shader, "effects/blur_downsample4");
        assert_eq!(plan.shader_combo_values, vec!["KERNEL=1".to_owned()]);
    }

    #[test]
    fn effect_shader_artifact_plan_maps_util_passthrough_to_top_level_shader() {
        let plan = native_vulkan_scene_effect_shader_artifact_path_plan(
            Path::new("artifacts/scene-shaders"),
            "util/passthrough",
        )
        .expect("util artifact path plan");

        assert_eq!(plan.shader, "util/passthrough");
        assert!(plan.shader_combo_values.is_empty());
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
        assert!(plan.shader_combo_values.is_empty());
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
            shader_combo_values: Vec::new(),
            vertex_spirv: vec![SPIRV_MAGIC, 1],
            fragment_spirv: vec![SPIRV_MAGIC, 2],
        };

        let shaders = artifacts.mesh_pipeline_shaders();

        assert_eq!(shaders.vertex_spirv, &[SPIRV_MAGIC, 1]);
        assert_eq!(shaders.fragment_spirv, &[SPIRV_MAGIC, 2]);
    }

    #[test]
    fn shader_catalog_requires_exact_we_combo_variant_for_pipeline_key() {
        use crate::engine::scene_engine::{
            SceneBlendContract, SceneGraphPipelineClass, SceneMaterialRenderState,
        };
        use crate::renderer::native_vulkan::scene_backend::pipeline::NativeVulkanScenePipelineVertexLayout;
        use vulkanalia::vk;

        let ordinary_key = NativeVulkanSceneShaderArtifactKey::plain("we/genericimage4");
        let variant_key = NativeVulkanSceneShaderArtifactKey::variant(
            "we/genericimage4",
            native_vulkan_scene_pipeline_shader_combo_values(&[
                ("CLIPPINGTARGET", 1),
                ("CLIPPINGUVS", 1),
            ]),
        );
        let catalog = NativeVulkanSceneShaderArtifactCatalog {
            shaders: BTreeMap::from([(
                ordinary_key,
                NativeVulkanSceneShaderArtifacts {
                    shader: "we/genericimage4".to_owned(),
                    shader_combo_values: Vec::new(),
                    vertex_spirv: vec![SPIRV_MAGIC, 1],
                    fragment_spirv: vec![SPIRV_MAGIC, 2],
                },
            )]),
            plan: NativeVulkanSceneShaderArtifactCatalogPlan {
                shader_count: 1,
                shaders: vec!["we/genericimage4".to_owned()],
                command_order: [
                    "collect_scene_graph_draw_shader_names",
                    "append_layer_alpha_mask_shader_variant_keys",
                    "resolve_we_scene_shader_variant_artifact_paths",
                    "read_unique_scene_shader_spirv",
                ],
            },
        };
        let variant_pipeline_key = NativeVulkanScenePipelineCacheKey {
            shader: "we/genericimage4".to_owned(),
            shader_combo_values: variant_key.shader_combo_values.clone(),
            blend: SceneBlendContract::TranslucentAlpha,
            render_state: SceneMaterialRenderState::translucent_2d(),
            pipeline_class: SceneGraphPipelineClass::Mesh,
            vertex_layout: NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
            target_format: vk::Format::B8G8R8A8_UNORM,
            texture_slot_mask: (1u32 << 0) | (1u32 << 8),
        };

        let err = catalog
            .mesh_pipeline_shaders_for_key(&variant_pipeline_key)
            .expect_err("ordinary artifact must not satisfy combo variant key");

        assert!(err.contains("combo variant artifact"));
        assert!(err.contains("CLIPPINGTARGET=1"));
    }

    #[test]
    fn loaded_effect_shader_artifacts_borrow_as_pipeline_shader_slices() {
        let artifacts = NativeVulkanSceneEffectShaderArtifacts {
            shader: "effects/iris".to_owned(),
            shader_combo_values: Vec::new(),
            vertex_spirv: vec![SPIRV_MAGIC, 1],
            fragment_spirv: vec![SPIRV_MAGIC, 2],
        };

        let shaders = artifacts.effect_pipeline_shaders();

        assert_eq!(shaders.vertex_spirv, &[SPIRV_MAGIC, 1]);
        assert_eq!(shaders.fragment_spirv, &[SPIRV_MAGIC, 2]);
    }

    #[test]
    fn effect_shader_catalog_collects_unique_shader_combo_variants() {
        let mut graph =
            effect_graph_with_shader_names(&["effects/iris", "effects/iris", "effects/iris"]);
        graph.passes[0].combos.insert("MASK".to_owned(), 1);
        graph.passes[0].combos.insert("BACKGROUND".to_owned(), 1);
        graph.passes[1].combos.insert("MASK".to_owned(), 1);
        graph.passes[1].combos.insert("BACKGROUND".to_owned(), 1);
        graph.passes[2].combos.insert("MASK".to_owned(), 1);

        let keys = required_effect_shader_artifact_keys(&graph)
            .expect("effect shader variant key collection");
        let labels = keys
            .iter()
            .map(NativeVulkanSceneEffectShaderArtifactKey::label)
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "effects/iris[BACKGROUND=1,MASK=1]".to_owned(),
                "effects/iris[MASK=1]".to_owned()
            ]
        );
    }

    #[test]
    fn effect_shader_catalog_requires_exact_we_combo_variant_for_pipeline_key() {
        use crate::engine::scene_engine::{
            SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
            we::WeEffectKind,
        };
        use crate::renderer::native_vulkan::scene_backend::effect_pipeline::{
            NativeVulkanSceneEffectPipelineCacheKey, NativeVulkanSceneEffectRasterGeometry,
        };
        use vulkanalia::vk;

        let ordinary_key = NativeVulkanSceneEffectShaderArtifactKey::plain("effects/iris");
        let variant_key = NativeVulkanSceneEffectShaderArtifactKey::variant(
            "effects/iris",
            native_vulkan_scene_pipeline_shader_combo_values(&[("MASK", 1), ("BACKGROUND", 1)]),
        );
        let catalog = NativeVulkanSceneEffectShaderArtifactCatalog {
            shaders: BTreeMap::from([(
                ordinary_key,
                NativeVulkanSceneEffectShaderArtifacts {
                    shader: "effects/iris".to_owned(),
                    shader_combo_values: Vec::new(),
                    vertex_spirv: vec![SPIRV_MAGIC, 1],
                    fragment_spirv: vec![SPIRV_MAGIC, 2],
                },
            )]),
            plan: NativeVulkanSceneEffectShaderArtifactCatalogPlan {
                shader_count: 1,
                shaders: vec!["effects/iris".to_owned()],
                command_order: [
                    "collect_effect_shader_variant_keys_from_pass_graph",
                    "append_layer_alpha_mask_copy_back_shader_variant_keys",
                    "resolve_we_effect_shader_variant_artifact_paths",
                    "read_unique_effect_vertex_spirv",
                    "read_unique_effect_fragment_spirv",
                ],
            },
        };
        let variant_pipeline_key = NativeVulkanSceneEffectPipelineCacheKey {
            shader: "effects/iris".to_owned(),
            shader_combo_values: variant_key.shader_combo_values.clone(),
            effect: WeEffectKind::Iris,
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
            target_format: vk::Format::R16G16B16A16_SFLOAT,
            texture_slot_mask: 1,
            effect_uniform_buffer_count: 0,
            raster_geometry: NativeVulkanSceneEffectRasterGeometry::FullscreenTriangle,
        };

        let err = catalog
            .effect_pipeline_shaders_for_key(&variant_pipeline_key)
            .expect_err("ordinary effect artifact must not satisfy combo variant key");

        assert!(err.contains("combo variant artifact"));
        assert!(err.contains("BACKGROUND=1"));
        assert!(err.contains("MASK=1"));
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
            has_active_aux_clear_target: false,
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
