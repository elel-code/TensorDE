use crate::renderer::native_vulkan::{
    NativeVulkanAv1DecodeReferencePlanEntrySnapshot, NativeVulkanAv1SequenceHeaderSnapshot,
    NativeVulkanEncodedAccessUnitPayload, NativeVulkanH264DecodeReferencePlanEntrySnapshot,
    NativeVulkanH264ParameterSetSnapshot, NativeVulkanH265DecodeReferencePlanEntrySnapshot,
    NativeVulkanH265ParameterSetSnapshot, NativeVulkanVideoSessionCodec,
};
use serde::Serialize;
use std::sync::Mutex;
use std::time::Instant;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::instance::{
    native_vulkan_vulkanalia_create_instance, native_vulkan_vulkanalia_destroy_instance,
};
use super::video_bitstream_buffer::{
    NativeVulkanVulkanaliaVideoSessionBitstreamBufferSmokeSnapshot,
    VulkanaliaVideoSessionBitstreamBuffer,
    native_vulkan_vulkanalia_create_video_session_bitstream_buffer,
    native_vulkan_vulkanalia_destroy_video_session_bitstream_buffer,
    native_vulkan_vulkanalia_ffmpeg_decode_bitstream_buffer_size,
    native_vulkan_vulkanalia_smoke_create_video_session_bitstream_buffer,
    native_vulkan_vulkanalia_write_ffmpeg_picture_slices_buffer,
};
use super::video_codec::{
    native_vulkan_vulkanalia_video_session_codec_name as vulkanalia_video_session_codec_name,
    native_vulkan_vulkanalia_video_session_codec_operation as vulkanalia_video_session_codec_operation,
    native_vulkan_vulkanalia_video_session_label as vulkanalia_video_session_label,
};
use super::video_command_pool::{
    VulkanaliaDecodeCommandBuffer, native_vulkan_vulkanalia_create_decode_command_buffers,
    native_vulkan_vulkanalia_destroy_decode_command_buffer,
};
use super::video_decode_commands::{
    native_vulkan_vulkanalia_record_av1_decode_command_buffer,
    native_vulkan_vulkanalia_record_h264_decode_command_buffer,
    native_vulkan_vulkanalia_record_h265_decode_command_buffer,
    native_vulkan_vulkanalia_submit_decode_command_buffer2,
};
use super::video_decode_submit::NativeVulkanVulkanaliaDecodeImageViewBindings;
use super::video_decode_submit::NativeVulkanVulkanaliaStreamingDecodeTimingSnapshot;
use super::video_decode_submit_av1::{
    NativeVulkanVulkanaliaAv1CommandSmokeSnapshot, NativeVulkanVulkanaliaAv1FrameSubmitInput,
    native_vulkan_vulkanalia_av1_decode_submit_plan,
};
use super::video_decode_submit_h264::{
    NativeVulkanVulkanaliaH264ParameterIds,
    NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaH264ReadyPrefixFrameInput,
    native_vulkan_vulkanalia_h264_ready_prefix_decode_submit_plan,
};
use super::video_decode_submit_h265::{
    NativeVulkanVulkanaliaH265ParameterIds,
    NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaH265ReadyPrefixFrameInput,
    native_vulkan_vulkanalia_h265_ready_prefix_decode_submit_plan,
};
use super::video_device::{
    NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
    NativeVulkanVulkanaliaVideoPhysicalDeviceSelection,
    native_vulkan_vulkanalia_create_video_decode_device,
    native_vulkan_vulkanalia_destroy_video_decode_device,
    native_vulkan_vulkanalia_select_video_decode_physical_device,
};
use super::video_format_probe::native_vulkan_vulkanalia_video_format_probe;
use super::video_profile_labels::{
    video_capability_flag_labels, video_decode_capability_flag_labels,
};
use super::video_session::{
    NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot,
    NativeVulkanVulkanaliaVideoSessionResourceProbePlan,
    native_vulkan_vulkanalia_bind_video_session_memory_resources,
    native_vulkan_vulkanalia_create_video_session, native_vulkan_vulkanalia_destroy_video_session,
    native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources,
    native_vulkan_vulkanalia_video_session_resource_plans_from_format_probe,
};
use super::video_session_capabilities::{
    VulkanaliaVideoSessionCapabilityQuery,
    native_vulkan_vulkanalia_video_format_probe_includes_format as video_format_probe_includes_format,
    native_vulkan_vulkanalia_video_session_effective_format_probe_profile,
    native_vulkan_vulkanalia_video_session_effective_picture_format,
    native_vulkan_vulkanalia_video_session_effective_profile_label,
    native_vulkan_vulkanalia_video_session_extent_supported,
    native_vulkan_vulkanalia_video_session_max_active_reference_pictures,
    native_vulkan_vulkanalia_video_session_max_dpb_slots,
    with_native_vulkan_vulkanalia_video_session_capabilities,
};
use super::video_session_images::{
    NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot,
    native_vulkan_vulkanalia_smoke_create_video_session_resource_image,
};
use super::video_session_parameters::{
    NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot,
    native_vulkan_vulkanalia_destroy_video_session_parameters,
    native_vulkan_vulkanalia_smoke_create_empty_video_session_parameters,
};
use super::video_session_parameters_av1::native_vulkan_vulkanalia_create_av1_video_session_parameters;
use super::video_session_parameters_av1::native_vulkan_vulkanalia_smoke_create_av1_video_session_parameters;
use super::video_session_parameters_h264::{
    native_vulkan_vulkanalia_create_h264_video_session_parameters,
    native_vulkan_vulkanalia_smoke_create_h264_video_session_parameters,
};
use super::video_session_parameters_h265::{
    native_vulkan_vulkanalia_create_h265_video_session_parameters,
    native_vulkan_vulkanalia_smoke_create_h265_video_session_parameters,
};

type NativeVulkanVulkanaliaAfterFrameSubmitted<'a> = &'a mut dyn FnMut(
    u32,
    u32,
    Option<u64>,
    Option<u64>,
    Option<u64>,
    Option<u64>,
    i64,
    &'static str,
    u64,
) -> Result<(), String>;
type NativeVulkanVulkanaliaBeforeOutputSlotReuse<'a> = &'a mut dyn FnMut(u32) -> Result<(), String>;

const NATIVE_VULKAN_VULKANALIA_STREAMING_DECODE_SUBMIT_FENCE_SYNC_MODEL: &str = "FFmpeg-style queue_submit2 async exec ring: each exec slot owns its mapped picture slices buffer until that slot fence completes; DPB output layer reuse stays independent; no per-frame submit wait and no queue_wait_idle";
const NATIVE_VULKAN_VULKANALIA_DECODE_FRAME_TELEMETRY_RETAINED_FRAMES: usize = 0;
const NATIVE_VULKAN_VULKANALIA_DECODE_FRAME_TELEMETRY_RETENTION_MODEL: &str = "FFmpeg-style scalar decode telemetry only; mirrors references/ffmpeg/libavcodec/vulkan_decode.h:73-106 and references/ffmpeg/libavcodec/vulkan_decode.c:488-536; no retained per-frame command snapshots";

#[derive(Clone, Copy)]
struct NativeVulkanVulkanaliaDecodeFrameLastFields {
    src_buffer_offset: u64,
    src_buffer_range: u64,
    dst_base_array_layer: u32,
    setup_slot_index: i32,
    begin_reference_slot_count: u32,
    decode_reference_slot_count: u32,
    reset_control_recorded: bool,
}

struct NativeVulkanVulkanaliaDecodeFrameTelemetry {
    submitted_frame_count: u32,
    last_frame: Option<NativeVulkanVulkanaliaDecodeFrameLastFields>,
    max_src_buffer_range: u64,
    first_frame_reset_control_recorded: Option<bool>,
    reset_control_recorded_frame_count: u32,
    p_frame_count: u32,
    b_frame_count: u32,
    max_begin_reference_slot_count: u32,
    max_decode_reference_slot_count: u32,
}

impl NativeVulkanVulkanaliaDecodeFrameTelemetry {
    fn new() -> Self {
        Self {
            submitted_frame_count: 0,
            last_frame: None,
            max_src_buffer_range: 0,
            first_frame_reset_control_recorded: None,
            reset_control_recorded_frame_count: 0,
            p_frame_count: 0,
            b_frame_count: 0,
            max_begin_reference_slot_count: 0,
            max_decode_reference_slot_count: 0,
        }
    }

    fn push(&mut self, frame: NativeVulkanVulkanaliaDecodeFrameLastFields) {
        self.max_src_buffer_range = self.max_src_buffer_range.max(frame.src_buffer_range);
        if self.submitted_frame_count == 0 {
            self.first_frame_reset_control_recorded = Some(frame.reset_control_recorded);
        }
        if frame.reset_control_recorded {
            self.reset_control_recorded_frame_count =
                self.reset_control_recorded_frame_count.saturating_add(1);
        } else if frame.decode_reference_slot_count > 0 {
            self.p_frame_count = self.p_frame_count.saturating_add(1);
        }
        if frame.begin_reference_slot_count > frame.decode_reference_slot_count {
            self.b_frame_count = self.b_frame_count.saturating_add(1);
        }
        self.max_begin_reference_slot_count = self
            .max_begin_reference_slot_count
            .max(frame.begin_reference_slot_count);
        self.max_decode_reference_slot_count = self
            .max_decode_reference_slot_count
            .max(frame.decode_reference_slot_count);

        self.last_frame = Some(frame);
        self.submitted_frame_count = self.submitted_frame_count.saturating_add(1);
    }

    fn last_frame(
        &self,
        error: &'static str,
    ) -> Result<NativeVulkanVulkanaliaDecodeFrameLastFields, String> {
        self.last_frame.ok_or_else(|| error.to_owned())
    }

    fn retained_frame_count(&self) -> u32 {
        0
    }
}

#[derive(Default)]
struct NativeVulkanVulkanaliaStreamingDecodeTiming {
    snapshot: NativeVulkanVulkanaliaStreamingDecodeTimingSnapshot,
}

#[derive(Default)]
struct NativeVulkanVulkanaliaStreamingDecodeFrameTiming {
    next_frame_micros: u64,
    bitstream_buffer_micros: u64,
    payload_write_micros: u64,
    decode_plan_micros: u64,
    image_view_bind_micros: u64,
    record_command_buffer_micros: u64,
    submit_wait_micros: u64,
    exec_slot_reuse_wait_micros: u64,
    output_slot_reuse_wait_micros: u64,
    after_frame_submitted_micros: u64,
}

