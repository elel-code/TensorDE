//! Output topology: connect / change / disconnect / reflow.
//!
//! Value types are host/drm; Smithay Output is created only at this adapter edge.

use tracing::info;

use crate::{
    backend::{BackendOutputEvent, BackendOutputId, OutputDescriptor},
    render::RenderOutputId,
};

use super::{
    ManagedOutput, OutputRedrawState, RuntimeState,
    output_helpers::{rects_overlap, renderer_target},
};

impl RuntimeState {
    pub(crate) fn apply_backend_output_events(
        &mut self,
        events: impl IntoIterator<Item = BackendOutputEvent>,
    ) -> Result<(), String> {
        let mut first_error = None;
        let mut any = false;
        for event in events {
            any = true;
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
        if any {
            // Topology-rate only; never on page-flip.
            self.refresh_output_management_protocol();
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
            mode_width = descriptor.mode.width,
            mode_height = descriptor.mode.height,
            refresh_millihertz = descriptor.mode.refresh_millihertz,
            scale = descriptor.scale.as_f64(),
            "Smithay output connected"
        );
        let preferred = crate::backend::smithay_mode(descriptor.mode);
        let output = smithay::output::Output::new(
            descriptor.name.clone(),
            smithay::output::PhysicalProperties {
                size: descriptor.physical_size.into(),
                subpixel: crate::backend::smithay_subpixel(descriptor.subpixel),
                make: "Unknown".to_owned(),
                model: descriptor.name.clone(),
                serial_number: "Unknown".to_owned(),
            },
        );
        for mode in &descriptor.modes {
            output.add_mode(crate::backend::smithay_mode(*mode));
        }
        output.set_preferred(preferred);
        output.change_current_state(
            Some(preferred),
            None,
            Some(smithay::output::Scale::Fractional(
                descriptor.scale.as_f64(),
            )),
            Some((0, 0).into()),
        );
        // Backend identity for gamma / capture without scanning by name.
        output.user_data().insert_if_missing(|| descriptor.id);
        let global = output.create_global::<Self>(&self.display_handle);
        self.space.map_output(&output, (0, 0));
        let output_id = descriptor.id;
        self.outputs.insert(
            output_id,
            ManagedOutput {
                output,
                global,
                descriptor,
                has_presented: false,
            },
        );
        self.set_redraw_state(output_id, OutputRedrawState::Queued);
        self.register_present_output(output_id);
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
            mode_width = descriptor.mode.width,
            mode_height = descriptor.mode.height,
            refresh_millihertz = descriptor.mode.refresh_millihertz,
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
        let preferred = crate::backend::smithay_mode(descriptor.mode);
        let managed = self
            .outputs
            .get_mut(&descriptor.id)
            .expect("output existence was checked before renderer registration");
        managed
            .output
            .user_data()
            .insert_if_missing(|| descriptor.id);
        for mode in managed.output.modes() {
            managed.output.delete_mode(mode);
        }
        for mode in &descriptor.modes {
            managed.output.add_mode(crate::backend::smithay_mode(*mode));
        }
        managed.output.set_preferred(preferred);
        managed.output.change_current_state(
            Some(preferred),
            None,
            Some(smithay::output::Scale::Fractional(
                descriptor.scale.as_f64(),
            )),
            None,
        );
        let output_id = descriptor.id;
        managed.descriptor = descriptor;
        let output = managed.output.clone();
        self.space.refresh_output_geometry(&output);
        self.arrange_layer_output(&output);
        // Mode replacement ends any in-flight flip; force a fresh first frame.
        self.set_redraw_state(output_id, OutputRedrawState::Queued);
        self.reflow_outputs();
        Ok(())
    }

    #[cfg(feature = "tty")]
    fn disconnect_output(&mut self, id: BackendOutputId) {
        let discarded = self.discard_output_presentations(id);
        let Some(managed) = self.outputs.remove(&id) else {
            return;
        };
        self.unregister_present_output(id);
        self.remove_layer_output(&managed.output);
        self.space.unmap_output(&managed.output, &self.popups);
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
        self.protocol_globals
            .gamma_control()
            .output_removed(&managed.output);
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
        outputs.sort_by(|(left_id, left), (right_id, right)| {
            left.descriptor
                .name
                .cmp(&right.descriptor.name)
                .then_with(|| left_id.cmp(right_id))
        });
        outputs.sort_by_key(|(_, managed)| managed.descriptor.position.is_none());

        let mut placed = Vec::<(i32, i32, i32, i32)>::new();
        let mut auto_x: i32 = 0;
        for (_, managed) in outputs {
            let size = self
                .space
                .output_geometry(&managed.output)
                .map(|geometry| (geometry.size.w, geometry.size.h))
                .unwrap_or_else(|| {
                    let scale = managed.descriptor.scale.as_f64();
                    let width = (f64::from(managed.descriptor.mode.width) / scale).round() as i32;
                    let height = (f64::from(managed.descriptor.mode.height) / scale).round() as i32;
                    (width.max(1), height.max(1))
                });
            let position = managed
                .descriptor
                .position
                .filter(|(x, y)| {
                    let target = (*x, *y, size.0, size.1);
                    !placed
                        .iter()
                        .any(|existing| rects_overlap(*existing, target))
                })
                .unwrap_or_else(|| {
                    let position = (auto_x, 0);
                    auto_x = auto_x.saturating_add(size.0);
                    position
                });
            managed.output.change_current_state(
                None,
                None,
                None,
                Some((position.0, position.1).into()),
            );
            self.space
                .map_output(&managed.output, (position.0, position.1));
            if let Some(geometry) = self.space.output_geometry(&managed.output) {
                placed.push((
                    geometry.loc.x,
                    geometry.loc.y,
                    geometry.size.w,
                    geometry.size.h,
                ));
                if managed.descriptor.position.is_none() {
                    auto_x = auto_x.max(geometry.loc.x.saturating_add(geometry.size.w));
                }
            }
        }
        self.reflow_default_workspace_layout();
        self.force_redraw_all();
    }
}
