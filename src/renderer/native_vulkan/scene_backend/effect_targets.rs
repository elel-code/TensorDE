//! Vulkan effect target metadata planning from the engine effect pass graph.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/texture-and-format.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneEffectFboFormat, SceneEffectPassGraphPlan, SceneGraphTarget, SceneObjectId,
};

use super::offscreen_targets::NativeVulkanSceneOffscreenTargetRequirement;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectTargetPlan {
    pub target_count: usize,
    pub entries: Vec<NativeVulkanSceneEffectTargetEntry>,
    pub scale_policy: &'static str,
    pub descriptor_model: &'static str,
    pub command_order: [&'static str; 4],
    #[serde(skip)]
    requirements: Vec<NativeVulkanSceneOffscreenTargetRequirement>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectTargetEntry {
    pub target: SceneGraphTarget,
    pub object: SceneObjectId,
    pub program_index: usize,
    pub name: String,
    pub format: &'static str,
    pub format_source: &'static str,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub unique: bool,
}

impl NativeVulkanSceneEffectTargetPlan {
    pub(in crate::renderer::native_vulkan) fn from_effect_pass_graph(
        graph: &SceneEffectPassGraphPlan,
        extent: vk::Extent2D,
        swapchain_format: vk::Format,
    ) -> Result<Self, String> {
        if extent.width == 0 || extent.height == 0 {
            return Err("scene effect target plan requires non-zero extent".to_owned());
        }
        if swapchain_format == vk::Format::UNDEFINED {
            return Err("scene effect target plan requires defined swapchain format".to_owned());
        }

        let mut entries = Vec::with_capacity(graph.targets.len());
        let mut requirements = Vec::with_capacity(graph.targets.len());
        for target in &graph.targets {
            let format = target.format.as_ref().ok_or_else(|| {
                format!(
                    "scene effect target {:?} ({}) requires explicit FBO format metadata",
                    target.target, target.name
                )
            })?;
            let (format, format_source) = effect_fbo_vk_format(format, swapchain_format)?;
            let (width, height) = effect_fbo_extent(extent, target.scale, &target.name)?;
            requirements.push(NativeVulkanSceneOffscreenTargetRequirement {
                target: target.target,
                format,
                width,
                height,
            });
            entries.push(NativeVulkanSceneEffectTargetEntry {
                target: target.target,
                object: target.object,
                program_index: target.program_index,
                name: target.name.clone(),
                format: vulkan_format_label(format),
                format_source,
                width,
                height,
                scale: target.scale,
                unique: target.unique,
            });
        }

        Ok(Self {
            target_count: entries.len(),
            entries,
            requirements,
            scale_policy: "WE FBO scale is a framebuffer-size divisor: target_extent=ceil(swapchain_extent/scale)",
            descriptor_model: "VK_EXT_descriptor_heap",
            command_order: [
                "read_scene_effect_pass_graph_targets",
                "map_we_fbo_format_to_vk_format",
                "derive_scaled_effect_target_extent",
                "emit_effect_target_requirements",
            ],
        })
    }

    pub(in crate::renderer::native_vulkan) fn requirements(
        &self,
    ) -> &[NativeVulkanSceneOffscreenTargetRequirement] {
        &self.requirements
    }

    pub(in crate::renderer::native_vulkan) fn format(
        &self,
        target: SceneGraphTarget,
    ) -> Result<vk::Format, String> {
        self.requirements
            .iter()
            .find(|requirement| requirement.target == target)
            .map(|requirement| requirement.format)
            .ok_or_else(|| format!("scene effect target plan has no format for {target:?}"))
    }
}

fn effect_fbo_vk_format(
    format: &SceneEffectFboFormat,
    swapchain_format: vk::Format,
) -> Result<(vk::Format, &'static str), String> {
    match format {
        SceneEffectFboFormat::Rgba16Float => {
            Ok((vk::Format::R16G16B16A16_SFLOAT, "we_fbo_rgba16f"))
        }
        SceneEffectFboFormat::Rg16Float => Ok((vk::Format::R16G16_SFLOAT, "we_fbo_rg1616f")),
        SceneEffectFboFormat::R16Float => Ok((vk::Format::R16_SFLOAT, "we_fbo_r16f")),
        SceneEffectFboFormat::R8Unorm => Ok((vk::Format::R8_UNORM, "we_runtime_target_r8_unorm")),
        SceneEffectFboFormat::Rgba8Unorm => Ok((vk::Format::R8G8B8A8_UNORM, "we_fbo_rgba8888")),
        SceneEffectFboFormat::RgbaBackbuffer => Ok((swapchain_format, "we_fbo_rgba_backbuffer")),
        SceneEffectFboFormat::Other(format) => Err(format!(
            "scene effect target format '{format}' has no Vulkan mapping"
        )),
    }
}

fn effect_fbo_extent(extent: vk::Extent2D, scale: f32, name: &str) -> Result<(u32, u32), String> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(format!(
            "scene effect target {name:?} scale must be finite and positive"
        ));
    }
    let width = ((extent.width as f32) / scale).ceil().max(1.0) as u32;
    let height = ((extent.height as f32) / scale).ceil().max(1.0) as u32;
    Ok((width, height))
}

