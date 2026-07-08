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
    SceneEffectFboFormat, SceneEffectPassGraphOutput, SceneEffectPassGraphPlan, SceneGraphTarget,
    SceneLayerCompositorPlan, SceneObjectId,
};

use super::offscreen_targets::NativeVulkanSceneOffscreenTargetRequirement;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectTargetPlan {
    pub target_count: usize,
    pub entries: Vec<NativeVulkanSceneEffectTargetEntry>,
    pub scale_policy: &'static str,
    pub descriptor_model: &'static str,
    pub command_order: [&'static str; 6],
    #[serde(skip)]
    requirements: Vec<NativeVulkanSceneOffscreenTargetRequirement>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectTargetEntry {
    pub target: SceneGraphTarget,
    pub object: Option<SceneObjectId>,
    pub program_index: Option<usize>,
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
        Self::from_effect_pass_graph_with_layer_compositor(graph, None, extent, swapchain_format)
    }

    pub(in crate::renderer::native_vulkan) fn from_effect_pass_graph_with_layer_compositor(
        graph: &SceneEffectPassGraphPlan,
        layer_compositor: Option<&SceneLayerCompositorPlan>,
        extent: vk::Extent2D,
        swapchain_format: vk::Format,
    ) -> Result<Self, String> {
        if extent.width == 0 || extent.height == 0 {
            return Err("scene effect target plan requires non-zero extent".to_owned());
        }
        if swapchain_format == vk::Format::UNDEFINED {
            return Err("scene effect target plan requires defined swapchain format".to_owned());
        }

        let mut entries = Vec::with_capacity(graph.targets.len() + graph.passes.len());
        let mut requirements = Vec::with_capacity(graph.targets.len() + graph.passes.len());
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
                object: Some(target.object),
                program_index: Some(target.program_index),
                name: target.name.clone(),
                format: vulkan_format_label(format),
                format_source,
                width,
                height,
                scale: target.scale,
                unique: target.unique,
            });
        }
        for target in &graph.image_layer_targets {
            push_image_layer_target_requirement(
                &mut entries,
                &mut requirements,
                target.object,
                target.prefill_target,
                "image_layer_prefill_target",
                extent,
                swapchain_format,
            );
            push_image_layer_target_requirement(
                &mut entries,
                &mut requirements,
                target.object,
                target.final_source_target,
                "image_layer_final_source_target",
                extent,
                swapchain_format,
            );
            for pass_target in &target.pass_targets {
                push_image_layer_target_requirement(
                    &mut entries,
                    &mut requirements,
                    target.object,
                    pass_target.source,
                    "image_layer_pass_source_target",
                    extent,
                    swapchain_format,
                );
                push_image_layer_target_requirement(
                    &mut entries,
                    &mut requirements,
                    target.object,
                    pass_target.output,
                    "image_layer_pass_output_target",
                    extent,
                    swapchain_format,
                );
            }
        }
        for pass in &graph.passes {
            if let SceneEffectPassGraphOutput::ObjectFinal(object) = pass.output {
                push_object_final_target_requirement(
                    &mut entries,
                    &mut requirements,
                    object,
                    pass.program_index,
                    extent,
                    swapchain_format,
                );
            }
        }
        if layer_compositor.is_some_and(|plan| plan.tokenized_layer_count > 0) {
            push_layer_alpha_mask_target_requirement(
                &mut entries,
                &mut requirements,
                SceneGraphTarget::FullAlphaMask,
                "_rt_FullAlphaMask",
                extent,
            );
            push_layer_alpha_mask_target_requirement(
                &mut entries,
                &mut requirements,
                SceneGraphTarget::FullAlphaMaskIntermediate,
                "_rt_FullAlphaMaskIntermediate",
                extent,
            );
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
                "merge_we_image_layer_source_composite_targets",
                "merge_layer_compositor_alpha_mask_targets",
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

fn push_image_layer_target_requirement(
    entries: &mut Vec<NativeVulkanSceneEffectTargetEntry>,
    requirements: &mut Vec<NativeVulkanSceneOffscreenTargetRequirement>,
    object: SceneObjectId,
    target: SceneGraphTarget,
    format_source: &'static str,
    extent: vk::Extent2D,
    swapchain_format: vk::Format,
) {
    if requirements
        .iter()
        .any(|requirement| requirement.target == target)
    {
        return;
    }
    let name = match target {
        SceneGraphTarget::ImageLayerCompositeA(object) => {
            format!("_rt_imageLayerComposite_{}_a", object.0)
        }
        SceneGraphTarget::ImageLayerSource(object) => {
            format!("_rt_imageLayerSource_{}", object.0)
        }
        _ => panic!("image-layer target requirement received non-image-layer target {target:?}"),
    };
    requirements.push(NativeVulkanSceneOffscreenTargetRequirement {
        target,
        format: swapchain_format,
        width: extent.width,
        height: extent.height,
    });
    entries.push(NativeVulkanSceneEffectTargetEntry {
        target,
        object: Some(object),
        program_index: None,
        name,
        format: vulkan_format_label(swapchain_format),
        format_source,
        width: extent.width,
        height: extent.height,
        scale: 1.0,
        unique: true,
    });
}

fn push_object_final_target_requirement(
    entries: &mut Vec<NativeVulkanSceneEffectTargetEntry>,
    requirements: &mut Vec<NativeVulkanSceneOffscreenTargetRequirement>,
    object: SceneObjectId,
    program_index: usize,
    extent: vk::Extent2D,
    swapchain_format: vk::Format,
) {
    let target = SceneGraphTarget::ObjectFinal(object);
    if requirements
        .iter()
        .any(|requirement| requirement.target == target)
    {
        return;
    }
    requirements.push(NativeVulkanSceneOffscreenTargetRequirement {
        target,
        format: swapchain_format,
        width: extent.width,
        height: extent.height,
    });
    entries.push(NativeVulkanSceneEffectTargetEntry {
        target,
        object: Some(object),
        program_index: Some(program_index),
        name: format!("object-final-{}", object.0),
        format: vulkan_format_label(swapchain_format),
        format_source: "object_final_surface_format",
        width: extent.width,
        height: extent.height,
        scale: 1.0,
        unique: true,
    });
}

fn push_layer_alpha_mask_target_requirement(
    entries: &mut Vec<NativeVulkanSceneEffectTargetEntry>,
    requirements: &mut Vec<NativeVulkanSceneOffscreenTargetRequirement>,
    target: SceneGraphTarget,
    name: &str,
    extent: vk::Extent2D,
) {
    if requirements
        .iter()
        .any(|requirement| requirement.target == target)
    {
        return;
    }
    let width = extent.width.saturating_add(1) / 2;
    let height = extent.height.saturating_add(1) / 2;
    requirements.push(NativeVulkanSceneOffscreenTargetRequirement {
        target,
        format: vk::Format::R8_UNORM,
        width,
        height,
    });
    entries.push(NativeVulkanSceneEffectTargetEntry {
        target,
        object: None,
        program_index: None,
        name: name.to_owned(),
        format: "R8_UNORM",
        format_source: "we_layer_alpha_mask_runtime_r8_unorm",
        width,
        height,
        scale: 2.0,
        unique: false,
    });
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
        SceneEffectFboFormat::RgbBackbuffer => Ok((swapchain_format, "we_fbo_rgb_backbuffer")),
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
        SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
        SceneEffectPassGraphMaterialPass, SceneEffectPassGraphOutput, SceneEffectPassGraphPlan,
        SceneEffectPassGraphTarget, SceneLayerCompositorPlan, SceneObjectId, we::WeEffectKind,
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
    fn effect_target_plan_keeps_rgb_backbuffer_format_source() {
        let graph = graph(vec![target(
            "_rt_RgbBackbuffer",
            SceneGraphTarget::NamedFbo(9),
            SceneEffectFboFormat::RgbBackbuffer,
            1.0,
        )]);

        let plan = NativeVulkanSceneEffectTargetPlan::from_effect_pass_graph(
            &graph,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("rgb backbuffer target plan");

        assert_eq!(plan.entries[0].format, "B8G8R8A8_UNORM");
        assert_eq!(plan.entries[0].format_source, "we_fbo_rgb_backbuffer");
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
    fn effect_target_plan_allocates_object_final_targets() {
        let graph = SceneEffectPassGraphPlan {
            material_pass_count: 1,
            passes: vec![object_final_pass(SceneObjectId(42))],
            ..SceneEffectPassGraphPlan::empty()
        };

        let plan = NativeVulkanSceneEffectTargetPlan::from_effect_pass_graph(
            &graph,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("object final effect target plan");

        assert_eq!(plan.target_count, 1);
        assert_eq!(
            plan.entries[0].target,
            SceneGraphTarget::ObjectFinal(SceneObjectId(42))
        );
        assert_eq!(plan.entries[0].format_source, "object_final_surface_format");
        assert_eq!(plan.entries[0].width, 3840);
        assert_eq!(plan.entries[0].height, 2160);
        assert_eq!(plan.requirements()[0].format, vk::Format::B8G8R8A8_UNORM);
    }

    #[test]
    fn effect_target_plan_allocates_image_layer_source_and_composite_targets() {
        let object = SceneObjectId(1530);
        let image_layer_target =
            crate::engine::scene_engine::SceneImageLayerTargetPlan::for_object(object, None, 1)
                .expect("image layer target");
        let graph = SceneEffectPassGraphPlan {
            image_layer_target_count: 1,
            image_layer_scene_output_pass_count: 1,
            image_layer_targets: vec![image_layer_target],
            ..SceneEffectPassGraphPlan::empty()
        };

        let plan = NativeVulkanSceneEffectTargetPlan::from_effect_pass_graph(
            &graph,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("image layer targets");

        assert_eq!(plan.target_count, 2);
        assert_eq!(
            plan.entries[0].target,
            SceneGraphTarget::ImageLayerSource(object)
        );
        assert_eq!(
            plan.entries[1].target,
            SceneGraphTarget::ImageLayerCompositeA(object)
        );
        assert_eq!(plan.entries[0].format_source, "image_layer_prefill_target");
        assert_eq!(
            plan.entries[1].format_source,
            "image_layer_final_source_target"
        );
        assert_eq!(plan.entries[0].width, 3840);
        assert_eq!(plan.entries[1].height, 2160);
        assert_eq!(plan.requirements()[0].format, vk::Format::B8G8R8A8_UNORM);
        assert_eq!(plan.requirements()[1].format, vk::Format::B8G8R8A8_UNORM);
    }

    #[test]
    fn effect_target_plan_allocates_layer_alpha_mask_targets() {
        let graph = SceneEffectPassGraphPlan::empty();
        let mut layer_compositor = SceneLayerCompositorPlan::empty();
        layer_compositor.tokenized_layer_count = 1;

        let plan = NativeVulkanSceneEffectTargetPlan::from_effect_pass_graph_with_layer_compositor(
            &graph,
            Some(&layer_compositor),
            vk::Extent2D {
                width: 3839,
                height: 2159,
            },
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("layer alpha mask target plan");

        assert_eq!(plan.target_count, 2);
        assert_eq!(plan.entries[0].target, SceneGraphTarget::FullAlphaMask);
        assert_eq!(plan.entries[0].object, None);
        assert_eq!(plan.entries[0].program_index, None);
        assert_eq!(plan.entries[0].format, "R8_UNORM");
        assert_eq!(
            plan.entries[0].format_source,
            "we_layer_alpha_mask_runtime_r8_unorm"
        );
        assert_eq!(plan.entries[0].width, 1920);
        assert_eq!(plan.entries[0].height, 1080);
        assert_eq!(
            plan.entries[1].target,
            SceneGraphTarget::FullAlphaMaskIntermediate
        );
        assert_eq!(plan.requirements()[0].format, vk::Format::R8_UNORM);
        assert_eq!(plan.requirements()[1].format, vk::Format::R8_UNORM);
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

    fn object_final_pass(object: SceneObjectId) -> SceneEffectPassGraphMaterialPass {
        SceneEffectPassGraphMaterialPass {
            graph_command_index: 0,
            graph_pass_index: 0,
            object,
            program_index: 0,
            pass_index: 0,
            effect_file: "effects/iris/effect.json".to_owned(),
            effect: WeEffectKind::Iris,
            shader: Some("effects/iris".to_owned()),
            source: None,
            input_bindings: Vec::new(),
            output: SceneEffectPassGraphOutput::ObjectFinal(object),
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
            texture_resources: Vec::new(),
            combos: Default::default(),
            constants: Default::default(),
        }
    }
}
