//! Shared direct Vulkan Video runtime summary helpers.
//!
//! This is the codec-neutral part of the direct route: codec adapters own
//! parser/reference/DPB rules, while the runtime reports FFmpeg-like
//! packet-to-frame/display ownership evidence with the same fields for
//! H.264, H.265 and AV1.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::NativeVulkanError;
use super::direct_zero_copy::{
    NATIVE_VULKAN_DIRECT_DECODED_FRAME_ZERO_COPY_SCOPE,
    native_vulkan_direct_decoded_frame_zero_copy_status,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanDirectDisplayHandoffMetrics {
    pub(super) display_copy_count: u32,
    pub(super) display_ring_memory_bytes: u64,
    pub(super) displayed_direct_dpb_count: Option<u32>,
    pub(super) display_handoff_strategy: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct NativeVulkanDirectRuntimeSummary {
    pub(super) runtime_elapsed_ms: u64,
    pub(super) average_present_fps: f64,
    pub(super) decoded_frame_zero_copy_scope: &'static str,
    pub(super) decoded_frame_zero_copy_status: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct NativeVulkanDirectPresentResultSummary {
    pub(super) average_present_result_fps: f64,
    pub(super) average_present_result_drop_first_fps: f64,
    pub(super) average_present_result_drop_first_60_fps: f64,
    pub(super) present_result_first_interval_us: u64,
    pub(super) present_result_max_interval_us: u64,
    pub(super) present_result_max_interval_after_warmup_us: u64,
    pub(super) present_result_over_budget_count: u32,
    pub(super) present_result_over_budget_after_warmup_count: u32,
    pub(super) present_result_missed_vblank_threshold_us: u64,
    pub(super) present_result_missed_vblank_count: u32,
    pub(super) present_result_missed_vblank_after_warmup_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanDirectPresentTiming {
    pub(super) frame_index: usize,
    pub(super) acquire_elapsed_us: u64,
    pub(super) acquire_not_ready_count: u32,
    pub(super) record_elapsed_us: u64,
    pub(super) queue_submit_elapsed_us: u64,
    pub(super) queue_present_elapsed_us: u64,
    pub(super) present_elapsed_us: u64,
    pub(super) present_result_since_start_us: u64,
}

pub(super) trait NativeVulkanDirectPresentTimedFrame {
    fn apply_direct_present_timing(&mut self, timing: NativeVulkanDirectPresentTiming);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanDirectOptionalPresentTiming {
    pub(super) frame_index: usize,
    pub(super) acquire_elapsed_us: Option<u64>,
    pub(super) acquire_start_since_start_us: Option<u64>,
    pub(super) acquire_end_since_start_us: Option<u64>,
    pub(super) record_elapsed_us: Option<u64>,
    pub(super) queue_submit_elapsed_us: u64,
    pub(super) queue_present_elapsed_us: u64,
    pub(super) present_elapsed_us: u64,
    pub(super) present_submit_start_since_start_us: Option<u64>,
    pub(super) queue_present_start_since_start_us: Option<u64>,
    pub(super) present_result_since_start_us: u64,
}

pub(super) trait NativeVulkanDirectOptionalPresentTimedFrame {
    fn apply_direct_optional_present_timing(
        &mut self,
        timing: NativeVulkanDirectOptionalPresentTiming,
    );
}

pub(super) trait NativeVulkanDirectPresentPendingContext {
    fn clear_direct_present_pending(&mut self);
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanDirectPresentWaitStats {
    pub(super) wait_count: u32,
    pub(super) elapsed_us: u64,
    pub(super) max_us: u64,
}

impl NativeVulkanDirectPresentWaitStats {
    pub(super) fn record_wait_elapsed_us(&mut self, elapsed_us: u64) {
        self.wait_count = self.wait_count.saturating_add(1);
        self.elapsed_us = self.elapsed_us.saturating_add(elapsed_us);
        self.max_us = self.max_us.max(elapsed_us);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanDirectPresentBackpressure {
    max_pending_results: u32,
    pending_results: u32,
}

impl NativeVulkanDirectPresentBackpressure {
    pub(super) fn new(max_pending_results: u32) -> Self {
        Self {
            max_pending_results: max_pending_results.max(1),
            pending_results: 0,
        }
    }

    pub(super) fn max_pending_results(&self) -> u32 {
        self.max_pending_results
    }

    pub(super) fn pending_results(&self) -> u32 {
        self.pending_results
    }

    pub(super) fn should_wait_for_result(&self) -> bool {
        self.pending_results >= self.max_pending_results
    }

    pub(super) fn has_pending_results(&self) -> bool {
        self.pending_results > 0
    }

    pub(super) fn record_submitted_result(&mut self) {
        self.pending_results = self.pending_results.saturating_add(1);
    }

    pub(super) fn record_completed_result(&mut self) {
        self.pending_results = self.pending_results.saturating_sub(1);
    }
}

pub(super) fn native_vulkan_direct_pending_flag_count<I>(pending_flags: I) -> usize
where
    I: IntoIterator<Item = bool>,
{
    pending_flags.into_iter().filter(|pending| *pending).count()
}

pub(super) fn native_vulkan_direct_has_pending_flags<I>(pending_flags: I) -> bool
where
    I: IntoIterator<Item = bool>,
{
    pending_flags.into_iter().any(|pending| pending)
}

pub(super) fn native_vulkan_direct_pending_flags_reached_limit<I>(
    pending_flags: I,
    max_pending_results: usize,
) -> bool
where
    I: IntoIterator<Item = bool>,
{
    max_pending_results > 0
        && native_vulkan_direct_pending_flag_count(pending_flags) >= max_pending_results
}

pub(super) fn native_vulkan_direct_clear_pending_present_context<C>(
    codec_label: &'static str,
    contexts: &mut [C],
    context_index: usize,
) -> Result<(), NativeVulkanError>
where
    C: NativeVulkanDirectPresentPendingContext,
{
    let context_count = contexts.len();
    let context = contexts.get_mut(context_index).ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "{codec_label} present worker returned context index {context_index} but only {context_count} context(s) exist",
        ))
    })?;
    context.clear_direct_present_pending();
    Ok(())
}

pub(super) fn native_vulkan_direct_apply_present_result_with_pending_context<T, C, F>(
    codec_label: &'static str,
    contexts: &mut [C],
    context_index: Option<usize>,
    result: Result<T, NativeVulkanError>,
    apply: F,
) -> Result<(), NativeVulkanError>
where
    C: NativeVulkanDirectPresentPendingContext,
    F: FnOnce(Result<T, NativeVulkanError>) -> Result<(), NativeVulkanError>,
{
    apply(result)?;
    if let Some(context_index) = context_index {
        native_vulkan_direct_clear_pending_present_context(codec_label, contexts, context_index)?;
    }
    Ok(())
}

pub(super) fn native_vulkan_direct_recv_present_result<T>(
    present_result_rx: &mpsc::Receiver<T>,
    wait_stats: &mut NativeVulkanDirectPresentWaitStats,
    disconnected_message: &'static str,
) -> Result<(T, u64), NativeVulkanError> {
    let present_result_wait_started_at = Instant::now();
    let present_result = present_result_rx
        .recv()
        .map_err(|_| NativeVulkanError::Video(disconnected_message.to_owned()))?;
    let present_result_wait_elapsed_us =
        native_vulkan_direct_elapsed_us(present_result_wait_started_at.elapsed());
    wait_stats.record_wait_elapsed_us(present_result_wait_elapsed_us);
    Ok((present_result, present_result_wait_elapsed_us))
}

pub(super) fn native_vulkan_direct_recv_pending_present_result<T>(
    present_result_rx: &mpsc::Receiver<T>,
    backpressure: &mut NativeVulkanDirectPresentBackpressure,
    wait_stats: &mut NativeVulkanDirectPresentWaitStats,
    disconnected_message: &'static str,
) -> Result<(T, u64), NativeVulkanError> {
    let result = native_vulkan_direct_recv_present_result(
        present_result_rx,
        wait_stats,
        disconnected_message,
    )?;
    backpressure.record_completed_result();
    Ok(result)
}

pub(super) fn native_vulkan_direct_try_recv_pending_present_result<T>(
    present_result_rx: &mpsc::Receiver<T>,
    backpressure: &mut NativeVulkanDirectPresentBackpressure,
    disconnected_pending_message: &'static str,
) -> Result<Option<T>, NativeVulkanError> {
    match present_result_rx.try_recv() {
        Ok(present_result) => {
            backpressure.record_completed_result();
            Ok(Some(present_result))
        }
        Err(mpsc::TryRecvError::Empty) => Ok(None),
        Err(mpsc::TryRecvError::Disconnected) => {
            if !backpressure.has_pending_results() {
                Ok(None)
            } else {
                Err(NativeVulkanError::Video(
                    disconnected_pending_message.to_owned(),
                ))
            }
        }
    }
}

pub(super) fn native_vulkan_direct_apply_present_result<F>(
    codec_label: &'static str,
    frames: &mut [F],
    acquire_not_ready_count: &mut u32,
    timing: Result<NativeVulkanDirectPresentTiming, NativeVulkanError>,
) -> Result<(), NativeVulkanError>
where
    F: NativeVulkanDirectPresentTimedFrame,
{
    let timing = timing?;
    let frame_count = frames.len();
    let frame = frames.get_mut(timing.frame_index).ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "{codec_label} present worker returned frame index {} but only {frame_count} frame(s) are recorded",
            timing.frame_index
        ))
    })?;
    *acquire_not_ready_count =
        acquire_not_ready_count.saturating_add(timing.acquire_not_ready_count);
    frame.apply_direct_present_timing(timing);
    Ok(())
}

