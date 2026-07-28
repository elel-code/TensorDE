pub(super) struct NativeVulkanVulkanaliaDecodedImagePresentSequenceBuilder {
    requested_present_frame_count: u32,
    started_at: Instant,
    first_presented_at: Option<Instant>,
    last_presented_at: Option<Instant>,
    present_delta_min_micros: Option<u64>,
    present_delta_max_micros: Option<u64>,
    present_delta_over_6250us_count: u32,
    present_delta_over_8334us_count: u32,
    slow_frames: Vec<NativeVulkanVulkanaliaDecodedImagePresentSlowFrameSnapshot>,
    submitted_present_frame_count: u32,
    presented_frame_count: u32,
    frame_sleep_count: u32,
    missed_frame_pacing_count: u32,
    total_pacing_sleep_micros: u64,
    total_present_call_micros: u64,
    max_present_call_micros: u64,
    total_present_wait_frame_slot_micros: u64,
    max_present_wait_frame_slot_micros: u64,
    total_present_acquire_next_image_micros: u64,
    max_present_acquire_next_image_micros: u64,
    total_present_record_command_buffer_micros: u64,
    max_present_record_command_buffer_micros: u64,
    total_present_submit_command_buffer_micros: u64,
    max_present_submit_command_buffer_micros: u64,
    total_present_queue_present_micros: u64,
    max_present_queue_present_micros: u64,
    total_present_wait_after_queue_present_micros: u64,
    max_present_wait_after_queue_present_micros: u64,
    pts_monotonic: bool,
    last_pts_ns: Option<u64>,
    source_frame_pts_delta_min_ns: Option<u64>,
    source_frame_pts_delta_max_ns: Option<u64>,
    last_pts_ms: Option<u64>,
    source_frame_pts_delta_min_ms: Option<u64>,
    source_frame_pts_delta_max_ms: Option<u64>,
    display_order_monotonic: bool,
    last_display_order_key: Option<i64>,
    uses_present_id2: bool,
    present_wait2_available: bool,
    present_wait_after_present: bool,
    all_zero_copy_presented: bool,
    sampled_array_layer_mask: u128,
    latest_draw: Option<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    draws_head: Vec<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    draws_tail: Vec<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence {
    ffmpeg_read_thread_active: bool,
    video_decode_worker_active: bool,
    present_worker_active: bool,
    source_count: u32,
    decode_thread_count: u32,
    decode_async_exec_depth: u32,
    ffmpeg_retained_avframe_count: u32,
    ffmpeg_retained_avframe_peak_count: u32,
    descriptor_sampler_cache_entry_count: u32,
    descriptor_sampler_cache_peak_entry_count: u32,
    descriptor_sampler_cache_rewrite_count: u32,
    descriptor_sampler_cache_recreate_count: u32,
    descriptor_sampler_cache_resource_heap_bytes: u64,
    descriptor_sampler_cache_sampler_heap_bytes: u64,
}

impl NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence {
    pub(super) fn execution_model(self) -> &'static str {
        if self.decode_async_exec_depth == 0 && self.decode_thread_count == 1 {
            return "FFmpeg avcodec Vulkan hwdecode send/receive -> AVVkFrame descriptor-source handoff -> dynamic-rendering present worker";
        }
        if self.source_count > 1 || self.decode_thread_count > 1 {
            "FFmpeg-style N-source read threads -> per-source bounded packet queues -> per-source Vulkan Video decode workers -> per-source decoded-frame handoffs -> one dynamic-rendering present worker"
        } else {
            "FFmpeg-style read thread -> bounded packet queue -> single Vulkan Video decode worker -> bounded decoded-frame handoff -> present worker"
        }
    }

    pub(super) fn ffmpeg_thread_model(self) -> &'static str {
        if self.decode_async_exec_depth == 0 && self.decode_thread_count == 1 {
            return "one FFmpeg avcodec Vulkan hwdecode owner on the Vulkanalia-provided device; Gilder presents retained AVVkFrame refs after waiting their timeline semaphores";
        }
        if self.source_count > 1 || self.decode_thread_count > 1 {
            "one FFmpeg packet read thread and one native Vulkan Video decode worker per streaming source; one shared native present worker composites decoded GPU images without CPU pixel copies"
        } else {
            "one FFmpeg packet read thread, one native Vulkan Video decode worker, one native present worker; Vulkan async-depth follows FFmpeg Vulkan decode formula"
        }
    }
}

