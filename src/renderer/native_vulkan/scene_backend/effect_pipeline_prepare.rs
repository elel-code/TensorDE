//! Scene effect pipeline cold-path prepare boundary.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/servers/rendering/renderer_rd/effects/copy_effects.cpp`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{SceneEffectPassGraphPlan, SceneGraphTarget};

use super::effect_pipeline_factory::NativeVulkanSceneEffectPipelineLayoutSpec;
use super::effect_pipeline_warmup::NativeVulkanSceneEffectPipelineWarmupPlan;
use super::frame_resources::NativeVulkanSceneFrameResources;
use super::shader_artifacts::NativeVulkanSceneEffectShaderArtifactCatalog;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectPipelinePreparePlan {
    pub target_formats: Vec<String>,
    pub target_format_count: usize,
    pub material_pass_count: usize,
    pub cache_key_count: usize,
    pub shader_artifact_count: usize,
    pub created_pipeline_count: usize,
    pub reused_pipeline_count: usize,
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub descriptor_model: &'static str,
    pub command_order: [&'static str; 6],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_prepare_scene_effect_pipeline_cache_with_target_formats<
    TargetFormat,
>(
    device: &Device,
    frame_resources: &mut NativeVulkanSceneFrameResources,
    graph: &SceneEffectPassGraphPlan,
    target_format: TargetFormat,
    shader_catalog: &NativeVulkanSceneEffectShaderArtifactCatalog,
) -> Result<NativeVulkanSceneEffectPipelinePreparePlan, String>
where
    TargetFormat: FnMut(SceneGraphTarget) -> Result<vk::Format, String>,
{
    let warmup =
        NativeVulkanSceneEffectPipelineWarmupPlan::from_effect_pass_graph_with_target_formats(
            graph,
            target_format,
        )?;
    let effect_resource_heap = frame_resources
        .current_effect_resource_heap_frame_plan()
        .ok_or_else(|| {
            "scene effect pipeline prepare requires current effect resource heap frame plan"
                .to_owned()
        })?;
    let descriptor_heap_plan = effect_resource_heap.descriptor_heap_plan.clone();
    let resource_descriptor_count = effect_resource_heap.resource_descriptor_count;
    let sampler_descriptor_count = effect_resource_heap.sampler_descriptor_count;
    let descriptor_model = effect_resource_heap.descriptor_model;
    let pipeline_layout = NativeVulkanSceneEffectPipelineLayoutSpec {
        effect_resource_heap_plan: &descriptor_heap_plan,
    };
    let mut created_pipeline_count = 0usize;
    let mut reused_pipeline_count = 0usize;

    for key in warmup.cache_keys().iter().cloned() {
        if frame_resources.has_effect_pipeline(&key) {
            reused_pipeline_count = reused_pipeline_count.saturating_add(1);
        } else {
            created_pipeline_count = created_pipeline_count.saturating_add(1);
        }
        let shaders = shader_catalog.effect_pipeline_shaders_for_key(&key)?;
        frame_resources.resolve_effect_pipeline(device, key, shaders, pipeline_layout)?;
    }

    NativeVulkanSceneEffectPipelinePreparePlan::from_counts(
        &warmup,
        shader_catalog.shader_count(),
        created_pipeline_count,
        reused_pipeline_count,
        resource_descriptor_count,
        sampler_descriptor_count,
        descriptor_model,
    )
}

impl NativeVulkanSceneEffectPipelinePreparePlan {
    fn from_counts(
        warmup: &NativeVulkanSceneEffectPipelineWarmupPlan,
        shader_artifact_count: usize,
        created_pipeline_count: usize,
        reused_pipeline_count: usize,
        resource_descriptor_count: usize,
        sampler_descriptor_count: usize,
        descriptor_model: &'static str,
    ) -> Result<Self, String> {
        let cache_key_count = warmup.cache_keys().len();
        if created_pipeline_count.saturating_add(reused_pipeline_count) != cache_key_count {
            return Err(format!(
                "scene effect pipeline prepare counted {} create/reuse actions for {} cache keys",
                created_pipeline_count.saturating_add(reused_pipeline_count),
                cache_key_count
            ));
        }
        Ok(Self {
            target_formats: warmup
                .target_formats()
                .iter()
                .map(|format| format!("{format:?}"))
                .collect(),
            target_format_count: warmup.target_formats().len(),
            material_pass_count: warmup.material_pass_count(),
            cache_key_count,
            shader_artifact_count,
            created_pipeline_count,
            reused_pipeline_count,
            resource_descriptor_count,
            sampler_descriptor_count,
            descriptor_model,
            command_order: [
                "collect_unique_effect_pipeline_keys",
                "read_current_effect_resource_heap_plan",
                "resolve_effect_shader_artifacts",
                "resolve_scene_effect_pipeline_cache",
                "preserve_copy_swap_commands_outside_pipeline_cache",
                "require_warmed_effect_pipeline_cache_for_present_frame",
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::engine::scene_engine::{
        SceneCullMode, SceneDepthTest, SceneEffectPassBlend, SceneEffectPassGraphInputBinding,
        SceneEffectPassGraphInputSource, SceneEffectPassGraphMaterialPass,
        SceneEffectPassGraphOutput, SceneEffectTextureResourceBinding, SceneObjectId,
        SceneResourceId, we::WeEffectKind,
    };

    #[test]
    fn effect_pipeline_prepare_plan_records_cold_path_counts() {
        let warmup =
            NativeVulkanSceneEffectPipelineWarmupPlan::from_effect_pass_graph_with_target_formats(
                &graph(vec![
                    pass(0, "effects/iris", SceneGraphTarget::EffectTarget(0)),
                    pass(1, "effects/blur_downsample4", SceneGraphTarget::NamedFbo(2)),
                ]),
                |target| match target {
                    SceneGraphTarget::EffectTarget(0) => Ok(vk::Format::R16G16B16A16_SFLOAT),
                    SceneGraphTarget::NamedFbo(2) => Ok(vk::Format::B8G8R8A8_UNORM),
                    target => Err(format!("unexpected target {target:?}")),
                },
            )
            .expect("warmup");

        let plan = NativeVulkanSceneEffectPipelinePreparePlan::from_counts(
            &warmup,
            2,
            1,
            1,
            3,
            3,
            "VK_EXT_descriptor_heap",
        )
        .expect("prepare plan");

        assert_eq!(plan.target_format_count, 2);
        assert_eq!(
            plan.target_formats,
            vec!["R16G16B16A16_SFLOAT", "B8G8R8A8_UNORM"]
        );
        assert_eq!(plan.material_pass_count, 2);
        assert_eq!(plan.cache_key_count, 2);
        assert_eq!(plan.shader_artifact_count, 2);
        assert_eq!(plan.created_pipeline_count, 1);
        assert_eq!(plan.reused_pipeline_count, 1);
        assert_eq!(plan.resource_descriptor_count, 3);
        assert_eq!(plan.sampler_descriptor_count, 3);
        assert_eq!(plan.descriptor_model, "VK_EXT_descriptor_heap");
        assert_eq!(
            plan.command_order,
            [
                "collect_unique_effect_pipeline_keys",
                "read_current_effect_resource_heap_plan",
                "resolve_effect_shader_artifacts",
                "resolve_scene_effect_pipeline_cache",
                "preserve_copy_swap_commands_outside_pipeline_cache",
                "require_warmed_effect_pipeline_cache_for_present_frame"
            ]
        );
    }

    #[test]
    fn effect_pipeline_prepare_plan_rejects_action_count_mismatch() {
        let warmup =
            NativeVulkanSceneEffectPipelineWarmupPlan::from_effect_pass_graph_with_target_formats(
                &graph(vec![pass(
                    0,
                    "effects/iris",
                    SceneGraphTarget::EffectTarget(0),
                )]),
                |_| Ok(vk::Format::R16G16B16A16_SFLOAT),
            )
            .expect("warmup");

        let err = NativeVulkanSceneEffectPipelinePreparePlan::from_counts(
            &warmup,
            1,
            0,
            0,
            1,
            1,
            "VK_EXT_descriptor_heap",
        )
        .expect_err("mismatch must fail");

        assert!(err.contains("create/reuse actions"));
    }

    fn graph(passes: Vec<SceneEffectPassGraphMaterialPass>) -> SceneEffectPassGraphPlan {
        SceneEffectPassGraphPlan {
            material_pass_count: passes.len(),
            passes,
            ..SceneEffectPassGraphPlan::empty()
        }
    }

    fn pass(
        graph_pass_index: usize,
        shader: &str,
        output: SceneGraphTarget,
    ) -> SceneEffectPassGraphMaterialPass {
        SceneEffectPassGraphMaterialPass {
            graph_pass_index,
            object: SceneObjectId(7),
            program_index: 0,
            pass_index: graph_pass_index,
            effect_file: "effects/test/effect.json".to_owned(),
            effect: WeEffectKind::Unknown,
            shader: Some(shader.to_owned()),
            source: Some(SceneEffectPassGraphInputBinding {
                slot: 0,
                image: crate::engine::scene_engine::SceneEffectImageRef::SourceTexture,
                source: SceneEffectPassGraphInputSource::ObjectSourceTexture(SceneResourceId(9)),
            }),
            input_bindings: Vec::new(),
            output: SceneEffectPassGraphOutput::GraphTarget(output),
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            texture_resources: vec![SceneEffectTextureResourceBinding {
                slot: 1,
                resource: SceneResourceId(10),
            }],
            combos: BTreeMap::new(),
            constants: BTreeMap::new(),
        }
    }
}
