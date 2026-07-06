use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::resource_state::SceneEffectTargetWriteTransition;
use super::sync::scene_color_image_barrier;
use super::{VulkanaliaSceneMsaaColorTarget, scene_sample_count_label};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SceneDynamicViewport {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) scissor_extent: vk::Extent2D,
}

impl SceneDynamicViewport {
    pub(super) fn full_extent(extent: vk::Extent2D) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            scissor_extent: extent,
        }
    }
}

pub(super) fn set_scene_dynamic_viewport_and_scissor(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    viewport: SceneDynamicViewport,
) {
    let vk_viewport = vk::Viewport::builder()
        .x(viewport.x)
        .y(viewport.y)
        .width(viewport.width)
        .height(viewport.height)
        .min_depth(0.0)
        .max_depth(1.0)
        .build();
    let scissor = vk::Rect2D::builder()
        .offset(vk::Offset2D { x: 0, y: 0 })
        .extent(viewport.scissor_extent)
        .build();
    unsafe {
        device.cmd_set_viewport(command_buffer, 0, &[vk_viewport]);
        device.cmd_set_scissor(command_buffer, 0, &[scissor]);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SceneSampledImageActiveRenderingTarget {
    Swapchain,
    EffectTarget(u32),
}

pub(super) fn validate_scene_msaa_color_target(
    label: &'static str,
    target: Option<&VulkanaliaSceneMsaaColorTarget>,
    extent: vk::Extent2D,
    sample_count: vk::SampleCountFlags,
) -> Result<(), String> {
    if sample_count == vk::SampleCountFlags::_1 {
        return Ok(());
    }
    let Some(target) = target else {
        return Err(format!(
            "scene {label} render uses {} pipelines but has no MSAA color target",
            scene_sample_count_label(sample_count)
        ));
    };
    if target.sample_count != sample_count {
        return Err(format!(
            "scene {label} MSAA target sample count {} does not match pipeline sample count {}",
            scene_sample_count_label(target.sample_count),
            scene_sample_count_label(sample_count)
        ));
    }
    if target.extent.width != extent.width || target.extent.height != extent.height {
        return Err(format!(
            "scene {label} MSAA target extent {}x{} does not match render extent {}x{}",
            target.extent.width, target.extent.height, extent.width, extent.height
        ));
    }
    Ok(())
}

pub(super) fn begin_scene_color_rendering(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image_view: vk::ImageView,
    msaa_target: Option<&VulkanaliaSceneMsaaColorTarget>,
    extent: vk::Extent2D,
    load_op: vk::AttachmentLoadOp,
    clear_color: [f32; 4],
) {
    let clear_value = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: clear_color,
        },
    };
    let mut color_attachment = vk::RenderingAttachmentInfo::builder()
        .image_view(msaa_target.map_or(image_view, |target| target.image_view))
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(load_op)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear_value);
    if msaa_target.is_some() {
        color_attachment = color_attachment
            .resolve_mode(vk::ResolveModeFlags::AVERAGE)
            .resolve_image_view(image_view)
            .resolve_image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    }
    let color_attachment = color_attachment.build();
    let color_attachments = [color_attachment];
    let render_area = vk::Rect2D::builder()
        .offset(vk::Offset2D { x: 0, y: 0 })
        .extent(extent)
        .build();
    let rendering_info = vk::RenderingInfo::builder()
        .render_area(render_area)
        .layer_count(1)
        .color_attachments(&color_attachments)
        .build();
    unsafe {
        device.cmd_begin_rendering(command_buffer, &rendering_info);
    }
    set_scene_dynamic_viewport_and_scissor(
        device,
        command_buffer,
        SceneDynamicViewport::full_extent(extent),
    );
}

pub(super) fn end_scene_color_rendering(device: &Device, command_buffer: vk::CommandBuffer) {
    unsafe {
        device.cmd_end_rendering(command_buffer);
    }
}

pub(super) fn scene_effect_target_shader_read_barrier(image: vk::Image) -> vk::ImageMemoryBarrier2 {
    scene_color_image_barrier(
        image,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
    )
}

pub(super) fn scene_effect_target_write_barrier(
    image: vk::Image,
    transition: SceneEffectTargetWriteTransition,
) -> vk::ImageMemoryBarrier2 {
    match transition {
        SceneEffectTargetWriteTransition::Discard => scene_color_image_barrier(
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::TOP_OF_PIPE,
            vk::AccessFlags2::empty(),
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        ),
        SceneEffectTargetWriteTransition::ShaderReadToColorAttachment => scene_color_image_barrier(
            image,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        ),
        SceneEffectTargetWriteTransition::ColorAttachmentDependency => scene_color_image_barrier(
            image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        ),
    }
}
