use serde::Serialize;

use super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub model: &'static str,
    pub capacity_frames: usize,
    pub queued_frame_count_before_drain: usize,
    pub enqueued_frame_count: u32,
    pub dropped_frame_count: u32,
    pub drained_frame_count: u32,
    pub peak_depth: usize,
    pub keep_last_overwrite_enabled: bool,
    pub drop_policy: &'static str,
    pub drain_order: &'static str,
    pub zero_copy_scope: &'static str,
    pub ffmpeg_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeVulkanVulkanaliaPendingDecodedPresentFrame {
    pub(super) decode_frame_index: u32,
    pub(super) sampled_array_layer: u32,
    pub(super) source_frame_pts_ms: Option<u64>,
    pub(super) source_frame_duration_ms: Option<u64>,
    pub(super) display_order_key: i64,
    pub(super) display_order_key_source: &'static str,
}

impl NativeVulkanVulkanaliaPendingDecodedPresentFrame {
    pub(super) fn new(
        decode_frame_index: u32,
        sampled_array_layer: u32,
        source_frame_pts_ms: Option<u64>,
        source_frame_duration_ms: Option<u64>,
        display_order_key: i64,
        display_order_key_source: &'static str,
    ) -> Self {
        Self {
            decode_frame_index,
            sampled_array_layer,
            source_frame_pts_ms,
            source_frame_duration_ms,
            display_order_key,
            display_order_key_source,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct NativeVulkanVulkanaliaDecodedPresentHandoff {
    capacity_frames: usize,
    frames: Vec<NativeVulkanVulkanaliaPendingDecodedPresentFrame>,
    enqueued_frame_count: u32,
    dropped_frame_count: u32,
    peak_depth: usize,
}

impl NativeVulkanVulkanaliaDecodedPresentHandoff {
    pub(super) fn new(capacity_frames: usize) -> Self {
        Self {
            capacity_frames: capacity_frames.max(1),
            frames: Vec::with_capacity(capacity_frames.max(1)),
            enqueued_frame_count: 0,
            dropped_frame_count: 0,
            peak_depth: 0,
        }
    }

    pub(super) fn push_keep_last(
        &mut self,
        frame: NativeVulkanVulkanaliaPendingDecodedPresentFrame,
    ) {
        self.enqueued_frame_count = self.enqueued_frame_count.saturating_add(1);
        if self.frames.len() >= self.capacity_frames {
            if let Some(drop_index) = self.oldest_display_order_index() {
                self.frames.remove(drop_index);
                self.dropped_frame_count = self.dropped_frame_count.saturating_add(1);
            }
        }
        self.frames.push(frame);
        self.peak_depth = self.peak_depth.max(self.frames.len());
    }

    pub(super) fn drain_sorted(
        &mut self,
    ) -> (
        Vec<NativeVulkanVulkanaliaPendingDecodedPresentFrame>,
        NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot,
    ) {
        let queued_frame_count_before_drain = self.frames.len();
        sort_pending_decoded_present_frames(&mut self.frames);
        let frames = self.frames.drain(..).collect::<Vec<_>>();
        let drained_frame_count = u32::try_from(frames.len()).unwrap_or(u32::MAX);
        (
            frames,
            NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot {
                binding: "vulkanalia",
                route: "decoded-image-present-bounded-keep-last-handoff",
                model: "bounded decoded-frame handoff between Vulkan Video decode completion and Vulkanalia dynamic-rendering present",
                capacity_frames: self.capacity_frames,
                queued_frame_count_before_drain,
                enqueued_frame_count: self.enqueued_frame_count,
                dropped_frame_count: self.dropped_frame_count,
                drained_frame_count,
                peak_depth: self.peak_depth,
                keep_last_overwrite_enabled: true,
                drop_policy: "when the handoff is full, drop the oldest display-order frame and keep the newest decoded frame",
                drain_order: "sort by display_order_key, then decode_frame_index",
                zero_copy_scope: "handoff stores decoded image array-layer identities and timing metadata only; frame pixels stay in Vulkan images",
                ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
            },
        )
    }

    fn oldest_display_order_index(&self) -> Option<usize> {
        self.frames
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| compare_display_order(left, right))
            .map(|(index, _)| index)
    }
}

pub(super) fn sort_pending_decoded_present_frames(
    pending_frames: &mut [NativeVulkanVulkanaliaPendingDecodedPresentFrame],
) {
    pending_frames.sort_by(compare_display_order);
}

fn compare_display_order(
    left: &NativeVulkanVulkanaliaPendingDecodedPresentFrame,
    right: &NativeVulkanVulkanaliaPendingDecodedPresentFrame,
) -> std::cmp::Ordering {
    left.display_order_key
        .cmp(&right.display_order_key)
        .then_with(|| left.decode_frame_index.cmp(&right.decode_frame_index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_present_frames_sort_by_display_key_then_decode_index() {
        let mut frames = vec![
            test_pending_frame(2, 1, 16),
            test_pending_frame(1, 2, 8),
            test_pending_frame(0, 0, 8),
            test_pending_frame(3, 3, 4),
        ];

        sort_pending_decoded_present_frames(&mut frames);

        assert_eq!(
            frames
                .iter()
                .map(|frame| (frame.display_order_key, frame.decode_frame_index))
                .collect::<Vec<_>>(),
            vec![(4, 3), (8, 0), (8, 1), (16, 2)]
        );
    }

    #[test]
    fn bounded_handoff_keeps_latest_and_drops_oldest_display_order_frame() {
        let mut handoff = NativeVulkanVulkanaliaDecodedPresentHandoff::new(3);
        handoff.push_keep_last(test_pending_frame(0, 0, 100));
        handoff.push_keep_last(test_pending_frame(1, 1, 90));
        handoff.push_keep_last(test_pending_frame(2, 2, 110));
        handoff.push_keep_last(test_pending_frame(3, 0, 120));

        let (frames, snapshot) = handoff.drain_sorted();

        assert_eq!(
            frames
                .iter()
                .map(|frame| (frame.display_order_key, frame.decode_frame_index))
                .collect::<Vec<_>>(),
            vec![(100, 0), (110, 2), (120, 3)]
        );
        assert_eq!(snapshot.capacity_frames, 3);
        assert_eq!(snapshot.enqueued_frame_count, 4);
        assert_eq!(snapshot.dropped_frame_count, 1);
        assert_eq!(snapshot.drained_frame_count, 3);
        assert_eq!(snapshot.peak_depth, 3);
        assert!(snapshot.keep_last_overwrite_enabled);
    }

    #[test]
    fn zero_capacity_handoff_is_clamped_to_one_frame() {
        let mut handoff = NativeVulkanVulkanaliaDecodedPresentHandoff::new(0);
        handoff.push_keep_last(test_pending_frame(0, 0, 10));
        handoff.push_keep_last(test_pending_frame(1, 1, 20));

        let (frames, snapshot) = handoff.drain_sorted();

        assert_eq!(
            frames
                .iter()
                .map(|frame| (frame.display_order_key, frame.decode_frame_index))
                .collect::<Vec<_>>(),
            vec![(20, 1)]
        );
        assert_eq!(snapshot.capacity_frames, 1);
        assert_eq!(snapshot.dropped_frame_count, 1);
        assert_eq!(snapshot.drained_frame_count, 1);
    }

    fn test_pending_frame(
        decode_frame_index: u32,
        sampled_array_layer: u32,
        display_order_key: i64,
    ) -> NativeVulkanVulkanaliaPendingDecodedPresentFrame {
        NativeVulkanVulkanaliaPendingDecodedPresentFrame::new(
            decode_frame_index,
            sampled_array_layer,
            None,
            None,
            display_order_key,
            "test-display-key",
        )
    }
}
