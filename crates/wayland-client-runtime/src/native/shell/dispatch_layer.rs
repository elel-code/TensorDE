//! wlr-layer-shell-v1 dispatch for the native shell.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use super::types::{NativeShellEvent, NativeShellState};
use crate::geometry::SuggestedSize;

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        layer: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = state
            .layer_objects
            .get(&layer.id().protocol_id())
            .copied();
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                layer.ack_configure(serial);
                if let Some(id) = id {
                    let suggested = if let Some(record) = state.layers.get_mut(&id) {
                        record.configured = true;
                        if width > 0 {
                            record.logical_w = width;
                        }
                        if height > 0 {
                            record.logical_h = height;
                        }
                        if width > 0 || height > 0 {
                            record.pending_size = Some((
                                if width > 0 { width } else { record.logical_w },
                                if height > 0 { height } else { record.logical_h },
                            ));
                        }
                        if let Some(buffer) = record.buffer.as_ref() {
                            record.wl.attach(Some(buffer), 0, 0);
                            record.wl.damage_buffer(0, 0, i32::MAX, i32::MAX);
                            record.wl.commit();
                        }
                        Some(SuggestedSize::new(
                            Some(record.logical_w).filter(|&w| w > 0),
                            Some(record.logical_h).filter(|&h| h > 0),
                        ))
                    } else {
                        None
                    };
                    if let Some(suggested_size) = suggested {
                        state.push(NativeShellEvent::LayerConfigure {
                            surface: id,
                            suggested_size,
                            serial,
                        });
                    }
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                if let Some(id) = id {
                    state.push(NativeShellEvent::LayerClosed { surface: id });
                }
            }
            _ => {}
        }
    }
}
