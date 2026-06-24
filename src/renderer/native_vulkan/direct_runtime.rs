//! Shared direct Vulkan Video runtime summary helpers.
//!
//! This is the codec-neutral part of the direct route: codec adapters own
//! parser/reference/DPB rules, while the runtime reports FFmpeg-like
//! packet-to-frame/display ownership evidence with the same fields for
//! H.264, H.265 and AV1.

use std::time::Duration;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
