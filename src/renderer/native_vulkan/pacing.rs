use std::thread;
use std::time::{Duration, Instant};

use ash::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanVideoPacingPlan {
    pub(super) strategy: &'static str,
    pub(super) frame_interval: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeVulkanVideoPacingMaster {
    TargetFps,
    AudioClock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanVideoClockPaceResult {
    pub(super) slept: bool,
    pub(super) sleep_duration: Duration,
    pub(super) late_duration: Option<Duration>,
}

#[derive(Debug, Clone)]
pub(super) struct NativeVulkanVideoClockPacer {
    target_fps: Option<u32>,
    spin_margin: Duration,
    frame_timer: Instant,
    advanced_frame_count: u64,
    resync_threshold: Duration,
}

pub(super) fn native_vulkan_video_pacing_plan(
    present_mode: vk::PresentModeKHR,
    target_max_fps: Option<u32>,
) -> NativeVulkanVideoPacingPlan {
    let frame_interval = target_max_fps
        .filter(|fps| *fps > 0)
        .map(|fps| Duration::from_secs_f64(1.0 / fps as f64));
    let strategy = match (
        frame_interval.is_some(),
        present_mode == vk::PresentModeKHR::FIFO,
    ) {
        (true, true) => "target-fps-cpu-sleep-with-fifo-present",
        (true, false) => "target-fps-cpu-sleep",
        (false, true) => "fifo-present-blocking-no-cpu-sleep",
        (false, false) => "unlimited",
    };

    NativeVulkanVideoPacingPlan {
        strategy,
        frame_interval,
    }
}

pub(super) fn native_vulkan_video_pacing_master(
    audio_clock_probe_enabled: bool,
) -> NativeVulkanVideoPacingMaster {
    if audio_clock_probe_enabled
        && std::env::var("GILDER_VIDEO_PACING_MASTER")
            .or_else(|_| std::env::var("GILDER_PACING_MASTER"))
            .map_or(false, |value| value.eq_ignore_ascii_case("audio"))
    {
        NativeVulkanVideoPacingMaster::AudioClock
    } else {
        NativeVulkanVideoPacingMaster::TargetFps
    }
}

pub(super) fn native_vulkan_video_pacing_strategy_label(
    base_strategy: &'static str,
    pacing_master: NativeVulkanVideoPacingMaster,
) -> &'static str {
    match pacing_master {
        NativeVulkanVideoPacingMaster::TargetFps => base_strategy,
        NativeVulkanVideoPacingMaster::AudioClock => match base_strategy {
            "target-fps-cpu-sleep-with-fifo-present" => {
                "audio-clock-master-with-target-fps-fallback-and-fifo-present"
            }
            "target-fps-cpu-sleep" => "audio-clock-master-with-target-fps-fallback",
            "fifo-present-blocking-no-cpu-sleep" => "audio-clock-master-with-fifo-present",
            _ => "audio-clock-master",
        },
    }
}

impl NativeVulkanVideoClockPacer {
    pub(super) fn new(target_fps: Option<u32>, spin_margin: Duration) -> Self {
        let now = Instant::now();
        Self {
            target_fps: target_fps.filter(|fps| *fps > 0),
            spin_margin,
            frame_timer: now,
            advanced_frame_count: 0,
            resync_threshold: native_vulkan_video_pacing_resync_threshold(target_fps),
        }
    }

    pub(super) fn reset(&mut self, now: Instant) {
        self.frame_timer = now;
        self.advanced_frame_count = 0;
    }

    pub(super) fn pace_after_frame_with_master_clock(
        &mut self,
        is_last_frame: bool,
        next_video_clock_ns: Option<u64>,
        master_clock_ns: Option<u64>,
    ) -> NativeVulkanVideoClockPaceResult {
        if is_last_frame {
            return NativeVulkanVideoClockPaceResult {
                slept: false,
                sleep_duration: Duration::ZERO,
                late_duration: None,
            };
        }

        let (Some(next_video_clock_ns), Some(master_clock_ns)) =
            (next_video_clock_ns, master_clock_ns)
        else {
            return self.pace_after_frame(false);
        };

        if next_video_clock_ns > master_clock_ns {
            let sleep_duration =
                Duration::from_nanos(next_video_clock_ns.saturating_sub(master_clock_ns));
            let deadline = Instant::now() + sleep_duration;
            return self.sleep_until_deadline(deadline, sleep_duration);
        }

        let late_duration =
            Duration::from_nanos(master_clock_ns.saturating_sub(next_video_clock_ns));
        if late_duration > self.resync_threshold {
            self.reset(Instant::now());
        }
        NativeVulkanVideoClockPaceResult {
            slept: false,
            sleep_duration: Duration::ZERO,
            late_duration: Some(late_duration),
        }
    }

    pub(super) fn pace_after_frame(
        &mut self,
        is_last_frame: bool,
    ) -> NativeVulkanVideoClockPaceResult {
        if is_last_frame {
            return NativeVulkanVideoClockPaceResult {
                slept: false,
                sleep_duration: Duration::ZERO,
                late_duration: None,
            };
        }

        let Some(fps) = self.target_fps else {
            return NativeVulkanVideoClockPaceResult {
                slept: false,
                sleep_duration: Duration::ZERO,
                late_duration: None,
            };
        };

        self.advanced_frame_count = self.advanced_frame_count.saturating_add(1);
        let deadline = self.frame_timer
            + native_vulkan_video_exact_frame_offset(self.advanced_frame_count, fps);
        let now = Instant::now();
        if deadline > now {
            let sleep_duration = deadline - now;
            return self.sleep_until_deadline(deadline, sleep_duration);
        }

        let late_duration = now.duration_since(deadline);
        if late_duration > self.resync_threshold {
            self.reset(now);
        }
        NativeVulkanVideoClockPaceResult {
            slept: false,
            sleep_duration: Duration::ZERO,
            late_duration: Some(late_duration),
        }
    }

    fn sleep_until_deadline(
        &self,
        deadline: Instant,
        sleep_duration: Duration,
    ) -> NativeVulkanVideoClockPaceResult {
        let sleep_for = sleep_duration
            .checked_sub(self.spin_margin)
            .unwrap_or_default();
        if !sleep_for.is_zero() {
            thread::sleep(sleep_for);
        }
        while Instant::now() < deadline {
            std::hint::spin_loop();
        }
        NativeVulkanVideoClockPaceResult {
            slept: true,
            sleep_duration,
            late_duration: None,
        }
    }
}

fn native_vulkan_video_exact_frame_offset(frame_count: u64, fps: u32) -> Duration {
    let total_ns = (u128::from(frame_count) * 1_000_000_000u128) / u128::from(fps);
    let total_ns = u64::try_from(total_ns).unwrap_or(u64::MAX);
    Duration::from_nanos(total_ns)
}

fn native_vulkan_video_pacing_resync_threshold(target_fps: Option<u32>) -> Duration {
    target_fps
        .filter(|fps| *fps > 0)
        .map(|fps| {
            let frame_ns = 1_000_000_000u64 / u64::from(fps);
            Duration::from_nanos(frame_ns.saturating_mul(3)).max(Duration::from_millis(10))
        })
        .unwrap_or(Duration::from_millis(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_target_fps_pacing_even_with_fifo_present() {
        let plan = native_vulkan_video_pacing_plan(vk::PresentModeKHR::FIFO, Some(60));

        assert_eq!(plan.strategy, "target-fps-cpu-sleep-with-fifo-present");
        assert_eq!(
            plan.frame_interval,
            Some(Duration::from_secs_f64(1.0 / 60.0))
        );
    }

    #[test]
    fn falls_back_to_fifo_blocking_only_without_a_target_fps() {
        let plan = native_vulkan_video_pacing_plan(vk::PresentModeKHR::FIFO, None);

        assert_eq!(plan.strategy, "fifo-present-blocking-no-cpu-sleep");
        assert_eq!(plan.frame_interval, None);
    }

    #[test]
    fn labels_audio_master_pacing_without_changing_default_plan() {
        let plan = native_vulkan_video_pacing_plan(vk::PresentModeKHR::FIFO, Some(60));

        assert_eq!(
            native_vulkan_video_pacing_strategy_label(
                plan.strategy,
                NativeVulkanVideoPacingMaster::TargetFps,
            ),
            "target-fps-cpu-sleep-with-fifo-present"
        );
        assert_eq!(
            native_vulkan_video_pacing_strategy_label(
                plan.strategy,
                NativeVulkanVideoPacingMaster::AudioClock,
            ),
            "audio-clock-master-with-target-fps-fallback-and-fifo-present"
        );
    }

    #[test]
    fn audio_master_pacing_reports_late_video_without_sleeping() {
        let mut pacer = NativeVulkanVideoClockPacer::new(Some(60), Duration::ZERO);

        let result = pacer.pace_after_frame_with_master_clock(
            false,
            Some(1_000_000_000),
            Some(1_020_000_000),
        );

        assert!(!result.slept);
        assert_eq!(result.late_duration, Some(Duration::from_millis(20)));
    }

    #[test]
    fn exact_frame_offset_preserves_fractional_target_rate() {
        assert_eq!(
            native_vulkan_video_exact_frame_offset(1, 60),
            Duration::from_nanos(16_666_666)
        );
        assert_eq!(
            native_vulkan_video_exact_frame_offset(60, 60),
            Duration::from_secs(1)
        );
        assert_eq!(
            native_vulkan_video_exact_frame_offset(240, 240),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn resync_threshold_tracks_high_refresh_deadline_slack() {
        assert_eq!(
            native_vulkan_video_pacing_resync_threshold(Some(240)),
            Duration::from_nanos(12_499_998)
        );
        assert_eq!(
            native_vulkan_video_pacing_resync_threshold(Some(60)),
            Duration::from_nanos(49_999_998)
        );
    }
}
