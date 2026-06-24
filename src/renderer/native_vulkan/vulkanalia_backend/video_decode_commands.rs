#![allow(dead_code)]

use serde::Serialize;
use vulkanalia::prelude::v1_4::{Device, DeviceV1_0, DeviceV1_3};
use vulkanalia::vk::{
    self, HasBuilder, KhrVideoDecodeQueueExtensionDeviceCommands,
    KhrVideoQueueExtensionDeviceCommands,
};

use super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE;
use super::video_decode_submit_av1::{
    NativeVulkanVulkanaliaAv1DecodeSubmitPlan, native_vulkan_vulkanalia_av1_with_vk_submit_info,
};
use super::video_decode_submit_h264::{
    NativeVulkanVulkanaliaH264DecodeSubmitPlan, native_vulkan_vulkanalia_h264_with_vk_submit_info,
};
use super::video_decode_submit_h265::{
    NativeVulkanVulkanaliaH265DecodeSubmitPlan, native_vulkan_vulkanalia_h265_with_vk_submit_info,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaDecodePrepareBarrierPlan {
    pub ffmpeg_reference: &'static str,
    pub uses_synchronization2: bool,
    pub buffer_barrier_count: u32,
    pub image_barrier_count: u32,
    pub old_layout: &'static str,
    pub new_layout: &'static str,
    pub buffer_src_stage: &'static str,
    pub buffer_dst_stage: &'static str,
    pub image_dst_stage: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaDecodeCommandBodyPlan {
    pub ffmpeg_reference: &'static str,
    pub command_order: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaDecodeCommandBufferPlan {
    pub ffmpeg_reference: &'static str,
    pub uses_synchronization2: bool,
    pub one_time_submit: bool,
    pub reset_before_record: bool,
    pub command_order: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaDecodeSubmit2Plan {
    pub ffmpeg_reference: &'static str,
    pub uses_submit2: bool,
    pub wait_idle_after_submit: bool,
    pub command_order: &'static [&'static str],
}

pub(super) fn native_vulkan_vulkanalia_decode_prepare_barrier_plan(
    image_barrier_count: u32,
) -> NativeVulkanVulkanaliaDecodePrepareBarrierPlan {
    NativeVulkanVulkanaliaDecodePrepareBarrierPlan {
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
        uses_synchronization2: true,
        buffer_barrier_count: 1,
        image_barrier_count,
        old_layout: "undefined",
        new_layout: "video-decode-dpb",
        buffer_src_stage: "host",
        buffer_dst_stage: "video-decode",
        image_dst_stage: "video-decode",
    }
}

pub(super) fn native_vulkan_vulkanalia_decode_command_body_plan(
    reset_control_recorded: bool,
) -> NativeVulkanVulkanaliaDecodeCommandBodyPlan {
    NativeVulkanVulkanaliaDecodeCommandBodyPlan {
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
        command_order: native_vulkan_vulkanalia_decode_command_body_order(reset_control_recorded),
    }
}

pub(super) fn native_vulkan_vulkanalia_decode_command_buffer_plan(
    reset_before_record: bool,
    reset_control_recorded: bool,
) -> NativeVulkanVulkanaliaDecodeCommandBufferPlan {
    NativeVulkanVulkanaliaDecodeCommandBufferPlan {
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
        uses_synchronization2: true,
        one_time_submit: true,
        reset_before_record,
        command_order: native_vulkan_vulkanalia_recorded_decode_command_order(
            reset_before_record,
            reset_control_recorded,
        ),
    }
}

pub(super) fn native_vulkan_vulkanalia_decode_submit2_plan(
    wait_idle_after_submit: bool,
) -> NativeVulkanVulkanaliaDecodeSubmit2Plan {
    NativeVulkanVulkanaliaDecodeSubmit2Plan {
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
        uses_submit2: true,
        wait_idle_after_submit,
        command_order: native_vulkan_vulkanalia_decode_submit2_order(wait_idle_after_submit),
    }
}

pub(super) unsafe fn native_vulkan_vulkanalia_record_decode_prepare_barriers(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    bitstream_buffer: vk::Buffer,
    bitstream_buffer_offset: u64,
    bitstream_buffer_range: u64,
    base_array_layer: u32,
    layer_count: u32,
) -> Result<NativeVulkanVulkanaliaDecodePrepareBarrierPlan, String> {
    if bitstream_buffer_range == 0 {
        return Err("Vulkanalia decode prepare barriers require non-empty bitstream range".into());
    }

    let buffer_barrier = vk::BufferMemoryBarrier2::builder()
        .src_stage_mask(vk::PipelineStageFlags2::HOST)
        .src_access_mask(vk::AccessFlags2::HOST_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::VIDEO_DECODE_KHR)
        .dst_access_mask(vk::AccessFlags2::VIDEO_DECODE_READ_KHR)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(bitstream_buffer)
        .offset(bitstream_buffer_offset)
        .size(bitstream_buffer_range)
        .build();
    let buffer_barriers = [buffer_barrier];
    let image_barriers = if layer_count > 0 {
        vec![
            vk::ImageMemoryBarrier2::builder()
                .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
                .src_access_mask(vk::AccessFlags2::empty())
                .dst_stage_mask(vk::PipelineStageFlags2::VIDEO_DECODE_KHR)
                .dst_access_mask(
                    vk::AccessFlags2::VIDEO_DECODE_READ_KHR
                        | vk::AccessFlags2::VIDEO_DECODE_WRITE_KHR,
                )
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::VIDEO_DECODE_DPB_KHR)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(native_vulkan_vulkanalia_decode_image_subresource_range(
                    base_array_layer,
                    layer_count,
                ))
                .build(),
        ]
    } else {
        Vec::new()
    };
    let dependency_info = vk::DependencyInfo::builder()
        .buffer_memory_barriers(&buffer_barriers)
        .image_memory_barriers(&image_barriers)
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &dependency_info);
    }

    Ok(native_vulkan_vulkanalia_decode_prepare_barrier_plan(
        image_barriers.len() as u32,
    ))
}