fn vulkan_format_label(format: vk::Format) -> &'static str {
    match format {
        vk::Format::R16G16B16A16_SFLOAT => "R16G16B16A16_SFLOAT",
        vk::Format::R16G16_SFLOAT => "R16G16_SFLOAT",
        vk::Format::R16_SFLOAT => "R16_SFLOAT",
        vk::Format::R8_UNORM => "R8_UNORM",
        vk::Format::R8G8B8A8_UNORM => "R8G8B8A8_UNORM",
        vk::Format::B8G8R8A8_UNORM => "B8G8R8A8_UNORM",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneEffectPassGraphPlan, SceneEffectPassGraphTarget, SceneObjectId,
    };

    #[test]
    fn effect_target_plan_maps_backbuffer_scaled_fbo() {
        let graph = graph(vec![target(
            "_rt_QuarterCompoBuffer1",
            SceneGraphTarget::NamedFbo(4),
            SceneEffectFboFormat::RgbaBackbuffer,
            4.0,
        )]);

        let plan = NativeVulkanSceneEffectTargetPlan::from_effect_pass_graph(
            &graph,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("effect target plan");

        assert_eq!(plan.target_count, 1);
        assert_eq!(plan.entries[0].format, "B8G8R8A8_UNORM");
        assert_eq!(plan.entries[0].format_source, "we_fbo_rgba_backbuffer");
        assert_eq!(plan.entries[0].width, 960);
        assert_eq!(plan.entries[0].height, 540);
        assert_eq!(plan.requirements()[0].format, vk::Format::B8G8R8A8_UNORM);
    }

    #[test]
    fn effect_target_plan_maps_fluid_vector_and_scalar_formats() {
        let graph = graph(vec![
            target(
                "_rt_SmokeVelocity1",
                SceneGraphTarget::NamedFbo(1),
                SceneEffectFboFormat::Rg16Float,
                2.0,
            ),
            target(
                "_rt_SmokePressure1",
                SceneGraphTarget::NamedFbo(2),
                SceneEffectFboFormat::R16Float,
                2.0,
            ),
        ]);

        let plan = NativeVulkanSceneEffectTargetPlan::from_effect_pass_graph(
            &graph,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("effect target plan");

        assert_eq!(plan.entries[0].format, "R16G16_SFLOAT");
        assert_eq!(plan.entries[1].format, "R16_SFLOAT");
        assert_eq!(plan.entries[0].width, 1920);
        assert_eq!(plan.entries[1].height, 1080);
    }

    #[test]
    fn effect_target_plan_maps_runtime_r8_alpha_mask_format() {
        let graph = graph(vec![target(
            "_rt_FullAlphaMask",
            SceneGraphTarget::NamedFbo(8),
            SceneEffectFboFormat::R8Unorm,
            2.0,
        )]);

        let plan = NativeVulkanSceneEffectTargetPlan::from_effect_pass_graph(
            &graph,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("runtime R8 alpha-mask target plan");

        assert_eq!(plan.entries[0].format, "R8_UNORM");
        assert_eq!(plan.entries[0].format_source, "we_runtime_target_r8_unorm");
        assert_eq!(plan.entries[0].width, 1920);
        assert_eq!(plan.entries[0].height, 1080);
        assert_eq!(plan.requirements()[0].format, vk::Format::R8_UNORM);
    }

    #[test]
    fn effect_target_plan_rejects_unknown_format() {
        let graph = graph(vec![target(
            "_rt_Vendor",
            SceneGraphTarget::NamedFbo(9),
            SceneEffectFboFormat::Other("vendor".to_owned()),
            1.0,
        )]);

        let err = NativeVulkanSceneEffectTargetPlan::from_effect_pass_graph(
            &graph,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect_err("unknown format must fail");

        assert!(err.contains("no Vulkan mapping"));
    }

    fn graph(targets: Vec<SceneEffectPassGraphTarget>) -> SceneEffectPassGraphPlan {
        SceneEffectPassGraphPlan {
            target_count: targets.len(),
            targets,
            ..SceneEffectPassGraphPlan::empty()
        }
    }

    fn target(
        name: &str,
        target: SceneGraphTarget,
        format: SceneEffectFboFormat,
        scale: f32,
    ) -> SceneEffectPassGraphTarget {
        SceneEffectPassGraphTarget {
            target,
            object: SceneObjectId(1),
            program_index: 0,
            name: name.to_owned(),
            format: Some(format),
            scale,
            unique: false,
        }
    }
}