impl NativeVulkanVulkanaliaDecodedImagePresentSequenceBuilder {
    pub(super) fn new(requested_present_frame_count: u32, started_at: Instant) -> Self {
        Self {
            requested_present_frame_count,
            started_at,
            first_presented_at: None,
            last_presented_at: None,
            present_delta_min_micros: None,
            present_delta_max_micros: None,
            present_delta_over_6250us_count: 0,
            present_delta_over_8334us_count: 0,
            slow_frames: Vec::new(),
            submitted_present_frame_count: 0,
            presented_frame_count: 0,
            frame_sleep_count: 0,
            missed_frame_pacing_count: 0,
            total_pacing_sleep_micros: 0,
            total_present_call_micros: 0,
            max_present_call_micros: 0,
            total_present_wait_frame_slot_micros: 0,
            max_present_wait_frame_slot_micros: 0,
            total_present_acquire_next_image_micros: 0,
            max_present_acquire_next_image_micros: 0,
            total_present_record_command_buffer_micros: 0,
            max_present_record_command_buffer_micros: 0,
            total_present_submit_command_buffer_micros: 0,
            max_present_submit_command_buffer_micros: 0,
            total_present_queue_present_micros: 0,
            max_present_queue_present_micros: 0,
            total_present_wait_after_queue_present_micros: 0,
            max_present_wait_after_queue_present_micros: 0,
            pts_monotonic: true,
            last_pts_ns: None,
            source_frame_pts_delta_min_ns: None,
            source_frame_pts_delta_max_ns: None,
            last_pts_ms: None,
            source_frame_pts_delta_min_ms: None,
            source_frame_pts_delta_max_ms: None,
            display_order_monotonic: true,
            last_display_order_key: None,
            uses_present_id2: false,
            present_wait2_available: false,
            present_wait_after_present: false,
            all_zero_copy_presented: true,
            sampled_array_layer_mask: 0,
            latest_draw: None,
            draws_head: Vec::with_capacity(DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES),
            draws_tail: Vec::with_capacity(DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES),
        }
    }

