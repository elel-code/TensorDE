//! Target graph contract for WE alpha-mask flattexture copy-back draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;
use vulkanalia::vk;
use vulkanalia::vk::Handle;

use crate::engine::scene_engine::SceneGraphTarget;
use crate::renderer::native_vulkan::scene_backend::frame_resources::NativeVulkanSceneFrameResources;
use crate::renderer::native_vulkan::scene_backend::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
use crate::renderer::native_vulkan::scene_backend::render_target::{
    NativeVulkanSceneOffscreenRenderTarget, NativeVulkanSceneRenderTarget,
    NativeVulkanSceneRenderTargetScopePlan, native_vulkan_scene_render_target_scope_plan,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackTargetGraphPlan
{
    pub source: SceneGraphTarget,
    pub target: SceneGraphTarget,
    pub width: u32,
    pub height: u32,
    pub format: &'static str,
    pub source_current_layout: &'static str,
    pub source_required_layout: &'static str,
    pub source_shader_read_transition_count: usize,
    pub target_current_layout: &'static str,
    pub target_required_layout: &'static str,
    pub target_color_write_transition_count: usize,
    pub target_scope_count: usize,
    pub target_scope: NativeVulkanSceneRenderTargetScopePlan,
    pub command_order: [&'static str; 6],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_copy_back_target_graph(
    frame_resources: &NativeVulkanSceneFrameResources,
) -> Result<NativeVulkanSceneLayerAlphaMaskCopyBackTargetGraphPlan, String> {
    let source =
        frame_resources.offscreen_target_binding(SceneGraphTarget::FullAlphaMaskIntermediate)?;
    let target = frame_resources.offscreen_target_binding(SceneGraphTarget::FullAlphaMask)?;
    native_vulkan_plan_scene_layer_alpha_mask_copy_back_target_graph_from_bindings(source, target)
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_copy_back_target_graph_from_bindings(
    source: NativeVulkanSceneOffscreenTargetBinding,
    target: NativeVulkanSceneOffscreenTargetBinding,
) -> Result<NativeVulkanSceneLayerAlphaMaskCopyBackTargetGraphPlan, String> {
    validate_copy_back_source_target(source, target)?;
    let source_current_layout = copy_back_layout_label(source.current_layout)?;
    let target_current_layout = copy_back_layout_label(target.current_layout)?;
    let target_scope = native_vulkan_scene_render_target_scope_plan(
        NativeVulkanSceneRenderTarget::Offscreen(NativeVulkanSceneOffscreenRenderTarget {
            target: target.target,
            image: target.image,
            image_view: target.view,
            extent: vk::Extent2D {
                width: target.width,
                height: target.height,
            },
            initial_layout: target.current_layout,
            final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }),
        None,
    )
    .map_err(|err| format!("{err}; scene layer alpha-mask copy-back target scope is invalid"))?;

    Ok(NativeVulkanSceneLayerAlphaMaskCopyBackTargetGraphPlan {
        source: source.target,
        target: target.target,
        width: target.width,
        height: target.height,
        format: "r8-unorm",
        source_current_layout,
        source_required_layout: "shader-read-only-optimal",
        source_shader_read_transition_count: usize::from(
            source.current_layout != vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        ),
        target_current_layout,
        target_required_layout: "color-attachment-optimal",
        target_color_write_transition_count: usize::from(
            target.current_layout != vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        ),
        target_scope_count: 1,
        target_scope,
        command_order: [
            "resolve_full_alpha_mask_intermediate_source_binding",
            "require_intermediate_source_sampleable_r8_unorm",
            "plan_source_transition_to_shader_read",
            "resolve_full_alpha_mask_target_binding",
            "plan_target_transition_to_color_attachment",
            "open_full_alpha_mask_load_scope_for_copy_back_draw",
        ],
    })
}

fn validate_copy_back_source_target(
    source: NativeVulkanSceneOffscreenTargetBinding,
    target: NativeVulkanSceneOffscreenTargetBinding,
) -> Result<(), String> {
    if source.target != SceneGraphTarget::FullAlphaMaskIntermediate {
        return Err(format!(
            "scene layer alpha-mask copy-back source must be FullAlphaMaskIntermediate, got {:?}",
            source.target
        ));
    }
    if target.target != SceneGraphTarget::FullAlphaMask {
        return Err(format!(
            "scene layer alpha-mask copy-back target must be FullAlphaMask, got {:?}",
            target.target
        ));
    }
    if source.image == vk::Image::null() || source.view == vk::ImageView::null() {
        return Err(
            "scene layer alpha-mask copy-back source requires resident image and view".to_owned(),
        );
    }
    if source.sampler == vk::Sampler::null() {
        return Err("scene layer alpha-mask copy-back source requires resident sampler".to_owned());
    }
    if target.image == vk::Image::null() || target.view == vk::ImageView::null() {
        return Err(
            "scene layer alpha-mask copy-back target requires resident image and view".to_owned(),
        );
    }
    if source.format != vk::Format::R8_UNORM || target.format != vk::Format::R8_UNORM {
        return Err(format!(
            "scene layer alpha-mask copy-back requires R8_UNORM source/target, got {:?}/{:?}",
            source.format, target.format
        ));
    }
    if source.width == 0 || source.height == 0 || target.width == 0 || target.height == 0 {
        return Err("scene layer alpha-mask copy-back requires non-zero target extents".to_owned());
    }
    if source.width != target.width || source.height != target.height {
        return Err(format!(
            "scene layer alpha-mask copy-back requires matching source/target extents, got {}x{} and {}x{}",
            source.width, source.height, target.width, target.height
        ));
    }
    if source.current_layout == vk::ImageLayout::UNDEFINED {
        return Err(
            "scene layer alpha-mask copy-back source is undefined; intermediate mask producer must run first"
                .to_owned(),
        );
    }
    if target.current_layout == vk::ImageLayout::UNDEFINED {
        return Err(
            "scene layer alpha-mask copy-back target is undefined; full mask destination must be initialized before load"
                .to_owned(),
        );
    }
    copy_back_layout_label(source.current_layout)?;
    copy_back_layout_label(target.current_layout)?;
    Ok(())
}

fn copy_back_layout_label(layout: vk::ImageLayout) -> Result<&'static str, String> {
    match layout {
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => Ok("color-attachment-optimal"),
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => Ok("shader-read-only-optimal"),
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL => Ok("transfer-src-optimal"),
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => Ok("transfer-dst-optimal"),
        _ => Err(format!(
            "scene layer alpha-mask copy-back layout {layout:?} has no graph access mapping"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;
    use vulkanalia::vk::Handle;

    #[test]
    fn copy_back_target_graph_plans_source_read_and_target_load_scope() {
        let plan = native_vulkan_plan_scene_layer_alpha_mask_copy_back_target_graph_from_bindings(
            binding(
                SceneGraphTarget::FullAlphaMaskIntermediate,
                1,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ),
            binding(
                SceneGraphTarget::FullAlphaMask,
                11,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ),
        )
        .expect("copy-back target graph");

        assert_eq!(plan.source, SceneGraphTarget::FullAlphaMaskIntermediate);
        assert_eq!(plan.target, SceneGraphTarget::FullAlphaMask);
        assert_eq!(plan.width, 1920);
        assert_eq!(plan.height, 1080);
        assert_eq!(plan.format, "r8-unorm");
        assert_eq!(plan.source_current_layout, "color-attachment-optimal");
        assert_eq!(plan.source_required_layout, "shader-read-only-optimal");
        assert_eq!(plan.source_shader_read_transition_count, 1);
        assert_eq!(plan.target_current_layout, "shader-read-only-optimal");
        assert_eq!(plan.target_required_layout, "color-attachment-optimal");
        assert_eq!(plan.target_color_write_transition_count, 1);
        assert_eq!(plan.target_scope_count, 1);
        assert_eq!(
            plan.target_scope.load_op,
            NativeVulkanSceneRenderTargetLoadOp::Load
        );
        assert_eq!(
            plan.target_scope.begin_command_order,
            [
                "cmd_pipeline_barrier2_color_attachment",
                "cmd_begin_rendering"
            ]
        );
        assert_eq!(
            plan.command_order,
            [
                "resolve_full_alpha_mask_intermediate_source_binding",
                "require_intermediate_source_sampleable_r8_unorm",
                "plan_source_transition_to_shader_read",
                "resolve_full_alpha_mask_target_binding",
                "plan_target_transition_to_color_attachment",
                "open_full_alpha_mask_load_scope_for_copy_back_draw"
            ]
        );
    }

    #[test]
    fn copy_back_target_graph_rejects_undefined_source_or_target() {
        let source_err =
            native_vulkan_plan_scene_layer_alpha_mask_copy_back_target_graph_from_bindings(
                binding(
                    SceneGraphTarget::FullAlphaMaskIntermediate,
                    1,
                    vk::ImageLayout::UNDEFINED,
                ),
                binding(
                    SceneGraphTarget::FullAlphaMask,
                    11,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
            )
            .expect_err("undefined source must fail");
        assert!(source_err.contains("source is undefined"));

        let target_err =
            native_vulkan_plan_scene_layer_alpha_mask_copy_back_target_graph_from_bindings(
                binding(
                    SceneGraphTarget::FullAlphaMaskIntermediate,
                    1,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
                binding(
                    SceneGraphTarget::FullAlphaMask,
                    11,
                    vk::ImageLayout::UNDEFINED,
                ),
            )
            .expect_err("undefined target must fail");
        assert!(target_err.contains("target is undefined"));
    }

    #[test]
    fn copy_back_target_graph_rejects_non_r8_or_mismatched_extent() {
        let mut wrong_format = binding(
            SceneGraphTarget::FullAlphaMaskIntermediate,
            1,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        wrong_format.format = vk::Format::R8G8B8A8_UNORM;
        let err = native_vulkan_plan_scene_layer_alpha_mask_copy_back_target_graph_from_bindings(
            wrong_format,
            binding(
                SceneGraphTarget::FullAlphaMask,
                11,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ),
        )
        .expect_err("format mismatch must fail");
        assert!(err.contains("requires R8_UNORM"));

        let mut mismatched = binding(
            SceneGraphTarget::FullAlphaMask,
            11,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );
        mismatched.width = 960;
        let err = native_vulkan_plan_scene_layer_alpha_mask_copy_back_target_graph_from_bindings(
            binding(
                SceneGraphTarget::FullAlphaMaskIntermediate,
                1,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ),
            mismatched,
        )
        .expect_err("extent mismatch must fail");
        assert!(err.contains("matching source/target extents"));
    }

    fn binding(
        target: SceneGraphTarget,
        raw: u64,
        current_layout: vk::ImageLayout,
    ) -> NativeVulkanSceneOffscreenTargetBinding {
        NativeVulkanSceneOffscreenTargetBinding {
            target,
            image: vk::Image::from_raw(raw),
            view: vk::ImageView::from_raw(raw + 1),
            sampler: vk::Sampler::from_raw(raw + 2),
            format: vk::Format::R8_UNORM,
            width: 1920,
            height: 1080,
            current_layout,
        }
    }
}