impl NativeVulkanVulkanaliaStreamingDecodeTiming {
    fn push(&mut self, frame: NativeVulkanVulkanaliaStreamingDecodeFrameTiming) {
        let frame_micros = frame
            .next_frame_micros
            .saturating_add(frame.bitstream_buffer_micros)
            .saturating_add(frame.payload_write_micros)
            .saturating_add(frame.decode_plan_micros)
            .saturating_add(frame.image_view_bind_micros)
            .saturating_add(frame.record_command_buffer_micros)
            .saturating_add(frame.submit_wait_micros)
            .saturating_add(frame.exec_slot_reuse_wait_micros)
            .saturating_add(frame.output_slot_reuse_wait_micros)
            .saturating_add(frame.after_frame_submitted_micros);
        let snapshot = &mut self.snapshot;
        snapshot.measured_frame_count = snapshot.measured_frame_count.saturating_add(1);
        snapshot.total_frame_micros = snapshot.total_frame_micros.saturating_add(frame_micros);
        snapshot.max_frame_micros = snapshot.max_frame_micros.max(frame_micros);
        snapshot.total_next_frame_micros = snapshot
            .total_next_frame_micros
            .saturating_add(frame.next_frame_micros);
        snapshot.max_next_frame_micros =
            snapshot.max_next_frame_micros.max(frame.next_frame_micros);
        snapshot.total_bitstream_buffer_micros = snapshot
            .total_bitstream_buffer_micros
            .saturating_add(frame.bitstream_buffer_micros);
        snapshot.max_bitstream_buffer_micros = snapshot
            .max_bitstream_buffer_micros
            .max(frame.bitstream_buffer_micros);
        snapshot.total_payload_write_micros = snapshot
            .total_payload_write_micros
            .saturating_add(frame.payload_write_micros);
        snapshot.max_payload_write_micros = snapshot
            .max_payload_write_micros
            .max(frame.payload_write_micros);
        snapshot.total_decode_plan_micros = snapshot
            .total_decode_plan_micros
            .saturating_add(frame.decode_plan_micros);
        snapshot.max_decode_plan_micros = snapshot
            .max_decode_plan_micros
            .max(frame.decode_plan_micros);
        snapshot.total_image_view_bind_micros = snapshot
            .total_image_view_bind_micros
            .saturating_add(frame.image_view_bind_micros);
        snapshot.max_image_view_bind_micros = snapshot
            .max_image_view_bind_micros
            .max(frame.image_view_bind_micros);
        snapshot.total_record_command_buffer_micros = snapshot
            .total_record_command_buffer_micros
            .saturating_add(frame.record_command_buffer_micros);
        snapshot.max_record_command_buffer_micros = snapshot
            .max_record_command_buffer_micros
            .max(frame.record_command_buffer_micros);
        snapshot.total_submit_wait_micros = snapshot
            .total_submit_wait_micros
            .saturating_add(frame.submit_wait_micros);
        snapshot.max_submit_wait_micros = snapshot
            .max_submit_wait_micros
            .max(frame.submit_wait_micros);
        snapshot.total_slot_reuse_wait_micros = snapshot
            .total_slot_reuse_wait_micros
            .saturating_add(frame.exec_slot_reuse_wait_micros)
            .saturating_add(frame.output_slot_reuse_wait_micros);
        snapshot.max_slot_reuse_wait_micros = snapshot.max_slot_reuse_wait_micros.max(
            frame
                .exec_slot_reuse_wait_micros
                .saturating_add(frame.output_slot_reuse_wait_micros),
        );
        snapshot.total_exec_slot_reuse_wait_micros = snapshot
            .total_exec_slot_reuse_wait_micros
            .saturating_add(frame.exec_slot_reuse_wait_micros);
        snapshot.max_exec_slot_reuse_wait_micros = snapshot
            .max_exec_slot_reuse_wait_micros
            .max(frame.exec_slot_reuse_wait_micros);
        snapshot.total_output_slot_reuse_wait_micros = snapshot
            .total_output_slot_reuse_wait_micros
            .saturating_add(frame.output_slot_reuse_wait_micros);
        snapshot.max_output_slot_reuse_wait_micros = snapshot
            .max_output_slot_reuse_wait_micros
            .max(frame.output_slot_reuse_wait_micros);
        snapshot.total_after_frame_submitted_micros = snapshot
            .total_after_frame_submitted_micros
            .saturating_add(frame.after_frame_submitted_micros);
        snapshot.max_after_frame_submitted_micros = snapshot
            .max_after_frame_submitted_micros
            .max(frame.after_frame_submitted_micros);
    }

    fn finish(
        mut self,
        total_loop_micros: u64,
        final_drain_wait_micros: u64,
    ) -> NativeVulkanVulkanaliaStreamingDecodeTimingSnapshot {
        self.snapshot.total_loop_micros = total_loop_micros;
        self.snapshot.final_drain_wait_micros = final_drain_wait_micros;
        self.snapshot
    }
}

pub(super) struct NativeVulkanVulkanaliaH264StreamingDecodeInput<'a> {
    pub(super) parameter_sets: NativeVulkanH264ParameterSetSnapshot,
    pub(super) requested_frame_count: u32,
    pub(super) next_frame:
        &'a mut dyn FnMut() -> Result<NativeVulkanVulkanaliaH264ReadyPrefixFrameInput, String>,
}

pub(super) struct NativeVulkanVulkanaliaH265StreamingDecodeInput<'a> {
    pub(super) parameter_sets: NativeVulkanH265ParameterSetSnapshot,
    pub(super) requested_frame_count: u32,
    pub(super) next_frame:
        &'a mut dyn FnMut() -> Result<NativeVulkanVulkanaliaH265ReadyPrefixFrameInput, String>,
}

pub(super) struct NativeVulkanVulkanaliaAv1StreamingDecodeInput<'a> {
    pub(super) sequence_header: NativeVulkanAv1SequenceHeaderSnapshot,
    pub(super) requested_frame_count: u32,
    pub(super) next_frame:
        &'a mut dyn FnMut() -> Result<NativeVulkanVulkanaliaAv1StreamingFrameInput, String>,
}

pub(super) struct NativeVulkanVulkanaliaAv1StreamingFrameInput {
    pub(super) entry: NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
    pub(super) frame: Option<NativeVulkanVulkanaliaAv1FrameSubmitInput>,
    pub(super) pts_ns: Option<u64>,
    pub(super) duration_ns: Option<u64>,
    pub(super) pts_ms: Option<u64>,
    pub(super) duration_ms: Option<u64>,
    pub(super) access_unit_payload: NativeVulkanEncodedAccessUnitPayload,
}

fn native_vulkan_vulkanalia_elapsed_micros(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX)
}

struct NativeVulkanVulkanaliaStreamingDecodeSubmitRing {
    submitted_slots: Vec<bool>,
    recorded_slots: Vec<bool>,
}

impl NativeVulkanVulkanaliaStreamingDecodeSubmitRing {
    fn new(slot_count: usize) -> Self {
        Self {
            submitted_slots: vec![false; slot_count],
            recorded_slots: vec![false; slot_count],
        }
    }

    fn slot_count(&self) -> usize {
        self.submitted_slots.len()
    }

    fn exec_slot_for_frame(&self, frame_index: u32) -> usize {
        frame_index as usize % self.slot_count().max(1)
    }

    fn reset_command_buffer_before_record(&self, slot: usize) -> Result<bool, String> {
        self.recorded_slots.get(slot).copied().ok_or_else(|| {
            format!(
                "Vulkanalia streaming decode slot {slot} exceeds ring size {}",
                self.slot_count()
            )
        })
    }

    fn mark_recorded(&mut self, slot: usize) -> Result<(), String> {
        let slot_count = self.slot_count();
        let recorded = self.recorded_slots.get_mut(slot).ok_or_else(|| {
            format!(
                "Vulkanalia streaming decode recorded slot {slot} exceeds ring size {}",
                slot_count
            )
        })?;
        *recorded = true;
        Ok(())
    }

    fn mark_submitted(&mut self, slot: usize) -> Result<(), String> {
        let slot_count = self.slot_count();
        let submitted = self.submitted_slots.get_mut(slot).ok_or_else(|| {
            format!(
                "Vulkanalia streaming decode submitted slot {slot} exceeds ring size {}",
                slot_count
            )
        })?;
        *submitted = true;
        Ok(())
    }

    fn wait_for_slot_reuse(
        &mut self,
        device: &Device,
        command_buffer: &VulkanaliaDecodeCommandBuffer,
        slot: usize,
    ) -> Result<u64, String> {
        let slot_count = self.slot_count();
        let submitted = self.submitted_slots.get_mut(slot).ok_or_else(|| {
            format!(
                "Vulkanalia streaming decode reuse slot {slot} exceeds ring size {}",
                slot_count
            )
        })?;
        if !*submitted {
            return Ok(0);
        }
        let fence = command_buffer.submit_fence_at(slot)?;
        let started_at = Instant::now();
        unsafe {
            device
                .wait_for_fences(&[fence], true, u64::MAX)
                .map_err(|err| format!("vkWaitForFences(vulkanalia decode slot reuse): {err:?}"))?;
            device
                .reset_fences(&[fence])
                .map_err(|err| format!("vkResetFences(vulkanalia decode slot reuse): {err:?}"))?;
        }
        *submitted = false;
        Ok(native_vulkan_vulkanalia_elapsed_micros(started_at))
    }

    fn wait_all_submitted(
        &mut self,
        device: &Device,
        command_buffer: &VulkanaliaDecodeCommandBuffer,
    ) -> Result<u64, String> {
        let mut total_micros = 0u64;
        for slot in 0..self.submitted_slots.len() {
            total_micros = total_micros.saturating_add(self.wait_for_slot_reuse(
                device,
                command_buffer,
                slot,
            )?);
        }
        Ok(total_micros)
    }
}

fn native_vulkan_vulkanalia_align_up_u64(value: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    value
        .checked_add(alignment - 1)
        .map(|value| value / alignment * alignment)
        .unwrap_or(u64::MAX)
}

struct NativeVulkanVulkanaliaFfmpegSlicesBufferPool {
    slots: Vec<Option<VulkanaliaVideoSessionBitstreamBuffer>>,
}

impl NativeVulkanVulkanaliaFfmpegSlicesBufferPool {
    fn new(slot_count: usize) -> Self {
        let slots = (0..slot_count.max(1)).map(|_| None).collect();
        Self { slots }
    }

    fn buffer_for_payload<'a>(
        &'a mut self,
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        profile_info: &vk::VideoProfileInfoKHR,
        slot: usize,
        payload_len: u64,
        min_size_alignment: u64,
        non_coherent_atom_size: u64,
    ) -> Result<&'a VulkanaliaVideoSessionBitstreamBuffer, String> {
        let slot_count = self.slots.len();
        let slot_buffer = self.slots.get_mut(slot).ok_or_else(|| {
            format!("Vulkanalia FFmpeg slices buffer slot {slot} exceeds pool size {slot_count}")
        })?;
        let ffmpeg_new_size = native_vulkan_vulkanalia_align_up_u64(
            payload_len.max(1).saturating_add(min_size_alignment.max(1)),
            min_size_alignment.max(1),
        );
        let target_size = native_vulkan_vulkanalia_ffmpeg_decode_bitstream_buffer_size(
            ffmpeg_new_size,
            min_size_alignment,
        );
        let needs_replace = slot_buffer
            .as_ref()
            .map(|buffer| buffer.snapshot.size < target_size)
            .unwrap_or(true);
        if needs_replace {
            if let Some(old_buffer) = slot_buffer.take() {
                native_vulkan_vulkanalia_destroy_video_session_bitstream_buffer(device, old_buffer);
            }
            *slot_buffer = Some(
                native_vulkan_vulkanalia_create_video_session_bitstream_buffer(
                    device,
                    memory_properties,
                    profile_info,
                    target_size,
                    min_size_alignment,
                    non_coherent_atom_size,
                    None,
                    true,
                )?,
            );
        }
        slot_buffer.as_ref().ok_or_else(|| {
            "Vulkanalia FFmpeg slices buffer pool failed to retain a slot buffer".to_owned()
        })
    }

    fn slot_count(&self) -> u32 {
        u32::try_from(self.slots.len()).unwrap_or(u32::MAX)
    }

    fn allocated_slot_count(&self) -> u32 {
        u32::try_from(self.slots.iter().filter(|buffer| buffer.is_some()).count())
            .unwrap_or(u32::MAX)
    }

    fn total_capacity_bytes(&self) -> u64 {
        self.slots
            .iter()
            .filter_map(|buffer| buffer.as_ref())
            .map(|buffer| buffer.snapshot.size)
            .sum()
    }

    fn max_slot_capacity_bytes(&self) -> u64 {
        self.slots
            .iter()
            .filter_map(|buffer| buffer.as_ref())
            .map(|buffer| buffer.snapshot.size)
            .max()
            .unwrap_or(0)
    }

    fn destroy_all(&mut self, device: &Device) {
        for slot_buffer in &mut self.slots {
            if let Some(buffer) = slot_buffer.take() {
                native_vulkan_vulkanalia_destroy_video_session_bitstream_buffer(device, buffer);
            }
        }
    }
}

fn native_vulkan_vulkanalia_trim_heap_after_decode_teardown() {
    #[cfg(all(
        feature = "native-vulkan-video",
        target_os = "linux",
        target_env = "gnu"
    ))]
    {
        crate::renderer::native_vulkan::native_vulkan_trim_process_heap();
    }
}