    pub(super) fn push(&mut self, draw: NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot) {
        if draw.submitted {
            self.submitted_present_frame_count =
                self.submitted_present_frame_count.saturating_add(1);
        }
        if draw.presented {
            let presented_at = Instant::now();
            self.first_presented_at.get_or_insert(presented_at);
            if let Some(last_presented_at) = self.last_presented_at {
                let delta_micros =
                    duration_micros_u64(presented_at.saturating_duration_since(last_presented_at));
                self.present_delta_min_micros = Some(
                    self.present_delta_min_micros
                        .map(|current| current.min(delta_micros))
                        .unwrap_or(delta_micros),
                );
                self.present_delta_max_micros = Some(
                    self.present_delta_max_micros
                        .map(|current| current.max(delta_micros))
                        .unwrap_or(delta_micros),
                );
                if delta_micros > 6_250 {
                    self.present_delta_over_6250us_count =
                        self.present_delta_over_6250us_count.saturating_add(1);
                }
                if delta_micros > 8_334 {
                    self.present_delta_over_8334us_count =
                        self.present_delta_over_8334us_count.saturating_add(1);
                }
                if delta_micros > DECODED_IMAGE_PRESENT_SLOW_FRAME_THRESHOLD_MICROS
                    && self.slow_frames.len() < DECODED_IMAGE_PRESENT_SLOW_FRAME_TELEMETRY_LIMIT
                {
                    self.slow_frames.push(
                        NativeVulkanVulkanaliaDecodedImagePresentSlowFrameSnapshot {
                            present_frame_index: draw.present_frame_index,
                            present_frame_slot: draw.present_frame_slot,
                            sampled_array_layer: draw.sampled_array_layer,
                            delta_micros,
                            present_call_total_micros: draw.present_call_total_micros,
                            present_record_command_buffer_micros: draw
                                .present_record_command_buffer_micros,
                            present_submit_command_buffer_micros: draw
                                .present_submit_command_buffer_micros,
                            present_queue_present_micros: draw.present_queue_present_micros,
                            present_wait_frame_slot_micros: draw.present_wait_frame_slot_micros,
                            source_frame_pts_ns: draw.source_frame_pts_ns,
                            display_order_key: draw.display_order_key,
                        },
                    );
                }
            }
            self.last_presented_at = Some(presented_at);
            self.presented_frame_count = self.presented_frame_count.saturating_add(1);
        }
        self.total_pacing_sleep_micros = self
            .total_pacing_sleep_micros
            .saturating_add(draw.pacing_sleep_micros);
        if draw.pacing_sleep_micros > 0 {
            self.frame_sleep_count = self.frame_sleep_count.saturating_add(1);
        }
        if draw.pacing_clock_model == "audio-clock-master-video-late-no-sleep" {
            self.missed_frame_pacing_count = self.missed_frame_pacing_count.saturating_add(1);
        }
        self.total_present_call_micros = self
            .total_present_call_micros
            .saturating_add(draw.present_call_total_micros);
        self.max_present_call_micros = self
            .max_present_call_micros
            .max(draw.present_call_total_micros);
        self.total_present_wait_frame_slot_micros = self
            .total_present_wait_frame_slot_micros
            .saturating_add(draw.present_wait_frame_slot_micros);
        self.max_present_wait_frame_slot_micros = self
            .max_present_wait_frame_slot_micros
            .max(draw.present_wait_frame_slot_micros);
        self.total_present_acquire_next_image_micros = self
            .total_present_acquire_next_image_micros
            .saturating_add(draw.present_acquire_next_image_micros);
        self.max_present_acquire_next_image_micros = self
            .max_present_acquire_next_image_micros
            .max(draw.present_acquire_next_image_micros);
        self.total_present_record_command_buffer_micros = self
            .total_present_record_command_buffer_micros
            .saturating_add(draw.present_record_command_buffer_micros);
        self.max_present_record_command_buffer_micros = self
            .max_present_record_command_buffer_micros
            .max(draw.present_record_command_buffer_micros);
        self.total_present_submit_command_buffer_micros = self
            .total_present_submit_command_buffer_micros
            .saturating_add(draw.present_submit_command_buffer_micros);
        self.max_present_submit_command_buffer_micros = self
            .max_present_submit_command_buffer_micros
            .max(draw.present_submit_command_buffer_micros);
        self.total_present_queue_present_micros = self
            .total_present_queue_present_micros
            .saturating_add(draw.present_queue_present_micros);
        self.max_present_queue_present_micros = self
            .max_present_queue_present_micros
            .max(draw.present_queue_present_micros);
        self.total_present_wait_after_queue_present_micros = self
            .total_present_wait_after_queue_present_micros
            .saturating_add(draw.present_wait_after_queue_present_micros);
        self.max_present_wait_after_queue_present_micros = self
            .max_present_wait_after_queue_present_micros
            .max(draw.present_wait_after_queue_present_micros);
        if let Some(pts_ms) = draw.source_frame_pts_ms {
            if let Some(last) = self.last_pts_ms {
                if last > pts_ms {
                    self.pts_monotonic = false;
                } else {
                    let delta = pts_ms.saturating_sub(last);
                    if delta > 0 {
                        self.source_frame_pts_delta_min_ms = Some(
                            self.source_frame_pts_delta_min_ms
                                .map(|current| current.min(delta))
                                .unwrap_or(delta),
                        );
                        self.source_frame_pts_delta_max_ms = Some(
                            self.source_frame_pts_delta_max_ms
                                .map(|current| current.max(delta))
                                .unwrap_or(delta),
                        );
                    }
                }
            }
            self.last_pts_ms = Some(pts_ms);
        }
        if self
            .last_display_order_key
            .is_some_and(|last| last > draw.display_order_key)
        {
            self.display_order_monotonic = false;
        }
        self.last_display_order_key = Some(draw.display_order_key);
        self.uses_present_id2 |= draw.uses_present_id2;
        self.present_wait2_available |= draw.present_wait2_available;
        self.present_wait_after_present |= draw.present_wait_after_present;
        self.all_zero_copy_presented &= draw.zero_copy_presented;
        if draw.sampled_array_layer < 128 {
            self.sampled_array_layer_mask |= 1u128 << draw.sampled_array_layer;
        }
        if let Some(pts_ns) = draw.source_frame_pts_ns {
            if let Some(last) = self.last_pts_ns {
                if last > pts_ns {
                    self.pts_monotonic = false;
                } else {
                    let delta = pts_ns.saturating_sub(last);
                    if delta > 0 {
                        self.source_frame_pts_delta_min_ns = Some(
                            self.source_frame_pts_delta_min_ns
                                .map(|current| current.min(delta))
                                .unwrap_or(delta),
                        );
                        self.source_frame_pts_delta_max_ns = Some(
                            self.source_frame_pts_delta_max_ns
                                .map(|current| current.max(delta))
                                .unwrap_or(delta),
                        );
                    }
                }
            }
            self.last_pts_ns = Some(pts_ns);
        }

        if DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES == 0 {
            self.latest_draw = Some(draw);
            return;
        }
        self.latest_draw = Some(draw.clone());
        if self.draws_head.len() < DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES {
            self.draws_head.push(draw.clone());
        }
        if self.draws_tail.len() == DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES {
            self.draws_tail.remove(0);
        }
        self.draws_tail.push(draw);
    }

