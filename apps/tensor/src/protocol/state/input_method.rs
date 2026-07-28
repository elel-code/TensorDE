//! Scene integration for Tensor-owned input-method popup surfaces.

use tensor_protocol::SurfaceTransform;
use tensor_util::{LogicalPoint, LogicalRect, OutputScale};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::protocol::globals::compositor::send_surface_state;

use super::{
    RuntimeState, output_integer_scale, space::update_surface_tree_output,
    surface_tree::for_each_surface_tree, surface_tree_under, wayland_transform,
    window::surface_tree_bbox,
};

struct InputMethodPopupAnchor {
    root: WlSurface,
    render_origin: LogicalPoint<i32>,
    cursor: LogicalRect<i32>,
    target: LogicalRect<i32>,
}

impl RuntimeState {
    /// Mapped view that currently anchors visible input-method popups.
    pub(crate) fn input_method_popup_root(&self) -> Option<WlSurface> {
        let (focused, _) = self.protocol_globals.input_method.active_popup_context()?;
        self.owning_view_root(&focused)
    }

    /// Rebuild the old and new owner without refreshing unrelated windows.
    pub(crate) fn refresh_input_method_popups(&mut self, previous_root: Option<WlSurface>) {
        let current_root = self.input_method_popup_root();
        let mut changed = false;
        if let Some(previous) = previous_root.as_ref() {
            changed |= self.update_surface_content(previous);
        }
        if let Some(current) = current_root.as_ref()
            && previous_root.as_ref().map(Resource::id) != Some(current.id())
        {
            changed |= self.update_surface_content(current);
        }

        self.refresh_input_method_popup_outputs();
        self.protocol_globals.input_method.for_each_popup(|popup| {
            let (scale, transform) = self
                .protocol_globals
                .input_method
                .popup_parent(popup.wl_surface())
                .and_then(|parent| self.owning_view_root(&parent))
                .and_then(|root| {
                    self.space
                        .elements()
                        .find(|window| window.wl_surface().as_deref() == Some(&root))
                        .map(|window| self.window_output_state(window))
                })
                .unwrap_or((OutputScale::ONE, SurfaceTransform::Normal));
            for_each_surface_tree(popup.wl_surface(), &mut |surface, states| {
                send_surface_state(
                    surface,
                    states,
                    output_integer_scale(scale),
                    wayland_transform(transform),
                );
                self.protocol_globals
                    .set_preferred_fractional_scale(surface, scale);
            });
        });
        if changed {
            self.request_redraw_workspace();
        }
    }

    /// Send leave before a live popup role is detached from protocol state.
    pub(crate) fn leave_input_method_popup(&self, surface: &WlSurface) {
        for output in self.space.outputs() {
            update_surface_tree_output(output, None, surface);
            output.cleanup();
        }
    }

    pub(crate) fn leave_all_input_method_popups(&self) {
        self.protocol_globals
            .input_method
            .for_each_popup(|popup| self.leave_input_method_popup(popup.wl_surface()));
    }

    pub(crate) fn leave_input_method_popups_from_output(
        &self,
        output: &crate::protocol::globals::output::Output,
    ) {
        self.protocol_globals.input_method.for_each_popup(|popup| {
            update_surface_tree_output(output, None, popup.wl_surface());
        });
        output.cleanup();
    }

    /// Recompute output membership after layout, output, focus, or cursor
    /// movement. The overlap rectangle is expressed in popup-local space so
    /// the existing surface-tree walker stays allocation-free.
    pub(crate) fn refresh_input_method_popup_outputs(&self) {
        self.protocol_globals.input_method.for_each_popup(|popup| {
            let popup_origin = self
                .input_method_popup_scene_context(popup.wl_surface())
                .map(|(_, origin)| origin);
            for output in self.space.outputs() {
                let overlap = popup_origin.and_then(|origin| {
                    let mut geometry = self.space.output_geometry(output)?;
                    geometry.loc -= origin;
                    Some(geometry)
                });
                update_surface_tree_output(output, overlap, popup.wl_surface());
            }
        });
        for output in self.space.outputs() {
            output.cleanup();
        }
    }

    /// Hit-test input-method popups in creation order, keeping the newest hit
    /// as the topmost result. Returned locations are global logical origins.
    pub(crate) fn input_method_popup_under(
        &self,
        location: LogicalPoint<f64>,
    ) -> Option<(WlSurface, LogicalPoint<f64>)> {
        let mut hit = None;
        self.protocol_globals
            .input_method
            .for_each_visible_popup(|popup| {
                let Some((_, popup_origin)) =
                    self.input_method_popup_scene_context(popup.wl_surface())
                else {
                    return;
                };
                if let Some((surface, surface_origin)) =
                    surface_tree_under(popup.wl_surface(), location, popup_origin)
                {
                    hit = Some((surface, surface_origin.to_f64()));
                }
            });
        hit
    }