fn native_vulkan_vulkanalia_streaming_decode_submit_fence_command_order() -> Vec<&'static str> {
    vec![
        "wait_for_exec_slot_fence_before_command_and_slices_buffer_reuse",
        "reset_slot_fence_before_submit",
        "write_ffmpeg_picture_slices_buffer",
        "reset_command_buffer_after_slot_first_use",
        "queue_submit2_per_frame",
        "defer_submit_fence_wait_until_slot_reuse_or_final_drain",
        "final_wait_for_submitted_slot_fences_before_slices_buffer_pool_teardown",
        "no_queue_wait_idle_after_decode",
    ]
}

fn native_vulkan_vulkanalia_h264_display_order_key(
    entry: &NativeVulkanH264DecodeReferencePlanEntrySnapshot,
    pts_ns: Option<u64>,
    frame_index: u32,
) -> (i64, &'static str) {
    if let Some(pts_ns) = pts_ns {
        (i64::try_from(pts_ns).unwrap_or(i64::MAX), "pts-ns")
    } else if let Some(pts_ms) = entry.pts_ms {
        (i64::try_from(pts_ms).unwrap_or(i64::MAX), "pts-ms")
    } else if let Some(poc) = entry.current_pic_order_cnt_val {
        (i64::from(poc), "h264-pic-order-count")
    } else {
        (i64::from(frame_index), "decode-submit-index")
    }
}

fn native_vulkan_vulkanalia_h265_display_order_key(
    entry: &NativeVulkanH265DecodeReferencePlanEntrySnapshot,
    pts_ns: Option<u64>,
    frame_index: u32,
) -> (i64, &'static str) {
    if let Some(pts_ns) = pts_ns {
        (i64::try_from(pts_ns).unwrap_or(i64::MAX), "pts-ns")
    } else if let Some(pts_ms) = entry.pts_ms {
        (i64::try_from(pts_ms).unwrap_or(i64::MAX), "pts-ms")
    } else if let Some(poc) = entry.current_poc {
        (i64::from(poc), "h265-pic-order-count")
    } else {
        (i64::from(frame_index), "decode-submit-index")
    }
}

fn native_vulkan_vulkanalia_av1_display_order_key(
    entry: &NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
    pts_ns: Option<u64>,
    pts_ms: Option<u64>,
    frame_index: u32,
) -> (i64, &'static str) {
    if let Some(pts_ns) = pts_ns {
        (i64::try_from(pts_ns).unwrap_or(i64::MAX), "pts-ns")
    } else if let Some(pts_ms) = pts_ms {
        (i64::try_from(pts_ms).unwrap_or(i64::MAX), "pts-ms")
    } else {
        let _ = entry;
        (i64::from(frame_index), "display-frame-index")
    }
}

#[derive(Debug)]
pub struct NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
    pub allocate_video_images: bool,
    pub allocate_bitstream_buffer: bool,
    pub create_empty_session_parameters: bool,
    pub create_session_parameters: bool,
    pub h264_parameter_sets: Option<NativeVulkanH264ParameterSetSnapshot>,
    pub h265_parameter_sets: Option<NativeVulkanH265ParameterSetSnapshot>,
    pub av1_sequence_header: Option<NativeVulkanAv1SequenceHeaderSnapshot>,
}

