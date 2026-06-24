//! Shared direct Vulkan Video runtime summary helpers.
//!
//! This is the codec-neutral part of the direct route: codec adapters own
//! parser/reference/DPB rules, while the runtime reports FFmpeg-like
//! packet-to-frame/display ownership evidence with the same fields for
//! H.264, H.265 and AV1.

use std::time::Duration;

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

    impl NativeVulkanDirectPresentTimedFrame for TestPresentFrame {
        fn apply_direct_present_timing(&mut self, timing: NativeVulkanDirectPresentTiming) {
            self.timing = Some(timing);
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
}