    /// Root plus the root-local logical origin used while collecting one input
    /// popup into the focused view's surface tree.
    pub(super) fn input_method_popup_tree_context(
        &self,
        popup: &WlSurface,
    ) -> Option<(WlSurface, LogicalPoint<i32>)> {
        let anchor = self.input_method_popup_anchor()?;
        let origin = position_input_method_popup(
            anchor.cursor,
            surface_tree_bbox(popup, (0, 0)),
            anchor.target,
        );
        Some((anchor.root, origin - anchor.render_origin))
    }

    /// Root plus the output-global logical origin for one input-popup root.
    fn input_method_popup_scene_context(
        &self,
        popup: &WlSurface,
    ) -> Option<(WlSurface, LogicalPoint<i32>)> {
        let anchor = self.input_method_popup_anchor()?;
        let origin = position_input_method_popup(
            anchor.cursor,
            surface_tree_bbox(popup, (0, 0)),
            anchor.target,
        );
        Some((anchor.root, origin))
    }

    fn input_method_popup_anchor(&self) -> Option<InputMethodPopupAnchor> {
        let (focused, rectangle) = self.protocol_globals.input_method.active_popup_context()?;
        let root = self.owning_view_root(&focused)?;
        let window = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(&root))?;
        let mapped = self.space.element_location(window)?;
        let render_origin = mapped - window.geometry().loc;
        let focused_content = self.surface_buffers.current_content(&focused.id())?;
        let cursor = LogicalRect::new(
            render_origin
                + LogicalPoint::from((
                    focused_content
                        .local_geometry
                        .x
                        .saturating_add(rectangle.loc.x),
                    focused_content
                        .local_geometry
                        .y
                        .saturating_add(rectangle.loc.y),
                )),
            rectangle.size,
        );
        let output = self
            .space
            .output_under(cursor.loc.to_f64())
            .next()
            .or_else(|| self.space.outputs_for_element(window).next())?;
        let target = self.space.output_geometry(output)?;
        Some(InputMethodPopupAnchor {
            root,
            render_origin,
            cursor,
            target,
        })
    }
}

fn position_input_method_popup(
    cursor: LogicalRect<i32>,
    popup_bbox: LogicalRect<i32>,
    target: LogicalRect<i32>,
) -> LogicalPoint<i32> {
    let target_right = target.loc.x.saturating_add(target.size.w);
    let popup_right = popup_bbox.loc.x.saturating_add(popup_bbox.size.w);
    let mut x = cursor.loc.x;
    let overflow = x.saturating_add(popup_right).saturating_sub(target_right);
    if overflow > 0 {
        x = x.saturating_sub(overflow);
    }
    let popup_left = x.saturating_add(popup_bbox.loc.x);
    if popup_left < target.loc.x {
        x = x.saturating_add(target.loc.x.saturating_sub(popup_left));
    }

    let below = cursor.loc.y.saturating_add(cursor.size.h);
    let below_bottom = below
        .saturating_add(popup_bbox.loc.y)
        .saturating_add(popup_bbox.size.h);
    let target_bottom = target.loc.y.saturating_add(target.size.h);
    let y = if below_bottom <= target_bottom {
        below
    } else {
        cursor
            .loc
            .y
            .saturating_sub(popup_bbox.loc.y)
            .saturating_sub(popup_bbox.size.h)
    };
    LogicalPoint::new(x, y)
}

#[cfg(test)]
mod tests {
    use super::position_input_method_popup;
    use tensor_util::{LogicalRect, LogicalSize};

    #[test]
    fn popup_prefers_below_and_stays_inside_horizontal_output_bounds() {
        let target = LogicalRect::new((100, 50).into(), (300, 200).into());
        let cursor = LogicalRect::new((380, 80).into(), (8, 20).into());
        let popup = LogicalRect::new((2, 1).into(), (80, 32).into());

        assert_eq!(
            position_input_method_popup(cursor, popup, target),
            (318, 100).into()
        );

        let left_cursor = LogicalRect::new((70, 80).into(), (8, 20).into());
        assert_eq!(
            position_input_method_popup(left_cursor, popup, target),
            (98, 100).into()
        );
    }

    #[test]
    fn popup_moves_above_cursor_when_below_would_cross_output() {
        let target = LogicalRect::new((0, 0).into(), (400, 240).into());
        let cursor = LogicalRect::new((120, 225).into(), (7, 10).into());
        let popup = LogicalRect::new((-3, 2).into(), LogicalSize::new(80, 32));

        assert_eq!(
            position_input_method_popup(cursor, popup, target),
            (120, 191).into()
        );
    }
}