impl Default for NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
    fn default() -> Self {
        Self {
            codec: NativeVulkanVideoSessionCodec::H265Main8,
            width: 3840,
            height: 2160,
            allocate_video_images: false,
            allocate_bitstream_buffer: false,
            create_empty_session_parameters: false,
            create_session_parameters: false,
            h264_parameter_sets: None,
            h265_parameter_sets: None,
            av1_sequence_header: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot {
    pub binding: &'static str,
    pub loader: String,
    pub requested_api_version: String,
    pub requested_codec: NativeVulkanVideoSessionCodec,
    pub requested_extent: (u32, u32),
    pub selected_physical_device_index: usize,
    pub selected_physical_device_name: String,
    pub selected_physical_device_type: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub api_version: String,
    pub driver_version: u32,
    pub selected_queue_family_index: u32,
    pub selected_queue_count: u32,
    pub selected_queue_flags: Vec<&'static str>,
    pub enabled_device_extensions: Vec<&'static str>,
    pub synchronization2_enabled: bool,
    pub dynamic_rendering_enabled: bool,
    pub video_maintenance1_enabled: bool,
    pub video_maintenance2_enabled: bool,
    pub inline_session_parameters_enabled: bool,
    pub inline_session_parameter_codecs: Vec<&'static str>,
    pub ffmpeg_submit_model: &'static str,
    pub video_codec_operation: Vec<&'static str>,
    pub profile: &'static str,
    pub format_probe_profile: &'static str,
    pub picture_format: String,
    pub reference_picture_format: String,
    pub target_picture_dpb_supported: bool,
    pub target_picture_sampled_output_supported: bool,
    pub target_resource_plan: NativeVulkanVulkanaliaVideoSessionResourceProbePlan,
    pub capability_flags: Vec<&'static str>,
    pub decode_capability_flags: Vec<&'static str>,
    pub min_bitstream_buffer_offset_alignment: u64,
    pub min_bitstream_buffer_size_alignment: u64,
    pub picture_access_granularity: (u32, u32),
    pub min_coded_extent: (u32, u32),
    pub max_coded_extent: (u32, u32),
    pub requested_extent_supported: bool,
    pub driver_max_dpb_slots: u32,
    pub driver_max_active_reference_pictures: u32,
    pub session_max_dpb_slots: u32,
    pub session_max_active_reference_pictures: u32,
    pub codec_max_level: Option<&'static str>,
    pub codec_max_level_raw: Option<i32>,
    pub std_header_version_name: String,
    pub std_header_version_spec_version: u32,
    pub memory_binding: NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot,
    pub resource_image_requested: bool,
    pub resource_image: Option<NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot>,
    pub bitstream_buffer_requested: bool,
    pub bitstream_buffer: Option<NativeVulkanVulkanaliaVideoSessionBitstreamBufferSmokeSnapshot>,
    pub session_parameters_requested: bool,
    pub session_parameters: Option<NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot>,
}

pub fn probe_native_vulkan_vulkanalia_video_session_bind(
    options: NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
    let vulkan = native_vulkan_vulkanalia_create_instance()?;
    let result = probe_native_vulkan_vulkanalia_video_session_bind_inner(
        &vulkan.instance,
        vulkan.loader_name,
        options,
    );
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn probe_native_vulkan_vulkanalia_video_session_bind_inner(
    instance: &Instance,
    loader_name: &'static str,
    options: NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
    let selection =
        native_vulkan_vulkanalia_select_video_decode_physical_device(instance, options.codec)?;
    let requested_extent = vk::Extent2D {
        width: options.width,
        height: options.height,
    };
    let h264_parameter_sets = options.h264_parameter_sets.clone();
    let av1_sequence_header = options.av1_sequence_header.clone();
    let picture_format = native_vulkan_vulkanalia_video_session_effective_picture_format(
        options.codec,
        av1_sequence_header.as_ref(),
    );
    let picture_format_label = format!("{picture_format:?}");
    let video_format_capabilities = native_vulkan_vulkanalia_video_format_probe(
        instance,
        selection.physical_device,
        &selection.device_extensions,
        true,
    );
    let format_probe_profile =
        native_vulkan_vulkanalia_video_session_effective_format_probe_profile(
            options.codec,
            h264_parameter_sets.as_ref(),
            av1_sequence_header.as_ref(),
        )?;
    let target_resource_plan =
        native_vulkan_vulkanalia_video_session_resource_plans_from_format_probe(
            &video_format_capabilities,
        )
        .into_iter()
        .find(|plan| {
            plan.codec == vulkanalia_video_session_codec_name(options.codec)
                && plan.profile == format_probe_profile
        })
        .ok_or_else(|| {
            format!(
                "missing Vulkanalia video format resource plan for {} {}",
                vulkanalia_video_session_codec_name(options.codec),
                format_probe_profile
            )
        })?;
    let target_picture_sampled_output_supported = video_format_probe_includes_format(
        &video_format_capabilities.decode_output_sampled_formats,
        vulkanalia_video_session_codec_name(options.codec),
        format_probe_profile,
        &picture_format_label,
    );
    let target_picture_dpb_supported = video_format_probe_includes_format(
        &video_format_capabilities.dpb_formats,
        vulkanalia_video_session_codec_name(options.codec),
        format_probe_profile,
        &picture_format_label,
    );
    if !target_picture_sampled_output_supported || !target_picture_dpb_supported {
        return Err(format!(
            "{} lacks {picture_format_label} decode sampled-output/DPB support in Vulkanalia probe",
            vulkanalia_video_session_label(options.codec),
        ));
    }

    let video_decode_device = native_vulkan_vulkanalia_create_video_decode_device(
        instance,
        selection.physical_device,
        selection.queue_family_index,
        options.codec,
        &selection.device_extensions,
        vulkanalia_video_session_decode_submit_requested(&options),
    )?;

    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };
    let result = with_native_vulkan_vulkanalia_video_session_capabilities(
        instance,
        selection.physical_device,
        options.codec,
        h264_parameter_sets.as_ref(),
        av1_sequence_header.as_ref(),
        |profile_info, queried| {
            smoke_bind_vulkanalia_video_session_profile(
                instance,
                &video_decode_device.device,
                video_decode_device.queue,
                &memory_properties,
                &selection,
                loader_name,
                options,
                requested_extent,
                picture_format,
                target_picture_dpb_supported,
                target_picture_sampled_output_supported,
                target_resource_plan,
                video_decode_device.enabled_device_extensions.clone(),
                video_decode_device.feature_selection,
                profile_info,
                queried,
            )
        },
    );

    native_vulkan_vulkanalia_destroy_video_decode_device(video_decode_device);
    result
}

fn vulkanalia_video_session_decode_submit_requested(
    _options: &NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> bool {
    false
}

fn smoke_bind_vulkanalia_video_session_profile(
    instance: &Instance,
    device: &Device,
    _queue: vk::Queue,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    selection: &NativeVulkanVulkanaliaVideoPhysicalDeviceSelection,
    loader_name: &'static str,
    options: NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
    requested_extent: vk::Extent2D,
    picture_format: vk::Format,
    target_picture_dpb_supported: bool,
    target_picture_sampled_output_supported: bool,
    target_resource_plan: NativeVulkanVulkanaliaVideoSessionResourceProbePlan,
    enabled_device_extensions: Vec<&'static str>,
    feature_selection: NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
    profile_info: &vk::VideoProfileInfoKHR,
    queried: VulkanaliaVideoSessionCapabilityQuery,
) -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
    let capabilities = queried.capabilities;
    let effective_profile_label = native_vulkan_vulkanalia_video_session_effective_profile_label(
        options.codec,
        options.h264_parameter_sets.as_ref(),
        options.av1_sequence_header.as_ref(),
    )?;
    let requested_extent_supported =
        native_vulkan_vulkanalia_video_session_extent_supported(requested_extent, capabilities);
    if !requested_extent_supported {
        return Err(format!(
            "requested Vulkanalia video extent {}x{} is outside ({}, {})..({}, {}) or is not aligned to ({}, {})",
            requested_extent.width,
            requested_extent.height,
            capabilities.min_coded_extent.width,
            capabilities.min_coded_extent.height,
            capabilities.max_coded_extent.width,
            capabilities.max_coded_extent.height,
            capabilities.picture_access_granularity.width,
            capabilities.picture_access_granularity.height,
        ));
    }

    let session_max_dpb_slots =
        native_vulkan_vulkanalia_video_session_max_dpb_slots(capabilities.max_dpb_slots);
    let session_max_active_reference_pictures =
        native_vulkan_vulkanalia_video_session_max_active_reference_pictures(
            capabilities.max_active_reference_pictures,
            session_max_dpb_slots,
        );
    let create_info = vk::VideoSessionCreateInfoKHR::builder()
        .queue_family_index(selection.queue_family_index)
        .video_profile(profile_info)
        .picture_format(picture_format)
        .reference_picture_format(picture_format)
        .max_coded_extent(requested_extent)
        .max_dpb_slots(session_max_dpb_slots)
        .max_active_reference_pictures(session_max_active_reference_pictures)
        .std_header_version(&capabilities.std_header_version)
        .build();
    let session = native_vulkan_vulkanalia_create_video_session(device, &create_info)?;
    let mut memory_resources = None;
    let result = (|| -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
        let resources = native_vulkan_vulkanalia_bind_video_session_memory_resources(
            device,
            memory_properties,
            session,
        )?;
        let memory_binding = resources.snapshot.clone();
        memory_resources = Some(resources);
        let resource_image = if options.allocate_video_images {
            Some(
                native_vulkan_vulkanalia_smoke_create_video_session_resource_image(
                    instance,
                    device,
                    memory_properties,
                    selection.physical_device,
                    profile_info,
                    requested_extent,
                    session_max_dpb_slots.max(1),
                    picture_format,
                    queried.decode_capability_flags,
                    &[selection.queue_family_index],
                )?,
            )
        } else {
            None
        };
        let bitstream_buffer = if options.allocate_bitstream_buffer {
            Some(
                native_vulkan_vulkanalia_smoke_create_video_session_bitstream_buffer(
                    device,
                    memory_properties,
                    profile_info,
                    native_vulkan_vulkanalia_ffmpeg_decode_bitstream_buffer_size(
                        1,
                        capabilities.min_bitstream_buffer_size_alignment,
                    ),
                    capabilities.min_bitstream_buffer_size_alignment,
                    selection.properties.limits.non_coherent_atom_size,
                    None,
                    false,
                )?,
            )
        } else {
            None
        };
        let session_parameters = if options.create_session_parameters {
            Some(match options.codec {
                NativeVulkanVideoSessionCodec::H264High8 => {
                    let parameter_sets = options.h264_parameter_sets.as_ref().ok_or_else(|| {
                        "Vulkanalia real H.264 session parameters require parsed H.264 parameter sets"
                            .to_owned()
                    })?;
                    native_vulkan_vulkanalia_smoke_create_h264_video_session_parameters(
                        device,
                        session,
                        options.codec,
                        parameter_sets,
                    )
                }
                NativeVulkanVideoSessionCodec::H265Main8
                | NativeVulkanVideoSessionCodec::H265Main10 => {
                    let parameter_sets = options.h265_parameter_sets.as_ref().ok_or_else(|| {
                        "Vulkanalia real H.265 session parameters require parsed H.265 parameter sets"
                            .to_owned()
                    })?;
                    native_vulkan_vulkanalia_smoke_create_h265_video_session_parameters(
                        device,
                        session,
                        options.codec,
                        parameter_sets,
                    )
                }
                NativeVulkanVideoSessionCodec::Av1Main8
                | NativeVulkanVideoSessionCodec::Av1Main10 => {
                    let sequence_header = options.av1_sequence_header.as_ref().ok_or_else(|| {
                        "Vulkanalia real AV1 session parameters require parsed AV1 sequence header"
                            .to_owned()
                    })?;
                    native_vulkan_vulkanalia_smoke_create_av1_video_session_parameters(
                        device,
                        session,
                        options.codec,
                        sequence_header,
                    )
                }
            })
        } else if options.create_empty_session_parameters {
            Some(
                native_vulkan_vulkanalia_smoke_create_empty_video_session_parameters(
                    device,
                    session,
                    options.codec,
                ),
            )
        } else {
            None
        };
        Ok(NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot {
            binding: "vulkanalia",
            loader: loader_name.to_owned(),
            requested_api_version: Version::V1_4_0.to_string(),
            requested_codec: options.codec,
            requested_extent: (requested_extent.width, requested_extent.height),
            selected_physical_device_index: selection.physical_device_index,
            selected_physical_device_name: selection
                .properties
                .device_name
                .to_string_lossy()
                .into_owned(),
            selected_physical_device_type: format!("{:?}", selection.properties.device_type),
            vendor_id: selection.properties.vendor_id,
            device_id: selection.properties.device_id,
            api_version: Version::from(selection.properties.api_version).to_string(),
            driver_version: selection.properties.driver_version,
            selected_queue_family_index: selection.queue_family_index,
            selected_queue_count: selection.queue_count,
            selected_queue_flags: queue_flag_labels(selection.queue_flags),
            enabled_device_extensions,
            synchronization2_enabled: feature_selection.synchronization2_enabled,
            dynamic_rendering_enabled: feature_selection.dynamic_rendering_enabled,
            video_maintenance1_enabled: feature_selection.video_maintenance1_enabled,
            video_maintenance2_enabled: feature_selection.video_maintenance2_enabled,
            inline_session_parameters_enabled: feature_selection.inline_session_parameters_enabled,
            inline_session_parameter_codecs: feature_selection.inline_session_parameter_codecs(),
            ffmpeg_submit_model: "references/ffmpeg/libavutil/vulkan.c: VkSubmitInfo2 + QueueSubmit2",
            video_codec_operation: video_codec_operation_labels(
                vulkanalia_video_session_codec_operation(options.codec),
            ),
            profile: effective_profile_label,
            format_probe_profile:
                native_vulkan_vulkanalia_video_session_effective_format_probe_profile(
                    options.codec,
                    options.h264_parameter_sets.as_ref(),
                    options.av1_sequence_header.as_ref(),
                )?,
            picture_format: format!("{picture_format:?}"),
            reference_picture_format: format!("{picture_format:?}"),
            target_picture_dpb_supported,
            target_picture_sampled_output_supported,
            target_resource_plan,
            capability_flags: video_capability_flag_labels(capabilities.flags),
            decode_capability_flags: video_decode_capability_flag_labels(
                queried.decode_capability_flags,
            ),
            min_bitstream_buffer_offset_alignment: capabilities
                .min_bitstream_buffer_offset_alignment,
            min_bitstream_buffer_size_alignment: capabilities.min_bitstream_buffer_size_alignment,
            picture_access_granularity: (
                capabilities.picture_access_granularity.width,
                capabilities.picture_access_granularity.height,
            ),
            min_coded_extent: (
                capabilities.min_coded_extent.width,
                capabilities.min_coded_extent.height,
            ),
            max_coded_extent: (
                capabilities.max_coded_extent.width,
                capabilities.max_coded_extent.height,
            ),
            requested_extent_supported,
            driver_max_dpb_slots: capabilities.max_dpb_slots,
            driver_max_active_reference_pictures: capabilities.max_active_reference_pictures,
            session_max_dpb_slots,
            session_max_active_reference_pictures,
            codec_max_level: queried.codec_max_level,
            codec_max_level_raw: queried.codec_max_level_raw,
            std_header_version_name: capabilities
                .std_header_version
                .extension_name
                .to_string_lossy()
                .into_owned(),
            std_header_version_spec_version: capabilities.std_header_version.spec_version,
            memory_binding,
            resource_image_requested: options.allocate_video_images,
            resource_image,
            bitstream_buffer_requested: options.allocate_bitstream_buffer,
            bitstream_buffer,
            session_parameters_requested: options.create_empty_session_parameters
                || options.create_session_parameters,
            session_parameters,
        })
    })();

    if let Some(resources) = memory_resources.take() {
        native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources(device, resources);
    }
    native_vulkan_vulkanalia_destroy_video_session(device, session);

    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_vulkanalia_record_av1_streaming_decode_into_image(
    device: &Device,
    queue: vk::Queue,
    queue_host_access_lock: Option<&Mutex<()>>,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    queue_family_index: u32,
    profile_info: &vk::VideoProfileInfoKHR,
    extent: vk::Extent2D,
    capabilities: vk::VideoCapabilitiesKHR,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    array_layers: u32,
    exec_ring_depth: u32,
    non_coherent_atom_size: u64,
    input: NativeVulkanVulkanaliaAv1StreamingDecodeInput<'_>,
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    mut before_output_slot_reuse: Option<NativeVulkanVulkanaliaBeforeOutputSlotReuse<'_>>,
    mut after_frame_submitted: Option<NativeVulkanVulkanaliaAfterFrameSubmitted<'_>>,
    decode_complete_semaphore: vk::Semaphore,
    decode_complete_value: &std::cell::Cell<u64>,
) -> Result<NativeVulkanVulkanaliaAv1CommandSmokeSnapshot, String> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) {
        return Err("Vulkanalia AV1 streaming decode requires an AV1 codec".into());
    }
    let requested_frame_count = input.requested_frame_count;
    if requested_frame_count == 0 {
        return Err("Vulkanalia AV1 streaming decode requires at least one frame".to_owned());
    }
    let array_layers = array_layers.max(1);
    let exec_ring_depth = exec_ring_depth.max(1);
    let sequence_header = input.sequence_header;
    let session_parameters = native_vulkan_vulkanalia_create_av1_video_session_parameters(
        device,
        session,
        codec,
        &sequence_header,
    )?;
    let mut session_parameters = Some(session_parameters);
    let command_buffer = native_vulkan_vulkanalia_create_decode_command_buffers(
        device,
        queue_family_index,
        exec_ring_depth,
    )?;
    let mut command_buffer = Some(command_buffer);
    let mut submit_ring =
        NativeVulkanVulkanaliaStreamingDecodeSubmitRing::new(exec_ring_depth as usize);
    let mut bitstream_buffers =
        NativeVulkanVulkanaliaFfmpegSlicesBufferPool::new(exec_ring_depth as usize);

    let result = (|| -> Result<NativeVulkanVulkanaliaAv1CommandSmokeSnapshot, String> {
        let session_parameters_ref = session_parameters
            .as_ref()
            .expect("Vulkanalia AV1 session parameters are alive during streaming decode");
        let command_buffer_ref = command_buffer
            .as_ref()
            .expect("Vulkanalia streaming command buffer is alive during AV1 decode");
        let mut initialized_slots = vec![false; array_layers as usize];
        let mut layer_decode_complete_values = vec![0u64; array_layers as usize];
        let mut frame_telemetry = NativeVulkanVulkanaliaDecodeFrameTelemetry::new();
        let mut last_tile_offsets = Vec::new();
        let mut last_tile_sizes = Vec::new();
        let mut av1_reference_infos = Vec::<
            super::video_decode_submit_av1::NativeVulkanVulkanaliaAv1ReferenceInfoPlan,
        >::new();
        let mut command_buffer_recorded = true;
        let mut submitted = true;
        let mut uses_synchronization2 = true;
        let mut uses_submit2 = true;
        let mut ffmpeg_reference = "references/ffmpeg/libavcodec/vulkan_decode.c";
        let mut src_buffer_total_bytes = 0u64;
        let mut displayed_frame_count = 0u32;
        let mut show_existing_frame_count = 0u32;
        let mut hidden_frame_count = 0u32;
        let decode_loop_started_at = Instant::now();
        let mut streaming_decode_timing = NativeVulkanVulkanaliaStreamingDecodeTiming::default();

        while displayed_frame_count < requested_frame_count {
            let mut frame_timing = NativeVulkanVulkanaliaStreamingDecodeFrameTiming::default();
            let stage_started_at = Instant::now();
            let mut frame = (input.next_frame)()?;
            frame_timing.next_frame_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            let (display_order_key, display_order_key_source) =
                native_vulkan_vulkanalia_av1_display_order_key(
                    &frame.entry,
                    frame.pts_ns,
                    frame.pts_ms,
                    displayed_frame_count,
                );

            if frame.entry.ready_for_display_handoff {
                let sampled_array_layer = frame.entry.displayed_slot.ok_or_else(|| {
                    format!(
                        "Vulkanalia AV1 TU {} display handoff has no displayed DPB slot",
                        frame.entry.temporal_unit_index
                    )
                })?;
                if sampled_array_layer >= array_layers {
                    return Err(format!(
                        "Vulkanalia AV1 display handoff slot {sampled_array_layer} exceeds image layers {array_layers}"
                    ));
                }
                let decode_complete_value_for_frame =
                    layer_decode_complete_values[sampled_array_layer as usize];
                if decode_complete_value_for_frame == 0 {
                    return Err(format!(
                        "Vulkanalia AV1 TU {} show_existing_frame references layer {sampled_array_layer} before any decode completed there",
                        frame.entry.temporal_unit_index
                    ));
                }
                if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                    let stage_started_at = Instant::now();
                    after_frame_submitted(
                        displayed_frame_count,
                        sampled_array_layer,
                        frame.pts_ns,
                        frame.duration_ns,
                        frame.pts_ms,
                        frame.duration_ms,
                        display_order_key,
                        display_order_key_source,
                        decode_complete_value_for_frame,
                    )?;
                    frame_timing.after_frame_submitted_micros =
                        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                }
                displayed_frame_count = displayed_frame_count.saturating_add(1);
                show_existing_frame_count = show_existing_frame_count.saturating_add(1);
                streaming_decode_timing.push(frame_timing);
                continue;
            }

            let submit_frame = frame.frame.take().ok_or_else(|| {
                format!(
                    "Vulkanalia AV1 TU {} has no decode payload and is not a display handoff",
                    frame.entry.temporal_unit_index
                )
            })?;
            let submit_frame_show_frame = submit_frame.show_frame;
            let reset_control_recorded = submit_frame.frame_type == 0;
            let output_slot = frame.entry.output_slot.ok_or_else(|| {
                format!(
                    "Vulkanalia AV1 TU {} has no planned output slot",
                    frame.entry.temporal_unit_index
                )
            })?;
            if output_slot >= array_layers {
                return Err(format!(
                    "Vulkanalia AV1 streaming planned output slot {output_slot} exceeds image layers {array_layers}"
                ));
            }
            for slot in &frame.entry.decode_reference_slots {
                if let Ok(slot) = u32::try_from(*slot)
                    && slot >= array_layers
                {
                    return Err(format!(
                        "Vulkanalia AV1 streaming reference slot {slot} exceeds image layers {array_layers}"
                    ));
                }
            }

            let submit_slot =
                submit_ring.exec_slot_for_frame(frame_telemetry.submitted_frame_count);
            frame_timing.exec_slot_reuse_wait_micros =
                submit_ring.wait_for_slot_reuse(device, command_buffer_ref, submit_slot)?;
            if let Some(before_output_slot_reuse) = before_output_slot_reuse.as_deref_mut() {
                let stage_started_at = Instant::now();
                before_output_slot_reuse(output_slot)?;
                frame_timing.output_slot_reuse_wait_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            }
            let payload_len = frame.access_unit_payload.len() as u64;
            let stage_started_at = Instant::now();
            let bitstream_buffer_ref = bitstream_buffers.buffer_for_payload(
                device,
                memory_properties,
                profile_info,
                submit_slot,
                payload_len,
                capabilities.min_bitstream_buffer_size_alignment,
                non_coherent_atom_size,
            )?;
            frame_timing.bitstream_buffer_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            let stage_started_at = Instant::now();
            let (src_buffer_offset, src_buffer_range) =
                native_vulkan_vulkanalia_write_ffmpeg_picture_slices_buffer(
                    device,
                    bitstream_buffer_ref,
                    frame.access_unit_payload.bytes(),
                    capabilities.min_bitstream_buffer_size_alignment,
                    non_coherent_atom_size,
                )?;
            frame_timing.payload_write_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            frame.access_unit_payload.clear();
            src_buffer_total_bytes = src_buffer_total_bytes.saturating_add(payload_len);

            let stage_started_at = Instant::now();
            let plan = native_vulkan_vulkanalia_av1_decode_submit_plan(
                extent,
                codec,
                &frame.entry,
                submit_frame,
                src_buffer_offset,
                src_buffer_range,
                reset_control_recorded,
                &mut av1_reference_infos,
            )?;
            frame_timing.decode_plan_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            ffmpeg_reference = plan.common.ffmpeg_reference;
            let stage_started_at = Instant::now();
            let image_views =
                native_vulkan_vulkanalia_av1_decode_image_view_bindings(image, &plan)?;
            frame_timing.image_view_bind_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            let dst_slot = plan.common.dst_picture_resource.base_array_layer as usize;
            let transition_dst_from_undefined = !initialized_slots[dst_slot];
            let decode_command_buffer = command_buffer_ref.command_buffer_at(submit_slot)?;
            let stage_started_at = Instant::now();
            let record_plan = unsafe {
                native_vulkan_vulkanalia_record_av1_decode_command_buffer(
                    device,
                    decode_command_buffer,
                    image.image,
                    &plan,
                    session,
                    session_parameters_ref.parameters,
                    bitstream_buffer_ref.buffer,
                    &image_views,
                    submit_ring.reset_command_buffer_before_record(submit_slot)?,
                    transition_dst_from_undefined,
                )
            }?;
            submit_ring.mark_recorded(submit_slot)?;
            frame_timing.record_command_buffer_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            let decode_complete_value_for_frame = decode_complete_value.get() + 1;
            decode_complete_value.set(decode_complete_value_for_frame);
            let stage_started_at = Instant::now();
            let queue_host_access_guard = if let Some(lock) = queue_host_access_lock {
                Some(lock.lock().map_err(|_| {
                    "Vulkanalia AV1 decode queue host-access lock is poisoned".to_owned()
                })?)
            } else {
                None
            };
            let submit_plan = unsafe {
                native_vulkan_vulkanalia_submit_decode_command_buffer2(
                    device,
                    queue,
                    decode_command_buffer,
                    command_buffer_ref.submit_fence_at(submit_slot)?,
                    false,
                    false,
                    decode_complete_semaphore,
                    decode_complete_value_for_frame,
                )
            }?;
            drop(queue_host_access_guard);
            submit_ring.mark_submitted(submit_slot)?;
            frame_timing.submit_wait_micros =
                native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
            initialized_slots[dst_slot] = true;
            layer_decode_complete_values[dst_slot] = decode_complete_value_for_frame;
            command_buffer_recorded &= record_plan.command_order.contains(&"vkEndCommandBuffer");
            submitted &= submit_plan.command_order.contains(&"queue_submit2");
            uses_synchronization2 &= record_plan.uses_synchronization2;
            uses_submit2 &= submit_plan.uses_submit2;

            if submit_frame_show_frame {
                let sampled_array_layer = frame.entry.displayed_slot.ok_or_else(|| {
                    format!(
                        "Vulkanalia AV1 TU {} is show_frame but has no displayed slot",
                        frame.entry.temporal_unit_index
                    )
                })?;
                if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                    let stage_started_at = Instant::now();
                    after_frame_submitted(
                        displayed_frame_count,
                        sampled_array_layer,
                        frame.pts_ns,
                        frame.duration_ns,
                        frame.pts_ms,
                        frame.duration_ms,
                        display_order_key,
                        display_order_key_source,
                        decode_complete_value_for_frame,
                    )?;
                    frame_timing.after_frame_submitted_micros =
                        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                }
                displayed_frame_count = displayed_frame_count.saturating_add(1);
            } else {
                hidden_frame_count = hidden_frame_count.saturating_add(1);
            }

            let begin_reference_slot_count = plan.common.begin_reference_slot_count as u32;
            let decode_reference_slot_count = plan.common.decode_reference_slot_count as u32;
            last_tile_offsets = plan.picture.tile_offsets;
            last_tile_sizes = plan.picture.tile_sizes;
            frame_telemetry.push(NativeVulkanVulkanaliaDecodeFrameLastFields {
                src_buffer_offset: plan.common.src_buffer_offset,
                src_buffer_range: plan.common.src_buffer_range,
                dst_base_array_layer: plan.common.dst_picture_resource.base_array_layer,
                setup_slot_index: plan.common.setup_reference_slot.slot_index,
                begin_reference_slot_count,
                decode_reference_slot_count,
                reset_control_recorded,
            });
            streaming_decode_timing.push(frame_timing);
        }
        let last_frame =
            frame_telemetry.last_frame("Vulkanalia AV1 streaming submitted no decode frames")?;
        let final_drain_wait_micros = submit_ring.wait_all_submitted(device, command_buffer_ref)?;
        let streaming_decode_timing = streaming_decode_timing.finish(
            native_vulkan_vulkanalia_elapsed_micros(decode_loop_started_at),
            final_drain_wait_micros,
        );

        Ok(NativeVulkanVulkanaliaAv1CommandSmokeSnapshot {
            requested_frame_count,
            recorded_frame_count: frame_telemetry.submitted_frame_count,
            submitted_frame_count: frame_telemetry.submitted_frame_count,
            displayed_frame_count,
            show_existing_frame_count,
            hidden_frame_count,
            ffmpeg_reference,
            command_buffer_recorded,
            submitted,
            uses_synchronization2,
            uses_submit2,
            wait_idle_after_submit: false,
            wait_fence_after_submit: false,
            batch_wait_fence_after_submit: true,
            uses_submit_fence: true,
            submit_sync_model: NATIVE_VULKAN_VULKANALIA_STREAMING_DECODE_SUBMIT_FENCE_SYNC_MODEL,
            submit_command_order:
                native_vulkan_vulkanalia_streaming_decode_submit_fence_command_order(),
            queue_family_index,
            bitstream_buffer_model: "ffmpeg-picture-slices-buffer-pool-exec-owned",
            ffmpeg_slices_buffer_pool_slot_count: bitstream_buffers.slot_count(),
            ffmpeg_slices_buffer_pool_allocated_slot_count: bitstream_buffers
                .allocated_slot_count(),
            ffmpeg_slices_buffer_pool_capacity_bytes: bitstream_buffers.total_capacity_bytes(),
            ffmpeg_slices_buffer_pool_max_slot_bytes: bitstream_buffers.max_slot_capacity_bytes(),
            input_payload_model: "bounded-streaming-packet-queue-per-frame-upload",
            src_buffer_total_bytes,
            streaming_decode_timing,
            retained_frame_telemetry_limit:
                NATIVE_VULKAN_VULKANALIA_DECODE_FRAME_TELEMETRY_RETAINED_FRAMES,
            retained_frame_telemetry_count: frame_telemetry.retained_frame_count(),
            frame_telemetry_retention_model:
                NATIVE_VULKAN_VULKANALIA_DECODE_FRAME_TELEMETRY_RETENTION_MODEL,
            max_src_buffer_range: frame_telemetry.max_src_buffer_range,
            first_frame_reset_control_recorded: frame_telemetry
                .first_frame_reset_control_recorded
                .unwrap_or(false),
            reset_control_recorded_frame_count: frame_telemetry.reset_control_recorded_frame_count,
            p_frame_count: frame_telemetry.p_frame_count,
            b_frame_count: frame_telemetry.b_frame_count,
            max_begin_reference_slot_count: frame_telemetry.max_begin_reference_slot_count,
            max_decode_reference_slot_count: frame_telemetry.max_decode_reference_slot_count,
            src_buffer_offset: last_frame.src_buffer_offset,
            src_buffer_range: last_frame.src_buffer_range,
            dst_base_array_layer: last_frame.dst_base_array_layer,
            setup_slot_index: last_frame.setup_slot_index,
            begin_reference_slot_count: last_frame.begin_reference_slot_count,
            decode_reference_slot_count: last_frame.decode_reference_slot_count,
            reset_control_recorded: last_frame.reset_control_recorded,
            tile_count: last_tile_offsets.len() as u32,
            tile_offsets: last_tile_offsets,
            tile_sizes: last_tile_sizes,
            frames: Vec::new(),
        })
    })();

    if result.is_err()
        && let Some(command_buffer_ref) = command_buffer.as_ref()
    {
        let _ = submit_ring.wait_all_submitted(device, command_buffer_ref);
    }
    bitstream_buffers.destroy_all(device);
    if let Some(command_buffer) = command_buffer.take() {
        native_vulkan_vulkanalia_destroy_decode_command_buffer(device, command_buffer);
    }
    if let Some(session_parameters) = session_parameters.take() {
        native_vulkan_vulkanalia_destroy_video_session_parameters(device, session_parameters);
    }
    native_vulkan_vulkanalia_trim_heap_after_decode_teardown();

    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_vulkanalia_record_h265_streaming_decode_into_image(
    device: &Device,
    queue: vk::Queue,
    queue_host_access_lock: Option<&Mutex<()>>,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    queue_family_index: u32,
    profile_info: &vk::VideoProfileInfoKHR,
    extent: vk::Extent2D,
    capabilities: vk::VideoCapabilitiesKHR,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    array_layers: u32,
    exec_ring_depth: u32,
    non_coherent_atom_size: u64,
    input: NativeVulkanVulkanaliaH265StreamingDecodeInput<'_>,
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    mut before_output_slot_reuse: Option<NativeVulkanVulkanaliaBeforeOutputSlotReuse<'_>>,
    mut after_frame_submitted: Option<NativeVulkanVulkanaliaAfterFrameSubmitted<'_>>,
    decode_complete_semaphore: vk::Semaphore,
    decode_complete_value: &std::cell::Cell<u64>,
) -> Result<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot, String> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    ) {
        return Err("Vulkanalia H.265 streaming decode requires an H.265 codec".into());
    }
    let requested_frame_count = input.requested_frame_count;
    if requested_frame_count == 0 {
        return Err("Vulkanalia H.265 streaming decode requires at least one frame".to_owned());
    }
    let array_layers = array_layers.max(1);
    let exec_ring_depth = exec_ring_depth.max(1);
    let parameter_sets = input.parameter_sets;
    let session_parameters = native_vulkan_vulkanalia_create_h265_video_session_parameters(
        device,
        session,
        codec,
        &parameter_sets,
    )?;
    let mut session_parameters = Some(session_parameters);
    let command_buffer = native_vulkan_vulkanalia_create_decode_command_buffers(
        device,
        queue_family_index,
        exec_ring_depth,
    )?;
    let mut command_buffer = Some(command_buffer);
    let mut submit_ring =
        NativeVulkanVulkanaliaStreamingDecodeSubmitRing::new(exec_ring_depth as usize);
    let mut bitstream_buffers =
        NativeVulkanVulkanaliaFfmpegSlicesBufferPool::new(exec_ring_depth as usize);

    let result =
        (|| -> Result<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot, String> {
            let parameter_ids =
                NativeVulkanVulkanaliaH265ParameterIds::from_parameter_sets(&parameter_sets)?;
            let session_parameters_ref = session_parameters
                .as_ref()
                .expect("Vulkanalia H.265 session parameters are alive during streaming decode");
            let command_buffer_ref = command_buffer
                .as_ref()
                .expect("Vulkanalia streaming command buffer is alive during decode");
            let mut initialized_slots = vec![false; array_layers as usize];
            let mut frame_telemetry = NativeVulkanVulkanaliaDecodeFrameTelemetry::new();
            let mut last_slice_segment_offsets = Vec::new();
            let mut h265_reference_infos = Vec::<
                super::video_decode_submit_h265::NativeVulkanVulkanaliaH265ReferenceInfoPlan,
            >::new();
            let mut command_buffer_recorded = true;
            let mut submitted = true;
            let mut uses_synchronization2 = true;
            let mut uses_submit2 = true;
            let mut ffmpeg_reference = "references/ffmpeg/libavcodec/vulkan_decode.c";
            let mut src_buffer_total_bytes = 0u64;
            let decode_loop_started_at = Instant::now();
            let mut streaming_decode_timing =
                NativeVulkanVulkanaliaStreamingDecodeTiming::default();

            for frame_index in 0..requested_frame_count {
                let mut frame_timing = NativeVulkanVulkanaliaStreamingDecodeFrameTiming::default();
                let submit_slot = submit_ring.exec_slot_for_frame(frame_index);
                frame_timing.exec_slot_reuse_wait_micros =
                    submit_ring.wait_for_slot_reuse(device, command_buffer_ref, submit_slot)?;
                let stage_started_at = Instant::now();
                let mut frame = (input.next_frame)()?;
                frame_timing.next_frame_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                if frame.entry.planned_output_slot >= array_layers {
                    return Err(format!(
                        "Vulkanalia H.265 streaming planned output slot {} exceeds image layers {array_layers}",
                        frame.entry.planned_output_slot
                    ));
                }
                for reference in &frame.entry.references {
                    if let Some(dpb_slot) = reference.dpb_slot
                        && dpb_slot >= array_layers
                    {
                        return Err(format!(
                            "Vulkanalia H.265 streaming reference slot {dpb_slot} exceeds image layers {array_layers}"
                        ));
                    }
                }
                if let Some(before_output_slot_reuse) = before_output_slot_reuse.as_deref_mut() {
                    let stage_started_at = Instant::now();
                    before_output_slot_reuse(frame.entry.planned_output_slot)?;
                    frame_timing.output_slot_reuse_wait_micros =
                        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                }
                let payload_len = frame.access_unit_payload.len() as u64;
                let stage_started_at = Instant::now();
                let bitstream_buffer_ref = bitstream_buffers.buffer_for_payload(
                    device,
                    memory_properties,
                    profile_info,
                    submit_slot,
                    payload_len,
                    capabilities.min_bitstream_buffer_size_alignment,
                    non_coherent_atom_size,
                )?;
                frame_timing.bitstream_buffer_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                let stage_started_at = Instant::now();
                let (src_buffer_offset, src_buffer_range) =
                    native_vulkan_vulkanalia_write_ffmpeg_picture_slices_buffer(
                        device,
                        bitstream_buffer_ref,
                        frame.access_unit_payload.bytes(),
                        capabilities.min_bitstream_buffer_size_alignment,
                        non_coherent_atom_size,
                    )?;
                frame_timing.payload_write_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                frame.access_unit_payload.clear();
                src_buffer_total_bytes = src_buffer_total_bytes.saturating_add(payload_len);

                let reset_control_recorded = frame.first_slice.idr || frame.first_slice.irap;
                let stage_started_at = Instant::now();
                let slice_segment_offsets = [frame.slice_segment_offset];
                let plan = native_vulkan_vulkanalia_h265_ready_prefix_decode_submit_plan(
                    extent,
                    parameter_ids,
                    &frame.entry,
                    &frame.first_slice,
                    src_buffer_offset,
                    src_buffer_range,
                    &slice_segment_offsets,
                    reset_control_recorded,
                    &mut h265_reference_infos,
                )?;
                frame_timing.decode_plan_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                ffmpeg_reference = plan.common.ffmpeg_reference;
                let stage_started_at = Instant::now();
                let image_views =
                    native_vulkan_vulkanalia_h265_decode_image_view_bindings(image, &plan)?;
                frame_timing.image_view_bind_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                let dst_slot = plan.common.dst_picture_resource.base_array_layer as usize;
                let transition_dst_from_undefined = !initialized_slots[dst_slot];
                let decode_command_buffer = command_buffer_ref.command_buffer_at(submit_slot)?;
                let stage_started_at = Instant::now();
                let record_plan = unsafe {
                    native_vulkan_vulkanalia_record_h265_decode_command_buffer(
                        device,
                        decode_command_buffer,
                        image.image,
                        &plan,
                        session,
                        session_parameters_ref.parameters,
                        bitstream_buffer_ref.buffer,
                        &image_views,
                        submit_ring.reset_command_buffer_before_record(submit_slot)?,
                        transition_dst_from_undefined,
                    )
                }?;
                submit_ring.mark_recorded(submit_slot)?;
                frame_timing.record_command_buffer_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                let decode_complete_value_for_frame = decode_complete_value.get() + 1;
                decode_complete_value.set(decode_complete_value_for_frame);
                let stage_started_at = Instant::now();
                let queue_host_access_guard = if let Some(lock) = queue_host_access_lock {
                    Some(lock.lock().map_err(|_| {
                        "Vulkanalia H.265 decode queue host-access lock is poisoned".to_owned()
                    })?)
                } else {
                    None
                };
                let submit_plan = unsafe {
                    native_vulkan_vulkanalia_submit_decode_command_buffer2(
                        device,
                        queue,
                        decode_command_buffer,
                        command_buffer_ref.submit_fence_at(submit_slot)?,
                        false,
                        false,
                        decode_complete_semaphore,
                        decode_complete_value_for_frame,
                    )
                }?;
                drop(queue_host_access_guard);
                submit_ring.mark_submitted(submit_slot)?;
                frame_timing.submit_wait_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                initialized_slots[dst_slot] = true;
                command_buffer_recorded &=
                    record_plan.command_order.contains(&"vkEndCommandBuffer");
                submitted &= submit_plan.command_order.contains(&"queue_submit2");
                uses_synchronization2 &= record_plan.uses_synchronization2;
                uses_submit2 &= submit_plan.uses_submit2;
                let (display_order_key, display_order_key_source) =
                    native_vulkan_vulkanalia_h265_display_order_key(
                        &frame.entry,
                        frame.pts_ns,
                        frame_index,
                    );

                if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                    let stage_started_at = Instant::now();
                    after_frame_submitted(
                        frame_index,
                        plan.common.dst_picture_resource.base_array_layer,
                        frame.pts_ns,
                        frame.duration_ns,
                        frame.entry.pts_ms,
                        frame.duration_ms,
                        display_order_key,
                        display_order_key_source,
                        decode_complete_value_for_frame,
                    )?;
                    frame_timing.after_frame_submitted_micros =
                        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                }

                let begin_reference_slot_count = plan.common.begin_reference_slot_count as u32;
                let decode_reference_slot_count = plan.common.decode_reference_slot_count as u32;
                last_slice_segment_offsets.clear();
                last_slice_segment_offsets.extend_from_slice(plan.picture.slice_segment_offsets);
                frame_telemetry.push(NativeVulkanVulkanaliaDecodeFrameLastFields {
                    src_buffer_offset: plan.common.src_buffer_offset,
                    src_buffer_range: plan.common.src_buffer_range,
                    dst_base_array_layer: plan.common.dst_picture_resource.base_array_layer,
                    setup_slot_index: plan.common.setup_reference_slot.slot_index,
                    begin_reference_slot_count,
                    decode_reference_slot_count,
                    reset_control_recorded,
                });
                streaming_decode_timing.push(frame_timing);
            }
            let last_frame =
                frame_telemetry.last_frame("Vulkanalia H.265 streaming submitted no frames")?;
            let final_drain_wait_micros =
                submit_ring.wait_all_submitted(device, command_buffer_ref)?;
            let streaming_decode_timing = streaming_decode_timing.finish(
                native_vulkan_vulkanalia_elapsed_micros(decode_loop_started_at),
                final_drain_wait_micros,
            );

            Ok(NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot {
                requested_frame_count,
                recorded_frame_count: frame_telemetry.submitted_frame_count,
                submitted_frame_count: frame_telemetry.submitted_frame_count,
                ffmpeg_reference,
                command_buffer_recorded,
                submitted,
                uses_synchronization2,
                uses_submit2,
                wait_idle_after_submit: false,
                wait_fence_after_submit: false,
                batch_wait_fence_after_submit: true,
                uses_submit_fence: true,
                submit_sync_model:
                    NATIVE_VULKAN_VULKANALIA_STREAMING_DECODE_SUBMIT_FENCE_SYNC_MODEL,
                submit_command_order:
                    native_vulkan_vulkanalia_streaming_decode_submit_fence_command_order(),
                queue_family_index,
                bitstream_buffer_model: "ffmpeg-picture-slices-buffer-pool-exec-owned",
                ffmpeg_slices_buffer_pool_slot_count: bitstream_buffers.slot_count(),
                ffmpeg_slices_buffer_pool_allocated_slot_count: bitstream_buffers
                    .allocated_slot_count(),
                ffmpeg_slices_buffer_pool_capacity_bytes: bitstream_buffers.total_capacity_bytes(),
                ffmpeg_slices_buffer_pool_max_slot_bytes: bitstream_buffers
                    .max_slot_capacity_bytes(),
                input_payload_model: "bounded-streaming-packet-queue-per-frame-upload",
                src_buffer_total_bytes,
                streaming_decode_timing,
                retained_frame_telemetry_limit:
                    NATIVE_VULKAN_VULKANALIA_DECODE_FRAME_TELEMETRY_RETAINED_FRAMES,
                retained_frame_telemetry_count: frame_telemetry.retained_frame_count(),
                frame_telemetry_retention_model:
                    NATIVE_VULKAN_VULKANALIA_DECODE_FRAME_TELEMETRY_RETENTION_MODEL,
                max_src_buffer_range: frame_telemetry.max_src_buffer_range,
                first_frame_reset_control_recorded: frame_telemetry
                    .first_frame_reset_control_recorded
                    .unwrap_or(false),
                reset_control_recorded_frame_count: frame_telemetry
                    .reset_control_recorded_frame_count,
                p_frame_count: frame_telemetry.p_frame_count,
                b_frame_count: frame_telemetry.b_frame_count,
                max_begin_reference_slot_count: frame_telemetry.max_begin_reference_slot_count,
                max_decode_reference_slot_count: frame_telemetry.max_decode_reference_slot_count,
                src_buffer_offset: last_frame.src_buffer_offset,
                src_buffer_range: last_frame.src_buffer_range,
                dst_base_array_layer: last_frame.dst_base_array_layer,
                setup_slot_index: last_frame.setup_slot_index,
                begin_reference_slot_count: last_frame.begin_reference_slot_count,
                decode_reference_slot_count: last_frame.decode_reference_slot_count,
                reset_control_recorded: last_frame.reset_control_recorded,
                slice_segment_count: last_slice_segment_offsets.len() as u32,
                slice_segment_offsets: last_slice_segment_offsets,
                frames: Vec::new(),
            })
        })();

    if result.is_err()
        && let Some(command_buffer_ref) = command_buffer.as_ref()
    {
        let _ = submit_ring.wait_all_submitted(device, command_buffer_ref);
    }
    bitstream_buffers.destroy_all(device);
    if let Some(command_buffer) = command_buffer.take() {
        native_vulkan_vulkanalia_destroy_decode_command_buffer(device, command_buffer);
    }
    if let Some(session_parameters) = session_parameters.take() {
        native_vulkan_vulkanalia_destroy_video_session_parameters(device, session_parameters);
    }
    native_vulkan_vulkanalia_trim_heap_after_decode_teardown();

    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_vulkanalia_record_h264_streaming_decode_into_image(
    device: &Device,
    queue: vk::Queue,
    queue_host_access_lock: Option<&Mutex<()>>,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    queue_family_index: u32,
    profile_info: &vk::VideoProfileInfoKHR,
    extent: vk::Extent2D,
    capabilities: vk::VideoCapabilitiesKHR,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    array_layers: u32,
    exec_ring_depth: u32,
    non_coherent_atom_size: u64,
    input: NativeVulkanVulkanaliaH264StreamingDecodeInput<'_>,
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    mut before_output_slot_reuse: Option<NativeVulkanVulkanaliaBeforeOutputSlotReuse<'_>>,
    mut after_frame_submitted: Option<NativeVulkanVulkanaliaAfterFrameSubmitted<'_>>,
    decode_complete_semaphore: vk::Semaphore,
    decode_complete_value: &std::cell::Cell<u64>,
) -> Result<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot, String> {
    if codec != NativeVulkanVideoSessionCodec::H264High8 {
        return Err("Vulkanalia H.264 streaming decode requires H.264 high-8".into());
    }
    let requested_frame_count = input.requested_frame_count;
    if requested_frame_count == 0 {
        return Err("Vulkanalia H.264 streaming decode requires at least one frame".to_owned());
    }
    let array_layers = array_layers.max(1);
    let exec_ring_depth = exec_ring_depth.max(1);
    let parameter_sets = input.parameter_sets;
    let session_parameters = native_vulkan_vulkanalia_create_h264_video_session_parameters(
        device,
        session,
        codec,
        &parameter_sets,
    )?;
    let mut session_parameters = Some(session_parameters);
    let command_buffer = native_vulkan_vulkanalia_create_decode_command_buffers(
        device,
        queue_family_index,
        exec_ring_depth,
    )?;
    let mut command_buffer = Some(command_buffer);
    let mut submit_ring =
        NativeVulkanVulkanaliaStreamingDecodeSubmitRing::new(exec_ring_depth as usize);
    let mut bitstream_buffers =
        NativeVulkanVulkanaliaFfmpegSlicesBufferPool::new(exec_ring_depth as usize);

    let result =
        (|| -> Result<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot, String> {
            let parameter_ids =
                NativeVulkanVulkanaliaH264ParameterIds::from_parameter_sets(&parameter_sets)?;
            let session_parameters_ref = session_parameters
                .as_ref()
                .expect("Vulkanalia H.264 session parameters are alive during streaming decode");
            let command_buffer_ref = command_buffer
                .as_ref()
                .expect("Vulkanalia streaming command buffer is alive during decode");
            let mut initialized_slots = vec![false; array_layers as usize];
            let mut frame_telemetry = NativeVulkanVulkanaliaDecodeFrameTelemetry::new();
            let mut last_slice_segment_offsets = Vec::new();
            let mut h264_reference_infos = Vec::<
                super::video_decode_submit_h264::NativeVulkanVulkanaliaH264ReferenceInfoPlan,
            >::new();
            let mut command_buffer_recorded = true;
            let mut submitted = true;
            let mut uses_synchronization2 = true;
            let mut uses_submit2 = true;
            let mut ffmpeg_reference = "references/ffmpeg/libavcodec/vulkan_decode.c";
            let mut src_buffer_total_bytes = 0u64;
            let decode_loop_started_at = Instant::now();
            let mut streaming_decode_timing =
                NativeVulkanVulkanaliaStreamingDecodeTiming::default();

            for frame_index in 0..requested_frame_count {
                let mut frame_timing = NativeVulkanVulkanaliaStreamingDecodeFrameTiming::default();
                let submit_slot = submit_ring.exec_slot_for_frame(frame_index);
                frame_timing.exec_slot_reuse_wait_micros =
                    submit_ring.wait_for_slot_reuse(device, command_buffer_ref, submit_slot)?;
                let stage_started_at = Instant::now();
                let mut frame = (input.next_frame)()?;
                frame_timing.next_frame_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                if frame.entry.planned_output_slot >= array_layers {
                    return Err(format!(
                        "Vulkanalia H.264 streaming planned output slot {} exceeds image layers {array_layers}",
                        frame.entry.planned_output_slot
                    ));
                }
                for reference in &frame.entry.references {
                    if let Some(dpb_slot) = reference.dpb_slot
                        && dpb_slot >= array_layers
                    {
                        return Err(format!(
                            "Vulkanalia H.264 streaming reference slot {dpb_slot} exceeds image layers {array_layers}"
                        ));
                    }
                }
                if frame.first_slice.slice_offsets.is_empty() {
                    return Err(format!(
                        "Vulkanalia H.264 streaming AU {} has no slice offsets",
                        frame.entry.access_unit_index
                    ));
                }
                if let Some(before_output_slot_reuse) = before_output_slot_reuse.as_deref_mut() {
                    let stage_started_at = Instant::now();
                    before_output_slot_reuse(frame.entry.planned_output_slot)?;
                    frame_timing.output_slot_reuse_wait_micros =
                        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                }
                let payload_len = frame.access_unit_payload.len() as u64;
                let stage_started_at = Instant::now();
                let bitstream_buffer_ref = bitstream_buffers.buffer_for_payload(
                    device,
                    memory_properties,
                    profile_info,
                    submit_slot,
                    payload_len,
                    capabilities.min_bitstream_buffer_size_alignment,
                    non_coherent_atom_size,
                )?;
                frame_timing.bitstream_buffer_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                let stage_started_at = Instant::now();
                let (src_buffer_offset, src_buffer_range) =
                    native_vulkan_vulkanalia_write_ffmpeg_picture_slices_buffer(
                        device,
                        bitstream_buffer_ref,
                        frame.access_unit_payload.bytes(),
                        capabilities.min_bitstream_buffer_size_alignment,
                        non_coherent_atom_size,
                    )?;
                frame_timing.payload_write_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                frame.access_unit_payload.clear();
                src_buffer_total_bytes = src_buffer_total_bytes.saturating_add(payload_len);

                let reset_control_recorded = frame.first_slice.idr;
                let stage_started_at = Instant::now();
                let plan = native_vulkan_vulkanalia_h264_ready_prefix_decode_submit_plan(
                    extent,
                    parameter_ids,
                    &frame.entry,
                    &frame.first_slice,
                    src_buffer_offset,
                    src_buffer_range,
                    &frame.first_slice.slice_offsets,
                    reset_control_recorded,
                    &mut h264_reference_infos,
                )?;
                frame_timing.decode_plan_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                ffmpeg_reference = plan.common.ffmpeg_reference;
                let stage_started_at = Instant::now();
                let image_views =
                    native_vulkan_vulkanalia_h264_decode_image_view_bindings(image, &plan)?;
                frame_timing.image_view_bind_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                let dst_slot = plan.common.dst_picture_resource.base_array_layer as usize;
                let transition_dst_from_undefined = !initialized_slots[dst_slot];
                let decode_command_buffer = command_buffer_ref.command_buffer_at(submit_slot)?;
                let stage_started_at = Instant::now();
                let record_plan = unsafe {
                    native_vulkan_vulkanalia_record_h264_decode_command_buffer(
                        device,
                        decode_command_buffer,
                        image.image,
                        &plan,
                        session,
                        session_parameters_ref.parameters,
                        bitstream_buffer_ref.buffer,
                        &image_views,
                        submit_ring.reset_command_buffer_before_record(submit_slot)?,
                        transition_dst_from_undefined,
                    )
                }?;
                submit_ring.mark_recorded(submit_slot)?;
                frame_timing.record_command_buffer_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                let decode_complete_value_for_frame = decode_complete_value.get() + 1;
                decode_complete_value.set(decode_complete_value_for_frame);
                let stage_started_at = Instant::now();
                let queue_host_access_guard = if let Some(lock) = queue_host_access_lock {
                    Some(lock.lock().map_err(|_| {
                        "Vulkanalia H.264 decode queue host-access lock is poisoned".to_owned()
                    })?)
                } else {
                    None
                };
                let submit_plan = unsafe {
                    native_vulkan_vulkanalia_submit_decode_command_buffer2(
                        device,
                        queue,
                        decode_command_buffer,
                        command_buffer_ref.submit_fence_at(submit_slot)?,
                        false,
                        false,
                        decode_complete_semaphore,
                        decode_complete_value_for_frame,
                    )
                }?;
                drop(queue_host_access_guard);
                submit_ring.mark_submitted(submit_slot)?;
                frame_timing.submit_wait_micros =
                    native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                initialized_slots[dst_slot] = true;
                command_buffer_recorded &=
                    record_plan.command_order.contains(&"vkEndCommandBuffer");
                submitted &= submit_plan.command_order.contains(&"queue_submit2");
                uses_synchronization2 &= record_plan.uses_synchronization2;
                uses_submit2 &= submit_plan.uses_submit2;
                let (display_order_key, display_order_key_source) =
                    native_vulkan_vulkanalia_h264_display_order_key(
                        &frame.entry,
                        frame.pts_ns,
                        frame_index,
                    );

                if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                    let stage_started_at = Instant::now();
                    after_frame_submitted(
                        frame_index,
                        plan.common.dst_picture_resource.base_array_layer,
                        frame.pts_ns,
                        frame.duration_ns,
                        frame.entry.pts_ms,
                        frame.duration_ms,
                        display_order_key,
                        display_order_key_source,
                        decode_complete_value_for_frame,
                    )?;
                    frame_timing.after_frame_submitted_micros =
                        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
                }

                let begin_reference_slot_count = plan.common.begin_reference_slot_count as u32;
                let decode_reference_slot_count = plan.common.decode_reference_slot_count as u32;
                last_slice_segment_offsets.clear();
                last_slice_segment_offsets.extend_from_slice(plan.picture.slice_offsets);
                frame_telemetry.push(NativeVulkanVulkanaliaDecodeFrameLastFields {
                    src_buffer_offset: plan.common.src_buffer_offset,
                    src_buffer_range: plan.common.src_buffer_range,
                    dst_base_array_layer: plan.common.dst_picture_resource.base_array_layer,
                    setup_slot_index: plan.common.setup_reference_slot.slot_index,
                    begin_reference_slot_count,
                    decode_reference_slot_count,
                    reset_control_recorded,
                });
                streaming_decode_timing.push(frame_timing);
            }
            let last_frame =
                frame_telemetry.last_frame("Vulkanalia H.264 streaming submitted no frames")?;
            let final_drain_wait_micros =
                submit_ring.wait_all_submitted(device, command_buffer_ref)?;
            let streaming_decode_timing = streaming_decode_timing.finish(
                native_vulkan_vulkanalia_elapsed_micros(decode_loop_started_at),
                final_drain_wait_micros,
            );

            Ok(NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot {
                requested_frame_count,
                recorded_frame_count: frame_telemetry.submitted_frame_count,
                submitted_frame_count: frame_telemetry.submitted_frame_count,
                ffmpeg_reference,
                command_buffer_recorded,
                submitted,
                uses_synchronization2,
                uses_submit2,
                wait_idle_after_submit: false,
                wait_fence_after_submit: false,
                batch_wait_fence_after_submit: true,
                uses_submit_fence: true,
                submit_sync_model:
                    NATIVE_VULKAN_VULKANALIA_STREAMING_DECODE_SUBMIT_FENCE_SYNC_MODEL,
                submit_command_order:
                    native_vulkan_vulkanalia_streaming_decode_submit_fence_command_order(),
                queue_family_index,
                bitstream_buffer_model: "ffmpeg-picture-slices-buffer-pool-exec-owned",
                ffmpeg_slices_buffer_pool_slot_count: bitstream_buffers.slot_count(),
                ffmpeg_slices_buffer_pool_allocated_slot_count: bitstream_buffers
                    .allocated_slot_count(),
                ffmpeg_slices_buffer_pool_capacity_bytes: bitstream_buffers.total_capacity_bytes(),
                ffmpeg_slices_buffer_pool_max_slot_bytes: bitstream_buffers
                    .max_slot_capacity_bytes(),
                input_payload_model: "bounded-streaming-packet-queue-per-frame-upload",
                src_buffer_total_bytes,
                streaming_decode_timing,
                retained_frame_telemetry_limit:
                    NATIVE_VULKAN_VULKANALIA_DECODE_FRAME_TELEMETRY_RETAINED_FRAMES,
                retained_frame_telemetry_count: frame_telemetry.retained_frame_count(),
                frame_telemetry_retention_model:
                    NATIVE_VULKAN_VULKANALIA_DECODE_FRAME_TELEMETRY_RETENTION_MODEL,
                max_src_buffer_range: frame_telemetry.max_src_buffer_range,
                first_frame_reset_control_recorded: frame_telemetry
                    .first_frame_reset_control_recorded
                    .unwrap_or(false),
                reset_control_recorded_frame_count: frame_telemetry
                    .reset_control_recorded_frame_count,
                p_frame_count: frame_telemetry.p_frame_count,
                b_frame_count: frame_telemetry.b_frame_count,
                max_begin_reference_slot_count: frame_telemetry.max_begin_reference_slot_count,
                max_decode_reference_slot_count: frame_telemetry.max_decode_reference_slot_count,
                src_buffer_offset: last_frame.src_buffer_offset,
                src_buffer_range: last_frame.src_buffer_range,
                dst_base_array_layer: last_frame.dst_base_array_layer,
                setup_slot_index: last_frame.setup_slot_index,
                begin_reference_slot_count: last_frame.begin_reference_slot_count,
                decode_reference_slot_count: last_frame.decode_reference_slot_count,
                reset_control_recorded: last_frame.reset_control_recorded,
                slice_segment_count: last_slice_segment_offsets.len() as u32,
                slice_segment_offsets: last_slice_segment_offsets,
                frames: Vec::new(),
            })
        })();

    if result.is_err()
        && let Some(command_buffer_ref) = command_buffer.as_ref()
    {
        let _ = submit_ring.wait_all_submitted(device, command_buffer_ref);
    }
    bitstream_buffers.destroy_all(device);
    if let Some(command_buffer) = command_buffer.take() {
        native_vulkan_vulkanalia_destroy_decode_command_buffer(device, command_buffer);
    }
    if let Some(session_parameters) = session_parameters.take() {
        native_vulkan_vulkanalia_destroy_video_session_parameters(device, session_parameters);
    }
    native_vulkan_vulkanalia_trim_heap_after_decode_teardown();

    result
}

