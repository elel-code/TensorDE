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
pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaDecodedPresentHandoffFrame
{
    pub(in crate::renderer::native_vulkan::vulkan) decode_frame_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) sampled_array_layer: u32,
    pub(in crate::renderer::native_vulkan::vulkan) source_frame_pts_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan::vulkan) source_frame_duration_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan::vulkan) source_frame_pts_ms: Option<u64>,
    pub(in crate::renderer::native_vulkan::vulkan) source_frame_duration_ms: Option<u64>,
    pub(in crate::renderer::native_vulkan::vulkan) display_order_key: i64,
    pub(in crate::renderer::native_vulkan::vulkan) display_order_key_source: &'static str,
    pub(in crate::renderer::native_vulkan::vulkan) decode_complete_value: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) enum NativeVulkanVulkanaliaDecodedPresentHandoffRecv
{
    Frame(NativeVulkanVulkanaliaDecodedPresentHandoffFrame),
    ReleaseWaiter,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaDecodedPresentLayerRelease
{
    pub(in crate::renderer::native_vulkan::vulkan) sampled_array_layer: u32,
    pub(in crate::renderer::native_vulkan::vulkan) present_frame_slot: u32,
    pub(in crate::renderer::native_vulkan::vulkan) frame_count: u32,
}

#[derive(Clone)]
pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaDecodedPresentHandoff {
    inner: Arc<NativeVulkanVulkanaliaDecodedPresentHandoffInner>,
}

struct NativeVulkanVulkanaliaDecodedPresentHandoffInner {
    state: Mutex<NativeVulkanVulkanaliaDecodedPresentHandoffState>,
    changed: Condvar,
}

struct NativeVulkanVulkanaliaDecodedPresentHandoffState {
    capacity_frames: usize,
    queue: VecDeque<NativeVulkanVulkanaliaDecodedPresentHandoffFrame>,
    queued_by_layer: Vec<u32>,
    in_flight_by_layer: Vec<Option<NativeVulkanVulkanaliaDecodedPresentLayerRelease>>,
    enqueued_frame_count: u32,
    dropped_frame_count: u32,
    drained_frame_count: u32,
    peak_depth: usize,
    queued_frame_count_before_drain: usize,
    release_waiter_count: u32,
    closed: bool,
    error: Option<String>,
}

impl NativeVulkanVulkanaliaDecodedPresentHandoff {
    pub(in crate::renderer::native_vulkan::vulkan) fn new(
        capacity_frames: usize,
        layer_count: usize,
    ) -> Self {
        let capacity_frames = capacity_frames.max(1);
        let layer_count = layer_count.max(1);
        Self {
            inner: Arc::new(NativeVulkanVulkanaliaDecodedPresentHandoffInner {
                state: Mutex::new(NativeVulkanVulkanaliaDecodedPresentHandoffState {
                    capacity_frames,
                    queue: VecDeque::with_capacity(capacity_frames),
                    queued_by_layer: vec![0; layer_count],
                    in_flight_by_layer: vec![None; layer_count],
                    enqueued_frame_count: 0,
                    dropped_frame_count: 0,
                    drained_frame_count: 0,
                    peak_depth: 0,
                    queued_frame_count_before_drain: 0,
                    release_waiter_count: 0,
                    closed: false,
                    error: None,
                }),
                changed: Condvar::new(),
            }),
        }
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn enqueue(
        &self,
        frame: NativeVulkanVulkanaliaDecodedPresentHandoffFrame,
    ) -> Result<(), String> {
        let mut state = self.lock_state()?;
        let layer = frame.sampled_array_layer as usize;
        if layer >= state.queued_by_layer.len() {
            return Err(format!(
                "decoded present handoff layer {} exceeds {} tracked layer(s)",
                frame.sampled_array_layer,
                state.queued_by_layer.len()
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
        state.queued_by_layer[layer] = state.queued_by_layer[layer].saturating_add(1);
        state.queue.push_back(frame);
        state.enqueued_frame_count = state.enqueued_frame_count.saturating_add(1);
        state.peak_depth = state.peak_depth.max(state.queue.len());
        self.inner.changed.notify_all();
        Ok(())
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn recv(
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

    pub(in crate::renderer::native_vulkan::vulkan) fn recv_after_preroll(
        &self,
        min_queued_frames: usize,
    ) -> Result<Option<NativeVulkanVulkanaliaDecodedPresentHandoffFrame>, String> {
        let mut state = self.lock_state()?;
        let min_queued_frames = min_queued_frames.max(1).min(state.capacity_frames.max(1));
        loop {
            if state.queue.len() >= min_queued_frames || (state.closed && !state.queue.is_empty()) {
                let frame = state.queue.pop_front().ok_or_else(|| {
                    "decoded present handoff preroll queue became empty".to_owned()
                })?;
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

    pub(in crate::renderer::native_vulkan::vulkan) fn recv_or_release_waiter(
        &self,
    ) -> Result<NativeVulkanVulkanaliaDecodedPresentHandoffRecv, String> {
        let mut state = self.lock_state()?;
        loop {
            if let Some(frame) = state.queue.pop_front() {
                self.inner.changed.notify_all();
                return Ok(NativeVulkanVulkanaliaDecodedPresentHandoffRecv::Frame(
                    frame,
                ));
            }
            if let Some(err) = state.error.clone() {
                return Err(err);
            }
            if state.closed {
                return Ok(NativeVulkanVulkanaliaDecodedPresentHandoffRecv::Closed);
            }
            if state.release_waiter_count > 0 {
                return Ok(NativeVulkanVulkanaliaDecodedPresentHandoffRecv::ReleaseWaiter);
            }
            state = self.wait_state(state)?;
        }
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn try_recv(
        &self,
    ) -> Result<Option<NativeVulkanVulkanaliaDecodedPresentHandoffFrame>, String> {
        let mut state = self.lock_state()?;
        if let Some(frame) = state.queue.pop_front() {
            self.inner.changed.notify_all();
            return Ok(Some(frame));
        }
        if let Some(err) = state.error.clone() {
            return Err(err);
        }
        Ok(None)
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn record_layer_present_release(
        &self,
        sampled_array_layer: u32,
        present_frame_slot: u32,
    ) -> Result<(), String> {
        let mut state = self.lock_state()?;
        let layer = sampled_array_layer as usize;
        if layer >= state.queued_by_layer.len() {
            return Err(format!(
                "decoded present handoff release layer {sampled_array_layer} exceeds {} tracked layer(s)",
                state.queued_by_layer.len()
            ));
        }
        if let Some(err) = state.error.clone() {
            return Err(err);
        }
        let queued = state.queued_by_layer.get_mut(layer).ok_or_else(|| {
            "decoded present handoff queued-layer tracking is inconsistent".to_owned()
        })?;
        if *queued == 0 {
            return Err(format!(
                "decoded present handoff recorded layer {sampled_array_layer} without a queued frame"
            ));
        }
        *queued -= 1;
        Self::store_available_layer_release(
            &mut state,
            NativeVulkanVulkanaliaDecodedPresentLayerRelease {
                sampled_array_layer,
                present_frame_slot,
                frame_count: 1,
            },
        )?;
        self.inner.changed.notify_all();
        Ok(())
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn wait_layer_present_release_completed(
        &self,
        sampled_array_layer: u32,
    ) -> Result<(), String> {
        let mut state = self.lock_state()?;
        let layer = sampled_array_layer as usize;
        if layer >= state.queued_by_layer.len() {
            return Err(format!(
                "decoded present handoff release layer {sampled_array_layer} exceeds {} tracked layer(s)",
                state.queued_by_layer.len()
            ));
        }
        let mut registered_waiter = false;
        while (state.queued_by_layer[layer] > 0 || state.in_flight_by_layer[layer].is_some())
            && !state.closed
            && state.error.is_none()
        {
            if !registered_waiter {
                state.release_waiter_count = state.release_waiter_count.saturating_add(1);
                registered_waiter = true;
                self.inner.changed.notify_all();
            }
            state = self.wait_state(state)?;
        }
        if registered_waiter {
            state.release_waiter_count = state.release_waiter_count.saturating_sub(1);
            self.inner.changed.notify_all();
        }
        if let Some(err) = state.error.clone() {
            return Err(err);
        }
        Ok(())
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn complete_present_frame_slot_releases(
        &self,
        present_frame_slot: u32,
    ) -> Result<u32, String> {
        let mut state = self.lock_state()?;
        if let Some(err) = state.error.clone() {
            return Err(err);
        }
        let mut completed_frame_count = 0u32;
        for release in &mut state.in_flight_by_layer {
            if release
                .as_ref()
                .is_some_and(|release| release.present_frame_slot == present_frame_slot)
            {
                if let Some(release) = release.take() {
                    completed_frame_count =
                        completed_frame_count.saturating_add(release.frame_count);
                }
            }
        }
        state.drained_frame_count = state
            .drained_frame_count
            .saturating_add(completed_frame_count);
        self.inner.changed.notify_all();
        Ok(completed_frame_count)
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn mark_frame_released(
        &self,
        sampled_array_layer: u32,
    ) -> Result<(), String> {
        let mut state = self.lock_state()?;
        let layer = sampled_array_layer as usize;
        if layer >= state.queued_by_layer.len() {
            return Err(format!(
                "decoded present handoff released layer {sampled_array_layer} exceeds {} tracked layer(s)",
                state.queued_by_layer.len()
            ));
        }
        let Some(queued) = state.queued_by_layer.get_mut(layer) else {
            return Err("decoded present handoff layer tracking is inconsistent".to_owned());
        };
        if *queued > 0 {
            *queued -= 1;
        } else {
            return Err(format!(
                "decoded present handoff released layer {sampled_array_layer} without a queued frame"
            ));
        }
        state.drained_frame_count = state.drained_frame_count.saturating_add(1);
        self.inner.changed.notify_all();
        Ok(())
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn close(&self) -> Result<(), String> {
        let mut state = self.lock_state()?;
        state.closed = true;
        state.queued_frame_count_before_drain = state.queue.len();
        self.inner.changed.notify_all();
        Ok(())
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn fail(&self, error: String) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.error = Some(error);
            state.closed = true;
            self.inner.changed.notify_all();
        }
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn snapshot(
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

    fn store_available_layer_release(
        state: &mut NativeVulkanVulkanaliaDecodedPresentHandoffState,
        release: NativeVulkanVulkanaliaDecodedPresentLayerRelease,
    ) -> Result<(), String> {
        let layer = release.sampled_array_layer as usize;
        if layer >= state.in_flight_by_layer.len() {
            return Err(format!(
                "decoded present handoff available release layer {} exceeds {} tracked layer(s)",
                release.sampled_array_layer,
                state.in_flight_by_layer.len()
            ));
        }
        if let Some(existing) = &mut state.in_flight_by_layer[layer] {
            existing.present_frame_slot = release.present_frame_slot;
            existing.frame_count = existing.frame_count.saturating_add(release.frame_count);
        } else {
            state.in_flight_by_layer[layer] = Some(release);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        decode_frame_index: u32,
        sampled_array_layer: u32,
    ) -> NativeVulkanVulkanaliaDecodedPresentHandoffFrame {
        NativeVulkanVulkanaliaDecodedPresentHandoffFrame {
            decode_frame_index,
            sampled_array_layer,
            source_frame_pts_ns: None,
            source_frame_duration_ns: None,
            source_frame_pts_ms: None,
            source_frame_duration_ms: None,
            display_order_key: i64::from(decode_frame_index),
            display_order_key_source: "test",
            decode_complete_value: u64::from(decode_frame_index) + 1,
        }
    }

    #[test]
    fn render_submit_release_keeps_three_frame_metadata_window() {
        let handoff = NativeVulkanVulkanaliaDecodedPresentHandoff::new(3, 2);

        handoff.enqueue(frame(0, 0)).expect("enqueue first");
        handoff.enqueue(frame(1, 1)).expect("enqueue second");
        handoff.enqueue(frame(2, 1)).expect("enqueue third");

        let first = handoff.recv().expect("recv first").expect("first frame");
        handoff
            .record_layer_present_release(first.sampled_array_layer, 0)
            .expect("record first");
        assert_eq!(
            handoff
                .complete_present_frame_slot_releases(0)
                .expect("complete slot releases"),
            1
        );

        let second = handoff.recv().expect("recv second").expect("second frame");
        handoff
            .record_layer_present_release(second.sampled_array_layer, 1)
            .expect("record second");
        handoff.enqueue(frame(3, 0)).expect("enqueue after next");

        let snapshot = handoff
            .snapshot("test", "test", "test", "test", "test", "test")
            .expect("snapshot");
        assert_eq!(snapshot.capacity_frames, 3);
        assert_eq!(snapshot.peak_depth, 3);
        assert_eq!(snapshot.enqueued_frame_count, 4);
        assert_eq!(snapshot.drained_frame_count, 1);
    }

    #[test]
    fn recv_after_preroll_waits_for_ffmpeg_sized_fifo_depth() {
        let handoff = NativeVulkanVulkanaliaDecodedPresentHandoff::new(3, 3);

        handoff.enqueue(frame(0, 0)).expect("enqueue first");
        handoff.enqueue(frame(1, 1)).expect("enqueue second");
        handoff.enqueue(frame(2, 2)).expect("enqueue third");

        let first = handoff
            .recv_after_preroll(3)
            .expect("recv after preroll")
            .expect("first frame");
        assert_eq!(first.decode_frame_index, 0);

        let second = handoff.recv().expect("recv second").expect("second frame");
        assert_eq!(second.decode_frame_index, 1);
        let third = handoff.recv().expect("recv third").expect("third frame");
        assert_eq!(third.decode_frame_index, 2);
    }

    #[test]
    fn recv_reports_release_waiter_only_when_decode_blocks_on_layer_reuse() {
        let handoff = NativeVulkanVulkanaliaDecodedPresentHandoff::new(3, 1);

        handoff.enqueue(frame(0, 0)).expect("enqueue frame");
        let first = handoff.recv().expect("recv").expect("frame");
        handoff
            .record_layer_present_release(first.sampled_array_layer, 0)
            .expect("record in-flight release");

        let wait_handoff = handoff.clone();
        let waiter = std::thread::spawn(move || {
            wait_handoff
                .wait_layer_present_release_completed(0)
                .expect("layer release completed");
        });

        assert_eq!(
            handoff
                .recv_or_release_waiter()
                .expect("recv or release waiter"),
            NativeVulkanVulkanaliaDecodedPresentHandoffRecv::ReleaseWaiter
        );
        assert_eq!(
            handoff
                .complete_present_frame_slot_releases(0)
                .expect("complete slot release"),
            1
        );
        waiter.join().expect("release waiter joined");
    }
}