pub(super) fn native_vulkan_direct_apply_optional_present_result<F>(
    codec_label: &'static str,
    frames: &mut [F],
    timing: Result<NativeVulkanDirectOptionalPresentTiming, NativeVulkanError>,
) -> Result<(), NativeVulkanError>
where
    F: NativeVulkanDirectOptionalPresentTimedFrame,
{
    let timing = timing?;
    let frame_count = frames.len();
    let frame = frames.get_mut(timing.frame_index).ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "{codec_label} present worker returned frame index {} but only {frame_count} frame(s) are recorded",
            timing.frame_index
        ))
    })?;
    frame.apply_direct_optional_present_timing(timing);
    Ok(())
}

pub(super) fn native_vulkan_direct_runtime_summary(
    elapsed: Duration,
    presented_frame_count: u32,
    handoff: NativeVulkanDirectDisplayHandoffMetrics,
) -> NativeVulkanDirectRuntimeSummary {
    NativeVulkanDirectRuntimeSummary {
        runtime_elapsed_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
        average_present_fps: if elapsed.is_zero() {
            0.0
        } else {
            f64::from(presented_frame_count) / elapsed.as_secs_f64()
        },
        decoded_frame_zero_copy_scope: NATIVE_VULKAN_DIRECT_DECODED_FRAME_ZERO_COPY_SCOPE,
        decoded_frame_zero_copy_status: native_vulkan_direct_decoded_frame_zero_copy_status(
            presented_frame_count,
            handoff.display_copy_count,
            handoff.display_ring_memory_bytes,
            handoff.displayed_direct_dpb_count,
            handoff.display_handoff_strategy,
        ),
    }
}

