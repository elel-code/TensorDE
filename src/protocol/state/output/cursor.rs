use tracing::warn;

use super::super::{OutputRedrawState, RuntimeState};

fn queue_output(
    redraw_states: &mut std::collections::HashMap<
        crate::backend::BackendOutputId,
        OutputRedrawState,
    >,
    output: crate::backend::BackendOutputId,
) {
    let state = redraw_states
        .entry(output)
        .or_insert(OutputRedrawState::Idle);
    *state = state.queue();
}

impl RuntimeState {
    #[cfg(test)]
    pub(crate) fn test_cursor_surface_output_count(&self) -> usize {
        let crate::protocol::cursor::CursorImage::Surface(surface) = self.cursor.image() else {
            return 0;
        };
        self.space
            .outputs()
            .filter(|output| output.contains_surface(surface))
            .count()
    }

    #[cfg(test)]
    pub(crate) fn test_pointer_cursor_visible(&self) -> bool {
        let Some(location) = self.input_seat.pointer_location() else {
            return false;
        };
        self.space.outputs().any(|output| {
            let Some(geometry) = self.space.output_geometry(output) else {
                return false;
            };
            let scale = output.current_scale();
            let viewport = tensor_util::Rect::new(
                0,
                0,
                scale.physical_length_round(u32::try_from(geometry.size.w).unwrap_or(0)),
                scale.physical_length_round(u32::try_from(geometry.size.h).unwrap_or(0)),
            );
            self.cursor
                .overlay_for_source_at(0, location, geometry, scale, viewport, |surface, scale| {
                    crate::protocol::state::surfaces::cursor_surface_raster(
                        &self.surface_buffers,
                        surface,
                        scale,
                    )
                })
                .is_some()
        })
    }

    pub(crate) fn queue_cursor_redraw_between(
        &mut self,
        source: u64,
        previous: tensor_util::LogicalPoint<f64>,
        current: tensor_util::LogicalPoint<f64>,
    ) {
        let cursor = &self.cursor;
        let surface_buffers = &self.surface_buffers;
        let space = &self.space;
        let redraw_states = &mut self.redraw_states;
        for output in space.outputs() {
            let Some(geometry) = space.output_geometry(output) else {
                continue;
            };
            let scale = output.current_scale();
            let viewport = tensor_util::Rect::new(
                0,
                0,
                scale.physical_length_round(u32::try_from(geometry.size.w).unwrap_or(0)),
                scale.physical_length_round(u32::try_from(geometry.size.h).unwrap_or(0)),
            );
            let visible = [previous, current].into_iter().any(|location| {
                cursor
                    .overlay_for_source_at(
                        source,
                        location,
                        geometry,
                        scale,
                        viewport,
                        |surface, scale| {
                            crate::protocol::state::surfaces::cursor_surface_raster(
                                surface_buffers,
                                surface,
                                scale,
                            )
                        },
                    )
                    .is_some()
            });
            if visible {
                queue_output(redraw_states, output.id());
            }
        }
    }

    pub(crate) fn request_cursor_redraw_between(
        &mut self,
        source: u64,
        previous: tensor_util::LogicalPoint<f64>,
        current: tensor_util::LogicalPoint<f64>,
    ) {
        self.queue_cursor_redraw_between(source, previous, current);
        self.flush_queued_redraws();
    }

    pub(crate) fn refresh_cursor_surface_outputs(&mut self) {
        let space = &self.space;
        self.cursor.drain_retired_surfaces(|surface| {
            for output in space.outputs() {
                output.leave(&surface);
            }
            crate::protocol::globals::compositor::with_states(&surface, |states| {
                if let Some(storage) = states
                    .data_map
                    .get::<std::sync::Mutex<crate::protocol::globals::seat::CursorSurfaceState>>()
                {
                    storage.lock().unwrap().outputs.clear();
                }
            });
        });

        let pointer = self.input_seat.pointer_location();
        let space = &self.space;
        let protocol_globals = &self.protocol_globals;
        self.cursor
            .for_each_surface_position(pointer, |surface, location| {
                let mut preferred = None;
                crate::protocol::globals::compositor::with_states(surface, |states| {
                    let Some(storage) = states.data_map.get::<std::sync::Mutex<
                        crate::protocol::globals::seat::CursorSurfaceState,
                    >>() else {
                        return;
                    };
                    let view = crate::protocol::state::surfaces::surface_view(states);
                    let mut cursor = storage.lock().unwrap();
                    let bounds = view.and_then(|view| {
                        let width = f64::from(view.size.0);
                        let height = f64::from(view.size.1);
                        let left = location.x - f64::from(cursor.hotspot.x);
                        let top = location.y - f64::from(cursor.hotspot.y);
                        (location.x.is_finite()
                            && location.y.is_finite()
                            && width > 0.0
                            && height > 0.0)
                            .then_some((left, top, left + width, top + height))
                    });
                    cursor.outputs.retain(|instance| {
                        space
                            .outputs()
                            .any(|output| output.instance_id() == *instance)
                    });
                    for output in space.outputs() {
                        let Some(geometry) = space.output_geometry(output) else {
                            continue;
                        };
                        let output_left = f64::from(geometry.loc.x);
                        let output_top = f64::from(geometry.loc.y);
                        let output_right =
                            f64::from(geometry.loc.x.saturating_add(geometry.size.w));
                        let output_bottom =
                            f64::from(geometry.loc.y.saturating_add(geometry.size.h));
                        let inside = geometry.size.w > 0
                            && geometry.size.h > 0
                            && bounds.is_some_and(|(left, top, right, bottom)| {
                                right > output_left
                                    && bottom > output_top
                                    && left < output_right
                                    && top < output_bottom
                            });
                        let instance = output.instance_id();
                        let membership = cursor
                            .outputs
                            .iter()
                            .position(|current| *current == instance);
                        if inside && membership.is_none() {
                            output.enter(surface);
                            cursor.outputs.push(instance);
                        } else if !inside && let Some(index) = membership {
                            output.leave(surface);
                            cursor.outputs.swap_remove(index);
                        }
                        if inside {
                            let scale = output.current_scale();
                            if preferred.is_none_or(|preferred| scale > preferred) {
                                preferred = Some(scale);
                            }
                        }
                    }
                });
                let Some(scale) = preferred else {
                    return;
                };
                crate::protocol::globals::compositor::with_states(surface, |states| {
                    crate::protocol::globals::compositor::send_surface_state(
                        surface,
                        states,
                        super::super::output_values::output_integer_scale(scale),
                        wayland_server::protocol::wl_output::Transform::Normal,
                    );
                });
                protocol_globals.set_preferred_fractional_scale(surface, scale);
            });
    }