pub(super) unsafe fn native_vulkan_vulkanalia_record_h265_decode_commands(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    plan: &NativeVulkanVulkanaliaH265DecodeSubmitPlan,
    video_session: vk::VideoSessionKHR,
    session_parameters: vk::VideoSessionParametersKHR,
    bitstream_buffer: vk::Buffer,
    image_views: &super::video_decode_submit::NativeVulkanVulkanaliaDecodeImageViewBindings,
) -> Result<NativeVulkanVulkanaliaDecodeCommandBodyPlan, String> {
    native_vulkan_vulkanalia_h265_with_vk_submit_info(
        plan,
        video_session,
        session_parameters,
        bitstream_buffer,
        image_views,
        |vk_info| unsafe {
            device.cmd_begin_video_coding_khr(command_buffer, vk_info.begin_info);
            if plan.common.reset_control_recorded {
                let control_info = vk::VideoCodingControlInfoKHR::builder()
                    .flags(vk::VideoCodingControlFlagsKHR::RESET)
                    .build();
                device.cmd_control_video_coding_khr(command_buffer, &control_info);
            }
            device.cmd_decode_video_khr(command_buffer, vk_info.decode_info);
            device.cmd_end_video_coding_khr(
                command_buffer,
                &vk::VideoEndCodingInfoKHR::builder().build(),
            );
        },
    )?;

    Ok(native_vulkan_vulkanalia_decode_command_body_plan(
        plan.common.reset_control_recorded,
    ))
}

pub(super) unsafe fn native_vulkan_vulkanalia_record_av1_decode_commands(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    plan: &NativeVulkanVulkanaliaAv1DecodeSubmitPlan,
    video_session: vk::VideoSessionKHR,
    session_parameters: vk::VideoSessionParametersKHR,
    bitstream_buffer: vk::Buffer,
    image_views: &super::video_decode_submit::NativeVulkanVulkanaliaDecodeImageViewBindings,
) -> Result<NativeVulkanVulkanaliaDecodeCommandBodyPlan, String> {
    native_vulkan_vulkanalia_av1_with_vk_submit_info(
        plan,
        video_session,
        session_parameters,
        bitstream_buffer,
        image_views,
        |vk_info| unsafe {
            device.cmd_begin_video_coding_khr(command_buffer, vk_info.begin_info);
            if plan.common.reset_control_recorded {
                let control_info = vk::VideoCodingControlInfoKHR::builder()
                    .flags(vk::VideoCodingControlFlagsKHR::RESET)
                    .build();
                device.cmd_control_video_coding_khr(command_buffer, &control_info);
            }
            device.cmd_decode_video_khr(command_buffer, vk_info.decode_info);
            device.cmd_end_video_coding_khr(
                command_buffer,
                &vk::VideoEndCodingInfoKHR::builder().build(),
            );
        },
    )?;

    Ok(native_vulkan_vulkanalia_decode_command_body_plan(
        plan.common.reset_control_recorded,
    ))
}