fn native_vulkan_direct_elapsed_us(value: Duration) -> u64 {
    value.as_micros().min(u128::from(u64::MAX)) as u64
}

pub(super) fn native_vulkan_direct_present_result_summary<I>(
    target_max_fps: Option<u32>,
    present_result_times_us: I,
) -> NativeVulkanDirectPresentResultSummary
where
    I: IntoIterator<Item = u64>,
{
    let present_result_times = present_result_times_us
        .into_iter()
        .filter(|time| *time != 0)
        .collect::<Vec<_>>();
    let present_result_warmup_frame_count = 60usize;
    let present_result_budget_us = target_max_fps
        .map(|fps| {
            let fps = u64::from(fps.max(1));
            1_000_000u64.div_ceil(fps)
        })
        .unwrap_or(0);
    let present_result_missed_vblank_threshold_us = present_result_budget_us
        .saturating_mul(3)
        .checked_div(2)
        .unwrap_or(0);

    let mut present_result_first_interval_us = 0u64;
    let mut present_result_max_interval_us = 0u64;
    let mut present_result_max_interval_after_warmup_us = 0u64;
    let mut present_result_over_budget_count = 0u32;
    let mut present_result_over_budget_after_warmup_count = 0u32;
    let mut present_result_missed_vblank_count = 0u32;
    let mut present_result_missed_vblank_after_warmup_count = 0u32;
    for (index, window) in present_result_times.windows(2).enumerate() {
        let interval = window[1].saturating_sub(window[0]);
        if index == 0 {
            present_result_first_interval_us = interval;
        }
        present_result_max_interval_us = present_result_max_interval_us.max(interval);
        if index >= present_result_warmup_frame_count {
            present_result_max_interval_after_warmup_us =
                present_result_max_interval_after_warmup_us.max(interval);
        }
        if present_result_budget_us != 0 && interval > present_result_budget_us {
            present_result_over_budget_count = present_result_over_budget_count.saturating_add(1);
            if index >= present_result_warmup_frame_count {
                present_result_over_budget_after_warmup_count =
                    present_result_over_budget_after_warmup_count.saturating_add(1);
            }
        }
        if present_result_missed_vblank_threshold_us != 0
            && interval > present_result_missed_vblank_threshold_us
        {
            present_result_missed_vblank_count =
                present_result_missed_vblank_count.saturating_add(1);
            if index >= present_result_warmup_frame_count {
                present_result_missed_vblank_after_warmup_count =
                    present_result_missed_vblank_after_warmup_count.saturating_add(1);
            }
        }
    }

    let average_present_result_fps = if present_result_times.len() > 1 {
        let first = present_result_times[0];
        let last = present_result_times[present_result_times.len() - 1];
        if last > first {
            (present_result_times.len().saturating_sub(1) as f64) * 1_000_000.0
                / (last - first) as f64
        } else {
            0.0
        }
    } else {
        0.0
    };
    let average_present_result_drop_first_fps = if present_result_times.len() > 2 {
        let second = present_result_times[1];
        let last = present_result_times[present_result_times.len() - 1];
        if last > second {
            (present_result_times.len().saturating_sub(2) as f64) * 1_000_000.0
                / (last - second) as f64
        } else {
            0.0
        }
    } else {
        0.0
    };
    let average_present_result_drop_first_60_fps =
        if present_result_times.len() > present_result_warmup_frame_count + 1 {
            let first_after_warmup = present_result_times[present_result_warmup_frame_count];
            let last = present_result_times[present_result_times.len() - 1];
            if last > first_after_warmup {
                (present_result_times
                    .len()
                    .saturating_sub(present_result_warmup_frame_count + 1) as f64)
                    * 1_000_000.0
                    / (last - first_after_warmup) as f64
            } else {
                0.0
            }
        } else {
            0.0
        };

    NativeVulkanDirectPresentResultSummary {
        average_present_result_fps,
        average_present_result_drop_first_fps,
        average_present_result_drop_first_60_fps,
        present_result_first_interval_us,
        present_result_max_interval_us,
        present_result_max_interval_after_warmup_us,
        present_result_over_budget_count,
        present_result_over_budget_after_warmup_count,
        present_result_missed_vblank_threshold_us,
        present_result_missed_vblank_count,
        present_result_missed_vblank_after_warmup_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    struct TestPresentFrame {
        timing: Option<NativeVulkanDirectPresentTiming>,
    }

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    struct TestOptionalPresentFrame {
        timing: Option<NativeVulkanDirectOptionalPresentTiming>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestPendingContext {
        pending: bool,
    }

    impl NativeVulkanDirectPresentTimedFrame for TestPresentFrame {
        fn apply_direct_present_timing(&mut self, timing: NativeVulkanDirectPresentTiming) {
            self.timing = Some(timing);
        }
    }

    impl NativeVulkanDirectOptionalPresentTimedFrame for TestOptionalPresentFrame {
        fn apply_direct_optional_present_timing(
            &mut self,
            timing: NativeVulkanDirectOptionalPresentTiming,
        ) {
            self.timing = Some(timing);
        }
    }

    impl NativeVulkanDirectPresentPendingContext for TestPendingContext {
        fn clear_direct_present_pending(&mut self) {
            self.pending = false;
        }
    }

    #[test]
    fn summary_reports_present_rate_and_zero_copy_status() {
        let summary = native_vulkan_direct_runtime_summary(
            Duration::from_secs(10),
            2400,
            NativeVulkanDirectDisplayHandoffMetrics {
                display_copy_count: 0,
                display_ring_memory_bytes: 0,
                displayed_direct_dpb_count: Some(2400),
                display_handoff_strategy: "direct-sampled-dpb-output",
            },
        );

        assert_eq!(summary.runtime_elapsed_ms, 10_000);
        assert_eq!(summary.average_present_fps, 240.0);
        assert_eq!(
            summary.decoded_frame_zero_copy_status,
            "confirmed-direct-dpb-no-display-copy"
        );
    }

    #[test]
    fn summary_keeps_zero_duration_rate_defined() {
        let summary = native_vulkan_direct_runtime_summary(
            Duration::ZERO,
            1,
            NativeVulkanDirectDisplayHandoffMetrics {
                display_copy_count: 0,
                display_ring_memory_bytes: 0,
                displayed_direct_dpb_count: Some(1),
                display_handoff_strategy: "direct-sampled-dpb-output",
            },
        );

        assert_eq!(summary.runtime_elapsed_ms, 0);
        assert_eq!(summary.average_present_fps, 0.0);
        assert_eq!(
            summary.decoded_frame_zero_copy_scope,
            NATIVE_VULKAN_DIRECT_DECODED_FRAME_ZERO_COPY_SCOPE
        );
    }

    #[test]
    fn present_result_summary_reports_rate_and_budget_misses() {
        let summary = native_vulkan_direct_present_result_summary(
            Some(240),
            [10_000, 14_167, 18_334, 28_334],
        );

        assert!((summary.average_present_result_fps - 163.63).abs() < 0.01);
        assert!((summary.average_present_result_drop_first_fps - 141.17).abs() < 0.01);
        assert_eq!(summary.present_result_first_interval_us, 4_167);
        assert_eq!(summary.present_result_max_interval_us, 10_000);
        assert_eq!(summary.present_result_over_budget_count, 1);
        assert_eq!(summary.present_result_missed_vblank_threshold_us, 6_250);
        assert_eq!(summary.present_result_missed_vblank_count, 1);
    }

    #[test]
    fn present_result_summary_filters_zero_placeholders() {
        let summary =
            native_vulkan_direct_present_result_summary(Some(60), [0, 1_000, 17_667, 34_334]);

        assert_eq!(summary.present_result_first_interval_us, 16_667);
        assert_eq!(summary.present_result_over_budget_count, 0);
        assert_eq!(summary.present_result_missed_vblank_count, 0);
    }

    #[test]
    fn present_wait_stats_accumulate_waits_and_keep_max() {
        let mut stats = NativeVulkanDirectPresentWaitStats::default();

        stats.record_wait_elapsed_us(120);
        stats.record_wait_elapsed_us(30);
        stats.record_wait_elapsed_us(500);

        assert_eq!(stats.wait_count, 3);
        assert_eq!(stats.elapsed_us, 650);
        assert_eq!(stats.max_us, 500);
    }

    #[test]
    fn present_backpressure_clamps_depth_and_tracks_pending_results() {
        let mut backpressure = NativeVulkanDirectPresentBackpressure::new(0);

        assert_eq!(backpressure.max_pending_results(), 1);
        assert_eq!(backpressure.pending_results(), 0);
        assert!(!backpressure.should_wait_for_result());

        backpressure.record_submitted_result();
        assert_eq!(backpressure.pending_results(), 1);
        assert!(backpressure.should_wait_for_result());
        assert!(backpressure.has_pending_results());

        backpressure.record_completed_result();
        backpressure.record_completed_result();
        assert_eq!(backpressure.pending_results(), 0);
        assert!(!backpressure.has_pending_results());
    }

    #[test]
    fn pending_flag_helpers_count_any_and_limit() {
        assert_eq!(
            native_vulkan_direct_pending_flag_count([true, false, true]),
            2
        );
        assert!(native_vulkan_direct_has_pending_flags([false, true]));
        assert!(!native_vulkan_direct_has_pending_flags([false, false]));
        assert!(native_vulkan_direct_pending_flags_reached_limit(
            [true, false, true],
            2
        ));
        assert!(!native_vulkan_direct_pending_flags_reached_limit(
            [true, false],
            2
        ));
        assert!(!native_vulkan_direct_pending_flags_reached_limit(
            [true, false],
            0
        ));
    }

    #[test]
    fn apply_present_result_with_pending_context_clears_indexed_context_after_apply() {
        let mut contexts = vec![
            TestPendingContext { pending: true },
            TestPendingContext { pending: true },
        ];
        let mut applied = None;

        native_vulkan_direct_apply_present_result_with_pending_context(
            "test",
            &mut contexts,
            Some(1),
            Ok(42u32),
            |result| {
                applied = Some(result?);
                Ok(())
            },
        )
        .expect("apply present result with pending context");

        assert_eq!(applied, Some(42));
        assert!(contexts[0].pending);
        assert!(!contexts[1].pending);
    }

    #[test]
    fn apply_present_result_with_pending_context_rejects_missing_context() {
        let mut contexts = vec![TestPendingContext { pending: true }];

        let err = native_vulkan_direct_apply_present_result_with_pending_context(
            "test",
            &mut contexts,
            Some(3),
            Ok(42u32),
            |result| {
                let _ = result?;
                Ok(())
            },
        )
        .expect_err("missing pending context must fail");

        assert!(contexts[0].pending);
        assert!(
            err.to_string()
                .contains("test present worker returned context index 3")
        );
    }

    #[test]
    fn apply_present_result_with_pending_context_preserves_pending_on_apply_error() {
        let mut contexts = vec![TestPendingContext { pending: true }];

        let err = native_vulkan_direct_apply_present_result_with_pending_context(
            "test",
            &mut contexts,
            Some(0),
            Ok(42u32),
            |_result| Err(NativeVulkanError::Video("apply failed".to_owned())),
        )
        .expect_err("apply error must fail");

        assert!(contexts[0].pending);
        assert!(err.to_string().contains("apply failed"));
    }

    #[test]
    fn recv_present_result_records_wait_and_returns_payload() {
        let (tx, rx) = mpsc::channel();
        tx.send(42u32).expect("send present result");
        let mut stats = NativeVulkanDirectPresentWaitStats::default();

        let (payload, elapsed_us) =
            native_vulkan_direct_recv_present_result(&rx, &mut stats, "present worker exited")
                .expect("receive present result");

        assert_eq!(payload, 42);
        assert_eq!(stats.wait_count, 1);
        assert_eq!(stats.elapsed_us, elapsed_us);
        assert_eq!(stats.max_us, elapsed_us);
    }

    #[test]
    fn recv_present_result_reports_disconnected_worker_without_recording_wait() {
        let (tx, rx) = mpsc::channel::<u32>();
        drop(tx);
        let mut stats = NativeVulkanDirectPresentWaitStats::default();

        let err =
            native_vulkan_direct_recv_present_result(&rx, &mut stats, "present worker exited")
                .expect_err("disconnected worker must fail");

        assert_eq!(stats, NativeVulkanDirectPresentWaitStats::default());
        assert!(err.to_string().contains("present worker exited"));
    }

    #[test]
    fn recv_pending_present_result_records_wait_and_decrements_pending() {
        let (tx, rx) = mpsc::channel();
        tx.send(42u32).expect("send present result");
        let mut stats = NativeVulkanDirectPresentWaitStats::default();
        let mut backpressure = NativeVulkanDirectPresentBackpressure::new(2);
        backpressure.record_submitted_result();

        let (payload, elapsed_us) = native_vulkan_direct_recv_pending_present_result(
            &rx,
            &mut backpressure,
            &mut stats,
            "present worker exited",
        )
        .expect("receive pending present result");

        assert_eq!(payload, 42);
        assert_eq!(backpressure.pending_results(), 0);
        assert_eq!(stats.wait_count, 1);
        assert_eq!(stats.elapsed_us, elapsed_us);
    }

    #[test]
    fn try_recv_pending_present_result_decrements_pending_for_payload() {
        let (tx, rx) = mpsc::channel();
        tx.send(7u32).expect("send present result");
        let mut backpressure = NativeVulkanDirectPresentBackpressure::new(4);
        backpressure.record_submitted_result();
        backpressure.record_submitted_result();

        let payload = native_vulkan_direct_try_recv_pending_present_result(
            &rx,
            &mut backpressure,
            "present worker exited with pending results",
        )
        .expect("try recv present result");

        assert_eq!(payload, Some(7));
        assert_eq!(backpressure.pending_results(), 1);
    }

    #[test]
    fn try_recv_pending_present_result_keeps_pending_when_empty() {
        let (_tx, rx) = mpsc::channel::<u32>();
        let mut backpressure = NativeVulkanDirectPresentBackpressure::new(4);
        backpressure.record_submitted_result();
        backpressure.record_submitted_result();

        let payload = native_vulkan_direct_try_recv_pending_present_result(
            &rx,
            &mut backpressure,
            "present worker exited with pending results",
        )
        .expect("try recv empty channel");

        assert_eq!(payload, None);
        assert_eq!(backpressure.pending_results(), 2);
    }

    #[test]
    fn try_recv_pending_present_result_treats_disconnected_zero_pending_as_drained() {
        let (tx, rx) = mpsc::channel::<u32>();
        drop(tx);
        let mut backpressure = NativeVulkanDirectPresentBackpressure::new(4);

        let payload = native_vulkan_direct_try_recv_pending_present_result(
            &rx,
            &mut backpressure,
            "present worker exited with pending results",
        )
        .expect("disconnected without pending results");

        assert_eq!(payload, None);
        assert_eq!(backpressure.pending_results(), 0);
    }

    #[test]
    fn try_recv_pending_present_result_errors_when_disconnected_with_pending() {
        let (tx, rx) = mpsc::channel::<u32>();
        drop(tx);
        let mut backpressure = NativeVulkanDirectPresentBackpressure::new(4);
        backpressure.record_submitted_result();

        let err = native_vulkan_direct_try_recv_pending_present_result(
            &rx,
            &mut backpressure,
            "present worker exited with pending results",
        )
        .expect_err("disconnected with pending results must fail");

        assert_eq!(backpressure.pending_results(), 1);
        assert!(
            err.to_string()
                .contains("present worker exited with pending results")
        );
    }

    #[test]
    fn applies_present_timing_to_indexed_frame_and_accumulates_acquire_not_ready() {
        let mut frames = vec![TestPresentFrame::default(); 2];
        let mut acquire_not_ready_count = 7;
        let timing = NativeVulkanDirectPresentTiming {
            frame_index: 1,
            acquire_elapsed_us: 10,
            acquire_not_ready_count: 3,
            record_elapsed_us: 20,
            queue_submit_elapsed_us: 30,
            queue_present_elapsed_us: 40,
            present_elapsed_us: 50,
            present_result_since_start_us: 60,
        };

        native_vulkan_direct_apply_present_result(
            "test",
            &mut frames,
            &mut acquire_not_ready_count,
            Ok(timing),
        )
        .expect("apply present result");

        assert_eq!(acquire_not_ready_count, 10);
        assert_eq!(frames[0].timing, None);
        assert_eq!(frames[1].timing, Some(timing));
    }

    #[test]
    fn rejects_present_timing_for_missing_frame() {
        let mut frames = vec![TestPresentFrame::default(); 1];
        let mut acquire_not_ready_count = 0;
        let err = native_vulkan_direct_apply_present_result(
            "test",
            &mut frames,
            &mut acquire_not_ready_count,
            Ok(NativeVulkanDirectPresentTiming {
                frame_index: 4,
                acquire_elapsed_us: 0,
                acquire_not_ready_count: 1,
                record_elapsed_us: 0,
                queue_submit_elapsed_us: 0,
                queue_present_elapsed_us: 0,
                present_elapsed_us: 0,
                present_result_since_start_us: 0,
            }),
        )
        .expect_err("missing frame must fail");

        assert_eq!(acquire_not_ready_count, 0);
        assert!(
            err.to_string()
                .contains("test present worker returned frame index 4")
        );
    }

    #[test]
    fn applies_optional_present_timing_without_acquire_not_ready_accounting() {
        let mut frames = vec![TestOptionalPresentFrame::default(); 2];
        let timing = NativeVulkanDirectOptionalPresentTiming {
            frame_index: 1,
            acquire_elapsed_us: None,
            acquire_start_since_start_us: None,
            acquire_end_since_start_us: None,
            record_elapsed_us: Some(20),
            queue_submit_elapsed_us: 30,
            queue_present_elapsed_us: 40,
            present_elapsed_us: 50,
            present_submit_start_since_start_us: Some(60),
            queue_present_start_since_start_us: Some(70),
            present_result_since_start_us: 80,
        };

        native_vulkan_direct_apply_optional_present_result("test", &mut frames, Ok(timing))
            .expect("apply optional present result");

        assert_eq!(frames[0].timing, None);
        assert_eq!(frames[1].timing, Some(timing));
    }
}
