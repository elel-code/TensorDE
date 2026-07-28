#[cfg(feature = "tty")]
use std::os::fd::AsFd;

use tensor_util::{LogicalPoint, Rect};
use tracing::{debug, info, warn};

use crate::{backend::BackendOutputId, render::NativeOutputBuffer, scene::SceneSnapshot};

use super::{OutputRedrawState, RuntimeState, output_helpers::renderer_target};

impl RuntimeState {
    #[cfg(feature = "tty")]
    pub(crate) fn request_redraw_all(&mut self) {
        self.queue_redraw_all();
        self.redraw_queued_outputs();
    }

    #[cfg(feature = "tty")]
    pub(crate) fn submit_default_workspace_frame(&mut self) {
        self.request_redraw_workspace();
    }

    #[cfg(feature = "tty")]
    pub(crate) fn request_redraw_workspace(&mut self) {
        if self.force_full_redraw {
            return self.request_redraw_all();
        }
        let targets = self.workspace_output_ids();
        if targets.is_empty() {
            return;
        }
        for output in targets {
            self.queue_redraw(output);
        }
        self.redraw_queued_outputs();
    }

    /// Redraw only the output that contains a logical seat point (pointer).
    #[cfg(feature = "tty")]
    pub(crate) fn request_redraw_at(&mut self, location: LogicalPoint<f64>) {
        if self.force_full_redraw {
            return self.request_redraw_all();
        }
        let Some(output_id) = self.output_id_under(location) else {
            return;
        };
        self.queue_redraw(output_id);
        self.redraw_queued_outputs();
    }

    #[cfg(feature = "tty")]
    fn queue_redraw_all(&mut self) {
        let outputs = self.outputs.keys().copied().collect::<Vec<_>>();
        for output in outputs {
            self.queue_redraw(output);
        }
    }

    #[cfg(feature = "tty")]
    fn queue_redraw(&mut self, output: BackendOutputId) {
        let state = self
            .redraw_states
            .entry(output)
            .or_insert(OutputRedrawState::Idle);
        *state = state.queue();
    }

    #[cfg(feature = "tty")]
    pub(super) fn set_redraw_state(&mut self, output: BackendOutputId, state: OutputRedrawState) {
        self.redraw_states.insert(output, state);
    }

    /// Force a first frame for every output, even if a sibling is mid-flip.
    #[cfg(feature = "tty")]
    pub(super) fn force_redraw_all(&mut self) {
        let outputs = self.outputs.keys().copied().collect::<Vec<_>>();
        for output in outputs {
            self.set_redraw_state(output, OutputRedrawState::Queued);
        }
        self.redraw_queued_outputs();
    }

    #[cfg(feature = "tty")]
    fn workspace_output_ids(&self) -> Vec<BackendOutputId> {
        let Some(area) = self.default_workspace_area() else {
            return self.outputs.keys().copied().collect();
        };
        let mut matches = self
            .outputs
            .iter()
            .filter_map(|(id, managed)| {
                let geometry = self.space.output_geometry(&managed.output)?;
                let logical = Rect::new(
                    geometry.loc.x,
                    geometry.loc.y,
                    u32::try_from(geometry.size.w).unwrap_or(0),
                    u32::try_from(geometry.size.h).unwrap_or(0),
                );
                (logical == area).then_some(*id)
            })
            .collect::<Vec<_>>();
        if matches.is_empty() {
            matches = self.outputs.keys().copied().collect();
        }
        matches
    }

    #[cfg(feature = "tty")]
    fn output_id_under(&self, location: LogicalPoint<f64>) -> Option<BackendOutputId> {
        let output = self.space.output_under(location).next()?;
        self.outputs
            .iter()
            .find(|(_, managed)| managed.output == *output)
            .map(|(id, _)| *id)
    }

