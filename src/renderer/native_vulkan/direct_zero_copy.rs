//! Direct Vulkan Video zero-copy evidence helpers.
//!
//! These helpers classify decoded-frame display handoff only. Compressed
//! bitstream payload upload into the Vulkan Video bitstream ring remains a
//! separate copy scope.

pub(super) const NATIVE_VULKAN_DIRECT_DECODED_FRAME_ZERO_COPY_SCOPE: &str = "decoded-frame display handoff; bitstream upload still copies into the Vulkan Video bitstream ring";

pub(super) fn native_vulkan_direct_decoded_frame_zero_copy_status(
    presented_frame_count: u32,
    display_copy_count: u32,
    display_ring_memory_bytes: u64,
    displayed_direct_dpb_count: Option<u32>,
    display_handoff_strategy: &str,
) -> &'static str {
    if presented_frame_count == 0 {
        return "not-proven-no-presented-frames";
    }
    if display_copy_count > 0 {
        return "display-copy-active";
    }
    if display_ring_memory_bytes > 0 {
        return "display-ring-retained";
    }
    if let Some(displayed_direct_dpb_count) = displayed_direct_dpb_count {
        if displayed_direct_dpb_count >= presented_frame_count {
            return "confirmed-direct-dpb-no-display-copy";
        }
        if displayed_direct_dpb_count > 0 {
            return "mixed-direct-dpb-display-handoff";
        }
        return "copy-free-but-direct-dpb-count-missing";
    }
    if display_handoff_strategy.contains("direct-sampled-dpb")
        || display_handoff_strategy.contains("direct-sampled")
    {
        return "direct-sampled-output-path-no-display-copy-counter";
    }
    "copy-free-display-handoff-unclassified"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirms_direct_dpb_when_all_presented_frames_are_direct() {
        assert_eq!(
            native_vulkan_direct_decoded_frame_zero_copy_status(
                2400,
                0,
                0,
                Some(2400),
                "direct-sampled-dpb-general-layout+frame-context-retire",
            ),
            "confirmed-direct-dpb-no-display-copy"
        );
    }

    #[test]
    fn display_copy_wins_over_direct_claim() {
        assert_eq!(
            native_vulkan_direct_decoded_frame_zero_copy_status(
                2400,
                12,
                0,
                Some(2400),
                "direct-sampled-dpb-output",
            ),
            "display-copy-active"
        );
    }

    #[test]
    fn scope_keeps_bitstream_upload_separate() {
        assert!(NATIVE_VULKAN_DIRECT_DECODED_FRAME_ZERO_COPY_SCOPE.contains("decoded-frame"));
        assert!(NATIVE_VULKAN_DIRECT_DECODED_FRAME_ZERO_COPY_SCOPE.contains("bitstream ring"));
    }
}