fn native_vulkan_vulkanalia_h264_decode_image_view_bindings(
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    plan: &super::video_decode_submit_h264::NativeVulkanVulkanaliaH264DecodeSubmitPlan,
) -> Result<NativeVulkanVulkanaliaDecodeImageViewBindings, String> {
    Ok(NativeVulkanVulkanaliaDecodeImageViewBindings {
        dst_picture_image_view: native_vulkan_vulkanalia_layer_view(
            image,
            plan.common.dst_picture_resource.base_array_layer,
        )?,
        setup_reference_image_view: image.view,
        begin_reference_image_view: image.view,
        begin_reference_image_view_count: plan.common.begin_reference_slot_count,
        decode_reference_image_view: image.view,
        decode_reference_image_view_count: plan.common.decode_reference_slot_count,
    })
}

fn native_vulkan_vulkanalia_h265_decode_image_view_bindings(
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    plan: &super::video_decode_submit_h265::NativeVulkanVulkanaliaH265DecodeSubmitPlan,
) -> Result<NativeVulkanVulkanaliaDecodeImageViewBindings, String> {
    Ok(NativeVulkanVulkanaliaDecodeImageViewBindings {
        dst_picture_image_view: native_vulkan_vulkanalia_layer_view(
            image,
            plan.common.dst_picture_resource.base_array_layer,
        )?,
        setup_reference_image_view: image.view,
        begin_reference_image_view: image.view,
        begin_reference_image_view_count: plan.common.begin_reference_slot_count,
        decode_reference_image_view: image.view,
        decode_reference_image_view_count: plan.common.decode_reference_slot_count,
    })
}