    #[cfg(feature = "tty")]
    fn redraw_queued_outputs(&mut self) {
        // Snapshot first: a failed submit may leave the CRTC `Queued` for a
        // later GPU/KMS retry, and must not re-enter the same output forever
        // inside this drain (unit tests without a renderer hit that path).
        let queued = self
            .redraw_states
            .iter()
            .filter_map(|(id, state)| state.is_queued().then_some(*id))
            .collect::<Vec<_>>();
        for output_id in queued {
            self.submit_output_frame(output_id);
        }
    }

    #[cfg(feature = "tty")]
    fn submit_output_frame(&mut self, output_id: BackendOutputId) {
        if !matches!(
            self.redraw_states.get(&output_id).copied(),
            Some(OutputRedrawState::Queued)
        ) {
            return;
        }
        let frame_started = self.frame_stats.then(std::time::Instant::now);
        let Some(managed) = self.outputs.get(&output_id) else {
            self.redraw_states.remove(&output_id);
            return;
        };
        let output = managed.output.clone();
        let target = renderer_target(&managed.descriptor);
        let Some(geometry) = self.space.output_geometry(&output) else {
            self.set_redraw_state(output_id, OutputRedrawState::Idle);
            return;
        };
        let logical = Rect::new(
            geometry.loc.x,
            geometry.loc.y,
            u32::try_from(geometry.size.w).unwrap_or(0),
            u32::try_from(geometry.size.h).unwrap_or(0),
        );
        if logical.width == 0 || logical.height == 0 {
            self.set_redraw_state(output_id, OutputRedrawState::Idle);
            return;
        }

        if self.renderer.is_none() || self.backend.is_none() {
            // Keep the request latched until the renderer/backend exist; the
            // next successful attach path calls force_redraw_all / queue again.
            return;
        }

        let scene = self.scene_for_output(&output, logical);
        let cursors = self.cursor.overlays_for_output(
            self.input_seat.pointer_location(),
            geometry,
            target.scale,
            target.viewport,
        );
        let has_presented = self
            .outputs
            .get(&output_id)
            .is_some_and(|managed| managed.has_presented);
        if has_presented
            && !self.session_is_locked()
            && scene.nodes().is_empty()
            && cursors.is_empty()
        {
            self.set_redraw_state(output_id, OutputRedrawState::Idle);
            debug!(
                output_device = output_id.device_id,
                output_connector = output_id.connector_id,
                "skipped empty secondary output frame"
            );
            return;
        }
        if let Err(error) = self.prepare_surface_acquires(&scene) {
            self.flush_client_releases();
            warn!(%error, "client explicit-sync acquire is not ready");
            self.defer_output_repaint(output_id);
            return;
        }
        if let Some(renderer) = self.renderer.as_mut()
            && let Err(error) = renderer.refresh_completed()
        {
            self.defer_output_repaint(output_id);
            warn!(%error, "renderer completion query failed before output slot selection");
            return;
        }
        let render_output = target.output;
        let Some(mut next_slot) = self
            .renderer
            .as_ref()
            .and_then(|renderer| renderer.next_output_slot(render_output))
        else {
            self.defer_output_repaint(output_id);
            return;
        };
        let mut selected_slot = None;
        for attempt in 0..NativeOutputBuffer::COUNT {
            let policy_ready =
                self.present_slot_ready(output_id, tensor_host::PresentSlot(next_slot));
            let kms_ready = self
                .backend
                .as_ref()
                .is_some_and(|backend| backend.output_ready_for_slot(output_id, next_slot));
            if policy_ready && kms_ready {
                selected_slot = Some(next_slot);
                break;
            }
            if attempt + 1 < NativeOutputBuffer::COUNT {
                let Some(candidate) = self
                    .renderer
                    .as_mut()
                    .and_then(|renderer| renderer.advance_output_slot(render_output))
                else {
                    break;
                };
                next_slot = candidate;
            }
        }
        let Some(_selected_slot) = selected_slot else {
            self.defer_output_repaint(output_id);
            return;
        };
        // Drain feedback only after all retry gates passed. The local owner
        // below discards it if Vulkan or atomic KMS cannot accept this frame.
        let mut captured_presentation = self.capture_scene_presentation(output_id, &output, &scene);
        let Some(result) = self
            .renderer
            .as_mut()
            .map(|renderer| renderer.submit_scene(render_output, scene, cursors))
        else {
            self.discard_captured_presentation(captured_presentation);
            self.set_redraw_state(output_id, OutputRedrawState::Idle);
            return;
        };
        match result {
            Ok(frame) => {
                let sync_fd = self.renderer.as_mut().and_then(|renderer| {
                    renderer.take_sync_fd(render_output, frame.timeline_value)
                });
                let Some(sync_fd) = sync_fd else {
                    self.discard_captured_presentation(captured_presentation);
                    if let Some(backend) = self.backend.as_mut() {
                        backend.mark_output_faulted(output_id);
                    }
                    warn!(
                        output_device = output_id.device_id,
                        output_connector = output_id.connector_id,
                        timeline = frame.timeline_value,
                        "renderer submitted a native frame without a KMS SYNC_FD"
                    );
                    self.set_redraw_state(output_id, OutputRedrawState::Idle);
                    return;
                };
                let fence_fd = match sync_fd.as_fd().try_clone_to_owned() {
                    Ok(fd) => fd,
                    Err(error) => {
                        self.discard_captured_presentation(captured_presentation);
                        if let Some(backend) = self.backend.as_mut() {
                            backend.mark_output_faulted(output_id);
                        }
                        warn!(
                            output_device = output_id.device_id,
                            output_connector = output_id.connector_id,
                            timeline = frame.timeline_value,
                            %error,
                            "could not duplicate the renderer SYNC_FD for completion submission"
                        );
                        self.set_redraw_state(output_id, OutputRedrawState::Idle);
                        return;
                    }
                };
                let Some(fence_submitter) = self.gpu_fence_submitter.as_ref() else {
                    self.discard_captured_presentation(captured_presentation);
                    if let Some(backend) = self.backend.as_mut() {
                        backend.mark_output_faulted(output_id);
                    }
                    warn!(
                        output_device = output_id.device_id,
                        output_connector = output_id.connector_id,
                        timeline = frame.timeline_value,
                        "renderer frame has no GPU fence completion submitter"
                    );
                    self.set_redraw_state(output_id, OutputRedrawState::Idle);
                    return;
                };
                if let Err(error) =
                    fence_submitter.submit(output_id, frame.timeline_value, fence_fd)
                {
                    self.discard_captured_presentation(captured_presentation);
                    if let Some(backend) = self.backend.as_mut() {
                        backend.mark_output_faulted(output_id);
                    }
                    warn!(
                        output_device = output_id.device_id,
                        output_connector = output_id.connector_id,
                        timeline = frame.timeline_value,
                        %error,
                        "renderer fence could not enter the completion runtime"
                    );
                    self.set_redraw_state(output_id, OutputRedrawState::Idle);
                    return;
                }
                let Some(backend) = self.backend.as_mut() else {
                    self.discard_captured_presentation(captured_presentation);
                    warn!(
                        output_device = output_id.device_id,
                        output_connector = output_id.connector_id,
                        timeline = frame.timeline_value,
                        "renderer frame has no Tensor atomic KMS backend"
                    );
                    self.defer_output_repaint(output_id);
                    return;
                };
                // Value-only present intent: readiness table gates the slot
                // before the tty adapter touches KMS.
                let intent = tensor_host::PresentIntent::new(
                    output_id,
                    tensor_host::PresentSlot(frame.output_slot),
                    frame.serial,
                    frame.timeline_value,
                );
                if let Err(error) = self.event_loop.present_queue().try_push(intent) {
                    self.discard_captured_presentation(captured_presentation);
                    warn!(
                        output_device = output_id.device_id,
                        output_connector = output_id.connector_id,
                        %error,
                        "present queue rejected frame before KMS"
                    );
                    self.defer_output_repaint(output_id);
                    return;
                }
                // Drain one intent immediately on the compositor thread (no
                // worker hop — present stays latency-critical).
                let Some(queued) = self.event_loop.present_queue().try_pop() else {
                    self.discard_captured_presentation(captured_presentation);
                    self.defer_output_repaint(output_id);
                    return;
                };
                debug_assert_eq!(queued.output, output_id);
                if let Err(error) = backend.submit_output_frame(
                    queued.output,
                    queued.slot.0,
                    queued.timeline_value,
                    sync_fd,
                ) {
                    // Roll readiness so the slot is not stuck Queued forever.
                    if let Some(ready) = self.event_loop.present_queue().readiness_mut(output_id)
                        && let Some(slot) = ready.slot_mut(queued.slot)
                    {
                        slot.state = tensor_host::PresentState::Idle;
                    }
                    self.discard_captured_presentation(captured_presentation);
                    warn!(
                        output_device = output_id.device_id,
                        output_connector = output_id.connector_id,
                        %error,
                        "renderer frame could not enter atomic KMS"
                    );
                    self.defer_output_repaint(output_id);
                    return;
                }
                if let Some(ready) = self.event_loop.present_queue().readiness_mut(output_id) {
                    ready.mark_waiting_vblank(queued.slot);
                }
                self.protocol_globals
                    .session_lock
                    .frame_submitted(output_id, frame.timeline_value);
                // Atomic KMS has latched ownership of the submitted client
                // buffers. Let clients prepare their next frame immediately;
                // presentation feedback remains pending until vblank.
                self.release_submitted_fifo_barriers(&mut captured_presentation);
                self.send_submitted_frame_callbacks(&captured_presentation);
                self.queue_presentation(output_id, frame.timeline_value, captured_presentation);
                if let Some(managed) = self.outputs.get_mut(&output_id) {
                    managed.has_presented = true;
                }
                self.set_redraw_state(
                    output_id,
                    OutputRedrawState::WaitingForVBlank {
                        redraw_needed: false,
                    },
                );
                if let Some(started) = frame_started {
                    info!(
                        output_device = output_id.device_id,
                        output_connector = output_id.connector_id,
                        elapsed_us = started.elapsed().as_micros() as u64,
                        "frame submit"
                    );
                }
                debug!(
                    output_device = output_id.device_id,
                    output_connector = output_id.connector_id,
                    output_slot = frame.output_slot,
                    serial = frame.serial,
                    timeline = frame.timeline_value,
                    cursors = frame.draw_plan.cursors().len(),
                    damage_regions = frame.damage.regions().len(),
                    descriptor_offset = frame.descriptors.offset,
                    descriptor_bytes = frame.descriptors.size,
                    scene_nodes = frame.scene.nodes().len(),
                    damage_empty = frame.damage.is_empty(),
                    frame_output_device = frame.target.output.device_id,
                    frame_output_connector = frame.target.output.connector_id,
                    viewport = ?frame.target.viewport,
                    format = %frame.target.format.format.code,
                    modifier = %frame.target.format.format.modifier,
                    planes = frame.target.format.plane_count,
                    "renderer frame submitted to atomic KMS"
                );
            }
            Err(error) => {
                self.discard_captured_presentation(captured_presentation);
                warn!(
                    output_device = output_id.device_id,
                    output_connector = output_id.connector_id,
                    %error,
                    "renderer frame boundary failed"
                );
                self.defer_output_repaint(output_id);
            }
        }
    }

