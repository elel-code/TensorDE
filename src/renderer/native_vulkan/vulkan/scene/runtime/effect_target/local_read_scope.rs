//! Command recording for a two-attachment dynamic-rendering local-read scope.
//!
//! The producer and consumer remain distinct authored draws.  Their targets
//! stay attached in `VK_IMAGE_LAYOUT_RENDERING_LOCAL_READ`; a by-region
//! producer-write to input-read dependency separates the two commands.

use super::*;
use crate::renderer::native_vulkan::vulkan::scene::runtime::local_read::{
    SceneLocalReadDeviceLimits, SceneLocalReadScopePassRole, SceneLocalReadScopePlan,
    record_scene_local_read_attachment_mapping, scene_local_read_by_region_dependency,
    scene_local_read_producer_to_consumer_barrier, scene_local_read_scope_entry_barrier,
    scene_local_read_scope_exit_barrier,
};

pub(super) fn local_read_scope_matches_command(
    scope: &SceneLocalReadScopePlan,
    command: &SceneEffectTargetCommand,
    producer: bool,
) -> bool {
    let (pass_record_index, draw_range, target) = if producer {
        (
            scope.producer_pass_record_index(),
            scope.producer_draw_range(),
            scope.source(),
        )
    } else {
        (
            scope.consumer_pass_record_index(),
            scope.consumer_draw_range(),
            scope.destination(),
        )
    };
    command.kind == SceneEffectTargetCommandKind::DynamicRender
        && command.pass_record_index == pass_record_index
        && (command.mesh_draw_start, command.mesh_draw_count) == draw_range
        && command.target.graph_index == target.graph_index()
        && command.target.target == target.target()
        && command.target.name == target.target_name()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record_scene_local_read_scope(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    source: &SceneEffectTargetImageResource,
    destination: &SceneEffectTargetImageResource,
    producer: SceneEffectTargetCommand,
    consumer: SceneEffectTargetCommand,
    source_load_op: vk::AttachmentLoadOp,
    destination_load_op: vk::AttachmentLoadOp,
    scope: &SceneLocalReadScopePlan,
    limits: SceneLocalReadDeviceLimits,
    producer_position: usize,
    consumer_position: usize,
    record_draws: &mut impl FnMut(u32, u32, vk::Extent2D) -> Result<(), String>,
    record_command_timing: &mut impl FnMut(usize, bool),
) -> Result<(), String> {
    if source.plan.physical_slot == destination.plan.physical_slot {
        return Err(format!(
            "scene local-read scope aliases source and destination physical slot {}",
            source.plan.physical_slot
        ));
    }
    let scope_extent = scope.extent();
    let scope_formats = scope.color_attachment_formats();
    if (source.plan.width, source.plan.height, source.plan.format)
        != (scope_extent.width, scope_extent.height, scope_formats[0])
        || (destination.plan.width, destination.plan.height, destination.plan.format)
            != (scope_extent.width, scope_extent.height, scope_formats[1])
    {
        return Err("scene local-read runtime images do not match the retained scope plan".to_owned());
    }
    if !source.plan.input_attachment_required {
        return Err(format!(
            "scene local-read source physical slot {} lacks input-attachment usage",
            source.plan.physical_slot
        ));
    }
    if !destination.plan.input_attachment_required {
        return Err(format!(
            "scene local-read destination physical slot {} lacks input-attachment usage",
            destination.plan.physical_slot
        ));
    }

    let subresource_range = effect_target_subresource_range(0, 1);
    let entry_barriers = [
        scene_local_read_scope_entry_barrier(source.image.image, subresource_range),
        scene_local_read_scope_entry_barrier(destination.image.image, subresource_range),
    ];
    let entry_dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&entry_barriers)
        .build();
    let clear_value = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: [0.0, 0.0, 0.0, 0.0],
        },
    };
    let attachments = [
        vk::RenderingAttachmentInfo::builder()
            .image_view(source.image.view)
            .image_layout(vk::ImageLayout::RENDERING_LOCAL_READ)
            .load_op(source_load_op)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(clear_value)
            .build(),
        vk::RenderingAttachmentInfo::builder()
            .image_view(destination.image.view)
            .image_layout(vk::ImageLayout::RENDERING_LOCAL_READ)
            .load_op(destination_load_op)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(clear_value)
            .build(),
    ];
    let rendering = vk::RenderingInfo::builder()
        .render_area(
            vk::Rect2D::builder()
                .offset(vk::Offset2D { x: 0, y: 0 })
                .extent(scope_extent)
                .build(),
        )
        .layer_count(1)
        .color_attachments(&attachments)
        .build();
    let producer_mapping =
        scope.attachment_mapping(SceneLocalReadScopePassRole::Producer, limits)?;
    let consumer_mapping =
        scope.attachment_mapping(SceneLocalReadScopePassRole::Consumer, limits)?;

    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &entry_dependency);
        device.cmd_begin_rendering(command_buffer, &rendering);
        record_scene_local_read_attachment_mapping(device, command_buffer, &producer_mapping);
    }
    super::super::draw_recording::record_scene_draw_extent(device, command_buffer, scope_extent);
    record_command_timing(producer_position, true);
    let producer_result = record_draws(
        producer.mesh_draw_start,
        producer.mesh_draw_count,
        scope_extent,
    );
    record_command_timing(producer_position, false);

    let mut draw_result = producer_result;
    if draw_result.is_ok() {
        let barrier = scene_local_read_producer_to_consumer_barrier(
            source.image.image,
            subresource_range,
        );
        let dependency = scene_local_read_by_region_dependency(&barrier);
        record_command_timing(consumer_position, true);
        unsafe {
            device.cmd_pipeline_barrier2(command_buffer, &dependency);
            record_scene_local_read_attachment_mapping(device, command_buffer, &consumer_mapping);
        }
        draw_result = record_draws(
            consumer.mesh_draw_start,
            consumer.mesh_draw_count,
            scope_extent,
        );
        record_command_timing(consumer_position, false);
    }
    unsafe {
        device.cmd_end_rendering(command_buffer);
    }

    let exit_barriers = [
        scene_local_read_scope_exit_barrier(source.image.image, subresource_range),
        scene_local_read_scope_exit_barrier(destination.image.image, subresource_range),
    ];
    let exit_dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&exit_barriers)
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &exit_dependency);
    }
    draw_result
}
