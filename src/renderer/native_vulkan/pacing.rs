use std::time::Duration;

use ash::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanVideoPacingPlan {
    pub(super) strategy: &'static str,
    pub(super) frame_interval: Option<Duration>,
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
}