    #[cfg(feature = "tty")]
    pub(crate) fn scene_for_output(
        &mut self,
        output: &crate::protocol::globals::output::Output,
        logical: Rect,
    ) -> SceneSnapshot {
        let workspace = self.active_workspace();
        if self.session_is_locked() {
            return self.merge_session_lock_surfaces(
                SceneSnapshot::new(workspace, logical, Vec::new()),
                output,
                logical,
            );
        }
        let base = match self.world.extract_scene(workspace) {
            Some(scene) if scene.viewport == logical => scene,
            Some(scene) if scene.viewport.intersection(logical).is_some() => {
                SceneSnapshot::with_content(
                    scene.workspace_id,
                    logical,
                    scene.nodes().to_vec(),
                    scene.contents().to_vec(),
                )
            }
            _ => SceneSnapshot::new(workspace, logical, Vec::new()),
        };
        self.merge_layer_surfaces(base, output, logical)
    }

    fn defer_output_repaint(&mut self, output: BackendOutputId) {
        self.queue_redraw(output);
    }

    #[cfg(feature = "tty")]
    pub(crate) fn drm_completion_generation(&self) -> u64 {
        self.backend
            .as_ref()
            .map_or(0, crate::backend::TtyBackend::drm_completion_generation)
    }

    #[cfg(feature = "tty")]
    pub(crate) fn write_drm_completion_device_ids(
        &self,
        destination: &mut [u64],
    ) -> Result<usize, String> {
        self.backend.as_ref().map_or(Ok(0), |backend| {
            backend.write_drm_completion_device_ids(destination)
        })
    }

