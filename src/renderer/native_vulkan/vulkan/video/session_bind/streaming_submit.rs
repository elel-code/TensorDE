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
    native_vulkan_vulkanalia_video_session_create_flags,
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
    native_vulkan_vulkanalia_smoke_create_empty_video_session_parameters,
};
use super::video_session_parameters_av1::{
    native_vulkan_vulkanalia_av1_inline_session_parameters,
    native_vulkan_vulkanalia_smoke_create_av1_video_session_parameters,
};
use super::video_session_parameters_h264::{
    native_vulkan_vulkanalia_h264_inline_session_parameters,
    native_vulkan_vulkanalia_smoke_create_h264_video_session_parameters,
};
use super::video_session_parameters_h265::{
    native_vulkan_vulkanalia_h265_inline_session_parameters,
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

const NATIVE_VULKAN_VULKANALIA_STREAMING_DECODE_SUBMIT_FENCE_SYNC_MODEL: &str = "FFmpeg-style queue_submit2 async exec ring: each exec slot owns its mapped picture slices buffer until that slot fence completes; decode timeline signals at video-decode stage and present waits on the per-frame value before touching the decoded image; DPB output layer reuse stays independent; no per-frame submit wait and no queue_wait_idle";
const NATIVE_VULKAN_VULKANALIA_DECODE_FRAME_TELEMETRY_RETAINED_FRAMES: usize = 0;
const NATIVE_VULKAN_VULKANALIA_DECODE_FRAME_TELEMETRY_RETENTION_MODEL: &str = "FFmpeg-style scalar decode telemetry only; mirrors references/ffmpeg/libavcodec/vulkan_decode.h:73-106 and references/ffmpeg/libavcodec/vulkan_decode.c:488-536; no retained per-frame command snapshots";
const NATIVE_VULKAN_VULKANALIA_INLINE_SESSION_PARAMETER_STRATEGY: &str = "VK_KHR_video_maintenance2 inline codec parameters on VideoDecodeInfoKHR pNext; BeginCoding uses a null VideoSessionParametersKHR handle";

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

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaH264StreamingDecodeInput<
    'a,
> {
    pub(in crate::renderer::native_vulkan::vulkan) parameter_sets:
        NativeVulkanH264ParameterSetSnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) requested_frame_count: u32,
    pub(in crate::renderer::native_vulkan::vulkan) next_frame:
        &'a mut dyn FnMut() -> Result<NativeVulkanVulkanaliaH264ReadyPrefixFrameInput, String>,
}

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaH265StreamingDecodeInput<
    'a,
> {
    pub(in crate::renderer::native_vulkan::vulkan) parameter_sets:
        NativeVulkanH265ParameterSetSnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) requested_frame_count: u32,
    pub(in crate::renderer::native_vulkan::vulkan) next_frame:
        &'a mut dyn FnMut() -> Result<NativeVulkanVulkanaliaH265ReadyPrefixFrameInput, String>,
}

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaAv1StreamingDecodeInput<
    'a,
> {
    pub(in crate::renderer::native_vulkan::vulkan) sequence_header:
        NativeVulkanAv1SequenceHeaderSnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) requested_frame_count: u32,
    pub(in crate::renderer::native_vulkan::vulkan) next_frame:
        &'a mut dyn FnMut() -> Result<NativeVulkanVulkanaliaAv1StreamingFrameInput, String>,
}

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaAv1StreamingFrameInput {
    pub(in crate::renderer::native_vulkan::vulkan) entry:
        NativeVulkanAv1DecodeReferencePlanEntrySnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) frame:
        Option<NativeVulkanVulkanaliaAv1FrameSubmitInput>,
    pub(in crate::renderer::native_vulkan::vulkan) pts_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan::vulkan) duration_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan::vulkan) pts_ms: Option<u64>,
    pub(in crate::renderer::native_vulkan::vulkan) duration_ms: Option<u64>,
    pub(in crate::renderer::native_vulkan::vulkan) access_unit_payload:
        NativeVulkanEncodedAccessUnitPayload,
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
        let ffmpeg_new_size =
            native_vulkan_vulkanalia_align_up_u64(payload_len.max(1), min_size_alignment.max(1));
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

