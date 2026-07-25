use tensor_util::Rect;
use tracing::{debug, info, warn};

use crate::{
    backend::{BackendOutputEvent, BackendOutputId, OutputDescriptor},
    render::{NativeOutputBuffer, NativeOutputTarget, RenderOutputId},
    scene::SceneSnapshot,
};

use super::{DEFAULT_WORKSPACE, ManagedOutput, OutputRedrawState, RuntimeState};

impl RuntimeState {
    /// Queue a redraw for every connected output.
    ///
    /// Call sites that previously submitted a single default-workspace frame
    /// now mark every CRTC dirty; the per-output scheduler drains those that
    /// are not already waiting on a page flip.
    #[cfg(feature = "tty")]
    pub(crate) fn submit_default_workspace_frame(&mut self) {
        self.queue_redraw_all();
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

    /// Force a first frame for every output, even if a sibling is mid-flip.
    ///
    /// Mirrors Nourish's `force_redraw` / Hyprland's `AQ_SCHEDULE_NEW_MONITOR`:
    /// a CRTC that has never flipped has no vblank ring of its own.
    #[cfg(feature = "tty")]
    fn force_redraw_all(&mut self) {
        let outputs = self.outputs.keys().copied().collect::<Vec<_>>();
        for output in outputs {
            self.redraw_states.insert(output, OutputRedrawState::Queued);
        }
        self.redraw_queued_outputs();
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
        self.schedule_renderer_retry_if_needed();
    }

    #[cfg(feature = "tty")]
    fn submit_output_frame(&mut self, output_id: BackendOutputId) {
        if !matches!(
            self.redraw_states.get(&output_id).copied(),
            Some(OutputRedrawState::Queued)
        ) {
            return;
        }
        let Some(managed) = self.outputs.get(&output_id) else {
            self.redraw_states.remove(&output_id);
            return;
        };
        let output = managed.output.clone();
        let target = renderer_target(&managed.descriptor);
        let Some(geometry) = self.space.output_geometry(&output) else {
            self.redraw_states
                .insert(output_id, OutputRedrawState::Idle);
            return;
        };
        let logical = Rect::new(
            geometry.loc.x,
            geometry.loc.y,
            u32::try_from(geometry.size.w).unwrap_or(0),
            u32::try_from(geometry.size.h).unwrap_or(0),
        );
        if logical.width == 0 || logical.height == 0 {
            self.redraw_states
                .insert(output_id, OutputRedrawState::Idle);
            return;
        }

        if self.renderer.is_none() || self.backend.is_none() {
            // Keep the request latched until the renderer/backend exist; the
            // next successful attach path calls force_redraw_all / queue again.
            return;
        }

        let scene = self.scene_for_output(logical);
        if let Err(error) = self.prepare_surface_acquires(&scene) {
            self.flush_client_releases();
            warn!(%error, "client explicit-sync acquire is not ready");
            self.defer_output_repaint(output_id);
            return;
        }

        let pointer_location = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location());
        let cursor = match (pointer_location, self.space.output_geometry(&output)) {
            (Some(pointer), Some(geometry)) => {
                self.cursor
                    .overlay_for_output(pointer, geometry, target.scale, target.viewport)
            }
            _ => None,
        };
        if let Some(renderer) = self.renderer.as_mut()
            && let Err(error) = renderer.refresh_completed()
        {
            self.defer_output_repaint(output_id);
            warn!(%error, "renderer completion poll failed before output slot selection");
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
            if self
                .backend
                .as_ref()
                .is_some_and(|backend| backend.output_ready_for_slot(output_id, next_slot))
            {
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
        let captured_presentation = self.capture_scene_presentation(output_id, &output, &scene);
        let Some(result) = self
            .renderer
            .as_mut()
            .map(|renderer| renderer.submit_scene(render_output, scene, cursor))
        else {
            drop(captured_presentation);
            self.redraw_states
                .insert(output_id, OutputRedrawState::Idle);
            return;
        };
        match result {
            Ok(frame) => {
                let sync_fd = self.renderer.as_mut().and_then(|renderer| {
                    renderer.take_sync_fd(render_output, frame.timeline_value)
                });
                let Some(sync_fd) = sync_fd else {
                    drop(captured_presentation);
                    if let Some(backend) = self.backend.as_mut() {
                        backend.mark_output_faulted(output_id);
                    }
                    warn!(
                        output_device = output_id.device_id,
                        output_connector = output_id.connector_id,
                        timeline = frame.timeline_value,
                        "renderer submitted a native frame without a KMS SYNC_FD"
                    );
                    self.redraw_states
                        .insert(output_id, OutputRedrawState::Idle);
                    return;
                };
                let Some(backend) = self.backend.as_mut() else {
                    drop(captured_presentation);
                    warn!(
                        output_device = output_id.device_id,
                        output_connector = output_id.connector_id,
                        timeline = frame.timeline_value,
                        "renderer frame has no Smithay atomic KMS backend"
                    );
                    self.defer_output_repaint(output_id);
                    return;
                };
                if let Err(error) = backend.submit_output_frame(
                    output_id,
                    frame.output_slot,
                    frame.timeline_value,
                    sync_fd,
                ) {
                    drop(captured_presentation);
                    warn!(
                        output_device = output_id.device_id,
                        output_connector = output_id.connector_id,
                        %error,
                        "renderer frame could not enter atomic KMS"
                    );
                    self.defer_output_repaint(output_id);
                    return;
                }
                // Atomic KMS has latched ownership of the submitted client
                // buffers. Let clients prepare their next frame immediately;
                // presentation feedback remains pending until vblank.
                self.send_submitted_frame_callbacks(&output, &captured_presentation);
                self.queue_presentation(output_id, frame.timeline_value, captured_presentation);
                self.redraw_states.insert(
                    output_id,
                    OutputRedrawState::WaitingForVBlank {
                        redraw_needed: false,
                    },
                );
                debug!(
                    output_device = output_id.device_id,
                    output_connector = output_id.connector_id,
                    output_slot = frame.output_slot,
                    serial = frame.serial,
                    timeline = frame.timeline_value,
                    cursor = ?frame.cursor,
                    damage_regions = frame.damage.regions().len(),
                    descriptor_offset = frame.descriptors.offset,
                    descriptor_bytes = frame.descriptors.size,
                    scene_nodes = frame.scene.nodes().len(),
                    damage_empty = frame.damage.is_empty(),
                    frame_output_device = frame.target.output.device_id,
                    frame_output_connector = frame.target.output.connector_id,
                    viewport = ?frame.target.viewport,
                    format = %frame.target.format.format.code,
                    modifier = %format_args!("{:#x}", u64::from(frame.target.format.format.modifier)),
                    planes = frame.target.format.plane_count,
                    "renderer frame submitted to atomic KMS"
                );
            }
            Err(error) => {
                drop(captured_presentation);
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

    /// Workspace content lives on the default output; other CRTCs get a blank
    /// scene with their own logical viewport so each still starts a vblank ring.
    #[cfg(feature = "tty")]
    pub(super) fn scene_for_output(&mut self, logical: Rect) -> SceneSnapshot {
        if let Some(scene) = self.world.extract_scene(DEFAULT_WORKSPACE)
            && scene.viewport == logical
        {
            return scene;
        }
        SceneSnapshot::new(DEFAULT_WORKSPACE, logical, Vec::new())
    }

    /// Keep an input-driven redraw live when the previous submission still
    /// owns the only scheduler slot. Page-flip completion handles the normal
    /// KMS case; this additionally polls the Vulkan timeline when a pointer
    /// event arrives after that page flip but before GPU retirement.
    fn defer_output_repaint(&mut self, output: BackendOutputId) {
        self.queue_redraw(output);
        self.schedule_renderer_retry_if_needed();
    }

    #[cfg(feature = "tty")]
    pub(crate) fn dispatch_drm_vblank(
        &mut self,
        device_id: libc::dev_t,
        crtc: smithay::reexports::drm::control::crtc::Handle,
        metadata: Option<smithay::backend::drm::DrmEventMetadata>,
    ) {
        let presentation = self
            .backend
            .as_mut()
            .and_then(|backend| backend.handle_drm_vblank(device_id, crtc));
        let Some(presentation) = presentation else {
            return;
        };
        debug!(
            output_device = presentation.output.device_id,
            output_connector = presentation.output.connector_id,
            output_slot = presentation.slot,
            timeline = presentation.timeline_value,
            released_timeline = ?presentation.released_timeline,
            sequence = metadata.map(|metadata| metadata.sequence),
            "atomic KMS page flip completed"
        );
        if !self.finish_presentation(presentation.output, presentation.timeline_value, metadata) {
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
            self.redraw_states
                .insert(presentation.output, OutputRedrawState::Queued);
            self.submit_output_frame(presentation.output);
        } else {
            self.redraw_states
                .insert(presentation.output, OutputRedrawState::Idle);
        }
        self.schedule_renderer_retry_if_needed();
    }

    #[cfg(feature = "tty")]
    pub(crate) fn dispatch_udev_event(&mut self, event: smithay::backend::udev::UdevEvent) {
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
    pub(crate) fn dispatch_session_event(&mut self, event: smithay::backend::session::Event) {
        if matches!(event, smithay::backend::session::Event::PauseSession) {
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
    }

    /// Repaint only after the session notifier and any already-ready DRM
    /// events have run. The backend schedules this as a calloop idle callback
    /// so a stale page-flip cannot be mistaken for the resumed frame.
    pub(crate) fn repaint_after_session_resume(&mut self) {
        self.force_redraw_all();
    }

    pub(crate) fn retry_renderer_repaint(&mut self) {
        self.renderer_retry_scheduled = false;
        self.redraw_queued_outputs();
    }

    fn schedule_renderer_retry_if_needed(&mut self) {
        if self.renderer_retry_scheduled || !self.renderer_repaint_waits_for_gpu() {
            return;
        }
        let Some(backend) = self.backend.as_ref() else {
            return;
        };
        match backend.schedule_renderer_retry() {
            Ok(()) => self.renderer_retry_scheduled = true,
            Err(error) => warn!(%error, "failed to schedule renderer completion retry"),
        }
    }

    fn renderer_repaint_waits_for_gpu(&self) -> bool {
        let Some(renderer) = self.renderer.as_ref() else {
            return false;
        };
        self.redraw_states.iter().any(|(output, state)| {
            state.needs_gpu_retry()
                && renderer.output_waiting_for_gpu(RenderOutputId {
                    device_id: output.device_id,
                    connector_id: output.connector_id,
                })
        })
    }

    #[cfg(feature = "tty")]
    pub(crate) fn apply_backend_output_events(
        &mut self,
        events: impl IntoIterator<Item = BackendOutputEvent>,
    ) -> Result<(), String> {
        let mut first_error = None;
        for event in events {
            let result = match event {
                BackendOutputEvent::Connected(descriptor) => self.connect_output(descriptor),
                BackendOutputEvent::Changed(descriptor) => self.change_output(descriptor),
                BackendOutputEvent::Disconnected(id) => {
                    self.disconnect_output(id);
                    Ok(())
                }
            };
            if let Err(error) = result {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    #[cfg(feature = "tty")]
    fn connect_output(&mut self, descriptor: OutputDescriptor) -> Result<(), String> {
        if self.outputs.contains_key(&descriptor.id) {
            return self.change_output(descriptor);
        }
        self.register_renderer_output(&descriptor, None)?;
        info!(
            output = descriptor.name,
            device_id = descriptor.id.device_id,
            connector_id = descriptor.id.connector_id,
            crtc = descriptor.crtc,
            mode_width = descriptor.mode.size.w,
            mode_height = descriptor.mode.size.h,
            refresh_millihertz = descriptor.mode.refresh,
            scale = descriptor.scale.as_f64(),
            "Smithay output connected"
        );
        let output = smithay::output::Output::new(
            descriptor.name.clone(),
            smithay::output::PhysicalProperties {
                size: descriptor.physical_size.into(),
                subpixel: descriptor.subpixel,
                make: "Unknown".to_owned(),
                model: descriptor.name.clone(),
                serial_number: "Unknown".to_owned(),
            },
        );
        for mode in &descriptor.modes {
            output.add_mode(*mode);
        }
        output.set_preferred(descriptor.mode);
        output.change_current_state(
            Some(descriptor.mode),
            None,
            Some(smithay::output::Scale::Fractional(
                descriptor.scale.as_f64(),
            )),
            Some((0, 0).into()),
        );
        let global = output.create_global::<Self>(&self.display_handle);
        self.space.map_output(&output, (0, 0));
        let output_id = descriptor.id;
        self.outputs.insert(
            output_id,
            ManagedOutput {
                output,
                global,
                descriptor,
            },
        );
        self.redraw_states
            .insert(output_id, OutputRedrawState::Queued);
        self.reflow_outputs();
        Ok(())
    }

    #[cfg(feature = "tty")]
    fn change_output(&mut self, descriptor: OutputDescriptor) -> Result<(), String> {
        info!(
            output = descriptor.name,
            device_id = descriptor.id.device_id,
            connector_id = descriptor.id.connector_id,
            crtc = descriptor.crtc,
            mode_width = descriptor.mode.size.w,
            mode_height = descriptor.mode.size.h,
            refresh_millihertz = descriptor.mode.refresh,
            scale = descriptor.scale.as_f64(),
            "Smithay output modes changed"
        );
        if !self.outputs.contains_key(&descriptor.id) {
            return self.connect_output(descriptor);
        }
        let previous_descriptor = self
            .outputs
            .get(&descriptor.id)
            .expect("output existence was checked before renderer registration")
            .descriptor
            .clone();
        self.register_renderer_output(&descriptor, Some(&previous_descriptor))?;
        let discarded = self.discard_output_presentations(descriptor.id);
        if discarded > 0 {
            info!(
                output_device = descriptor.id.device_id,
                output_connector = descriptor.id.connector_id,
                discarded,
                "discarded presentation feedback for replaced output mode"
            );
        }
        let managed = self
            .outputs
            .get_mut(&descriptor.id)
            .expect("output existence was checked before renderer registration");
        for mode in managed.output.modes() {
            managed.output.delete_mode(mode);
        }
        for mode in &descriptor.modes {
            managed.output.add_mode(*mode);
        }
        managed.output.set_preferred(descriptor.mode);
        managed.output.change_current_state(
            Some(descriptor.mode),
            None,
            Some(smithay::output::Scale::Fractional(
                descriptor.scale.as_f64(),
            )),
            None,
        );
        let output_id = descriptor.id;
        managed.descriptor = descriptor;
        // Mode replacement ends any in-flight flip; force a fresh first frame.
        self.redraw_states
            .insert(output_id, OutputRedrawState::Queued);
        self.reflow_outputs();
        Ok(())
    }

    #[cfg(feature = "tty")]
    fn disconnect_output(&mut self, id: BackendOutputId) {
        let discarded = self.discard_output_presentations(id);
        let Some(managed) = self.outputs.remove(&id) else {
            return;
        };
        self.space.unmap_output(&managed.output);
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.unregister_output(RenderOutputId {
                device_id: id.device_id,
                connector_id: id.connector_id,
            });
        }
        self.redraw_states.remove(&id);
        if let Some(backend) = self.backend.as_mut() {
            backend.remove_output_buffers(id);
        }
        self.display_handle.remove_global::<Self>(managed.global);
        self.reflow_outputs();
        info!(
            device_id = id.device_id,
            connector_id = id.connector_id,
            discarded_presentations = discarded,
            "Smithay output disconnected"
        );
    }

    #[cfg(feature = "tty")]
    fn register_renderer_output(
        &mut self,
        descriptor: &OutputDescriptor,
        restore: Option<&OutputDescriptor>,
    ) -> Result<(), String> {
        let target = renderer_target(descriptor);
        let result = self
            .renderer
            .as_mut()
            .map(|renderer| renderer.register_output(target));
        let Some(result) = result else {
            return Ok(());
        };
        let buffers = result.map_err(|error| error.to_string())?;
        if let Some(backend) = self.backend.as_mut()
            && let Err(error) = backend.install_output_buffers(descriptor.id, buffers)
        {
            if let Some(previous) = restore {
                if let Some(renderer) = self.renderer.as_mut() {
                    let _ = renderer.register_output(renderer_target(previous));
                }
            } else if let Some(renderer) = self.renderer.as_mut() {
                renderer.unregister_output(target.output);
            }
            return Err(error.to_string());
        }
        Ok(())
    }

    #[cfg(feature = "tty")]
    fn reflow_outputs(&mut self) {
        let mut outputs = self.outputs.iter().collect::<Vec<_>>();
        outputs.sort_by_key(|(id, _)| (id.device_id, id.connector_id));
        let mut x = 0;
        for (_, managed) in outputs {
            managed
                .output
                .change_current_state(None, None, None, Some((x, 0).into()));
            self.space.map_output(&managed.output, (x, 0));
            x = x.saturating_add(
                self.space
                    .output_geometry(&managed.output)
                    .map(|geometry| geometry.size.w)
                    .unwrap_or(0),
            );
        }
        // Arrange the default workspace against the primary output, then force
        // every CRTC (including secondaries with no workspace content) through
        // a first frame so each page-flip ring starts.
        self.reflow_default_workspace();
        self.force_redraw_all();
    }
}

#[cfg(feature = "tty")]
fn renderer_target(descriptor: &OutputDescriptor) -> NativeOutputTarget {
    NativeOutputTarget {
        output: RenderOutputId {
            device_id: descriptor.id.device_id,
            connector_id: descriptor.id.connector_id,
        },
        viewport: Rect::new(
            0,
            0,
            u32::try_from(descriptor.mode.size.w).unwrap_or(0),
            u32::try_from(descriptor.mode.size.h).unwrap_or(0),
        ),
        format: descriptor.native_format,
        scale: descriptor.scale,
    }
}

#[cfg(all(test, feature = "tty"))]
mod tests {
    use super::*;

    #[test]
    fn queue_marks_idle_and_waiting_outputs_dirty() {
        assert_eq!(OutputRedrawState::Idle.queue(), OutputRedrawState::Queued);
        assert_eq!(OutputRedrawState::Queued.queue(), OutputRedrawState::Queued);
        assert_eq!(
            OutputRedrawState::WaitingForVBlank {
                redraw_needed: false
            }
            .queue(),
            OutputRedrawState::WaitingForVBlank {
                redraw_needed: true
            }
        );
        assert!(
            OutputRedrawState::WaitingForVBlank {
                redraw_needed: true
            }
            .queue()
            .needs_gpu_retry()
        );
    }
}