fn native_vulkan_vulkanalia_av1_decode_image_view_bindings(
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    plan: &super::video_decode_submit_av1::NativeVulkanVulkanaliaAv1DecodeSubmitPlan,
) -> Result<NativeVulkanVulkanaliaDecodeImageViewBindings, String> {
    Ok(NativeVulkanVulkanaliaDecodeImageViewBindings {
        dst_picture_image_view: native_vulkan_vulkanalia_layer_view(
            image,
            plan.common.dst_picture_resource.base_array_layer,
        )?,
        setup_reference_image_view: image.view,
        begin_reference_image_view: image.view,
        begin_reference_image_view_count: plan.common.begin_reference_slot_count,
        decode_reference_image_view: image.view,
        decode_reference_image_view_count: plan.common.decode_reference_slot_count,
    })
}

fn native_vulkan_vulkanalia_layer_view(
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    layer: u32,
) -> Result<vk::ImageView, String> {
    image
        .layer_views
        .get(layer as usize)
        .copied()
        .ok_or_else(|| {
            format!(
                "Vulkanalia video image has {} layer views but layer {layer} was requested",
                image.layer_views.len()
            )
        })
}

fn queue_flag_labels(flags: vk::QueueFlags) -> Vec<&'static str> {
    [
        (vk::QueueFlags::GRAPHICS, "graphics"),
        (vk::QueueFlags::COMPUTE, "compute"),
        (vk::QueueFlags::TRANSFER, "transfer"),
        (vk::QueueFlags::SPARSE_BINDING, "sparse-binding"),
        (vk::QueueFlags::PROTECTED, "protected"),
        (vk::QueueFlags::VIDEO_DECODE_KHR, "video-decode"),
        (vk::QueueFlags::VIDEO_ENCODE_KHR, "video-encode"),
    ]
    .into_iter()
    .filter_map(|(flag, label)| flags.contains(flag).then_some(label))
    .collect()
}

