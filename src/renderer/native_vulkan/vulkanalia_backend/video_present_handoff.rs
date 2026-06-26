use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanVulkanaliaDecodedPresentHandoffFrame {
    pub(super) decode_frame_index: u32,
    pub(super) sampled_array_layer: u32,
    pub(super) source_frame_pts_ms: Option<u64>,
    pub(super) source_frame_duration_ms: Option<u64>,
    pub(super) display_order_key: i64,
    pub(super) display_order_key_source: &'static str,
    pub(super) decode_complete_value: u64,
}

#[derive(Clone)]
pub(super) struct NativeVulkanVulkanaliaDecodedPresentHandoff {
    inner: Arc<NativeVulkanVulkanaliaDecodedPresentHandoffInner>,
}

struct NativeVulkanVulkanaliaDecodedPresentHandoffInner {
    state: Mutex<NativeVulkanVulkanaliaDecodedPresentHandoffState>,
    changed: Condvar,
}

struct NativeVulkanVulkanaliaDecodedPresentHandoffState {
    capacity_frames: usize,
    queue: VecDeque<NativeVulkanVulkanaliaDecodedPresentHandoffFrame>,
    pending_by_layer: Vec<u32>,
    enqueued_frame_count: u32,
    dropped_frame_count: u32,
    drained_frame_count: u32,
    peak_depth: usize,
    queued_frame_count_before_drain: usize,
    closed: bool,
    error: Option<String>,
}

impl NativeVulkanVulkanaliaDecodedPresentHandoff {
    pub(super) fn new(capacity_frames: usize, layer_count: usize) -> Self {
        let capacity_frames = capacity_frames.max(1);
        let layer_count = layer_count.max(1);
        Self {
            inner: Arc::new(NativeVulkanVulkanaliaDecodedPresentHandoffInner {
                state: Mutex::new(NativeVulkanVulkanaliaDecodedPresentHandoffState {
                    capacity_frames,
                    queue: VecDeque::with_capacity(capacity_frames),
                    pending_by_layer: vec![0; layer_count],
                    enqueued_frame_count: 0,
                    dropped_frame_count: 0,
                    drained_frame_count: 0,
                    peak_depth: 0,
                    queued_frame_count_before_drain: 0,
                    closed: false,
                    error: None,
                }),
                changed: Condvar::new(),
            }),
        }
    }

    pub(super) fn enqueue(
        &self,
        frame: NativeVulkanVulkanaliaDecodedPresentHandoffFrame,
    ) -> Result<(), String> {
        let mut state = self.lock_state()?;
        let layer = frame.sampled_array_layer as usize;
        if layer >= state.pending_by_layer.len() {
            return Err(format!(
                "decoded present handoff layer {} exceeds {} tracked layer(s)",
                frame.sampled_array_layer,
                state.pending_by_layer.len()
            ));
        }
        while state.queue.len() >= state.capacity_frames && !state.closed && state.error.is_none() {
            state = self.wait_state(state)?;
        }
        if let Some(err) = state.error.clone() {
            return Err(err);
        }
        if state.closed {
            return Err("decoded present handoff is closed".to_owned());
        }
        state.pending_by_layer[layer] = state.pending_by_layer[layer].saturating_add(1);
        state.queue.push_back(frame);
        state.enqueued_frame_count = state.enqueued_frame_count.saturating_add(1);
        state.peak_depth = state.peak_depth.max(state.queue.len());
        self.inner.changed.notify_all();
        Ok(())
    }

    pub(super) fn recv(
        &self,
    ) -> Result<Option<NativeVulkanVulkanaliaDecodedPresentHandoffFrame>, String> {
        let mut state = self.lock_state()?;
        loop {
            if let Some(frame) = state.queue.pop_front() {
                self.inner.changed.notify_all();
                return Ok(Some(frame));
            }
            if let Some(err) = state.error.clone() {
                return Err(err);
            }
            if state.closed {
                return Ok(None);
            }
            state = self.wait_state(state)?;
        }
    }

    pub(super) fn wait_for_layer_release(&self, sampled_array_layer: u32) -> Result<(), String> {
        let mut state = self.lock_state()?;
        let layer = sampled_array_layer as usize;
        if layer >= state.pending_by_layer.len() {
            return Err(format!(
                "decoded present handoff release layer {sampled_array_layer} exceeds {} tracked layer(s)",
                state.pending_by_layer.len()
            ));
        }
        while state.pending_by_layer[layer] > 0 && state.error.is_none() {
            state = self.wait_state(state)?;
        }
        if let Some(err) = state.error.clone() {
            return Err(err);
        }
        Ok(())
    }

    pub(super) fn mark_frame_released(&self, sampled_array_layer: u32) -> Result<(), String> {
        let mut state = self.lock_state()?;
        let layer = sampled_array_layer as usize;
        if layer >= state.pending_by_layer.len() {
            return Err(format!(
                "decoded present handoff released layer {sampled_array_layer} exceeds {} tracked layer(s)",
                state.pending_by_layer.len()
            ));
        }
        let Some(pending) = state.pending_by_layer.get_mut(layer) else {
            return Err("decoded present handoff layer tracking is inconsistent".to_owned());
        };
        if *pending == 0 {
            return Err(format!(
                "decoded present handoff released layer {sampled_array_layer} without a pending frame"
            ));
        }
        *pending -= 1;
        state.drained_frame_count = state.drained_frame_count.saturating_add(1);
        self.inner.changed.notify_all();
        Ok(())
    }

    pub(super) fn close(&self) -> Result<(), String> {
        let mut state = self.lock_state()?;
        state.closed = true;
        state.queued_frame_count_before_drain = state.queue.len();
        self.inner.changed.notify_all();
        Ok(())
    }

    pub(super) fn fail(&self, error: String) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.error = Some(error);
            state.closed = true;
            self.inner.changed.notify_all();
        }
    }

    pub(super) fn snapshot(
        &self,
        route: &'static str,
        model: &'static str,
        drop_policy: &'static str,
        drain_order: &'static str,
        zero_copy_scope: &'static str,
        ffmpeg_reference: &'static str,
    ) -> Result<NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot, String> {
        let state = self.lock_state()?;
        Ok(NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot {
            binding: "vulkanalia",
            route,
            model,
            capacity_frames: state.capacity_frames,
            queued_frame_count_before_drain: state.queued_frame_count_before_drain,
            enqueued_frame_count: state.enqueued_frame_count,
            dropped_frame_count: state.dropped_frame_count,
            drained_frame_count: state.drained_frame_count,
            peak_depth: state.peak_depth,
            keep_last_overwrite_enabled: true,
            drop_policy,
            drain_order,
            zero_copy_scope,
            ffmpeg_reference,
        })
    }

    fn lock_state(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, NativeVulkanVulkanaliaDecodedPresentHandoffState>, String>
    {
        self.inner
            .state
            .lock()
            .map_err(|_| "decoded present handoff mutex is poisoned".to_owned())
    }

    fn wait_state<'a>(
        &self,
        state: std::sync::MutexGuard<'a, NativeVulkanVulkanaliaDecodedPresentHandoffState>,
    ) -> Result<std::sync::MutexGuard<'a, NativeVulkanVulkanaliaDecodedPresentHandoffState>, String>
    {
        self.inner
            .changed
            .wait(state)
            .map_err(|_| "decoded present handoff condvar wait is poisoned".to_owned())
    }
}
