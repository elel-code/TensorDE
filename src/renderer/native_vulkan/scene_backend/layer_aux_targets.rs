//! WE auxiliary layer target requirements for retained Vulkan targets.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/reconstructed/cpp/wallpaper64/layer/resource_update_0x1402065e0.cpp`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use std::collections::BTreeSet;

use serde::Serialize;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneGraphTarget, SceneObjectId, SceneResidentResource, SceneResourceResidencyPlan,
    WE_LAYER_AUX_CLEAR_TARGET_DEFAULT_COLOR_FORMAT, WE_LAYER_AUX_CLEAR_TARGET_HDR_COLOR_FORMAT,
};

use super::offscreen_targets::NativeVulkanSceneOffscreenTargetRequirement;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxTargetPlan {
    pub target_count: usize,
    pub targets: Vec<NativeVulkanSceneLayerAuxTargetRequirement>,
    pub command_order: [&'static str; 5],
    #[serde(skip)]
    requirements: Vec<NativeVulkanSceneOffscreenTargetRequirement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxTargetRequirement {
    pub object: SceneObjectId,
    pub target: SceneGraphTarget,
    pub format: &'static str,
    pub width: u32,
    pub height: u32,
    pub color_format_selector: u32,
    pub aux_format_selector: u32,
    pub r9_selector: u32,
    pub resource_selector: u32,
    pub cache_selector: u32,
    pub reference_points: [&'static str; 4],
}

impl NativeVulkanSceneLayerAuxTargetPlan {
    pub(in crate::renderer::native_vulkan) fn from_residency(
        residency: &SceneResourceResidencyPlan,
    ) -> Result<Self, String> {
        let mut seen = BTreeSet::new();
        let mut targets = Vec::new();
        let mut requirements = Vec::new();

        for resource in &residency.resources {
            let SceneResidentResource::LayerAuxCompositeTargets(aux) = resource else {
                continue;
            };
            if !aux.clear_prep_ready {
                continue;
            }
            if !seen.insert(aux.object) {
                return Err(format!(
                    "scene layer aux target plan saw duplicate aux clear target for object {:?}",
                    aux.object
                ));
            }
            let format = layer_aux_color_format(aux.clear_target_color_format)?;
            let target = SceneGraphTarget::LayerAuxClear(aux.object);
            let requirement = NativeVulkanSceneOffscreenTargetRequirement {
                target,
                format,
                width: aux.clear_target_width,
                height: aux.clear_target_height,
            };
            targets.push(NativeVulkanSceneLayerAuxTargetRequirement {
                object: aux.object,
                target,
                format: layer_aux_color_format_label(format),
                width: aux.clear_target_width,
                height: aux.clear_target_height,
                color_format_selector: aux.clear_target_color_format,
                aux_format_selector: aux.clear_target_aux_format,
                r9_selector: aux.clear_target_r9_selector,
                resource_selector: aux.clear_target_resource_selector,
                cache_selector: aux.clear_target_cache_selector,
                reference_points: [
                    "0x14020a000 reads descriptor +0x2c/+0x30 for aux+0x3e8 width/height",
                    "0x14020a03b writes aux/depth format selector 0x1b before 0x1401aadb0",
                    "0x14020a043..0x14020a064 selects color format 0 or 0xe from render-state bit 0x2000",
                    "0x14020a07b creates target and 0x14020a083 stores [aux+0x3e8]",
                ],
            });
            requirements.push(requirement);
        }

        Ok(Self {
            target_count: targets.len(),
            targets,
            requirements,
            command_order: [
                "read_layer_aux_composite_target_residency",
                "require_0x14020a07b_dimensions_and_format_selectors",
                "map_we_aux_color_format_to_vk",
                "emit_layer_aux_clear_offscreen_requirements",
                "merge_with_rendering_device_graph_targets",
            ],
        })
    }

    pub(in crate::renderer::native_vulkan) fn requirements(
        &self,
    ) -> &[NativeVulkanSceneOffscreenTargetRequirement] {
        &self.requirements
    }
}

fn layer_aux_color_format(selector: u32) -> Result<vk::Format, String> {
    match selector {
        WE_LAYER_AUX_CLEAR_TARGET_DEFAULT_COLOR_FORMAT => Ok(vk::Format::R8G8B8A8_UNORM),
        WE_LAYER_AUX_CLEAR_TARGET_HDR_COLOR_FORMAT => Ok(vk::Format::R16G16B16A16_SFLOAT),
        other => Err(format!(
            "scene layer aux target color format selector {other:#x} is not closed by reverse-engineered docs"
        )),
    }
}

fn layer_aux_color_format_label(format: vk::Format) -> &'static str {
    match format {
        vk::Format::R8G8B8A8_UNORM => "R8G8B8A8_UNORM",
        vk::Format::R16G16B16A16_SFLOAT => "R16G16B16A16_SFLOAT",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneLayerAuxCompositeTargetsResidency, WE_LAYER_AUX_CLEAR_TARGET_AUX_FORMAT,
        WE_LAYER_AUX_CLEAR_TARGET_CACHE_SELECTOR, WE_LAYER_AUX_CLEAR_TARGET_R9_SELECTOR,
        WE_LAYER_AUX_CLEAR_TARGET_RESOURCE_SELECTOR,
    };

    #[test]
    fn layer_aux_target_plan_emits_retained_offscreen_requirement() {
        let object = SceneObjectId(7);
        let plan = NativeVulkanSceneLayerAuxTargetPlan::from_residency(&residency(
            object,
            WE_LAYER_AUX_CLEAR_TARGET_DEFAULT_COLOR_FORMAT,
        ))
        .expect("layer aux target plan");

        assert_eq!(plan.target_count, 1);
        assert_eq!(plan.targets[0].object, object);
        assert_eq!(
            plan.targets[0].target,
            SceneGraphTarget::LayerAuxClear(object)
        );
        assert_eq!(plan.targets[0].format, "R8G8B8A8_UNORM");
        assert_eq!(plan.targets[0].width, 3840);
        assert_eq!(plan.targets[0].height, 2160);
        assert_eq!(
            plan.requirements()[0].target,
            SceneGraphTarget::LayerAuxClear(object)
        );
        assert_eq!(plan.requirements()[0].format, vk::Format::R8G8B8A8_UNORM);
        assert_eq!(
            plan.command_order,
            [
                "read_layer_aux_composite_target_residency",
                "require_0x14020a07b_dimensions_and_format_selectors",
                "map_we_aux_color_format_to_vk",
                "emit_layer_aux_clear_offscreen_requirements",
                "merge_with_rendering_device_graph_targets"
            ]
        );
    }

    #[test]
    fn layer_aux_target_plan_maps_hdr_selector_without_guessing() {
        let plan = NativeVulkanSceneLayerAuxTargetPlan::from_residency(&residency(
            SceneObjectId(8),
            WE_LAYER_AUX_CLEAR_TARGET_HDR_COLOR_FORMAT,
        ))
        .expect("layer aux hdr target plan");

        assert_eq!(plan.targets[0].format, "R16G16B16A16_SFLOAT");
        assert_eq!(
            plan.requirements()[0].format,
            vk::Format::R16G16B16A16_SFLOAT
        );
    }

    #[test]
    fn layer_aux_target_plan_rejects_unknown_format_selector() {
        let err =
            NativeVulkanSceneLayerAuxTargetPlan::from_residency(&residency(SceneObjectId(9), 0x55))
                .expect_err("unknown selector must fail");

        assert!(err.contains("0x55"));
    }

    fn residency(object: SceneObjectId, color_format: u32) -> SceneResourceResidencyPlan {
        SceneResourceResidencyPlan {
            resources: vec![SceneResidentResource::LayerAuxCompositeTargets(
                SceneLayerAuxCompositeTargetsResidency {
                    object,
                    clear_target_3e8: true,
                    material_target_3f0: true,
                    effect_target_3f8: true,
                    generated_material_408: true,
                    clear_material_410: true,
                    clear_target_width: 3840,
                    clear_target_height: 2160,
                    clear_target_color_format: color_format,
                    clear_target_aux_format: WE_LAYER_AUX_CLEAR_TARGET_AUX_FORMAT,
                    clear_target_r9_selector: WE_LAYER_AUX_CLEAR_TARGET_R9_SELECTOR,
                    clear_target_resource_selector: WE_LAYER_AUX_CLEAR_TARGET_RESOURCE_SELECTOR,
                    clear_target_cache_selector: WE_LAYER_AUX_CLEAR_TARGET_CACHE_SELECTOR,
                    clear_prep_ready: true,
                },
            )],
        }
    }
}