fn video_codec_operation_labels(flags: vk::VideoCodecOperationFlagsKHR) -> Vec<&'static str> {
    [
        (vk::VideoCodecOperationFlagsKHR::DECODE_H264, "decode-h264"),
        (vk::VideoCodecOperationFlagsKHR::DECODE_H265, "decode-h265"),
        (vk::VideoCodecOperationFlagsKHR::DECODE_AV1, "decode-av1"),
    ]
    .into_iter()
    .filter_map(|(flag, label)| flags.contains(flag).then_some(label))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::super::video_codec::{
        native_vulkan_vulkanalia_video_session_format_probe_profile as vulkanalia_video_session_format_probe_profile,
        native_vulkan_vulkanalia_video_session_picture_format as vulkanalia_video_session_picture_format,
        native_vulkan_vulkanalia_video_session_profile_label as vulkanalia_video_session_profile_label,
    };
    use super::super::video_device::native_vulkan_vulkanalia_video_decode_required_device_extensions;
    use super::*;
    use vulkanalia::vk::Handle;

    #[test]
    fn session_bind_smoke_maps_codec_extensions_and_formats() {
        assert_eq!(
            native_vulkan_vulkanalia_video_decode_required_device_extensions(
                NativeVulkanVideoSessionCodec::H265Main10
            ),
            vec![
                "VK_KHR_video_queue",
                "VK_KHR_video_decode_queue",
                "VK_KHR_video_decode_h265"
            ]
        );
        assert_eq!(
            vulkanalia_video_session_picture_format(NativeVulkanVideoSessionCodec::Av1Main10),
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
        );
        assert_eq!(
            vulkanalia_video_session_format_probe_profile(NativeVulkanVideoSessionCodec::H264High8),
            "high"
        );
        assert_eq!(
            vulkanalia_video_session_profile_label(NativeVulkanVideoSessionCodec::H264High8),
            "high-8"
        );
    }

    #[test]
    fn h264_decode_bindings_use_ffmpeg_dst_layer_view_and_layered_refs() {
        let reference_infos = Vec::new();
        let plan = super::super::video_decode_submit_h264::NativeVulkanVulkanaliaH264DecodeSubmitPlan {
            common: super::super::video_decode_submit::NativeVulkanVulkanaliaDecodeSubmitPlan::new(
                NativeVulkanVideoSessionCodec::H264High8,
                0,
                0,
                super::super::video_decode_submit::NativeVulkanVulkanaliaPictureResourcePlan::new(
                    vk::Extent2D {
                        width: 1280,
                        height: 720,
                    },
                    2,
                ),
                super::super::video_decode_submit::NativeVulkanVulkanaliaReferenceSlotPlan::setup_current(
                    2,
                    super::super::video_decode_submit::NativeVulkanVulkanaliaPictureResourcePlan::new(
                        vk::Extent2D {
                            width: 1280,
                            height: 720,
                        },
                        2,
                    ),
                ),
                0,
                0,
                false,
            ),
            picture: super::super::video_decode_submit_h264::NativeVulkanVulkanaliaH264PictureInfoPlan {
                ffmpeg_reference: super::super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE,
                seq_parameter_set_id: 0,
                pic_parameter_set_id: 0,
                field_pic_flag: false,
                bottom_field_flag: false,
                is_intra: false,
                is_idr: false,
                is_reference: false,
                frame_num: 0,
                idr_pic_id: 0,
                pic_order_cnt: [0, 0],
                slice_offsets: &[0],
                references: reference_infos.as_slice(),
            },
        };
        let image = super::super::video_session_images::VulkanaliaVideoSessionResourceImage {
            image: vk::Image::null(),
            memory: vk::DeviceMemory::null(),
            view: vk::ImageView::from_raw(100),
            layer_views: vec![
                vk::ImageView::from_raw(101),
                vk::ImageView::from_raw(102),
                vk::ImageView::from_raw(103),
            ],
            snapshot: super::super::video_session_images::NativeVulkanVulkanaliaVideoSessionResourceImageSnapshot {
                role: "coincident-dpb-output-sampled-video",
                format: "G8_B8R8_2PLANE_420_UNORM".to_owned(),
                image_type: "_2D".to_owned(),
                image_tiling: "OPTIMAL".to_owned(),
                image_usage_flags: vec!["sampled", "video-decode-dst", "video-decode-dpb"],
                image_create_flags: vec!["mutable-format"],
                extent: (1280, 720, 1),
                array_layers: 3,
                image_view_type: "2d-array",
                image_view_created: true,
                layer_view_count: 3,
                memory_size: 0,
                memory_alignment: 0,
                memory_type_bits: 0,
                selected_memory_type_index: 0,
                selected_memory_property_flags: vec![],
            },
        };

        let bindings = native_vulkan_vulkanalia_h264_decode_image_view_bindings(&image, &plan)
            .expect("bindings should resolve");

        assert_eq!(
            bindings.dst_picture_image_view,
            vk::ImageView::from_raw(103)
        );
        assert_eq!(
            bindings.setup_reference_image_view,
            vk::ImageView::from_raw(100)
        );
    }
}
