//! AV1 direct-present and acquire policy helpers.
//!
//! These helpers keep the renderer's hot video loop closer to ffplay's
//! separation between decode scheduling, frame handoff, and presentation.

use super::*;

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Copy)]
pub(super) struct NativeVulkanAv1AcquireWorkerRequest {
    pub(super) frame_context_index: usize,
    pub(super) image_available: vk::Semaphore,
    pub(super) timeout_ns: u64,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Copy)]
pub(super) struct NativeVulkanAv1AcquireWorkerResult {
    pub(super) frame_context_index: usize,
    pub(super) image_index: Option<u32>,
    pub(super) acquire_elapsed_us: u64,
    pub(super) not_ready: bool,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug)]
pub(super) struct NativeVulkanAv1AcquiredImageQueueSlot {
    pub(super) image_available: vk::Semaphore,
    pub(super) request_pending: bool,
    pub(super) acquired_image_index: Option<u32>,
    pub(super) in_use: bool,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Copy)]
pub(super) struct NativeVulkanAv1AcquiredImageQueueRequest {
    pub(super) slot_index: usize,
    pub(super) image_available: vk::Semaphore,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Copy)]
pub(super) struct NativeVulkanAv1AcquiredImageQueueResult {
    pub(super) slot_index: usize,
    pub(super) image_index: Option<u32>,
    pub(super) acquire_elapsed_us: u64,
    pub(super) not_ready: bool,
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) enum NativeVulkanAv1PresentWorkerJob {
    Present(NativeVulkanAv1PresentJob),
    PresentFrame(NativeVulkanAv1PresentFrameJob),
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Copy)]
pub(super) struct NativeVulkanAv1PresentWorkerResult {
    pub(super) counted: bool,
    pub(super) frame_index: usize,
    pub(super) frame_context_index: usize,
    pub(super) acquire_elapsed_us: Option<u64>,
    pub(super) acquire_start_since_start_us: Option<u64>,
    pub(super) acquire_end_since_start_us: Option<u64>,
    pub(super) record_elapsed_us: Option<u64>,
    pub(super) queue_submit_elapsed_us: u64,
    pub(super) queue_present_elapsed_us: u64,
    pub(super) present_elapsed_us: u64,
    pub(super) present_submit_start_since_start_us: u64,
    pub(super) queue_present_start_since_start_us: u64,
    pub(super) present_result_since_start_us: u64,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct NativeVulkanAv1DecodeWaitStats {
    pub(super) wait_count: u32,
    pub(super) elapsed_us: u64,
    pub(super) max_us: u64,
    pub(super) hidden_wait_count: u32,
    pub(super) hidden_elapsed_us: u64,
    pub(super) hidden_max_us: u64,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanAv1DecodeWaitStats {
    pub(super) fn add_wait(&mut self, elapsed_us: u64, hidden: bool) {
        self.wait_count = self.wait_count.saturating_add(1);
        self.elapsed_us = self.elapsed_us.saturating_add(elapsed_us);
        self.max_us = self.max_us.max(elapsed_us);
        if hidden {
            self.hidden_wait_count = self.hidden_wait_count.saturating_add(1);
            self.hidden_elapsed_us = self.hidden_elapsed_us.saturating_add(elapsed_us);
            self.hidden_max_us = self.hidden_max_us.max(elapsed_us);
        }
    }

    pub(super) fn merge(&mut self, other: NativeVulkanAv1DecodeWaitStats) {
        self.wait_count = self.wait_count.saturating_add(other.wait_count);
        self.elapsed_us = self.elapsed_us.saturating_add(other.elapsed_us);
        self.max_us = self.max_us.max(other.max_us);
        self.hidden_wait_count = self
            .hidden_wait_count
            .saturating_add(other.hidden_wait_count);
        self.hidden_elapsed_us = self
            .hidden_elapsed_us
            .saturating_add(other.hidden_elapsed_us);
        self.hidden_max_us = self.hidden_max_us.max(other.hidden_max_us);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_wait_av1_decode_command_slot(
    device: &ash::Device,
    command_slots: &mut [NativeVulkanAv1DecodeCommandSlot],
    pending_submissions: &mut Vec<NativeVulkanAv1PendingDecodeSubmission>,
    slot_index: usize,
    operation: &'static str,
) -> Result<NativeVulkanAv1DecodeWaitStats, NativeVulkanError> {
    let hidden = pending_submissions
        .iter()
        .any(|pending| pending.command_slot_index == slot_index && pending.hidden);
    let mut stats = NativeVulkanAv1DecodeWaitStats::default();
    let command_slot_count = command_slots.len();
    let slot = command_slots.get_mut(slot_index).ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "AV1 decode command slot {slot_index} exceeds {command_slot_count} slots"
        ))
    })?;
    if slot.in_flight {
        let started_at = Instant::now();
        unsafe {
            device
                .wait_for_fences(&[slot.fence], true, u64::MAX)
                .map_err(|result| NativeVulkanError::Vulkan { operation, result })?;
        }
        slot.in_flight = false;
        stats.add_wait(native_vulkan_elapsed_us(started_at.elapsed()), hidden);
    }
    pending_submissions.retain(|pending| pending.command_slot_index != slot_index);
    Ok(stats)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_wait_av1_pending_decode_submissions(
    device: &ash::Device,
    command_slots: &mut [NativeVulkanAv1DecodeCommandSlot],
    pending_submissions: &mut Vec<NativeVulkanAv1PendingDecodeSubmission>,
    mut predicate: impl FnMut(&NativeVulkanAv1PendingDecodeSubmission) -> bool,
    operation: &'static str,
) -> Result<NativeVulkanAv1DecodeWaitStats, NativeVulkanError> {
    let mut slot_indices = pending_submissions
        .iter()
        .filter(|pending| predicate(pending))
        .map(|pending| pending.command_slot_index)
        .collect::<Vec<_>>();
    slot_indices.sort_unstable();
    slot_indices.dedup();
    let mut stats = NativeVulkanAv1DecodeWaitStats::default();
    for slot_index in slot_indices {
        let slot_stats = native_vulkan_wait_av1_decode_command_slot(
            device,
            command_slots,
            pending_submissions,
            slot_index,
            operation,
        )?;
        stats.wait_count = stats.wait_count.saturating_add(slot_stats.wait_count);
        stats.elapsed_us = stats.elapsed_us.saturating_add(slot_stats.elapsed_us);
        stats.max_us = stats.max_us.max(slot_stats.max_us);
        stats.hidden_wait_count = stats
            .hidden_wait_count
            .saturating_add(slot_stats.hidden_wait_count);
        stats.hidden_elapsed_us = stats
            .hidden_elapsed_us
            .saturating_add(slot_stats.hidden_elapsed_us);
        stats.hidden_max_us = stats.hidden_max_us.max(slot_stats.hidden_max_us);
    }
    Ok(stats)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_decode_command_ring_depth(default_depth: u32) -> u32 {
    std::env::var("GILDER_VULKAN_AV1_DECODE_COMMAND_RING")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_depth.max(16))
        .clamp(1, 32)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_async_present_enabled() -> bool {
    !matches!(
        std::env::var("GILDER_VULKAN_AV1_ASYNC_PRESENT")
            .ok()
            .as_deref(),
        Some("0") | Some("false") | Some("off") | Some("no")
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_present_frame_queue_enabled() -> bool {
    !matches!(
        std::env::var("GILDER_VULKAN_AV1_PRESENT_FRAME_QUEUE")
            .ok()
            .as_deref(),
        Some("0")
            | Some("false")
            | Some("off")
            | Some("no")
            | Some("legacy")
            | Some("submit-ready")
            | Some("submit_ready")
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_present_frame_clear_preroll_enabled() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_PRESENT_FRAME_CLEAR_PREROLL")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("on") | Some("yes")
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_present_frame_clear_preroll_count() -> u32 {
    std::env::var("GILDER_VULKAN_AV1_PRESENT_FRAME_CLEAR_PREROLL_COUNT")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(2)
        .clamp(1, 8)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_present_frame_video_preroll_enabled() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_PRESENT_FRAME_PREROLL")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("on") | Some("yes") | Some("video") | Some("first-frame")
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_present_frame_inline_present_enabled(
    default_enabled: bool,
) -> bool {
    std::env::var("GILDER_VULKAN_AV1_PRESENT_FRAME_INLINE_PRESENT")
        .ok()
        .map(|value| {
            !matches!(
                value.to_ascii_lowercase().as_str(),
                "0" | "false" | "off" | "no"
            )
        })
        .unwrap_or(default_enabled)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_async_present_depth(
    frame_context_count: usize,
    swapchain_image_count: usize,
) -> usize {
    let max_depth = frame_context_count.max(1).min(swapchain_image_count.max(1));
    std::env::var("GILDER_VULKAN_AV1_ASYNC_PRESENT_DEPTH")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(max_depth)
        .clamp(1, max_depth)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_present_frame_queue_depth(frame_context_count: usize) -> usize {
    let max_depth = frame_context_count.max(1);
    std::env::var("GILDER_VULKAN_AV1_PRESENT_FRAME_QUEUE_DEPTH")
        .ok()
        .or_else(|| std::env::var("GILDER_VULKAN_AV1_ASYNC_PRESENT_DEPTH").ok())
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(max_depth)
        .clamp(1, max_depth)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_present_frame_acquire_retry_sleep() -> Duration {
    let sleep_us = std::env::var("GILDER_VULKAN_AV1_PRESENT_FRAME_ACQUIRE_RETRY_SLEEP_US")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50)
        .clamp(0, 1_000);
    Duration::from_micros(sleep_us)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_ready_frame_context_selection_enabled() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_READY_CONTEXT_SELECTION")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("on") | Some("yes")
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_ready_display_slot_selection_enabled() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_READY_DISPLAY_SLOT")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("on") | Some("yes")
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_display_slot_gpu_wait_enabled() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_DISPLAY_SLOT_GPU_WAIT")
            .ok()
            .as_deref(),
        Some("1")
            | Some("true")
            | Some("on")
            | Some("yes")
            | Some("timeline")
            | Some("gpu")
            | Some("gpu-wait")
            | Some("gpu_wait")
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_frame_context_present_command_buffer_enabled(
    default_enabled: bool,
) -> bool {
    match std::env::var("GILDER_VULKAN_AV1_PRESENT_COMMAND_BUFFER")
        .ok()
        .as_deref()
    {
        Some("frame-context")
        | Some("frame_context")
        | Some("context")
        | Some("1")
        | Some("true")
        | Some("on")
        | Some("yes") => true,
        Some("0")
        | Some("false")
        | Some("off")
        | Some("no")
        | Some("swapchain")
        | Some("swapchain-image")
        | Some("swapchain_image") => false,
        _ => default_enabled,
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_choose_av1_display_ring_slot(
    device: &ash::Device,
    slot_fences: &[vk::Fence],
    preferred_slot: usize,
    current_frame_fence: vk::Fence,
    ready_probe_count: &mut u32,
    ready_hit_count: &mut u32,
    ready_fallback_count: &mut u32,
) -> Result<usize, NativeVulkanError> {
    if slot_fences.is_empty() {
        return Err(NativeVulkanError::Video(
            "AV1 display ring has no slots".to_owned(),
        ));
    }
    let preferred_slot = preferred_slot % slot_fences.len();
    for offset in 0..slot_fences.len() {
        let slot = (preferred_slot + offset) % slot_fences.len();
        let fence = slot_fences[slot];
        if fence == vk::Fence::null() || fence == current_frame_fence {
            *ready_hit_count = ready_hit_count.saturating_add(1);
            return Ok(slot);
        }
        *ready_probe_count = ready_probe_count.saturating_add(1);
        let ready = unsafe { device.get_fence_status(fence) }.map_err(|result| {
            NativeVulkanError::Vulkan {
                operation: "vkGetFenceStatus(direct av1 display ring slot)",
                result,
            }
        })?;
        if ready {
            *ready_hit_count = ready_hit_count.saturating_add(1);
            return Ok(slot);
        }
    }
    *ready_fallback_count = ready_fallback_count.saturating_add(1);
    Ok(preferred_slot)
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Clone, Copy)]
pub(super) struct NativeVulkanAv1PreparedShowExistingDisplayCopy {
    pub(super) display_slot: usize,
    pub(super) wait_semaphore: vk::Semaphore,
    pub(super) handoff_value: u64,
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_prepared_show_existing_display_invalidate_dpb_slot(
    prepared_by_dpb_slot: &mut [Option<NativeVulkanAv1PreparedShowExistingDisplayCopy>],
    dpb_slot: u32,
) -> bool {
    prepared_by_dpb_slot
        .get_mut(dpb_slot as usize)
        .and_then(Option::take)
        .is_some()
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_prepared_show_existing_display_invalidate_display_slot(
    prepared_by_dpb_slot: &mut [Option<NativeVulkanAv1PreparedShowExistingDisplayCopy>],
    display_slot: usize,
) -> bool {
    let mut invalidated = false;
    for prepared in prepared_by_dpb_slot.iter_mut() {
        if matches!(*prepared, Some(prepared_copy) if prepared_copy.display_slot == display_slot) {
            *prepared = None;
            invalidated = true;
        }
    }
    invalidated
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_display_cache_invalidate(
    display_slot_sources: &mut [Option<u32>],
    dpb_display_slots: &mut [Option<usize>],
    dpb_slot: u32,
) -> bool {
    let Some(mapped_display_slot) = dpb_display_slots
        .get_mut(dpb_slot as usize)
        .and_then(Option::take)
    else {
        return false;
    };
    if let Some(source) = display_slot_sources.get_mut(mapped_display_slot)
        && *source == Some(dpb_slot)
    {
        *source = None;
    }
    true
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_display_cache_update(
    display_slot_sources: &mut [Option<u32>],
    dpb_display_slots: &mut [Option<usize>],
    dpb_slot: u32,
    display_slot: usize,
) -> bool {
    if display_slot >= display_slot_sources.len() || dpb_slot as usize >= dpb_display_slots.len() {
        return false;
    }
    if let Some(previous_dpb_slot) = display_slot_sources[display_slot]
        && let Some(mapped_display_slot) = dpb_display_slots.get_mut(previous_dpb_slot as usize)
        && *mapped_display_slot == Some(display_slot)
    {
        *mapped_display_slot = None;
    }
    if let Some(previous_display_slot) = dpb_display_slots[dpb_slot as usize]
        && previous_display_slot != display_slot
        && let Some(source) = display_slot_sources.get_mut(previous_display_slot)
        && *source == Some(dpb_slot)
    {
        *source = None;
    }
    display_slot_sources[display_slot] = Some(dpb_slot);
    dpb_display_slots[dpb_slot as usize] = Some(display_slot);
    true
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_frame_context_selectable(
    context: &NativeVulkanAv1FrameContext,
) -> bool {
    !context.pending_present_result && !context.preacquire_pending
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_find_ready_av1_frame_context(
    device: &ash::Device,
    frame_contexts: &[NativeVulkanAv1FrameContext],
    start_index: usize,
    mut predicate: impl FnMut(&NativeVulkanAv1FrameContext) -> bool,
    ready_probe_count: &mut u32,
    ready_hit_count: &mut u32,
) -> Result<Option<usize>, NativeVulkanError> {
    if frame_contexts.is_empty() {
        return Ok(None);
    }
    for offset in 0..frame_contexts.len() {
        let context_index = (start_index + offset) % frame_contexts.len();
        let context = &frame_contexts[context_index];
        if !predicate(context) {
            continue;
        }
        *ready_probe_count = ready_probe_count.saturating_add(1);
        let ready = unsafe { device.get_fence_status(context.in_flight) }.map_err(|result| {
            NativeVulkanError::Vulkan {
                operation: "vkGetFenceStatus(direct av1 frame context)",
                result,
            }
        })?;
        if ready {
            *ready_hit_count = ready_hit_count.saturating_add(1);
            return Ok(Some(context_index));
        }
    }
    Ok(None)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_find_selectable_av1_frame_context(
    frame_contexts: &[NativeVulkanAv1FrameContext],
    start_index: usize,
    mut predicate: impl FnMut(&NativeVulkanAv1FrameContext) -> bool,
) -> Option<usize> {
    if frame_contexts.is_empty() {
        return None;
    }
    (0..frame_contexts.len())
        .map(|offset| (start_index + offset) % frame_contexts.len())
        .find(|context_index| predicate(&frame_contexts[*context_index]))
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_choose_av1_ready_frame_context(
    device: &ash::Device,
    frame_contexts: &[NativeVulkanAv1FrameContext],
    start_index: usize,
    preacquire_enabled: bool,
    ready_probe_count: &mut u32,
    ready_hit_count: &mut u32,
    fallback_count: &mut u32,
) -> Result<usize, NativeVulkanError> {
    if frame_contexts.is_empty() {
        return Err(NativeVulkanError::Video(
            "AV1 frame context pool is empty".to_owned(),
        ));
    }
    let start_index = start_index % frame_contexts.len();
    if preacquire_enabled
        && let Some(context_index) = native_vulkan_find_ready_av1_frame_context(
            device,
            frame_contexts,
            start_index,
            |context| {
                native_vulkan_av1_frame_context_selectable(context)
                    && context.preacquired_image_index.is_some()
            },
            ready_probe_count,
            ready_hit_count,
        )?
    {
        return Ok(context_index);
    }
    if let Some(context_index) = native_vulkan_find_ready_av1_frame_context(
        device,
        frame_contexts,
        start_index,
        |context| {
            native_vulkan_av1_frame_context_selectable(context)
                && (!preacquire_enabled || context.preacquired_image_index.is_none())
        },
        ready_probe_count,
        ready_hit_count,
    )? {
        return Ok(context_index);
    }
    *fallback_count = fallback_count.saturating_add(1);
    if preacquire_enabled
        && let Some(context_index) = native_vulkan_find_selectable_av1_frame_context(
            frame_contexts,
            start_index,
            |context| {
                native_vulkan_av1_frame_context_selectable(context)
                    && context.preacquired_image_index.is_some()
            },
        )
    {
        return Ok(context_index);
    }
    Ok(
        native_vulkan_find_selectable_av1_frame_context(frame_contexts, start_index, |context| {
            native_vulkan_av1_frame_context_selectable(context)
        })
        .unwrap_or(start_index),
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_preacquire_enabled() -> bool {
    !matches!(
        std::env::var("GILDER_VULKAN_AV1_PREACQUIRE")
            .ok()
            .as_deref(),
        Some("0") | Some("false") | Some("off") | Some("no")
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_preacquire_target_depth(
    frame_context_count: usize,
    swapchain_image_count: usize,
) -> usize {
    let max_depth = swapchain_image_count
        .saturating_sub(1)
        .max(1)
        .min(frame_context_count.saturating_sub(1).max(1));
    std::env::var("GILDER_VULKAN_AV1_PREACQUIRE_DEPTH")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1)
        .clamp(1, max_depth)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_preacquire_grace_wait(
    codec: NativeVulkanVideoSessionCodec,
) -> Duration {
    let default_grace_us = match codec {
        NativeVulkanVideoSessionCodec::Av1Main10 => 250,
        _ => 0,
    };
    std::env::var("GILDER_VULKAN_AV1_PREACQUIRE_GRACE_US")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_micros)
        .unwrap_or_else(|| Duration::from_micros(default_grace_us))
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_acquired_image_queue_enabled() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_ACQUIRED_IMAGE_QUEUE")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("on") | Some("yes")
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_acquired_image_queue_depth(swapchain_image_count: usize) -> usize {
    let max_depth = swapchain_image_count.max(1).min(8);
    std::env::var("GILDER_VULKAN_AV1_ACQUIRED_IMAGE_QUEUE_DEPTH")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(swapchain_image_count.max(1))
        .clamp(1, max_depth)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_acquired_image_queue_target_depth(
    slot_count: usize,
    swapchain_image_count: usize,
) -> usize {
    let max_depth = slot_count
        .max(1)
        .min(swapchain_image_count.saturating_sub(1).max(1));
    std::env::var("GILDER_VULKAN_AV1_ACQUIRED_IMAGE_QUEUE_TARGET_DEPTH")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(max_depth)
        .clamp(1, max_depth)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_acquired_image_queue_grace_wait() -> Duration {
    std::env::var("GILDER_VULKAN_AV1_ACQUIRED_IMAGE_QUEUE_GRACE_US")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_micros)
        .unwrap_or_else(|| Duration::from_micros(250))
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_acquired_image_queue_block_wait() -> Duration {
    std::env::var("GILDER_VULKAN_AV1_ACQUIRED_IMAGE_QUEUE_BLOCK_WAIT_US")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_micros)
        .unwrap_or_else(|| Duration::from_micros(8_000))
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_acquired_image_queue_acquire_timeout_ns() -> u64 {
    let timeout_us = std::env::var("GILDER_VULKAN_AV1_ACQUIRED_IMAGE_QUEUE_ACQUIRE_TIMEOUT_US")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1_000)
        .clamp(1, 100_000);
    timeout_us.saturating_mul(1_000)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_request_av1_acquired_image_queue_slot(
    slots: &mut [NativeVulkanAv1AcquiredImageQueueSlot],
    slot_index: usize,
    request_tx: &mpsc::Sender<NativeVulkanAv1AcquiredImageQueueRequest>,
    attempt_count: &mut u32,
) -> Result<(), NativeVulkanError> {
    let slot_count = slots.len();
    let slot = slots.get_mut(slot_index).ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "AV1 acquired-image queue slot {slot_index} exceeds {slot_count} slots"
        ))
    })?;
    if slot.request_pending || slot.acquired_image_index.is_some() || slot.in_use {
        return Ok(());
    }
    slot.request_pending = true;
    let request = NativeVulkanAv1AcquiredImageQueueRequest {
        slot_index,
        image_available: slot.image_available,
    };
    if request_tx.send(request).is_err() {
        slot.request_pending = false;
        return Err(NativeVulkanError::Video(
            "AV1 acquired-image queue worker exited before accepting an acquire request".to_owned(),
        ));
    }
    *attempt_count = attempt_count.saturating_add(1);
    Ok(())
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_request_one_av1_acquired_image_queue_slot(
    slots: &mut [NativeVulkanAv1AcquiredImageQueueSlot],
    request_tx: &mpsc::Sender<NativeVulkanAv1AcquiredImageQueueRequest>,
    attempt_count: &mut u32,
) -> Result<bool, NativeVulkanError> {
    let Some(slot_index) = slots.iter().position(|slot| {
        !slot.request_pending && slot.acquired_image_index.is_none() && !slot.in_use
    }) else {
        return Ok(false);
    };
    native_vulkan_request_av1_acquired_image_queue_slot(
        slots,
        slot_index,
        request_tx,
        attempt_count,
    )?;
    Ok(true)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_request_av1_acquired_image_queue_until_depth(
    slots: &mut [NativeVulkanAv1AcquiredImageQueueSlot],
    target_depth: usize,
    request_tx: &mpsc::Sender<NativeVulkanAv1AcquiredImageQueueRequest>,
    attempt_count: &mut u32,
) -> Result<(), NativeVulkanError> {
    if target_depth == 0 {
        return Ok(());
    }
    loop {
        let current_depth = slots
            .iter()
            .filter(|slot| slot.request_pending || slot.acquired_image_index.is_some())
            .count();
        if current_depth >= target_depth {
            return Ok(());
        }
        if !native_vulkan_request_one_av1_acquired_image_queue_slot(
            slots,
            request_tx,
            attempt_count,
        )? {
            return Ok(());
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_apply_av1_acquired_image_queue_result(
    slots: &mut [NativeVulkanAv1AcquiredImageQueueSlot],
    result: Result<NativeVulkanAv1AcquiredImageQueueResult, NativeVulkanError>,
) -> Result<NativeVulkanAv1AcquiredImageQueueResult, NativeVulkanError> {
    let result = result?;
    let slot_count = slots.len();
    let slot = slots.get_mut(result.slot_index).ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "AV1 acquired-image queue worker returned slot {} but only {} slot(s) exist",
            result.slot_index, slot_count
        ))
    })?;
    slot.request_pending = false;
    if let Some(image_index) = result.image_index {
        slot.acquired_image_index = Some(image_index);
    }
    Ok(result)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_drain_av1_acquired_image_queue_results(
    result_rx: &mpsc::Receiver<Result<NativeVulkanAv1AcquiredImageQueueResult, NativeVulkanError>>,
    slots: &mut [NativeVulkanAv1AcquiredImageQueueSlot],
) -> Result<Vec<NativeVulkanAv1AcquiredImageQueueResult>, NativeVulkanError> {
    let mut results = Vec::new();
    loop {
        match result_rx.try_recv() {
            Ok(result) => {
                results.push(native_vulkan_apply_av1_acquired_image_queue_result(
                    slots, result,
                )?);
            }
            Err(mpsc::TryRecvError::Empty) => return Ok(results),
            Err(mpsc::TryRecvError::Disconnected) => {
                if slots.iter().any(|slot| slot.request_pending) {
                    return Err(NativeVulkanError::Video(
                        "AV1 acquired-image queue worker exited with pending acquire requests"
                            .to_owned(),
                    ));
                }
                return Ok(results);
            }
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_wait_av1_acquired_image_queue_result_for_grace(
    result_rx: &mpsc::Receiver<Result<NativeVulkanAv1AcquiredImageQueueResult, NativeVulkanError>>,
    slots: &mut [NativeVulkanAv1AcquiredImageQueueSlot],
    timeout: Duration,
) -> Result<Vec<NativeVulkanAv1AcquiredImageQueueResult>, NativeVulkanError> {
    let mut results = native_vulkan_drain_av1_acquired_image_queue_results(result_rx, slots)?;
    if slots.iter().any(|slot| slot.acquired_image_index.is_some())
        || timeout.is_zero()
        || !slots.iter().any(|slot| slot.request_pending)
    {
        return Ok(results);
    }
    let deadline = Instant::now() + timeout;
    loop {
        if slots.iter().any(|slot| slot.acquired_image_index.is_some())
            || !slots.iter().any(|slot| slot.request_pending)
        {
            return Ok(results);
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(results);
        }
        match result_rx.recv_timeout(deadline.duration_since(now)) {
            Ok(result) => {
                results.push(native_vulkan_apply_av1_acquired_image_queue_result(
                    slots, result,
                )?);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => return Ok(results),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if slots.iter().any(|slot| slot.request_pending) {
                    return Err(NativeVulkanError::Video(
                        "AV1 acquired-image queue worker exited with pending acquire requests"
                            .to_owned(),
                    ));
                }
                return Ok(results);
            }
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_wait_av1_acquired_image_queue_result_for_image(
    result_rx: &mpsc::Receiver<Result<NativeVulkanAv1AcquiredImageQueueResult, NativeVulkanError>>,
    slots: &mut [NativeVulkanAv1AcquiredImageQueueSlot],
    timeout: Duration,
) -> Result<Vec<NativeVulkanAv1AcquiredImageQueueResult>, NativeVulkanError> {
    let mut results = native_vulkan_drain_av1_acquired_image_queue_results(result_rx, slots)?;
    if slots.iter().any(|slot| slot.acquired_image_index.is_some())
        || timeout.is_zero()
        || !slots.iter().any(|slot| slot.request_pending)
    {
        return Ok(results);
    }
    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Ok(results);
        }
        match result_rx.recv_timeout(deadline.duration_since(now)) {
            Ok(result) => {
                results.push(native_vulkan_apply_av1_acquired_image_queue_result(
                    slots, result,
                )?);
                if slots.iter().any(|slot| slot.acquired_image_index.is_some())
                    || !slots.iter().any(|slot| slot.request_pending)
                {
                    return Ok(results);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => return Ok(results),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if slots.iter().any(|slot| slot.request_pending) {
                    return Err(NativeVulkanError::Video(
                        "AV1 acquired-image queue worker exited with pending acquire requests"
                            .to_owned(),
                    ));
                }
                return Ok(results);
            }
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_take_av1_acquired_image_queue_slot(
    slots: &mut [NativeVulkanAv1AcquiredImageQueueSlot],
) -> Option<(usize, u32, vk::Semaphore)> {
    for (slot_index, slot) in slots.iter_mut().enumerate() {
        if slot.in_use || slot.request_pending {
            continue;
        }
        if let Some(image_index) = slot.acquired_image_index.take() {
            slot.in_use = true;
            return Some((slot_index, image_index, slot.image_available));
        }
    }
    None
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_record_av1_acquired_image_queue_results(
    results: &[NativeVulkanAv1AcquiredImageQueueResult],
    acquire_elapsed_us: &mut u64,
    acquire_max_us: &mut u64,
    miss_count: &mut u32,
) {
    for result in results {
        *acquire_elapsed_us = acquire_elapsed_us.saturating_add(result.acquire_elapsed_us);
        *acquire_max_us = (*acquire_max_us).max(result.acquire_elapsed_us);
        if result.not_ready || result.image_index.is_none() {
            *miss_count = miss_count.saturating_add(1);
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_release_av1_acquired_image_queue_slot(
    slots: &mut [NativeVulkanAv1AcquiredImageQueueSlot],
    slot_index: usize,
    request_tx: &mpsc::Sender<NativeVulkanAv1AcquiredImageQueueRequest>,
    request_more: bool,
    attempt_count: &mut u32,
) -> Result<(), NativeVulkanError> {
    let slot_count = slots.len();
    let slot = slots.get_mut(slot_index).ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "AV1 acquired-image queue release slot {slot_index} exceeds {slot_count} slots"
        ))
    })?;
    slot.in_use = false;
    slot.acquired_image_index = None;
    if request_more {
        native_vulkan_request_av1_acquired_image_queue_slot(
            slots,
            slot_index,
            request_tx,
            attempt_count,
        )?;
    }
    Ok(())
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_fill_av1_post_present_preacquire_requests(
    frame_contexts: &mut [NativeVulkanAv1FrameContext],
    present_frame_context_index: usize,
    target_depth: usize,
    post_present_acquire_requests: &mut Vec<NativeVulkanAv1AcquireWorkerRequest>,
    attempt_count: &mut u32,
) {
    if frame_contexts.is_empty() || target_depth == 0 {
        return;
    }
    let current_depth = frame_contexts
        .iter()
        .enumerate()
        .filter(|(context_index, context)| {
            *context_index != present_frame_context_index
                && (context.preacquire_pending || context.preacquired_image_index.is_some())
        })
        .count();
    let mut remaining_depth = target_depth.saturating_sub(current_depth);
    if remaining_depth == 0 {
        return;
    }
    let mut blocking_request_available = current_depth == 0;
    let start_context = (present_frame_context_index + 1) % frame_contexts.len();
    for offset in 0..frame_contexts.len() {
        if remaining_depth == 0 {
            break;
        }
        let context_index = (start_context + offset) % frame_contexts.len();
        if context_index == present_frame_context_index {
            continue;
        }
        let context = &mut frame_contexts[context_index];
        if context.pending_present_result
            || context.preacquire_pending
            || context.preacquired_image_index.is_some()
        {
            continue;
        }
        let timeout_ns = if blocking_request_available {
            blocking_request_available = false;
            u64::MAX
        } else {
            0
        };
        context.preacquire_pending = true;
        post_present_acquire_requests.push(NativeVulkanAv1AcquireWorkerRequest {
            frame_context_index: context_index,
            image_available: context.image_available,
            timeout_ns,
        });
        *attempt_count = attempt_count.saturating_add(1);
        remaining_depth = remaining_depth.saturating_sub(1);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_decoupled_acquire_worker_enabled() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_DECOUPLED_ACQUIRE_WORKER")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("on") | Some("yes")
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_pre_present_preacquire_enabled() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_PRE_PRESENT_PREACQUIRE")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("on") | Some("yes")
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_pre_present_preacquire_timeout_ns() -> u64 {
    let timeout_us = std::env::var("GILDER_VULKAN_AV1_PRE_PRESENT_PREACQUIRE_TIMEOUT_US")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(250)
        .clamp(0, 10_000);
    timeout_us.saturating_mul(1_000)
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_acquire_worker_blocking_timeout_ns() -> u64 {
    let timeout_us = std::env::var("GILDER_VULKAN_AV1_ACQUIRE_WORKER_TIMEOUT_US")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1_000)
        .clamp(1, 100_000);
    timeout_us.saturating_mul(1_000)
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NativeVulkanAv1WaitPresentBeforeAcquire {
    Disabled,
    One,
    All,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanAv1WaitPresentBeforeAcquire {
    pub(super) fn from_env() -> Self {
        match std::env::var("GILDER_VULKAN_AV1_WAIT_PRESENT_BEFORE_ACQUIRE")
            .ok()
            .as_deref()
        {
            Some("0") | Some("false") | Some("off") | Some("no") => Self::Disabled,
            Some("one") | Some("single") | Some("oldest") => Self::One,
            Some("1") | Some("true") | Some("on") | Some("yes") | Some("all") => Self::All,
            _ => Self::Disabled,
        }
    }

    pub(super) fn enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_present_result_after_preacquire_enabled() -> bool {
    matches!(
        std::env::var("GILDER_VULKAN_AV1_PRESENT_RESULT_AFTER_PREACQUIRE")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("on") | Some("yes")
    )
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_av1_frame_context_count(
    swapchain_image_count: usize,
    present_frame_queue_requested: bool,
) -> usize {
    let max_contexts = if present_frame_queue_requested {
        8
    } else {
        swapchain_image_count.saturating_add(1).max(1).min(8)
    };
    let default_contexts = if present_frame_queue_requested {
        swapchain_image_count
            .saturating_add(1)
            .max(4)
            .min(max_contexts)
    } else {
        3
    };
    std::env::var("GILDER_VULKAN_AV1_FRAME_CONTEXTS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_contexts)
        .clamp(1, max_contexts)
}