    pub(super) fn finish(
        self,
        present_handoff: NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot,
        execution: NativeVulkanVulkanaliaDecodedImagePresentExecutionEvidence,
    ) -> Option<NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot> {
        let latest_draw = self.latest_draw;
        if latest_draw.is_none() {
            return None;
        }
        let teardown_inclusive_elapsed = self.started_at.elapsed();
        let average_present_teardown_inclusive_fps =
            if self.presented_frame_count == 0 || teardown_inclusive_elapsed.is_zero() {
                0.0
            } else {
                f64::from(self.presented_frame_count) / teardown_inclusive_elapsed.as_secs_f64()
            };
        let present_interval_elapsed = match (
            self.first_presented_at,
            self.last_presented_at,
            self.presented_frame_count,
        ) {
            (Some(first), Some(last), presented_frame_count) if presented_frame_count > 1 => {
                last.saturating_duration_since(first)
            }
            _ => Duration::ZERO,
        };
        let average_present_fps =
            if self.presented_frame_count > 1 && !present_interval_elapsed.is_zero() {
                f64::from(self.presented_frame_count.saturating_sub(1))
                    / present_interval_elapsed.as_secs_f64()
            } else {
                average_present_teardown_inclusive_fps
            };
        let wait_tuning = native_vulkan_vulkanalia_present_wait_tuning();
        Some(NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot {
            binding: "vulkanalia",
            route: "decoded-image-dynamic-rendering-present-sequence",
            execution_model: execution.execution_model(),
            ffmpeg_thread_model: execution.ffmpeg_thread_model(),
            ffmpeg_read_thread_active: execution.ffmpeg_read_thread_active,
            video_decode_worker_active: execution.video_decode_worker_active,
            present_worker_active: execution.present_worker_active,
            decode_thread_count: execution.decode_thread_count,
            decode_async_exec_depth: execution.decode_async_exec_depth,
            requested_present_frame_count: self.requested_present_frame_count,
            submitted_present_frame_count: self.submitted_present_frame_count,
            presented_frame_count: self.presented_frame_count,
            average_present_fps,
            average_present_teardown_inclusive_fps,
            present_interval_elapsed_micros: duration_micros_u64(present_interval_elapsed),
            present_teardown_inclusive_elapsed_micros: duration_micros_u64(
                teardown_inclusive_elapsed,
            ),
            present_delta_min_micros: self.present_delta_min_micros,
            present_delta_max_micros: self.present_delta_max_micros,
            present_delta_over_6250us_count: self.present_delta_over_6250us_count,
            present_delta_over_8334us_count: self.present_delta_over_8334us_count,
            slow_frame_telemetry_limit: DECODED_IMAGE_PRESENT_SLOW_FRAME_TELEMETRY_LIMIT,
            slow_frames: self.slow_frames,
            retained_frame_telemetry_limit: DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES,
            distinct_sampled_array_layer_count: self.sampled_array_layer_mask.count_ones(),
            sampled_array_layers_head: self
                .draws_head
                .iter()
                .map(|draw| draw.sampled_array_layer)
                .collect(),
            sampled_array_layers_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.sampled_array_layer)
                .collect(),
            source_frame_pts_ns_head: self
                .draws_head
                .iter()
                .map(|draw| draw.source_frame_pts_ns)
                .collect(),
            source_frame_pts_ns_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.source_frame_pts_ns)
                .collect(),
            source_frame_pts_delta_min_ns: self.source_frame_pts_delta_min_ns,
            source_frame_pts_delta_max_ns: self.source_frame_pts_delta_max_ns,
            source_frame_duration_ns_head: self
                .draws_head
                .iter()
                .map(|draw| draw.source_frame_duration_ns)
                .collect(),
            source_frame_duration_ns_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.source_frame_duration_ns)
                .collect(),
            source_frame_pts_ms_head: self
                .draws_head
                .iter()
                .map(|draw| draw.source_frame_pts_ms)
                .collect(),
            source_frame_pts_ms_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.source_frame_pts_ms)
                .collect(),
            source_frame_pts_delta_min_ms: self.source_frame_pts_delta_min_ms,
            source_frame_pts_delta_max_ms: self.source_frame_pts_delta_max_ms,
            source_frame_duration_ms_head: self
                .draws_head
                .iter()
                .map(|draw| draw.source_frame_duration_ms)
                .collect(),
            source_frame_duration_ms_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.source_frame_duration_ms)
                .collect(),
            display_order_keys_head: self
                .draws_head
                .iter()
                .map(|draw| draw.display_order_key)
                .collect(),
            display_order_keys_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.display_order_key)
                .collect(),
            display_order_key_sources_head: self
                .draws_head
                .iter()
                .map(|draw| draw.display_order_key_source)
                .collect(),
            display_order_key_sources_tail: self
                .draws_tail
                .iter()
                .map(|draw| draw.display_order_key_source)
                .collect(),
            present_ids_head: self.draws_head.iter().map(|draw| draw.present_id).collect(),
            present_ids_tail: self.draws_tail.iter().map(|draw| draw.present_id).collect(),
            frame_sleep_count: self.frame_sleep_count,
            missed_frame_pacing_count: self.missed_frame_pacing_count,
            total_pacing_sleep_micros: self.total_pacing_sleep_micros,
            present_sleep_guard_micros: duration_micros_u64(wait_tuning.sleep_guard),
            present_spin_guard_micros: duration_micros_u64(wait_tuning.spin_guard),
            total_present_call_micros: self.total_present_call_micros,
            max_present_call_micros: self.max_present_call_micros,
            total_present_wait_frame_slot_micros: self.total_present_wait_frame_slot_micros,
            max_present_wait_frame_slot_micros: self.max_present_wait_frame_slot_micros,
            total_present_acquire_next_image_micros: self.total_present_acquire_next_image_micros,
            max_present_acquire_next_image_micros: self.max_present_acquire_next_image_micros,
            total_present_record_command_buffer_micros: self
                .total_present_record_command_buffer_micros,
            max_present_record_command_buffer_micros: self.max_present_record_command_buffer_micros,
            total_present_submit_command_buffer_micros: self
                .total_present_submit_command_buffer_micros,
            max_present_submit_command_buffer_micros: self.max_present_submit_command_buffer_micros,
            total_present_queue_present_micros: self.total_present_queue_present_micros,
            max_present_queue_present_micros: self.max_present_queue_present_micros,
            total_present_wait_after_queue_present_micros: self
                .total_present_wait_after_queue_present_micros,
            max_present_wait_after_queue_present_micros: self
                .max_present_wait_after_queue_present_micros,
            pts_monotonic: self.pts_monotonic,
            display_order_monotonic: self.display_order_monotonic,
            uses_present_id2: self.uses_present_id2,
            present_wait2_available: self.present_wait2_available,
            present_wait_after_present: self.present_wait_after_present,
            present_handoff,
            latest_draw,
            draws_head: self.draws_head,
            draws_tail: self.draws_tail,
            frame_order_model: "FFmpeg avcodec display order: AVFrame PTS is primary and present-frame index is the explicit missing-PTS fallback",
            present_resource_reuse_model: "one swapchain image-view set, one command pool, one semaphore pair, one fence set and one bounded decoded-frame handoff reused across decoded-image present frames",
            ffmpeg_retained_avframe_count: execution.ffmpeg_retained_avframe_count,
            ffmpeg_retained_avframe_peak_count: execution.ffmpeg_retained_avframe_peak_count,
            descriptor_sampler_cache_entry_count: execution.descriptor_sampler_cache_entry_count,
            descriptor_sampler_cache_peak_entry_count: execution
                .descriptor_sampler_cache_peak_entry_count,
            descriptor_sampler_cache_rewrite_count: execution
                .descriptor_sampler_cache_rewrite_count,
            descriptor_sampler_cache_recreate_count: execution
                .descriptor_sampler_cache_recreate_count,
            descriptor_sampler_cache_resource_heap_bytes: execution
                .descriptor_sampler_cache_resource_heap_bytes,
            descriptor_sampler_cache_sampler_heap_bytes: execution
                .descriptor_sampler_cache_sampler_heap_bytes,
            descriptor_sampler_cache_total_heap_bytes: execution
                .descriptor_sampler_cache_resource_heap_bytes
                .saturating_add(execution.descriptor_sampler_cache_sampler_heap_bytes),
            telemetry_retention_model: "compact head/tail/latest frame telemetry only; hot video runtime does not retain every draw snapshot",
            all_zero_copy_presented: self.all_zero_copy_presented,
            uses_dynamic_rendering: true,
            uses_synchronization2: true,
            uses_submit2: true,
            ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct NativeVulkanVulkanaliaPresentFrameTimer {
    frame_timer: Option<Instant>,
    target_max_fps: Option<u32>,
    audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    audio_master_started_at: Option<Instant>,
    last_pts_ns: Option<u64>,
    last_duration_ns: Option<u64>,
}

