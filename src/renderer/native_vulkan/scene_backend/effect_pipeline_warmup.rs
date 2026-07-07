//! Scene effect pipeline warmup planning.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneEffectPassGraphOutput, SceneEffectPassGraphPlan, SceneGraphTarget,
};

use super::effect_pipeline::{
    NativeVulkanSceneEffectPipelineCacheKey, NativeVulkanSceneEffectPipelineKey,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectPipelineWarmupPlan {
    material_pass_count: usize,
    target_formats: Vec<vk::Format>,
    cache_keys: Vec<NativeVulkanSceneEffectPipelineCacheKey>,
    command_order: [&'static str; 4],
}

impl NativeVulkanSceneEffectPipelineWarmupPlan {
    pub(in crate::renderer::native_vulkan) fn from_effect_pass_graph_with_target_formats<
        TargetFormat,
    >(
        graph: &SceneEffectPassGraphPlan,
        mut target_format: TargetFormat,
    ) -> Result<Self, String>
    where
        TargetFormat: FnMut(SceneGraphTarget) -> Result<vk::Format, String>,
    {
        let mut cache_keys = Vec::new();
        let mut target_formats = Vec::new();
        for pass in &graph.passes {
            let output = effect_pass_render_target(&pass.output, pass.pass_index, pass.object)?;
            let pass_target_format = target_format(output)?;
            if pass_target_format == vk::Format::UNDEFINED {
                return Err(format!(
                    "scene effect pipeline warmup requires defined format for graph target {:?}",
                    output
                ));
            }
            if !target_formats.contains(&pass_target_format) {
                target_formats.push(pass_target_format);
            }

            let key = NativeVulkanSceneEffectPipelineCacheKey::from_bind_key(
                NativeVulkanSceneEffectPipelineKey::from_pass_and_target_format(
                    pass,
                    pass_target_format,
                )?,
            );
            if !cache_keys.iter().any(|existing| existing == &key) {
                cache_keys.push(key);
            }
        }

        Ok(Self {
            material_pass_count: graph.passes.len(),
            target_formats,
            cache_keys,
            command_order: [
                "resolve_effect_pass_target_formats",
                "collect_unique_effect_pipeline_keys",
                "preserve_effect_copy_swap_as_non_pipeline_commands",
                "require_warmed_effect_pipeline_cache",
            ],
        })
    }

    pub(in crate::renderer::native_vulkan) fn material_pass_count(&self) -> usize {
        self.material_pass_count
    }

    pub(in crate::renderer::native_vulkan) fn target_formats(&self) -> &[vk::Format] {
        &self.target_formats
    }

    pub(in crate::renderer::native_vulkan) fn cache_keys(
        &self,
    ) -> &[NativeVulkanSceneEffectPipelineCacheKey] {
        &self.cache_keys
    }

    pub(in crate::renderer::native_vulkan) fn command_order(&self) -> [&'static str; 4] {
        self.command_order
    }
}

fn effect_pass_render_target(
    output: &SceneEffectPassGraphOutput,
    pass_index: usize,
    object: crate::engine::scene_engine::SceneObjectId,
) -> Result<SceneGraphTarget, String> {
    match output {
        SceneEffectPassGraphOutput::GraphTarget(target) => Ok(*target),
        SceneEffectPassGraphOutput::ObjectFinal(object_final) => {
            if *object_final != object {
                return Err(format!(
                    "scene effect pipeline warmup pass {pass_index} object {:?} has mismatched ObjectFinal({object_final:?}) output",
                    object
                ));
            }
            Ok(SceneGraphTarget::ObjectFinal(*object_final))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::engine::scene_engine::{
        SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
        SceneEffectPassGraphInputBinding, SceneEffectPassGraphInputSource,
        SceneEffectPassGraphMaterialPass, SceneEffectPassGraphOutput,
        SceneEffectTextureResourceBinding, SceneObjectId, SceneResourceId, we::WeEffectKind,
    };

    #[test]
    fn effect_warmup_collects_unique_shader_target_texture_keys() {
        let graph = graph(vec![
            pass(
                0,
                "effects/iris",
                SceneGraphTarget::EffectTarget(0),
                vec![0, 1],
            ),
            pass(
                1,
                "effects/iris",
                SceneGraphTarget::EffectTarget(0),
                vec![0, 1],
            ),
            pass(
                2,
                "effects/blur_downsample4",
                SceneGraphTarget::NamedFbo(2),
                vec![0],
            ),
        ]);

        let plan =
            NativeVulkanSceneEffectPipelineWarmupPlan::from_effect_pass_graph_with_target_formats(
                &graph,
                |target| match target {
                    SceneGraphTarget::EffectTarget(0) => Ok(vk::Format::R16G16B16A16_SFLOAT),
                    SceneGraphTarget::NamedFbo(2) => Ok(vk::Format::B8G8R8A8_UNORM),
                    target => Err(format!("unexpected target {target:?}")),
                },
            )
            .expect("effect warmup");

        assert_eq!(plan.material_pass_count(), 3);
        assert_eq!(plan.cache_keys().len(), 2);
        assert_eq!(plan.cache_keys()[0].shader, "effects/iris");
        assert_eq!(plan.cache_keys()[0].texture_slot_mask, 0b11);
        assert_eq!(plan.cache_keys()[1].shader, "effects/blur_downsample4");
        assert_eq!(
            plan.target_formats(),
            &[vk::Format::R16G16B16A16_SFLOAT, vk::Format::B8G8R8A8_UNORM]
        );
    }

    #[test]
    fn effect_warmup_resolves_object_final_to_retained_target() {
        let mut effect_pass = pass(
            0,
            "effects/iris",
            SceneGraphTarget::EffectTarget(0),
            vec![0],
        );
        effect_pass.output = SceneEffectPassGraphOutput::ObjectFinal(SceneObjectId(7));
        let graph = graph(vec![effect_pass]);

        let plan =
            NativeVulkanSceneEffectPipelineWarmupPlan::from_effect_pass_graph_with_target_formats(
                &graph,
                |target| match target {
                    SceneGraphTarget::ObjectFinal(SceneObjectId(7)) => {
                        Ok(vk::Format::B8G8R8A8_UNORM)
                    }
                    target => Err(format!("unexpected target {target:?}")),
                },
            )
            .expect("object final warmup");

        assert_eq!(
            plan.cache_keys()[0].target_format,
            vk::Format::B8G8R8A8_UNORM
        );
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
        slots: Vec<u32>,
    ) -> SceneEffectPassGraphMaterialPass {
        let source = slots
            .first()
            .copied()
            .map(|slot| SceneEffectPassGraphInputBinding {
                slot,
                image: crate::engine::scene_engine::SceneEffectImageRef::SourceTexture,
                source: SceneEffectPassGraphInputSource::ObjectSourceTexture(SceneResourceId(9)),
            });
        let texture_resources = slots
            .into_iter()
            .skip(source.iter().count())
            .map(|slot| SceneEffectTextureResourceBinding {
                slot,
                resource: SceneResourceId(10 + slot),
            })
            .collect();

        SceneEffectPassGraphMaterialPass {
            graph_command_index: graph_pass_index,
            graph_pass_index,
            object: SceneObjectId(7),
            program_index: 0,
            pass_index: graph_pass_index,
            effect_file: "effects/test/effect.json".to_owned(),
            effect: WeEffectKind::Unknown,
            shader: Some(shader.to_owned()),
            source,
            input_bindings: Vec::new(),
            output: SceneEffectPassGraphOutput::GraphTarget(output),
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
            texture_resources,
            combos: BTreeMap::new(),
            constants: BTreeMap::new(),
        }
    }
}
