#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanTimelinePtsDeltaSummary {
    pub actual_min_ms: Option<u64>,
    pub actual_max_ms: Option<u64>,
    pub expected_min_ms: Option<u64>,
    pub expected_max_ms: Option<u64>,
    pub in_expected_range: Option<bool>,
}

pub(super) fn native_vulkan_timeline_pts_delta_summary<I>(
    target_fps: Option<u32>,
    deltas_ms: I,
) -> NativeVulkanTimelinePtsDeltaSummary
where
    I: IntoIterator<Item = Option<u64>>,
{
    let mut actual_min_ms = None;
    let mut actual_max_ms = None;
    for delta in deltas_ms.into_iter().flatten() {
        actual_min_ms = Some(actual_min_ms.map_or(delta, |current: u64| current.min(delta)));
        actual_max_ms = Some(actual_max_ms.map_or(delta, |current: u64| current.max(delta)));
    }

    let (expected_min_ms, expected_max_ms) =
        native_vulkan_timeline_expected_pts_delta_range_ms(target_fps);
    let in_expected_range = match (
        actual_min_ms,
        actual_max_ms,
        expected_min_ms,
        expected_max_ms,
    ) {
        (Some(actual_min), Some(actual_max), Some(expected_min), Some(expected_max)) => {
            Some(actual_min >= expected_min && actual_max <= expected_max)
        }
        _ => None,
    };

    NativeVulkanTimelinePtsDeltaSummary {
        actual_min_ms,
        actual_max_ms,
        expected_min_ms,
        expected_max_ms,
        in_expected_range,
    }
}

fn native_vulkan_timeline_expected_pts_delta_range_ms(
    target_fps: Option<u32>,
) -> (Option<u64>, Option<u64>) {
    let Some(target_fps) = target_fps else {
        return (None, None);
    };
    if target_fps == 0 {
        return (None, None);
    }
    let target_fps = u64::from(target_fps);
    (Some(1000 / target_fps), Some(1000_u64.div_ceil(target_fps)))
}

pub(super) fn native_vulkan_timeline_loop_boundary_reset(
    timeline_item_index: u32,
    frame_serial: u32,
    previous_frame_serial: u32,
) -> bool {
    timeline_item_index > 0
        && native_vulkan_timeline_frame_serial_stale(frame_serial, previous_frame_serial)
}

pub(super) fn native_vulkan_timeline_frame_serial_stale(
    frame_serial: u32,
    current_queue_serial: u32,
) -> bool {
    frame_serial != current_queue_serial
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_expected_pts_delta_range_from_target_fps() {
        assert_eq!(
            native_vulkan_timeline_expected_pts_delta_range_ms(Some(240)),
            (Some(4), Some(5))
        );
        assert_eq!(
            native_vulkan_timeline_expected_pts_delta_range_ms(Some(60)),
            (Some(16), Some(17))
        );
    }

    #[test]
    fn marks_pts_delta_range_valid_when_inside_frame_period_bounds() {
        let summary = native_vulkan_timeline_pts_delta_summary(Some(240), [Some(4), Some(5)]);
        assert_eq!(summary.actual_min_ms, Some(4));
        assert_eq!(summary.actual_max_ms, Some(5));
        assert_eq!(summary.expected_min_ms, Some(4));
        assert_eq!(summary.expected_max_ms, Some(5));
        assert_eq!(summary.in_expected_range, Some(true));
    }

    #[test]
    fn marks_pts_delta_range_invalid_when_outside_frame_period_bounds() {
        let summary = native_vulkan_timeline_pts_delta_summary(Some(240), [Some(3), Some(6)]);
        assert_eq!(summary.in_expected_range, Some(false));
    }

    #[test]
    fn loop_boundary_reset_requires_non_initial_serial_change() {
        assert!(!native_vulkan_timeline_loop_boundary_reset(0, 1, 0));
        assert!(!native_vulkan_timeline_loop_boundary_reset(10, 1, 1));
        assert!(native_vulkan_timeline_loop_boundary_reset(10, 2, 1));
    }

    #[test]
    fn frame_serial_stale_follows_ffplay_queue_serial_rule() {
        assert!(!native_vulkan_timeline_frame_serial_stale(3, 3));
        assert!(native_vulkan_timeline_frame_serial_stale(2, 3));
    }
}
