use serde::Serialize;

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