impl NativeVulkanVulkanaliaPresentFrameTimer {
    pub(super) fn new(
        target_max_fps: Option<u32>,
        audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    ) -> Self {
        Self {
            frame_timer: None,
            target_max_fps: target_max_fps.filter(|fps| *fps > 0),
            audio_master_clock,
            audio_master_started_at: None,
            last_pts_ns: None,
            last_duration_ns: None,
        }
    }

    pub(super) fn reset(&mut self, now: Instant) {
        self.frame_timer = Some(now);
        self.audio_master_started_at = self.audio_master_clock.enabled.then_some(now);
        self.last_pts_ns = None;
        self.last_duration_ns = None;
    }

    pub(super) fn pace_frame(
        &mut self,
        present_frame_index: u32,
        source_frame_pts_ns: Option<u64>,
        source_frame_duration_ns: Option<u64>,
        source_frame_pts_ms: Option<u64>,
        source_frame_duration_ms: Option<u64>,
    ) -> (u64, &'static str) {
        let pts_ns = source_frame_pts_ns
            .or_else(|| source_frame_pts_ms.map(|pts| pts.saturating_mul(1_000_000)));
        let duration_ns = source_frame_duration_ns.or_else(|| {
            source_frame_duration_ms.map(|duration| duration.saturating_mul(1_000_000))
        });
        let now = Instant::now();
        if self.frame_timer.is_none() || present_frame_index == 0 {
            self.frame_timer = Some(now);
            self.audio_master_started_at = self.audio_master_clock.enabled.then_some(now);
            self.last_pts_ns = pts_ns;
            self.last_duration_ns = duration_ns;
            return (
                0,
                if self.audio_master_clock.enabled {
                    "audio-clock-master-first-frame"
                } else {
                    "ffmpeg-frame-timer-first-frame"
                },
            );
        }

        if let Some((delay, clock_model)) =
            self.audio_master_delay_for_frame(now, present_frame_index, pts_ns, duration_ns)
        {
            if delay.is_zero() {
                self.last_pts_ns = pts_ns;
                self.last_duration_ns = duration_ns;
                return (0, clock_model);
            }
            let deadline = now + delay;
            let slept = native_vulkan_vulkanalia_wait_until_video_present_deadline(deadline);
            self.frame_timer = Some(deadline);
            self.last_pts_ns = pts_ns;
            self.last_duration_ns = duration_ns;
            return (
                u64::try_from(slept.as_micros()).unwrap_or(u64::MAX),
                clock_model,
            );
        }

        let (delay, clock_model) = self.next_delay(pts_ns, duration_ns);
        if delay.is_zero() {
            self.last_pts_ns = pts_ns;
            self.last_duration_ns = duration_ns;
            return (0, clock_model);
        }
        let frame_timer = self.frame_timer.unwrap_or(now);
        let deadline = frame_timer + delay;
        let wait_started_at = Instant::now();
        let slept = if deadline > wait_started_at {
            native_vulkan_vulkanalia_wait_until_video_present_deadline(deadline)
        } else {
            Duration::ZERO
        };
        self.frame_timer = Some(deadline);
        let after_wait = Instant::now();
        if after_wait > deadline
            && after_wait.duration_since(deadline) > FFMPEG_AV_SYNC_THRESHOLD_MAX
        {
            // FFmpeg's video_refresh() advances frame_timer by the nominal
            // delay, then only resynchronizes on large lateness
            // (references/gilder/ffmpeg/fftools/ffplay.c:1665-1683).
            self.frame_timer = Some(after_wait);
        }
        self.last_pts_ns = pts_ns;
        self.last_duration_ns = duration_ns;
        (
            u64::try_from(slept.as_micros()).unwrap_or(u64::MAX),
            clock_model,
        )
    }