    #[cfg(feature = "tty")]
    pub(crate) fn duplicate_drm_completion_fd(
        &self,
        device_id: u64,
    ) -> Result<std::os::fd::OwnedFd, String> {
        self.backend
            .as_ref()
            .ok_or_else(|| "tty backend is unavailable".to_owned())?
            .duplicate_drm_completion_fd(device_id)
    }

    #[cfg(feature = "tty")]
    pub(crate) fn dispatch_drm_completion(&mut self, device_id: u64) -> Result<(), String> {
        let Some(backend) = self.backend.as_ref() else {
            return Ok(());
        };
        let mut events = backend.receive_drm_events(device_id)?;
        for event in events.drain() {
            self.dispatch_drm_vblank(event);
        }
        Ok(())
    }

    #[cfg(feature = "tty")]
    pub(crate) fn dispatch_drm_vblank(&mut self, event: tensor_host::VblankEvent) {
        let presentation = self
            .backend
            .as_mut()
            .and_then(|backend| backend.handle_drm_vblank(event.device_id, event.crtc_id));
        let Some(presentation) = presentation else {
            return;
        };
        let sequence = event.metadata.sequence;
        debug!(
            output_device = presentation.output.device_id,
            output_connector = presentation.output.connector_id,
            output_slot = presentation.slot,
            timeline = presentation.timeline_value,
            released_timeline = ?presentation.released_timeline,
            sequence,
            "atomic KMS page flip completed"
        );
        self.push_vblank(presentation.output, u64::from(sequence));
        if let Some(lock) = self
            .protocol_globals
            .session_lock
            .frame_completed(presentation.output, presentation.timeline_value)
        {
            lock.locked();
            info!("session locked after protected frames completed");
        }
        // Free the present slot for the next triple-buffer cycle.
        if let Some(ready) = self
            .event_loop
            .present_queue()
            .readiness_mut(presentation.output)
        {
            ready.mark_presented(tensor_host::PresentSlot(presentation.slot));
        }
        if !self.finish_presentation(
            presentation.output,
            presentation.timeline_value,
            Some(event.metadata),
        ) {
            warn!(
                output_device = presentation.output.device_id,
                output_connector = presentation.output.connector_id,
                timeline = presentation.timeline_value,
                "KMS page flip had no pending Wayland presentation feedback"
            );
        }
        let redraw_needed = match self.redraw_states.get(&presentation.output).copied() {
            Some(OutputRedrawState::WaitingForVBlank { redraw_needed }) => redraw_needed,
            Some(OutputRedrawState::Queued) => true,
            Some(OutputRedrawState::Idle) | None => false,
        };
        if redraw_needed {
            self.set_redraw_state(presentation.output, OutputRedrawState::Queued);
            self.submit_output_frame(presentation.output);
        } else {
            self.set_redraw_state(presentation.output, OutputRedrawState::Idle);
        }
    }