fn native_vulkan_vulkanalia_h264_initial_has_b_frames(
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> usize {
    parameter_sets
        .sps
        .vui
        .as_ref()
        .filter(|vui| vui.bitstream_restriction_flag)
        .map(|vui| vui.num_reorder_frames as usize)
        .unwrap_or(0)
        .min(16)
}

fn native_vulkan_vulkanalia_h265_max_output_reorder_pics(
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> usize {
    let layer_index = usize::from(parameter_sets.sps.max_sub_layers_minus1).min(
        parameter_sets
            .sps
            .dec_pic_buf_mgr
            .max_num_reorder_pics
            .len()
            - 1,
    );
    usize::from(parameter_sets.sps.dec_pic_buf_mgr.max_num_reorder_pics[layer_index])
}

struct NativeVulkanVulkanaliaDisplayOrderHandoffFrame {
    decode_frame_index: u32,
    sampled_array_layer: u32,
    h264_b_picture: bool,
    source_frame_pts_ns: Option<u64>,
    source_frame_duration_ns: Option<u64>,
    source_frame_pts_ms: Option<u64>,
    source_frame_duration_ms: Option<u64>,
    display_order_key: i64,
    display_order_key_source: &'static str,
    decode_complete_value: u64,
}

impl NativeVulkanVulkanaliaDisplayOrderHandoffFrame {
    fn submit<F>(self, after_frame_submitted: &mut F) -> Result<(), String>
    where
        F: FnMut(
                u32,
                u32,
                Option<u64>,
                Option<u64>,
                Option<u64>,
                Option<u64>,
                i64,
                &'static str,
                u64,
            ) -> Result<(), String>
            + ?Sized,
    {
        after_frame_submitted(
            self.decode_frame_index,
            self.sampled_array_layer,
            self.source_frame_pts_ns,
            self.source_frame_duration_ns,
            self.source_frame_pts_ms,
            self.source_frame_duration_ms,
            self.display_order_key,
            self.display_order_key_source,
            self.decode_complete_value,
        )
    }
}

struct NativeVulkanVulkanaliaDisplayOrderHandoff {
    reorder_depth: usize,
    max_reorder_depth: usize,
    adaptive_h264_reorder: bool,
    recent_decode_order_keys: Vec<i64>,
    frames: Vec<NativeVulkanVulkanaliaDisplayOrderHandoffFrame>,
}

impl NativeVulkanVulkanaliaDisplayOrderHandoff {
    fn fixed(reorder_depth: usize) -> Self {
        Self {
            reorder_depth,
            max_reorder_depth: reorder_depth,
            adaptive_h264_reorder: false,
            recent_decode_order_keys: Vec::new(),
            frames: Vec::with_capacity(reorder_depth.saturating_add(1)),
        }
    }

    fn h264_ffmpeg(initial_has_b_frames: usize) -> Self {
        let reorder_depth = initial_has_b_frames.min(16);
        Self {
            reorder_depth,
            max_reorder_depth: 16,
            adaptive_h264_reorder: true,
            recent_decode_order_keys: Vec::with_capacity(16),
            frames: Vec::with_capacity(reorder_depth.saturating_add(1)),
        }
    }

    fn push<F>(
        &mut self,
        frame: NativeVulkanVulkanaliaDisplayOrderHandoffFrame,
        after_frame_submitted: &mut F,
    ) -> Result<(), String>
    where
        F: FnMut(
                u32,
                u32,
                Option<u64>,
                Option<u64>,
                Option<u64>,
                Option<u64>,
                i64,
                &'static str,
                u64,
            ) -> Result<(), String>
            + ?Sized,
    {
        self.observe_h264_reorder_depth(&frame);
        self.frames.push(frame);
        while self.frames.len() > self.reorder_depth {
            self.submit_next(after_frame_submitted)?;
        }
        Ok(())
    }

    fn observe_h264_reorder_depth(
        &mut self,
        frame: &NativeVulkanVulkanaliaDisplayOrderHandoffFrame,
    ) {
        if !self.adaptive_h264_reorder {
            return;
        }

        let out_of_order = self
            .recent_decode_order_keys
            .iter()
            .filter(|key| frame.display_order_key < **key)
            .count()
            .max(usize::from(frame.h264_b_picture));
        if out_of_order > self.reorder_depth {
            self.reorder_depth = out_of_order.min(self.max_reorder_depth);
        }

        self.recent_decode_order_keys.push(frame.display_order_key);
        if self.recent_decode_order_keys.len() > self.max_reorder_depth {
            self.recent_decode_order_keys.remove(0);
        }
    }

    fn flush<F>(&mut self, after_frame_submitted: &mut F) -> Result<(), String>
    where
        F: FnMut(
                u32,
                u32,
                Option<u64>,
                Option<u64>,
                Option<u64>,
                Option<u64>,
                i64,
                &'static str,
                u64,
            ) -> Result<(), String>
            + ?Sized,
    {
        while !self.frames.is_empty() {
            self.submit_next(after_frame_submitted)?;
        }
        Ok(())
    }

    fn submit_next<F>(&mut self, after_frame_submitted: &mut F) -> Result<(), String>
    where
        F: FnMut(
                u32,
                u32,
                Option<u64>,
                Option<u64>,
                Option<u64>,
                Option<u64>,
                i64,
                &'static str,
                u64,
            ) -> Result<(), String>
            + ?Sized,
    {
        let next_index = self
            .frames
            .iter()
            .enumerate()
            .min_by_key(|(_, frame)| (frame.display_order_key, frame.decode_frame_index))
            .map(|(index, _)| index)
            .ok_or_else(|| "decoded display-order handoff has no frame to submit".to_owned())?;
        self.frames.remove(next_index).submit(after_frame_submitted)
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

include!("streaming_submit/smoke_and_av1.rs");