pub(super) unsafe fn native_vulkan_vulkanalia_record_h264_decode_commands(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    plan: &NativeVulkanVulkanaliaH264DecodeSubmitPlan,
    video_session: vk::VideoSessionKHR,
    session_parameters: vk::VideoSessionParametersKHR,
    bitstream_buffer: vk::Buffer,
    image_views: &super::video_decode_submit::NativeVulkanVulkanaliaDecodeImageViewBindings,
) -> Result<NativeVulkanVulkanaliaDecodeCommandBodyPlan, String> {
    native_vulkan_vulkanalia_h264_with_vk_submit_info(
        plan,
        video_session,
        session_parameters,
        bitstream_buffer,
        image_views,
        |vk_info| unsafe {
            device.cmd_begin_video_coding_khr(command_buffer, vk_info.begin_info);
            if plan.common.reset_control_recorded {
                let control_info = vk::VideoCodingControlInfoKHR::builder()
                    .flags(vk::VideoCodingControlFlagsKHR::RESET)
                    .build();
                device.cmd_control_video_coding_khr(command_buffer, &control_info);
            }
            device.cmd_decode_video_khr(command_buffer, vk_info.decode_info);
            device.cmd_end_video_coding_khr(
                command_buffer,
                &vk::VideoEndCodingInfoKHR::builder().build(),
            );
        },
    )?;

    Ok(native_vulkan_vulkanalia_decode_command_body_plan(
        plan.common.reset_control_recorded,
    ))
}

pub(super) unsafe fn native_vulkan_vulkanalia_record_av1_decode_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    plan: &NativeVulkanVulkanaliaAv1DecodeSubmitPlan,
    video_session: vk::VideoSessionKHR,
    session_parameters: vk::VideoSessionParametersKHR,
    bitstream_buffer: vk::Buffer,
    image_views: &super::video_decode_submit::NativeVulkanVulkanaliaDecodeImageViewBindings,
    reset_command_buffer: bool,
    transition_dst_from_undefined: bool,
) -> Result<NativeVulkanVulkanaliaDecodeCommandBufferPlan, String> {
    if reset_command_buffer {
        unsafe {
            device
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
                .map_err(|err| format!("vkResetCommandBuffer(vulkanalia av1 decode): {err:?}"))?;
        }
    }

    let begin_info = vk::CommandBufferBeginInfo::builder()
        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
        .build();
    unsafe {
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| format!("vkBeginCommandBuffer(vulkanalia av1 decode): {err:?}"))?;
        native_vulkan_vulkanalia_record_decode_prepare_barriers(
            device,
            command_buffer,
            image,
            bitstream_buffer,
            plan.common.src_buffer_offset,
            plan.common.src_buffer_range,
            plan.common.dst_picture_resource.base_array_layer,
            u32::from(transition_dst_from_undefined),
        )?;
        native_vulkan_vulkanalia_record_av1_decode_commands(
            device,
            command_buffer,
            plan,
            video_session,
            session_parameters,
            bitstream_buffer,
            image_views,
        )?;
        device
            .end_command_buffer(command_buffer)
            .map_err(|err| format!("vkEndCommandBuffer(vulkanalia av1 decode): {err:?}"))?;
    }

    Ok(native_vulkan_vulkanalia_decode_command_buffer_plan(
        reset_command_buffer,
        plan.common.reset_control_recorded,
    ))
}

