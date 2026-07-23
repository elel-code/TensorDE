use tensor_util::Rect;
use tracing::{info, warn};

use crate::{
    backend::{BackendOutputEvent, BackendOutputId, OutputDescriptor},
    render::{NativeOutputBuffer, NativeOutputTarget, RenderOutputId},
};

use super::{DEFAULT_WORKSPACE, ManagedOutput, RuntimeState};

impl RuntimeState {
    #[cfg(feature = "tty")]
    pub(crate) fn submit_default_workspace_frame(&mut self) {
        let Some(scene) = self.world.extract_scene(DEFAULT_WORKSPACE) else {
            return;
        };
        if let Err(error) = self.prepare_surface_acquires(&scene) {
            self.flush_client_releases();
            warn!(%error, "client explicit-sync acquire is not ready");
            return;
        }
        let Some((output_id, output)) = self
            .outputs
            .iter()
            .find(|(_, managed)| {
                let Some(geometry) = self.space.output_geometry(&managed.output) else {
                    return false;
                };
                geometry.loc.x == scene.viewport.x
                    && geometry.loc.y == scene.viewport.y
                    && geometry.size.w == i32::try_from(scene.viewport.width).unwrap_or(i32::MAX)
                    && geometry.size.h == i32::try_from(scene.viewport.height).unwrap_or(i32::MAX)
            })
            .map(|(id, managed)| (*id, managed.output.clone()))
        else {
            return;
        };
        if let Some(renderer) = self.renderer.as_mut()
            && let Err(error) = renderer.refresh_completed()
        {
            self.repaint_pending.insert(output_id);
            warn!(%error, "renderer completion poll failed before output slot selection");
            return;
        }
        let render_output = RenderOutputId {
            device_id: output_id.device_id,
            connector_id: output_id.connector_id,
        };
        let Some(mut next_slot) = self
            .renderer
            .as_ref()
            .and_then(|renderer| renderer.next_output_slot(render_output))
        else {
            self.repaint_pending.insert(output_id);
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
            self.repaint_pending.insert(output_id);
            return;
        };
        // Drain feedback only after all retry gates passed. The local owner
        // below discards it if Vulkan or atomic KMS cannot accept this frame.
        let captured_presentation = self.capture_scene_presentation(output_id, &output, &scene);
        let Some(result) = self
            .renderer
            .as_mut()
            .map(|renderer| renderer.submit_scene(render_output, scene))
        else {
            drop(captured_presentation);
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
                    self.repaint_pending.insert(output_id);
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
                    self.repaint_pending.insert(output_id);
                    return;
                }
                // Atomic KMS has latched ownership of the submitted client
                // buffers. Let clients prepare their next frame immediately;
                // presentation feedback remains pending until vblank.
                self.send_submitted_frame_callbacks(&output, &captured_presentation);
                self.queue_presentation(output_id, frame.timeline_value, captured_presentation);
                self.repaint_pending.remove(&output_id);
                info!(
                    output_device = output_id.device_id,
                    output_connector = output_id.connector_id,
                    output_slot = frame.output_slot,
                    serial = frame.serial,
                    timeline = frame.timeline_value,
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
                self.repaint_pending.insert(output_id);
                warn!(
                    output_device = output_id.device_id,
                    output_connector = output_id.connector_id,
                    %error,
                    "renderer frame boundary failed"
                );
            }
        }
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
        info!(
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
        if self.repaint_pending.remove(&presentation.output) {
            self.submit_default_workspace_frame();
        }
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
        let outputs = self.outputs.keys().copied().collect::<Vec<_>>();
        self.repaint_pending.extend(outputs);
        self.submit_default_workspace_frame();
        self.schedule_renderer_retry_if_needed();
    }

    pub(crate) fn retry_renderer_repaint(&mut self) {
        self.renderer_retry_scheduled = false;
        self.submit_default_workspace_frame();
        self.schedule_renderer_retry_if_needed();
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
        self.repaint_pending.iter().copied().any(|output| {
            renderer.output_waiting_for_gpu(RenderOutputId {
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
        output.set_preferred(descriptor.preferred_mode);
        output.change_current_state(
            Some(descriptor.preferred_mode),
            None,
            None,
            Some((0, 0).into()),
        );
        let global = output.create_global::<Self>(&self.display_handle);
        self.space.map_output(&output, (0, 0));
        self.outputs.insert(
            descriptor.id,
            ManagedOutput {
                output,
                global,
                descriptor,
            },
        );
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
        managed.output.set_preferred(descriptor.preferred_mode);
        managed
            .output
            .change_current_state(Some(descriptor.preferred_mode), None, None, None);
        managed.descriptor = descriptor;
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
        self.repaint_pending.remove(&id);
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
            x += managed
                .output
                .current_mode()
                .map(|mode| mode.size.w)
                .unwrap_or(0);
        }
        self.reflow_default_workspace();
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
            u32::try_from(descriptor.preferred_mode.size.w).unwrap_or(0),
            u32::try_from(descriptor.preferred_mode.size.h).unwrap_or(0),
        ),
        format: descriptor.native_format,
    }
}
