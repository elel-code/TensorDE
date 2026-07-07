//! Scene graph target format planning for Vulkan pipelines and offscreen targets.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`

use std::collections::BTreeMap;

use serde::Serialize;
use vulkanalia::vk;

use crate::engine::scene_engine::{SceneGraphExecutionPlan, SceneGraphTarget};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneGraphTargetFormatPlan {
    pub target_count: usize,
    pub entries: Vec<NativeVulkanSceneGraphTargetFormatEntry>,
    pub offscreen_policy: &'static str,
    pub command_order: [&'static str; 3],
    #[serde(skip)]
    formats: BTreeMap<SceneGraphTarget, vk::Format>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneGraphTargetFormatEntry {
    pub target: SceneGraphTarget,
    pub format: &'static str,
    pub source: &'static str,
}

impl NativeVulkanSceneGraphTargetFormatPlan {
    pub(in crate::renderer::native_vulkan) fn from_execution_plan(
        execution: &SceneGraphExecutionPlan,
        swapchain_format: vk::Format,
    ) -> Result<Self, String> {
        if swapchain_format == vk::Format::UNDEFINED {
            return Err(
                "scene graph target format plan requires defined swapchain format".to_owned(),
            );
        }

        let mut formats = BTreeMap::new();
        let mut entries = Vec::new();
        for lifetime in &execution.target_lifetimes {
            let (format, source) = match lifetime.target {
                SceneGraphTarget::Swapchain => (swapchain_format, "swapchain_surface_format"),
                SceneGraphTarget::ImageLocalMain(_)
                | SceneGraphTarget::ImageLocalSub(_)
                | SceneGraphTarget::EffectTarget(_) => (
                    vk::Format::R16G16B16A16_SFLOAT,
                    "scene_effect_target_half_float_policy",
                ),
                SceneGraphTarget::NamedFbo(_) => {
                    return Err(format!(
                        "scene graph target {:?} requires explicit effect FBO format metadata",
                        lifetime.target
                    ));
                }
            };
            formats.insert(lifetime.target, format);
            entries.push(NativeVulkanSceneGraphTargetFormatEntry {
                target: lifetime.target,
                format: vulkan_format_label(format),
                source,
            });
        }

        Ok(Self {
            target_count: entries.len(),
            entries,
            offscreen_policy: "internal ImageLocal*/EffectTarget use rgba16f until convert supplies per-FBO format; NamedFbo must be explicit",
            command_order: [
                "read_scene_graph_target_lifetimes",
                "assign_swapchain_or_effect_target_format",
                "reject_named_fbo_without_explicit_format",
            ],
            formats,
        })
    }

    pub(in crate::renderer::native_vulkan) fn format(
        &self,
        target: SceneGraphTarget,
    ) -> Result<vk::Format, String> {
        self.formats
            .get(&target)
            .copied()
            .ok_or_else(|| format!("scene graph target format plan has no entry for {target:?}"))
    }

    pub(in crate::renderer::native_vulkan) fn target_format_count(&self) -> usize {
        self.formats.len()
    }

    pub(in crate::renderer::native_vulkan) fn contains_swapchain(&self) -> bool {
        self.formats.contains_key(&SceneGraphTarget::Swapchain)
    }
}

fn vulkan_format_label(format: vk::Format) -> &'static str {
    match format {
        vk::Format::R16G16B16A16_SFLOAT => "R16G16B16A16_SFLOAT",
        vk::Format::B8G8R8A8_UNORM => "B8G8R8A8_UNORM",
        vk::Format::R8G8B8A8_UNORM => "R8G8B8A8_UNORM",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraph, SceneGraphDraw, SceneGraphPass,
        SceneGraphPipelineClass, SceneMaterialKey, SceneObjectId,
    };

    #[test]
    fn target_format_plan_assigns_swapchain_and_internal_effect_targets() {
        let graph = SceneGraph {
            passes: vec![
                pass("effect", None, SceneGraphTarget::EffectTarget(0)),
                pass(
                    "scene-main",
                    Some(SceneGraphTarget::EffectTarget(0)),
                    SceneGraphTarget::Swapchain,
                ),
            ],
        };
        let execution = SceneGraphExecutionPlan::from_graph(&graph);

        let plan = NativeVulkanSceneGraphTargetFormatPlan::from_execution_plan(
            &execution,
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("target format plan");

        assert_eq!(plan.target_count, 2);
        assert_eq!(
            plan.format(SceneGraphTarget::EffectTarget(0)).unwrap(),
            vk::Format::R16G16B16A16_SFLOAT
        );
        assert_eq!(
            plan.format(SceneGraphTarget::Swapchain).unwrap(),
            vk::Format::B8G8R8A8_UNORM
        );
        assert!(plan.contains_swapchain());
    }

    #[test]
    fn target_format_plan_rejects_named_fbo_without_effect_metadata() {
        let graph = SceneGraph {
            passes: vec![pass("named-fbo", None, SceneGraphTarget::NamedFbo(7))],
        };
        let execution = SceneGraphExecutionPlan::from_graph(&graph);

        let err = NativeVulkanSceneGraphTargetFormatPlan::from_execution_plan(
            &execution,
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect_err("NamedFbo format must be explicit");

        assert!(err.contains("explicit effect FBO format metadata"));
    }

    fn pass(
        name: &str,
        input: Option<SceneGraphTarget>,
        output: SceneGraphTarget,
    ) -> SceneGraphPass {
        SceneGraphPass {
            name: name.to_owned(),
            input,
            output,
            draws: vec![SceneGraphDraw {
                object: SceneObjectId(1),
                pipeline: SceneGraphPipelineClass::Mesh,
                material: SceneMaterialKey {
                    shader: "we/genericimage4".to_owned(),
                    blend: SceneBlendContract::TranslucentAlpha,
                    render_state:
                        crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(),
                },
                geometry: Some(SceneGeometryId(1)),
                puppet: None,
                resources: Vec::new(),
                index_count: 6,
            }],
        }
    }
}
