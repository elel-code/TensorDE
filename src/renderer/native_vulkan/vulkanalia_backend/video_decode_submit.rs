#![allow(dead_code)]

use serde::Serialize;
use vulkanalia::vk::{self, HasBuilder};

use crate::renderer::native_vulkan::NativeVulkanVideoSessionCodec;

pub(super) const FFMPEG_VULKAN_DECODE_REFERENCE: &str =
    "references/ffmpeg/libavcodec/vulkan_decode.c";
pub(super) const FFMPEG_VULKAN_DECODE_PICTURE_REFERENCE: &str =
    "references/ffmpeg/libavcodec/vulkan_decode.h:88-93";
pub(super) const FFMPEG_VULKAN_DECODE_BEGIN_CURRENT_REFERENCE: &str =
    "references/ffmpeg/libavcodec/vulkan_decode.c:529-532";

// FFmpeg keeps `refs[36]` and `ref_slots[36]` inside FFVulkanDecodePicture,
// then appends the current picture as one inactive begin-coding reference.
pub(super) const NATIVE_VULKAN_VULKANALIA_MAX_DECODE_REFERENCE_SLOTS: usize = 36;
pub(super) const NATIVE_VULKAN_VULKANALIA_MAX_BEGIN_REFERENCE_SLOTS: usize =
    NATIVE_VULKAN_VULKANALIA_MAX_DECODE_REFERENCE_SLOTS + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct NativeVulkanVulkanaliaStreamingDecodeTimingSnapshot {
    pub measured_frame_count: u32,
    pub total_loop_micros: u64,
    pub total_frame_micros: u64,
    pub max_frame_micros: u64,
    pub total_next_frame_micros: u64,
    pub max_next_frame_micros: u64,
    pub total_bitstream_buffer_micros: u64,
    pub max_bitstream_buffer_micros: u64,
    pub total_payload_write_micros: u64,
    pub max_payload_write_micros: u64,
    pub total_decode_plan_micros: u64,
    pub max_decode_plan_micros: u64,
    pub total_image_view_bind_micros: u64,
    pub max_image_view_bind_micros: u64,
    pub total_record_command_buffer_micros: u64,
    pub max_record_command_buffer_micros: u64,
    pub total_submit_wait_micros: u64,
    pub max_submit_wait_micros: u64,
    pub total_slot_reuse_wait_micros: u64,
    pub max_slot_reuse_wait_micros: u64,
    pub total_exec_slot_reuse_wait_micros: u64,
    pub max_exec_slot_reuse_wait_micros: u64,
    pub total_output_slot_reuse_wait_micros: u64,
    pub max_output_slot_reuse_wait_micros: u64,
    pub final_drain_wait_micros: u64,
    pub total_after_frame_submitted_micros: u64,
    pub max_after_frame_submitted_micros: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaPictureResourcePlan {
    pub coded_offset: (i32, i32),
    pub coded_extent: (u32, u32),
    pub base_array_layer: u32,
    pub image_view_binding_required: bool,
}

impl NativeVulkanVulkanaliaPictureResourcePlan {
    pub(super) fn new(extent: vk::Extent2D, base_array_layer: u32) -> Self {
        Self {
            coded_offset: (0, 0),
            coded_extent: (extent.width, extent.height),
            base_array_layer,
            image_view_binding_required: true,
        }
    }

    pub(super) fn to_vk(
        &self,
        image_view_binding: vk::ImageView,
    ) -> vk::VideoPictureResourceInfoKHR {
        vk::VideoPictureResourceInfoKHR::builder()
            .coded_offset(vk::Offset2D {
                x: self.coded_offset.0,
                y: self.coded_offset.1,
            })
            .coded_extent(vk::Extent2D {
                width: self.coded_extent.0,
                height: self.coded_extent.1,
            })
            .base_array_layer(self.base_array_layer)
            .image_view_binding(image_view_binding)
            .build()
    }

    pub(super) fn to_vk_with_base_array_layer(
        &self,
        image_view_binding: vk::ImageView,
        base_array_layer: u32,
    ) -> vk::VideoPictureResourceInfoKHR {
        vk::VideoPictureResourceInfoKHR::builder()
            .coded_offset(vk::Offset2D {
                x: self.coded_offset.0,
                y: self.coded_offset.1,
            })
            .coded_extent(vk::Extent2D {
                width: self.coded_extent.0,
                height: self.coded_extent.1,
            })
            .base_array_layer(base_array_layer)
            .image_view_binding(image_view_binding)
            .build()
    }

    pub(super) fn with_base_array_layer(&self, base_array_layer: u32) -> Self {
        Self {
            base_array_layer,
            ..*self
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeVulkanVulkanaliaDecodeImageViewBindings {
    pub dst_picture_image_view: vk::ImageView,
    pub setup_reference_image_view: vk::ImageView,
    pub begin_reference_image_view: vk::ImageView,
    pub begin_reference_image_view_count: usize,
    pub decode_reference_image_view: vk::ImageView,
    pub decode_reference_image_view_count: usize,
}

impl NativeVulkanVulkanaliaDecodeImageViewBindings {
    #[cfg(test)]
    pub(super) fn repeated(
        image_view: vk::ImageView,
        begin_reference_slot_count: usize,
        decode_reference_slot_count: usize,
    ) -> Self {
        Self {
            dst_picture_image_view: image_view,
            setup_reference_image_view: image_view,
            begin_reference_image_view: image_view,
            begin_reference_image_view_count: begin_reference_slot_count,
            decode_reference_image_view: image_view,
            decode_reference_image_view_count: decode_reference_slot_count,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaReferenceSlotPlan {
    pub slot_index: i32,
    pub resource: NativeVulkanVulkanaliaPictureResourcePlan,
}

impl NativeVulkanVulkanaliaReferenceSlotPlan {
    pub(super) fn setup_current(
        slot_index: i32,
        resource: NativeVulkanVulkanaliaPictureResourcePlan,
    ) -> Self {
        Self {
            slot_index,
            resource,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaDecodeSubmitPlan {
    pub codec: NativeVulkanVideoSessionCodec,
    pub ffmpeg_reference: &'static str,
    pub command_order: &'static [&'static str],
    pub reset_control_recorded: bool,
    pub src_buffer_offset: u64,
    pub src_buffer_range: u64,
    pub dst_picture_resource: NativeVulkanVulkanaliaPictureResourcePlan,
    pub setup_reference_slot: NativeVulkanVulkanaliaReferenceSlotPlan,
    pub begin_reference_slot_count: usize,
    pub decode_reference_slot_count: usize,
}

impl NativeVulkanVulkanaliaDecodeSubmitPlan {
    pub(super) fn new(
        codec: NativeVulkanVideoSessionCodec,
        src_buffer_offset: u64,
        src_buffer_range: u64,
        dst_picture_resource: NativeVulkanVulkanaliaPictureResourcePlan,
        setup_reference_slot: NativeVulkanVulkanaliaReferenceSlotPlan,
        begin_reference_slot_count: usize,
        decode_reference_slot_count: usize,
        reset_control_recorded: bool,
    ) -> Self {
        Self {
            codec,
            ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
            command_order: native_vulkan_vulkanalia_decode_command_order(reset_control_recorded),
            reset_control_recorded,
            src_buffer_offset,
            src_buffer_range,
            dst_picture_resource,
            setup_reference_slot,
            begin_reference_slot_count,
            decode_reference_slot_count,
        }
    }
}

const NATIVE_VULKAN_VULKANALIA_DECODE_COMMAND_ORDER: &[&str] = &[
    "cmd_pipeline_barrier2",
    "cmd_begin_video_coding_khr",
    "cmd_decode_video_khr",
    "cmd_end_video_coding_khr",
    "queue_submit2",
];

const NATIVE_VULKAN_VULKANALIA_DECODE_COMMAND_ORDER_WITH_RESET: &[&str] = &[
    "cmd_pipeline_barrier2",
    "cmd_begin_video_coding_khr",
    "cmd_control_video_coding_khr",
    "cmd_decode_video_khr",
    "cmd_end_video_coding_khr",
    "queue_submit2",
];

pub(super) fn native_vulkan_vulkanalia_decode_command_order(
    reset_control_recorded: bool,
) -> &'static [&'static str] {
    if reset_control_recorded {
        NATIVE_VULKAN_VULKANALIA_DECODE_COMMAND_ORDER_WITH_RESET
    } else {
        NATIVE_VULKAN_VULKANALIA_DECODE_COMMAND_ORDER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_command_order_matches_ffmpeg_begin_decode_end_shape() {
        assert_eq!(
            native_vulkan_vulkanalia_decode_command_order(false),
            &[
                "cmd_pipeline_barrier2",
                "cmd_begin_video_coding_khr",
                "cmd_decode_video_khr",
                "cmd_end_video_coding_khr",
                "queue_submit2",
            ]
        );
        assert_eq!(
            native_vulkan_vulkanalia_decode_command_order(true),
            &[
                "cmd_pipeline_barrier2",
                "cmd_begin_video_coding_khr",
                "cmd_control_video_coding_khr",
                "cmd_decode_video_khr",
                "cmd_end_video_coding_khr",
                "queue_submit2",
            ]
        );
    }
}