pub(super) unsafe fn native_vulkan_vulkanalia_record_h265_decode_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    plan: &NativeVulkanVulkanaliaH265DecodeSubmitPlan,
    video_session: vk::VideoSessionKHR,
    session_parameters: vk::VideoSessionParametersKHR,
    bitstream_buffer: vk::Buffer,
    image_views: &super::video_decode_submit::NativeVulkanVulkanaliaDecodeImageViewBindings,
    reset_command_buffer: bool,
    transition_dst_from_undefined: bool,
) -> Result<NativeVulkanVulkanaliaDecodeCommandBufferPlan, String> {
    if reset_command_buffer {
        unsafe {
            device
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
                .map_err(|err| format!("vkResetCommandBuffer(vulkanalia h265 decode): {err:?}"))?;
        }
    }

    let begin_info = vk::CommandBufferBeginInfo::builder()
        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
        .build();
    unsafe {
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| format!("vkBeginCommandBuffer(vulkanalia h265 decode): {err:?}"))?;
        native_vulkan_vulkanalia_record_decode_prepare_barriers(
            device,
            command_buffer,
            image,
            bitstream_buffer,
            plan.common.src_buffer_offset,
            plan.common.src_buffer_range,
            plan.common.dst_picture_resource.base_array_layer,
            u32::from(transition_dst_from_undefined),
        )?;
        native_vulkan_vulkanalia_record_h265_decode_commands(
            device,
            command_buffer,
            plan,
            video_session,
            session_parameters,
            bitstream_buffer,
            image_views,
        )?;
        device
            .end_command_buffer(command_buffer)
            .map_err(|err| format!("vkEndCommandBuffer(vulkanalia h265 decode): {err:?}"))?;
    }

    Ok(native_vulkan_vulkanalia_decode_command_buffer_plan(
        reset_command_buffer,
        plan.common.reset_control_recorded,
    ))
}

pub(super) unsafe fn native_vulkan_vulkanalia_record_h264_decode_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    plan: &NativeVulkanVulkanaliaH264DecodeSubmitPlan,
    video_session: vk::VideoSessionKHR,
    session_parameters: vk::VideoSessionParametersKHR,
    bitstream_buffer: vk::Buffer,
    image_views: &super::video_decode_submit::NativeVulkanVulkanaliaDecodeImageViewBindings,
    reset_command_buffer: bool,
    transition_dst_from_undefined: bool,
) -> Result<NativeVulkanVulkanaliaDecodeCommandBufferPlan, String> {
    if reset_command_buffer {
        unsafe {
            device
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
                .map_err(|err| format!("vkResetCommandBuffer(vulkanalia h264 decode): {err:?}"))?;
        }
    }

    let begin_info = vk::CommandBufferBeginInfo::builder()
        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
        .build();
    unsafe {
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| format!("vkBeginCommandBuffer(vulkanalia h264 decode): {err:?}"))?;
        native_vulkan_vulkanalia_record_decode_prepare_barriers(
            device,
            command_buffer,
            image,
            bitstream_buffer,
            plan.common.src_buffer_offset,
            plan.common.src_buffer_range,
            plan.common.dst_picture_resource.base_array_layer,
            u32::from(transition_dst_from_undefined),
        )?;
        native_vulkan_vulkanalia_record_h264_decode_commands(
            device,
            command_buffer,
            plan,
            video_session,
            session_parameters,
            bitstream_buffer,
            image_views,
        )?;
        device
            .end_command_buffer(command_buffer)
            .map_err(|err| format!("vkEndCommandBuffer(vulkanalia h264 decode): {err:?}"))?;
    }

    Ok(native_vulkan_vulkanalia_decode_command_buffer_plan(
        reset_command_buffer,
        plan.common.reset_control_recorded,
    ))
}

pub(super) unsafe fn native_vulkan_vulkanalia_submit_decode_command_buffer2(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,
    wait_idle_after_submit: bool,
) -> Result<NativeVulkanVulkanaliaDecodeSubmit2Plan, String> {
    let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
        .command_buffer(command_buffer)
        .build();
    let command_buffer_infos = [command_buffer_info];
    let submit_info = vk::SubmitInfo2::builder()
        .command_buffer_infos(&command_buffer_infos)
        .build();
    unsafe {
        device
            .queue_submit2(queue, &[submit_info], fence)
            .map_err(|err| format!("vkQueueSubmit2(vulkanalia decode): {err:?}"))?;
        if wait_idle_after_submit {
            device
                .queue_wait_idle(queue)
                .map_err(|err| format!("vkQueueWaitIdle(vulkanalia decode): {err:?}"))?;
        }
    }

    Ok(native_vulkan_vulkanalia_decode_submit2_plan(
        wait_idle_after_submit,
    ))
}

fn native_vulkan_vulkanalia_decode_image_subresource_range(
    base_array_layer: u32,
    layer_count: u32,
) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(base_array_layer)
        .layer_count(layer_count)
        .build()
}