    pub(crate) fn install_dnd_icon(
        &mut self,
        surface: Option<wayland_server::protocol::wl_surface::WlSurface>,
    ) {
        self.retire_dnd_icon();
        self.dnd_icon.set(surface);
        self.refresh_dnd_icon_outputs();
    }

    pub(crate) fn retire_dnd_icon(&mut self) {
        let Some(surface) = self.dnd_icon.surface().cloned() else {
            self.dnd_icon.clear();
            return;
        };
        let redraw_states = &mut self.redraw_states;
        for output in self.space.outputs() {
            if self.dnd_icon.outputs.contains(&output.instance_id()) {
                queue_output(redraw_states, output.id());
            }
            output.leave(&surface);
        }
        self.dnd_icon.clear();
    }

    pub(crate) fn destroy_dnd_icon(
        &mut self,
        surface: &wayland_server::protocol::wl_surface::WlSurface,
    ) -> bool {
        if !self.dnd_icon.uses_surface(surface) {
            return false;
        }
        let redraw_states = &mut self.redraw_states;
        for output in self.space.outputs() {
            if self.dnd_icon.outputs.contains(&output.instance_id()) {
                queue_output(redraw_states, output.id());
            }
            output.forget_surface(surface);
        }
        self.dnd_icon.clear();
        true
    }

    pub(crate) fn refresh_dnd_icon_outputs(&mut self) {
        let Some(surface) = self.dnd_icon.surface().cloned() else {
            return;
        };
        let pointer = self.input_seat.pointer_location();
        let view = crate::protocol::globals::compositor::with_states(&surface, |states| {
            crate::protocol::state::surfaces::surface_view(states)
        });
        let bounds = pointer
            .zip(view)
            .and_then(|(pointer, view)| self.dnd_icon.logical_bounds(pointer, view.size));
        let space = &self.space;
        let redraw_states = &mut self.redraw_states;
        self.dnd_icon.outputs.retain(|instance| {
            space
                .outputs()
                .any(|output| output.instance_id() == *instance)
        });
        let mut preferred = None;
        for output in space.outputs() {
            let Some(geometry) = space.output_geometry(output) else {
                continue;
            };
            let inside = geometry.size.w > 0
                && geometry.size.h > 0
                && bounds.is_some_and(|(left, top, right, bottom)| {
                    right > f64::from(geometry.loc.x)
                        && bottom > f64::from(geometry.loc.y)
                        && left < f64::from(geometry.loc.x.saturating_add(geometry.size.w))
                        && top < f64::from(geometry.loc.y.saturating_add(geometry.size.h))
                });
            let instance = output.instance_id();
            let membership = self
                .dnd_icon
                .outputs
                .iter()
                .position(|current| *current == instance);
            if inside || membership.is_some() {
                queue_output(redraw_states, output.id());
            }
            if inside && membership.is_none() {
                output.enter(&surface);
                self.dnd_icon.outputs.push(instance);
            } else if !inside && let Some(index) = membership {
                output.leave(&surface);
                self.dnd_icon.outputs.swap_remove(index);
            }
            if inside {
                let scale = output.current_scale();
                if preferred.is_none_or(|preferred| scale > preferred) {
                    preferred = Some(scale);
                }
            }
        }
        let Some(scale) = preferred else {
            return;
        };
        crate::protocol::globals::compositor::with_states(&surface, |states| {
            crate::protocol::globals::compositor::send_surface_state(
                &surface,
                states,
                super::super::output_values::output_integer_scale(scale),
                wayland_server::protocol::wl_output::Transform::Normal,
            );
        });
        self.protocol_globals
            .set_preferred_fractional_scale(&surface, scale);
    }

    pub(crate) fn duplicate_cursor_animation_timer_fd(
        &self,
    ) -> std::io::Result<Option<std::os::fd::OwnedFd>> {
        self.cursor.duplicate_animation_timer_fd()
    }

    pub(crate) fn complete_cursor_animation_timer(&mut self) -> bool {
        match self.cursor.complete_animation_timer() {
            Ok(redraw) => {
                if redraw {
                    self.request_redraw_all();
                }
                true
            }
            Err(error) => {
                self.cursor_animation_timer_failed(&error);
                false
            }
        }
    }

    pub(crate) fn cursor_animation_timer_failed(&mut self, error: &std::io::Error) {
        warn!(%error, "cursor animation io_uring completion failed");
        self.cursor.animation_timer_failed();
    }
}