    #[cfg(feature = "tty")]
    pub(crate) fn drain_backend_completions(&mut self) -> Result<(), String> {
        if self.backend.is_none() {
            return Ok(());
        }
        while let Some(event) = self
            .backend
            .as_mut()
            .expect("tty backend exists during completion dispatch")
            .next_session_completion_event()?
        {
            self.dispatch_session_event(event);
        }

        while let Some(event) = self
            .backend
            .as_mut()
            .expect("session dispatch restored the tty backend")
            .next_udev_completion_event()?
        {
            self.dispatch_udev_event(event);
        }

        while let Some(event) = self
            .backend
            .as_mut()
            .expect("udev dispatch restored the tty backend")
            .next_libinput_completion_event()?
        {
            self.process_input_event(event);
        }
        Ok(())
    }

    #[cfg(feature = "tty")]
    pub(crate) fn dispatch_udev_event(&mut self, event: crate::backend::UdevEvent) {
        let Some(mut backend) = self.backend.take() else {
            return;
        };
        backend.handle_udev_event(event);
        self.backend = Some(backend);
        self.refresh_syncobj_device();
        let events = self
            .backend
            .as_mut()
            .expect("backend was restored")
            .take_output_events();
        if let Err(error) = self.apply_backend_output_events(events) {
            warn!(%error, "failed to apply udev output event");
        }
    }

