//! Scene integration for Tensor-owned input-method popup surfaces.

use tensor_protocol::SurfaceTransform;
use tensor_util::{LogicalPoint, OutputScale};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::protocol::globals::compositor::send_surface_state;

use super::{
    RuntimeState, output_integer_scale, space::update_surface_tree_output,
    surface_tree::for_each_surface_tree, surface_tree_under, wayland_transform,
};

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
        let popup_origin = self
            .input_method_popup_scene_context()
            .map(|(_, origin)| origin);
        self.protocol_globals.input_method.for_each_popup(|popup| {
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
        let (_, popup_origin) = self.input_method_popup_scene_context()?;
        let mut hit = None;
        self.protocol_globals
            .input_method
            .for_each_visible_popup(|popup| {
                if let Some((surface, surface_origin)) =
                    surface_tree_under(popup.wl_surface(), location, popup_origin)
                {
                    hit = Some((surface, surface_origin.to_f64()));
                }
            });
        hit
    }

    /// Root plus the global logical origin shared by input-popup roots.
    fn input_method_popup_scene_context(&self) -> Option<(WlSurface, LogicalPoint<i32>)> {
        let (focused, rectangle) = self.protocol_globals.input_method.active_popup_context()?;
        let root = self.owning_view_root(&focused)?;
        let window = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(&root))?;
        let mapped = self.space.element_location(window)?;
        let render_origin = mapped - window.geometry().loc;
        let focused_content = self.surface_buffers.current_content(&focused.id())?;
        let popup_origin = render_origin
            + LogicalPoint::from((
                focused_content
                    .local_geometry
                    .x
                    .saturating_add(rectangle.loc.x),
                focused_content
                    .local_geometry
                    .y
                    .saturating_add(rectangle.loc.y)
                    .saturating_add(rectangle.size.h),
            ));
        Some((root, popup_origin))
    }
}
