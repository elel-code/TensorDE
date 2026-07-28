//! Output dispatch for the native shell.

use wayland_client::protocol::wl_output;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};

use super::types::{NativeShellEvent, NativeShellState};

impl Dispatch<wl_output::WlOutput, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(&name) = state.output_objects.get(&output.id().protocol_id()) else {
            return;
        };
        match event {
            wl_output::Event::Geometry {
                x,
                y,
                physical_width,
                physical_height,
                make,
                model,
                ..
            } => {
                if let Some(record) = state.outputs.get_mut(&name) {
                    record.make = make.clone();
                    record.model = model.clone();
                    record.x = x;
                    record.y = y;
                    record.physical_width = physical_width;
                    record.physical_height = physical_height;
                }
                state.push(NativeShellEvent::OutputGeometry {
                    output: name,
                    x,
                    y,
                    physical_width,
                    physical_height,
                    make,
                    model,
                });
            }
            wl_output::Event::Mode {
                flags,
                width,
                height,
                refresh,
            } => {
                let current = match flags {
                    WEnum::Value(f) => f.contains(wl_output::Mode::Current),
                    _ => false,
                };
                if current && let Some(record) = state.outputs.get_mut(&name) {
                    record.mode_width = width;
                    record.mode_height = height;
                    record.mode_refresh_mhz = refresh;
                }
                state.push(NativeShellEvent::OutputMode {
                    output: name,
                    width,
                    height,
                    refresh,
                    current,
                });
            }
            wl_output::Event::Scale { factor } => {
                if let Some(record) = state.outputs.get_mut(&name) {
                    record.scale = factor;
                }
                state.push(NativeShellEvent::OutputScale {
                    output: name,
                    factor,
                });
            }
            wl_output::Event::Name { name: output_name } => {
                if let Some(record) = state.outputs.get_mut(&name) {
                    record.name = Some(output_name);
                }
            }
            wl_output::Event::Description { description } => {
                if let Some(record) = state.outputs.get_mut(&name) {
                    record.description = Some(description);
                }
            }
            wl_output::Event::Done => {
                if let Some(record) = state.outputs.get_mut(&name) {
                    record.done = true;
                }
                state.push(NativeShellEvent::OutputDone { output: name });
            }
            _ => {}
        }
    }
}
