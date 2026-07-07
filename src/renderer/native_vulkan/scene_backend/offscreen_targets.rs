//! Retained offscreen render targets for scene graph/effect passes.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`
//! - `references/godot/servers/rendering/storage/texture_storage.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{SceneGraphExecutionPlan, SceneGraphTarget};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaImage, native_vulkan_vulkanalia_create_color_attachment_sampled_image,
    native_vulkan_vulkanalia_destroy_image,
};

use super::frame_completion::NativeVulkanSceneFrameSubmission;

const SCENE_OFFSCREEN_TARGET_ROLE: &str = "scene-offscreen-target";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneOffscreenTargetFramePlan {
    pub target_count: usize,
    pub targets: Vec<NativeVulkanSceneOffscreenTargetRequirement>,
    pub command_order: [&'static str; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneOffscreenTargetRequirement {
    pub target: SceneGraphTarget,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneOffscreenTargetSyncAction {
    Create {
        record: NativeVulkanSceneOffscreenTargetRecord,
    },
    Reuse {
        record: NativeVulkanSceneOffscreenTargetRecord,
    },
    Replace {
        retired: NativeVulkanSceneOffscreenTargetRecord,
        replacement: NativeVulkanSceneOffscreenTargetRecord,
    },
    Release {
        record: NativeVulkanSceneOffscreenTargetRecord,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneOffscreenTargetRecord {
    pub target: SceneGraphTarget,
    pub format: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneOffscreenTargetBinding {
    pub target: SceneGraphTarget,
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub current_layout: vk::ImageLayout,
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneOffscreenTargetStore {
    targets: BTreeMap<SceneGraphTarget, NativeVulkanSceneOffscreenTargetSlot>,
    pending_retirements: Vec<NativeVulkanSceneOffscreenTargetRetirement>,
    last_actions: Vec<NativeVulkanSceneOffscreenTargetSyncAction>,
}

impl NativeVulkanSceneOffscreenTargetFramePlan {
    pub(in crate::renderer::native_vulkan) fn from_execution_plan<TargetFormat>(
        execution: &SceneGraphExecutionPlan,
        extent: vk::Extent2D,
        target_format: TargetFormat,
    ) -> Result<Self, String>
    where
        TargetFormat: FnMut(SceneGraphTarget) -> Result<vk::Format, String>,
    {
        Self::from_execution_plan_with_effect_targets(execution, extent, target_format, &[])
    }

    pub(in crate::renderer::native_vulkan) fn from_execution_plan_with_effect_targets<
        TargetFormat,
    >(
        execution: &SceneGraphExecutionPlan,
        extent: vk::Extent2D,
        mut target_format: TargetFormat,
        effect_targets: &[NativeVulkanSceneOffscreenTargetRequirement],
    ) -> Result<Self, String>
    where
        TargetFormat: FnMut(SceneGraphTarget) -> Result<vk::Format, String>,
    {
        if extent.width == 0 || extent.height == 0 {
            return Err("scene offscreen target plan requires non-zero extent".to_owned());
        }

        let mut targets =
            BTreeMap::<SceneGraphTarget, NativeVulkanSceneOffscreenTargetRequirement>::new();
        for lifetime in &execution.target_lifetimes {
            if lifetime.target == SceneGraphTarget::Swapchain || lifetime.first_write_pass.is_none()
            {
                continue;
            }
            let format = target_format(lifetime.target)?;
            if format == vk::Format::UNDEFINED {
                return Err(format!(
                    "scene offscreen target {:?} requires a defined format",
                    lifetime.target
                ));
            }
            targets.insert(
                lifetime.target,
                NativeVulkanSceneOffscreenTargetRequirement {
                    target: lifetime.target,
                    format,
                    width: extent.width,
                    height: extent.height,
                },
            );
        }
        for requirement in effect_targets {
            validate_offscreen_target_requirement(*requirement)?;
            if let Some(existing) = targets.get(&requirement.target)
                && existing != requirement
            {
                return Err(format!(
                    "scene offscreen target {:?} has conflicting graph/effect requirements",
                    requirement.target
                ));
            }
            targets.insert(requirement.target, *requirement);
        }

        let targets = targets.into_values().collect::<Vec<_>>();
        Ok(Self {
            target_count: targets.len(),
            targets,
            command_order: [
                "collect_written_non_swapchain_targets",
                "resolve_graph_target_formats",
                "merge_effect_target_requirements",
                "sync_retained_offscreen_target_store",
            ],
        })
    }
}

fn validate_offscreen_target_requirement(
    requirement: NativeVulkanSceneOffscreenTargetRequirement,
) -> Result<(), String> {
    if requirement.target == SceneGraphTarget::Swapchain {
        return Err("scene offscreen target requirement cannot allocate swapchain".to_owned());
    }
    if requirement.format == vk::Format::UNDEFINED {
        return Err(format!(
            "scene offscreen target {:?} requires a defined format",
            requirement.target
        ));
    }
    if requirement.width == 0 || requirement.height == 0 {
        return Err(format!(
            "scene offscreen target {:?} requires non-zero extent",
            requirement.target
        ));
    }
    Ok(())
}

impl NativeVulkanSceneOffscreenTargetStore {
    pub(in crate::renderer::native_vulkan) fn new() -> Self {
        Self {
            targets: BTreeMap::new(),
            pending_retirements: Vec::new(),
            last_actions: Vec::new(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn sync_frame_plan(
        &mut self,
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        frame_submission: NativeVulkanSceneFrameSubmission,
        frame_plan: &NativeVulkanSceneOffscreenTargetFramePlan,
    ) -> Result<&[NativeVulkanSceneOffscreenTargetSyncAction], String> {
        self.last_actions.clear();
        let requirements = offscreen_target_requirement_map(&frame_plan.targets)?;
        let active = requirements.keys().copied().collect::<BTreeSet<_>>();
        let stale = self
            .targets
            .keys()
            .copied()
            .filter(|target| !active.contains(target))
            .collect::<Vec<_>>();
        for target in stale {
            if let Some(slot) = self.targets.remove(&target) {
                self.defer_retirement(frame_submission, slot.image);
                self.last_actions
                    .push(NativeVulkanSceneOffscreenTargetSyncAction::Release {
                        record: slot.record,
                    });
            }
        }

        for (target, requirement) in requirements {
            let new_record = offscreen_target_record(requirement);
            if let Some(slot) = self.targets.get(&target)
                && slot.requirement == requirement
            {
                self.last_actions
                    .push(NativeVulkanSceneOffscreenTargetSyncAction::Reuse {
                        record: slot.record.clone(),
                    });
                continue;
            }

            let image = native_vulkan_vulkanalia_create_color_attachment_sampled_image(
                device,
                memory_properties,
                SCENE_OFFSCREEN_TARGET_ROLE,
                requirement.format,
                requirement.width,
                requirement.height,
            )?;

            match self.targets.insert(
                target,
                NativeVulkanSceneOffscreenTargetSlot {
                    requirement,
                    record: new_record.clone(),
                    image,
                    current_layout: vk::ImageLayout::UNDEFINED,
                },
            ) {
                Some(retired_slot) => {
                    self.defer_retirement(frame_submission, retired_slot.image);
                    self.last_actions
                        .push(NativeVulkanSceneOffscreenTargetSyncAction::Replace {
                            retired: retired_slot.record,
                            replacement: new_record,
                        });
                }
                None => {
                    self.last_actions
                        .push(NativeVulkanSceneOffscreenTargetSyncAction::Create {
                            record: new_record,
                        });
                }
            }
        }

        Ok(&self.last_actions)
    }

    pub(in crate::renderer::native_vulkan) fn release_completed_targets(
        &mut self,
        device: &Device,
        completed_submission: NativeVulkanSceneFrameSubmission,
    ) -> usize {
        let mut retained = Vec::new();
        let mut released = 0usize;
        for retirement in std::mem::take(&mut self.pending_retirements) {
            if completed_submission.covers(retirement.frame_submission) {
                native_vulkan_vulkanalia_destroy_image(device, retirement.image);
                released = released.saturating_add(1);
            } else {
                retained.push(retirement);
            }
        }
        self.pending_retirements = retained;
        released
    }

    pub(in crate::renderer::native_vulkan) fn target_binding(
        &self,
        target: SceneGraphTarget,
    ) -> Result<NativeVulkanSceneOffscreenTargetBinding, String> {
        let slot = self
            .targets
            .get(&target)
            .ok_or_else(|| format!("missing retained scene offscreen target {target:?}"))?;
        Ok(NativeVulkanSceneOffscreenTargetBinding {
            target,
            image: slot.image.image,
            view: slot.image.view,
            sampler: slot.image.sampler,
            format: slot.requirement.format,
            width: slot.requirement.width,
            height: slot.requirement.height,
            current_layout: slot.current_layout,
        })
    }

    pub(in crate::renderer::native_vulkan) fn mark_target_layout(
        &mut self,
        target: SceneGraphTarget,
        layout: vk::ImageLayout,
    ) -> Result<(), String> {
        let slot = self
            .targets
            .get_mut(&target)
            .ok_or_else(|| format!("missing retained scene offscreen target {target:?}"))?;
        slot.current_layout = layout;
        Ok(())
    }

    pub(in crate::renderer::native_vulkan) fn last_actions(
        &self,
    ) -> &[NativeVulkanSceneOffscreenTargetSyncAction] {
        &self.last_actions
    }

    pub(in crate::renderer::native_vulkan) fn destroy_all(&mut self, device: &Device) {
        for (_, slot) in std::mem::take(&mut self.targets) {
            native_vulkan_vulkanalia_destroy_image(device, slot.image);
        }
        for retirement in std::mem::take(&mut self.pending_retirements) {
            native_vulkan_vulkanalia_destroy_image(device, retirement.image);
        }
        self.last_actions.clear();
    }

    fn defer_retirement(
        &mut self,
        frame_submission: NativeVulkanSceneFrameSubmission,
        image: NativeVulkanVulkanaliaImage,
    ) {
        self.pending_retirements
            .push(NativeVulkanSceneOffscreenTargetRetirement {
                frame_submission,
                image,
            });
    }
}

impl Default for NativeVulkanSceneOffscreenTargetStore {
    fn default() -> Self {
        Self::new()
    }
}

struct NativeVulkanSceneOffscreenTargetSlot {
    requirement: NativeVulkanSceneOffscreenTargetRequirement,
    record: NativeVulkanSceneOffscreenTargetRecord,
    image: NativeVulkanVulkanaliaImage,
    current_layout: vk::ImageLayout,
}

struct NativeVulkanSceneOffscreenTargetRetirement {
    frame_submission: NativeVulkanSceneFrameSubmission,
    image: NativeVulkanVulkanaliaImage,
}

fn offscreen_target_requirement_map(
    requirements: &[NativeVulkanSceneOffscreenTargetRequirement],
) -> Result<BTreeMap<SceneGraphTarget, NativeVulkanSceneOffscreenTargetRequirement>, String> {
    let mut by_target = BTreeMap::new();
    for requirement in requirements {
        if by_target.insert(requirement.target, *requirement).is_some() {
            return Err(format!(
                "duplicate scene offscreen target requirement for {:?}",
                requirement.target
            ));
        }
    }
    Ok(by_target)
}

fn offscreen_target_record(
    requirement: NativeVulkanSceneOffscreenTargetRequirement,
) -> NativeVulkanSceneOffscreenTargetRecord {
    NativeVulkanSceneOffscreenTargetRecord {
        target: requirement.target,
        format: format!("{:?}", requirement.format),
        width: requirement.width,
        height: requirement.height,
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
    fn offscreen_target_plan_collects_written_non_swapchain_targets() {
        let execution = SceneGraphExecutionPlan::from_graph(&SceneGraph {
            passes: vec![
                pass(
                    "effect-a",
                    None,
                    SceneGraphTarget::ImageLocalMain(0),
                    SceneObjectId(1),
                ),
                pass(
                    "scene-main",
                    Some(SceneGraphTarget::ImageLocalMain(0)),
                    SceneGraphTarget::Swapchain,
                    SceneObjectId(2),
                ),
            ],
        });

        let plan = NativeVulkanSceneOffscreenTargetFramePlan::from_execution_plan(
            &execution,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            |target| match target {
                SceneGraphTarget::ImageLocalMain(0) => Ok(vk::Format::R16G16B16A16_SFLOAT),
                target => Err(format!("unexpected target {target:?}")),
            },
        )
        .expect("offscreen target plan");

        assert_eq!(plan.target_count, 1);
        assert_eq!(plan.targets[0].target, SceneGraphTarget::ImageLocalMain(0));
        assert_eq!(plan.targets[0].format, vk::Format::R16G16B16A16_SFLOAT);
        assert_eq!(plan.targets[0].width, 3840);
        assert_eq!(
            plan.command_order,
            [
                "collect_written_non_swapchain_targets",
                "resolve_graph_target_formats",
                "merge_effect_target_requirements",
                "sync_retained_offscreen_target_store"
            ]
        );
    }

    #[test]
    fn offscreen_target_plan_merges_effect_target_requirements() {
        let execution = SceneGraphExecutionPlan::from_graph(&SceneGraph { passes: Vec::new() });
        let effect_target = NativeVulkanSceneOffscreenTargetRequirement {
            target: SceneGraphTarget::NamedFbo(7),
            format: vk::Format::R16_SFLOAT,
            width: 960,
            height: 540,
        };

        let plan =
            NativeVulkanSceneOffscreenTargetFramePlan::from_execution_plan_with_effect_targets(
                &execution,
                vk::Extent2D {
                    width: 3840,
                    height: 2160,
                },
                |_| Err("empty graph should not resolve graph target formats".to_owned()),
                &[effect_target],
            )
            .expect("offscreen target plan");

        assert_eq!(plan.target_count, 1);
        assert_eq!(plan.targets[0], effect_target);
    }

    #[test]
    fn offscreen_target_plan_rejects_conflicting_effect_target_requirement() {
        let execution = SceneGraphExecutionPlan::from_graph(&SceneGraph {
            passes: vec![pass(
                "effect-a",
                None,
                SceneGraphTarget::NamedFbo(7),
                SceneObjectId(1),
            )],
        });
        let effect_target = NativeVulkanSceneOffscreenTargetRequirement {
            target: SceneGraphTarget::NamedFbo(7),
            format: vk::Format::R16_SFLOAT,
            width: 960,
            height: 540,
        };

        let err =
            NativeVulkanSceneOffscreenTargetFramePlan::from_execution_plan_with_effect_targets(
                &execution,
                vk::Extent2D {
                    width: 3840,
                    height: 2160,
                },
                |_| Ok(vk::Format::R16G16B16A16_SFLOAT),
                &[effect_target],
            )
            .expect_err("conflicting requirements must fail");

        assert!(err.contains("conflicting graph/effect requirements"));
    }

    #[test]
    fn offscreen_target_plan_ignores_input_only_external_targets() {
        let execution = SceneGraphExecutionPlan::from_graph(&SceneGraph {
            passes: vec![pass(
                "scene-main",
                Some(SceneGraphTarget::NamedFbo(7)),
                SceneGraphTarget::Swapchain,
                SceneObjectId(1),
            )],
        });

        let plan = NativeVulkanSceneOffscreenTargetFramePlan::from_execution_plan(
            &execution,
            vk::Extent2D {
                width: 1920,
                height: 1080,
            },
            |_| Err("input-only target should not be allocated".to_owned()),
        )
        .expect("input-only target plan");

        assert_eq!(plan.target_count, 0);
        assert!(plan.targets.is_empty());
    }

    #[test]
    fn offscreen_target_requirement_map_rejects_duplicates() {
        let requirement = NativeVulkanSceneOffscreenTargetRequirement {
            target: SceneGraphTarget::EffectTarget(0),
            format: vk::Format::B8G8R8A8_UNORM,
            width: 1280,
            height: 720,
        };

        let err = offscreen_target_requirement_map(&[requirement, requirement])
            .expect_err("duplicate target must fail");

        assert!(err.contains("duplicate scene offscreen target requirement"));
    }

    fn pass(
        name: &str,
        input: Option<SceneGraphTarget>,
        output: SceneGraphTarget,
        object: SceneObjectId,
    ) -> SceneGraphPass {
        SceneGraphPass {
            name: name.to_owned(),
            input,
            output,
            draws: vec![SceneGraphDraw {
                object,
                pipeline: SceneGraphPipelineClass::Mesh,
                material: SceneMaterialKey {
                    shader: "we/genericimage4".to_owned(),
                    blend: SceneBlendContract::TranslucentAlpha,
                    render_state:
                        crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(),
                },
                geometry: Some(SceneGeometryId(object.0)),
                puppet: None,
                resources: Vec::new(),
                index_count: 6,
            }],
        }
    }
}