    #[cfg(feature = "tty")]
    pub(crate) fn dispatch_session_event(&mut self, event: tensor_host::SessionEvent) {
        let resumed = matches!(event, tensor_host::SessionEvent::Activated);
        if matches!(event, tensor_host::SessionEvent::Paused) {
            let discarded = self.discard_all_presentations();
            if discarded > 0 {
                info!(
                    discarded,
                    "discarded in-flight presentation feedback on session pause"
                );
            }
            for state in self.redraw_states.values_mut() {
                *state = OutputRedrawState::Idle;
            }
        }
        let Some(mut backend) = self.backend.take() else {
            return;
        };
        backend.handle_session_event(event);
        self.backend = Some(backend);
        self.refresh_syncobj_device();
        let events = self
            .backend
            .as_mut()
            .expect("backend was restored")
            .take_output_events();
        if let Err(error) = self.apply_backend_output_events(events) {
            warn!(%error, "failed to apply session output event");
        }
        if resumed {
            self.event_loop.defer_session_resume_repaint();
        }
    }

    /// End the source-completion phase after every already-published DRM CQE
    /// has been consumed. A stale page flip can therefore never be mistaken
    /// for the first frame submitted after session activation.
    pub(crate) fn finish_completion_turn(&mut self) {
        if self.event_loop.take_session_resume_repaint() {
            self.force_redraw_all();
        }
    }

    pub(crate) fn handle_gpu_fence_completion(
        &mut self,
        output: BackendOutputId,
        timeline_value: u64,
    ) -> Result<(), String> {
        let completed = self
            .renderer
            .as_mut()
            .ok_or_else(|| "GPU fence completed without an installed renderer".to_owned())?
            .refresh_completed()
            .map_err(|error| error.to_string())?;
        if completed < timeline_value {
            return Err(format!(
                "SYNC_FD for timeline {timeline_value} completed while Vulkan reported only {completed}"
            ));
        }
        debug!(
            output_device = output.device_id,
            output_connector = output.connector_id,
            timeline = timeline_value,
            completed_timeline = completed,
            "renderer SYNC_FD completion retired GPU work"
        );
        let _ = self.push_event(tensor_event::Event::Gpu(tensor_event::GpuTimeline {
            output: Self::event_output_id(output),
            value: timeline_value,
        }));
        self.flush_client_releases();
        self.redraw_queued_outputs();
        Ok(())
    }
}