fn native_vulkan_vulkanalia_recorded_decode_command_order(
    reset_before_record: bool,
    reset_control_recorded: bool,
) -> &'static [&'static str] {
    match (reset_before_record, reset_control_recorded) {
        (false, false) => &[
            "vkBeginCommandBuffer",
            "cmd_pipeline_barrier2",
            "cmd_begin_video_coding_khr",
            "cmd_decode_video_khr",
            "cmd_end_video_coding_khr",
            "vkEndCommandBuffer",
        ],
        (false, true) => &[
            "vkBeginCommandBuffer",
            "cmd_pipeline_barrier2",
            "cmd_begin_video_coding_khr",
            "cmd_control_video_coding_khr",
            "cmd_decode_video_khr",
            "cmd_end_video_coding_khr",
            "vkEndCommandBuffer",
        ],
        (true, false) => &[
            "vkResetCommandBuffer",
            "vkBeginCommandBuffer",
            "cmd_pipeline_barrier2",
            "cmd_begin_video_coding_khr",
            "cmd_decode_video_khr",
            "cmd_end_video_coding_khr",
            "vkEndCommandBuffer",
        ],
        (true, true) => &[
            "vkResetCommandBuffer",
            "vkBeginCommandBuffer",
            "cmd_pipeline_barrier2",
            "cmd_begin_video_coding_khr",
            "cmd_control_video_coding_khr",
            "cmd_decode_video_khr",
            "cmd_end_video_coding_khr",
            "vkEndCommandBuffer",
        ],
    }
}

fn native_vulkan_vulkanalia_decode_command_body_order(
    reset_control_recorded: bool,
) -> &'static [&'static str] {
    if reset_control_recorded {
        &[
            "cmd_begin_video_coding_khr",
            "cmd_control_video_coding_khr",
            "cmd_decode_video_khr",
            "cmd_end_video_coding_khr",
        ]
    } else {
        &[
            "cmd_begin_video_coding_khr",
            "cmd_decode_video_khr",
            "cmd_end_video_coding_khr",
        ]
    }
}

fn native_vulkan_vulkanalia_decode_submit2_order(
    wait_idle_after_submit: bool,
) -> &'static [&'static str] {
    if wait_idle_after_submit {
        &["queue_submit2", "queue_wait_idle"]
    } else {
        &["queue_submit2"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_prepare_barrier_plan_matches_ffmpeg_sync2_shape() {
        let plan = native_vulkan_vulkanalia_decode_prepare_barrier_plan(1);

        assert_eq!(plan.ffmpeg_reference, FFMPEG_VULKAN_DECODE_REFERENCE);
        assert!(plan.uses_synchronization2);
        assert_eq!(plan.buffer_barrier_count, 1);
        assert_eq!(plan.image_barrier_count, 1);
        assert_eq!(plan.old_layout, "undefined");
        assert_eq!(plan.new_layout, "video-decode-dpb");
        assert_eq!(plan.buffer_src_stage, "host");
        assert_eq!(plan.buffer_dst_stage, "video-decode");
    }

    #[test]
    fn h265_decode_command_buffer_plan_uses_submit2_and_ffmpeg_order() {
        let record_plan = native_vulkan_vulkanalia_decode_command_buffer_plan(true, true);
        let submit_plan = native_vulkan_vulkanalia_decode_submit2_plan(true);

        assert_eq!(record_plan.ffmpeg_reference, FFMPEG_VULKAN_DECODE_REFERENCE);
        assert!(record_plan.uses_synchronization2);
        assert!(record_plan.one_time_submit);
        assert!(record_plan.reset_before_record);
        assert_eq!(
            record_plan.command_order,
            &[
                "vkResetCommandBuffer",
                "vkBeginCommandBuffer",
                "cmd_pipeline_barrier2",
                "cmd_begin_video_coding_khr",
                "cmd_control_video_coding_khr",
                "cmd_decode_video_khr",
                "cmd_end_video_coding_khr",
                "vkEndCommandBuffer",
            ]
        );
        assert_eq!(submit_plan.ffmpeg_reference, FFMPEG_VULKAN_DECODE_REFERENCE);
        assert!(submit_plan.uses_submit2);
        assert!(submit_plan.wait_idle_after_submit);
        assert_eq!(
            submit_plan.command_order,
            &["queue_submit2", "queue_wait_idle"]
        );
    }
}
