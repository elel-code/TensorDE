#![allow(dead_code)]

use serde::Serialize;
use vulkanalia::vk;

use crate::renderer::native_vulkan::NativeVulkanVideoSessionCodec;

pub(super) const FFMPEG_VULKAN_DECODE_REFERENCE: &str =
    "references/ffmpeg/libavcodec/vulkan_decode.c";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) enum NativeVulkanVulkanaliaReferenceSlotRole {
    BeginInactive,
    DecodeReference,
    SetupCurrent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaReferenceSlotPlan {
    pub role: NativeVulkanVulkanaliaReferenceSlotRole,
    pub slot_index: i32,
    pub resource: NativeVulkanVulkanaliaPictureResourcePlan,
    pub codec_dpb_info_required: bool,
}

impl NativeVulkanVulkanaliaReferenceSlotPlan {
    pub(super) fn begin_inactive(resource: NativeVulkanVulkanaliaPictureResourcePlan) -> Self {
        Self {
            role: NativeVulkanVulkanaliaReferenceSlotRole::BeginInactive,
            slot_index: -1,
            resource,
            codec_dpb_info_required: false,
        }
    }

    pub(super) fn decode_reference(
        slot_index: i32,
        resource: NativeVulkanVulkanaliaPictureResourcePlan,
    ) -> Self {
        Self {
            role: NativeVulkanVulkanaliaReferenceSlotRole::DecodeReference,
            slot_index,
            resource,
            codec_dpb_info_required: true,
        }
    }

    pub(super) fn setup_current(
        slot_index: i32,
        resource: NativeVulkanVulkanaliaPictureResourcePlan,
    ) -> Self {
        Self {
            role: NativeVulkanVulkanaliaReferenceSlotRole::SetupCurrent,
            slot_index,
            resource,
            codec_dpb_info_required: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct NativeVulkanVulkanaliaDecodeSubmitPlan {
    pub codec: NativeVulkanVideoSessionCodec,
    pub ffmpeg_reference: &'static str,
    pub command_order: Vec<&'static str>,
    pub reset_control_recorded: bool,
    pub src_buffer_offset: u64,
    pub src_buffer_range: u64,
    pub dst_picture_resource: NativeVulkanVulkanaliaPictureResourcePlan,
    pub setup_reference_slot: NativeVulkanVulkanaliaReferenceSlotPlan,
    pub begin_reference_slots: Vec<NativeVulkanVulkanaliaReferenceSlotPlan>,
    pub decode_reference_slots: Vec<NativeVulkanVulkanaliaReferenceSlotPlan>,
}

impl NativeVulkanVulkanaliaDecodeSubmitPlan {
    pub(super) fn new(
        codec: NativeVulkanVideoSessionCodec,
        src_buffer_offset: u64,
        src_buffer_range: u64,
        dst_picture_resource: NativeVulkanVulkanaliaPictureResourcePlan,
        setup_reference_slot: NativeVulkanVulkanaliaReferenceSlotPlan,
        begin_reference_slots: Vec<NativeVulkanVulkanaliaReferenceSlotPlan>,
        decode_reference_slots: Vec<NativeVulkanVulkanaliaReferenceSlotPlan>,
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
            begin_reference_slots,
            decode_reference_slots,
        }
    }
}

pub(super) fn native_vulkan_vulkanalia_decode_command_order(
    reset_control_recorded: bool,
) -> Vec<&'static str> {
    let mut order = vec!["cmd_pipeline_barrier2", "cmd_begin_video_coding_khr"];
    if reset_control_recorded {
        order.push("cmd_control_video_coding_khr");
    }
    order.extend([
        "cmd_decode_video_khr",
        "cmd_end_video_coding_khr",
        "queue_submit",
    ]);
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_command_order_matches_ffmpeg_begin_decode_end_shape() {
        assert_eq!(
            native_vulkan_vulkanalia_decode_command_order(false),
            vec![
                "cmd_pipeline_barrier2",
                "cmd_begin_video_coding_khr",
                "cmd_decode_video_khr",
                "cmd_end_video_coding_khr",
                "queue_submit",
            ]
        );
        assert_eq!(
            native_vulkan_vulkanalia_decode_command_order(true),
            vec![
                "cmd_pipeline_barrier2",
                "cmd_begin_video_coding_khr",
                "cmd_control_video_coding_khr",
                "cmd_decode_video_khr",
                "cmd_end_video_coding_khr",
                "queue_submit",
            ]
        );
    }

    #[test]
    fn reference_slot_roles_preserve_inactive_and_codec_dpb_requirements() {
        let resource = NativeVulkanVulkanaliaPictureResourcePlan::new(
            vk::Extent2D {
                width: 640,
                height: 368,
            },
            3,
        );

        let inactive = NativeVulkanVulkanaliaReferenceSlotPlan::begin_inactive(resource.clone());
        assert_eq!(inactive.slot_index, -1);
        assert!(!inactive.codec_dpb_info_required);

        let setup = NativeVulkanVulkanaliaReferenceSlotPlan::setup_current(3, resource);
        assert_eq!(setup.slot_index, 3);
        assert!(setup.codec_dpb_info_required);
    }
}