    pub(super) fn next_delay(
        &self,
        pts_ns: Option<u64>,
        duration_ns: Option<u64>,
    ) -> (Duration, &'static str) {
        if let (Some(last_pts_ns), Some(pts_ns)) = (self.last_pts_ns, pts_ns) {
            if pts_ns > last_pts_ns {
                return (
                    Duration::from_nanos(pts_ns - last_pts_ns),
                    "ffmpeg-frame-timer-pts-delta-sleep",
                );
            }
        }
        if let Some(last_duration_ns) = self.last_duration_ns.filter(|duration| *duration > 0) {
            return (
                Duration::from_nanos(last_duration_ns),
                "ffmpeg-frame-timer-last-duration-sleep",
            );
        }
        if let Some(duration_ns) = duration_ns.filter(|duration| *duration > 0) {
            return (
                Duration::from_nanos(duration_ns),
                "ffmpeg-frame-timer-duration-sleep",
            );
        }
        if let Some(target_max_fps) = self.target_max_fps {
            return (
                native_vulkan_vulkanalia_frame_count_duration(1, target_max_fps),
                "ffmpeg-frame-timer-target-fps-sleep",
            );
        }
        (Duration::ZERO, "unpaced-no-video-clock")
    }

    pub(super) fn audio_master_delay_for_frame(
        &self,
        now: Instant,
        present_frame_index: u32,
        pts_ns: Option<u64>,
        duration_ns: Option<u64>,
    ) -> Option<(Duration, &'static str)> {
        if !self.audio_master_clock.enabled || present_frame_index == 0 {
            return None;
        }
        let master_clock_ns = self.audio_master_clock_ns(now)?;
        let video_clock_ns =
            self.current_video_clock_ns(present_frame_index, pts_ns, duration_ns)?;
        if video_clock_ns <= master_clock_ns {
            return Some((Duration::ZERO, "audio-clock-master-video-late-no-sleep"));
        }
        let delay_ns = video_clock_ns.saturating_sub(master_clock_ns);
        Some((
            Duration::from_nanos(delay_ns),
            "audio-clock-master-pts-sync-sleep",
        ))
    }

    pub(super) fn audio_master_clock_ns(&self, now: Instant) -> Option<u64> {
        let started_at = self.audio_master_started_at?;
        Some(
            self.audio_master_clock
                .start_clock_ns
                .unwrap_or(0)
                .saturating_add(
                    u64::try_from(now.duration_since(started_at).as_nanos()).unwrap_or(u64::MAX),
                ),
        )
    }

    pub(super) fn current_video_clock_ns(
        &self,
        present_frame_index: u32,
        pts_ns: Option<u64>,
        duration_ns: Option<u64>,
    ) -> Option<u64> {
        if let Some(pts_ns) = pts_ns {
            return Some(pts_ns);
        }
        if let (Some(last_pts_ns), Some(duration_ns)) =
            (self.last_pts_ns, self.last_duration_ns.or(duration_ns))
        {
            if duration_ns > 0 {
                return Some(last_pts_ns.saturating_add(duration_ns));
            }
        }
        self.target_max_fps.filter(|fps| *fps > 0).map(|fps| {
            let clock_ns = (u128::from(present_frame_index) * 1_000_000_000u128) / u128::from(fps);
            u64::try_from(clock_ns).unwrap_or(u64::MAX)
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct NativeVulkanVulkanaliaPresentWaitTuning {
    sleep_guard: Duration,
    spin_guard: Duration,
}

pub(super) fn native_vulkan_vulkanalia_present_wait_tuning() -> NativeVulkanVulkanaliaPresentWaitTuning {
    static TUNING: OnceLock<NativeVulkanVulkanaliaPresentWaitTuning> = OnceLock::new();
    *TUNING.get_or_init(|| {
        let sleep_guard = native_vulkan_duration_from_env_micros(
            VIDEO_PRESENT_SLEEP_GUARD_ENV,
            VIDEO_PRESENT_SLEEP_GUARD_DEFAULT_MICROS,
        );
        let spin_guard = native_vulkan_duration_from_env_micros(
            VIDEO_PRESENT_SPIN_GUARD_ENV,
            VIDEO_PRESENT_SPIN_GUARD_DEFAULT_MICROS,
        )
        .min(sleep_guard);
        NativeVulkanVulkanaliaPresentWaitTuning {
            sleep_guard,
            spin_guard,
        }
    })
}

pub(super) fn native_vulkan_duration_from_env_micros(name: &str, default_micros: u64) -> Duration {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_micros)
        .unwrap_or_else(|| Duration::from_micros(default_micros))
}

pub(super) fn native_vulkan_vulkanalia_wait_until_video_present_deadline(deadline: Instant) -> Duration {
    let started_at = Instant::now();
    let wait_tuning = native_vulkan_vulkanalia_present_wait_tuning();
    loop {
        let now = Instant::now();
        if now >= deadline {
            return now.saturating_duration_since(started_at);
        }
        let remaining = deadline.duration_since(now);
        if remaining > wait_tuning.sleep_guard {
            thread::sleep(remaining - wait_tuning.sleep_guard);
        } else if remaining > wait_tuning.spin_guard {
            thread::yield_now();
        } else {
            std::hint::spin_loop();
        }
    }
}

pub(super) fn native_vulkan_vulkanalia_frame_count_duration(
    frame_count: u32,
    target_max_fps: u32,
) -> Duration {
    let fps = u128::from(target_max_fps.max(1));
    let nanos = u128::from(frame_count).saturating_mul(1_000_000_000u128) / fps;
    Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64)
}

pub(super) fn duration_micros_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

pub(super) fn native_vulkan_vulkanalia_ffmpeg_decode_async_exec_depth(video_queue_count: u32) -> u32 {
    let queue_context_count = video_queue_count.max(1);
    let thread_count = FFMPEG_SINGLE_DECODE_THREAD_COUNT.max(1);
    // Exact FFmpeg Vulkan decode async-depth formula for this runtime's single
    // decode worker thread (references/gilder/ffmpeg/libavcodec/vulkan_decode.c:1368-1378).
    queue_context_count
        .saturating_mul(2)
        .min(thread_count.saturating_mul(2))
        .max(thread_count)
        .max(1)
}
