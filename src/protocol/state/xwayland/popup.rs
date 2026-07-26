use std::collections::{HashMap, HashSet};

use smithay::{
    utils::{Logical, Point},
    xwayland::X11Surface,
};
use tracing::debug;
use wayland_server::{Resource, backend::ObjectId, protocol::wl_surface::WlSurface};

use super::super::{ProtocolWindow, RuntimeState};

/// Protocol-owned attachment for one override-redirect X11 surface. The
/// renderer only sees the associated surface through its owning view's flat
/// scene-content table.
#[derive(Clone, Debug)]
pub(super) struct XWaylandPopupAttachment {
    pub(super) window: X11Surface,
    pub(super) surface: WlSurface,
    pub(super) owner: ObjectId,
    pub(super) offset: Point<i32, Logical>,
}

#[derive(Debug)]
struct XWaylandPopupState {
    window: X11Surface,
    surface: Option<WlSurface>,
    lifecycle: PopupLifecycle,
    attachment: Option<XWaylandPopupAttachment>,
}

/// The protocol-side gate deliberately separates XWM mapping and
/// xwayland-shell association. A transient owner is checked by runtime state,
/// because only it knows which X11 windows are managed ECS views.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PopupLifecycle {
    mapped: bool,
    surface_associated: bool,
}

impl PopupLifecycle {
    fn protocol_ready(self) -> bool {
        self.mapped && self.surface_associated
    }

    fn attachment_allowed(self, has_managed_owner: bool) -> bool {
        self.protocol_ready() && has_managed_owner
    }
}

/// Tracks only rootless override-redirect windows. A popup becomes active
/// after XWM mapping, xwayland-shell association, and a managed X11 ancestor
/// have all been observed.
#[derive(Debug, Default)]
pub(crate) struct XWaylandPopupRegistry {
    popups: HashMap<u32, XWaylandPopupState>,
    stacking: Vec<u32>,
}

impl XWaylandPopupRegistry {
    pub(super) fn observe(&mut self, window: X11Surface) {
        self.popups
            .entry(window.window_id())
            .and_modify(|state| state.window = window.clone())
            .or_insert(XWaylandPopupState {
                window,
                surface: None,
                lifecycle: PopupLifecycle::default(),
                attachment: None,
            });
    }

    pub(super) fn mark_mapped(&mut self, window: X11Surface) {
        self.observe(window.clone());
        if let Some(state) = self.popups.get_mut(&window.window_id()) {
            state.lifecycle.mapped = true;
        }
    }

    pub(super) fn associate(&mut self, window: X11Surface, surface: WlSurface) {
        self.observe(window.clone());
        if let Some(state) = self.popups.get_mut(&window.window_id()) {
            state.surface = Some(surface);
            state.lifecycle.surface_associated = true;
        }
    }

    pub(super) fn ready_window(&self, window: u32) -> Option<X11Surface> {
        let state = self.popups.get(&window)?;
        (state.lifecycle.protocol_ready() && state.surface.is_some()).then(|| state.window.clone())
    }

    pub(super) fn ready_windows(&self) -> Vec<u32> {
        let mut windows = self
            .popups
            .iter()
            .filter_map(|(window, state)| {
                (state.lifecycle.protocol_ready() && state.surface.is_some()).then_some(*window)
            })
            .collect::<Vec<_>>();
        windows.sort_unstable();
        windows
    }

    pub(super) fn window(&self, window: u32) -> Option<X11Surface> {
        self.popups.get(&window).map(|state| state.window.clone())
    }

    pub(super) fn attachment_allowed(&self, window: u32, has_managed_owner: bool) -> bool {
        self.popups.get(&window).is_some_and(|state| {
            state.lifecycle.attachment_allowed(has_managed_owner) && state.surface.is_some()
        })
    }

    pub(super) fn set_attachment(
        &mut self,
        window: u32,
        owner: ObjectId,
        offset: Point<i32, Logical>,
    ) -> Option<(XWaylandPopupAttachment, bool)> {
        let state = self.popups.get_mut(&window)?;
        let surface = state.surface.clone()?;
        if !state.lifecycle.protocol_ready() {
            return None;
        }
        let changed = state.attachment.as_ref().is_none_or(|current| {
            current.owner != owner
                || current.offset != offset
                || current.surface.id() != surface.id()
        });
        let attachment = XWaylandPopupAttachment {
            window: state.window.clone(),
            surface,
            owner,
            offset,
        };
        state.attachment = Some(attachment.clone());
        if !self.stacking.contains(&window) {
            self.stacking.push(window);
        }
        Some((attachment, changed))
    }

    pub(super) fn restack(&mut self, window: u32, above: Option<u32>) -> Option<ObjectId> {
        let owner = self.popups.get(&window)?.attachment.as_ref()?.owner.clone();
        let index = self.stacking.iter().position(|id| *id == window)?;
        self.stacking.remove(index);
        let insertion = stack_above_index(&self.stacking, above);
        self.stacking.insert(insertion, window);
        Some(owner)
    }

    pub(super) fn attachment(&self, window: u32) -> Option<XWaylandPopupAttachment> {
        self.popups.get(&window)?.attachment.clone()
    }

    pub(super) fn attachments_for_owner(&self, owner: &ObjectId) -> Vec<XWaylandPopupAttachment> {
        self.stacking
            .iter()
            .filter_map(|window| self.popups.get(window)?.attachment.as_ref())
            .filter(|attachment| &attachment.owner == owner)
            .cloned()
            .collect()
    }

    pub(super) fn attachments(&self) -> Vec<XWaylandPopupAttachment> {
        self.stacking
            .iter()
            .filter_map(|window| self.popups.get(window)?.attachment.as_ref())
            .cloned()
            .collect()
    }

    pub(super) fn owner_for_surface(&self, surface: &ObjectId) -> Option<ObjectId> {
        self.popups
            .values()
            .filter_map(|state| state.attachment.as_ref())
            .find(|attachment| attachment.surface.id() == *surface)
            .map(|attachment| attachment.owner.clone())
    }

    pub(super) fn detach(&mut self, window: u32) -> Option<XWaylandPopupAttachment> {
        self.stacking.retain(|id| *id != window);
        self.popups.get_mut(&window)?.attachment.take()
    }

    pub(super) fn unmap(&mut self, window: u32) -> Option<XWaylandPopupAttachment> {
        let state = self.popups.get_mut(&window)?;
        state.lifecycle.mapped = false;
        self.stacking.retain(|id| *id != window);
        state.attachment.take()
    }

    pub(super) fn remove(&mut self, window: u32) -> Option<XWaylandPopupAttachment> {
        self.stacking.retain(|id| *id != window);
        self.popups.remove(&window)?.attachment
    }

    pub(super) fn surface_destroyed(
        &mut self,
        surface: &ObjectId,
    ) -> Option<XWaylandPopupAttachment> {
        let window = self.popups.iter().find_map(|(window, state)| {
            (state.surface.as_ref().map(Resource::id) == Some(surface.clone())).then_some(*window)
        })?;
        self.stacking.retain(|id| *id != window);
        let state = self
            .popups
            .get_mut(&window)
            .expect("popup window was collected from this registry");
        state.surface = None;
        state.lifecycle.surface_associated = false;
        state.attachment.take()
    }

    pub(super) fn detach_owner(&mut self, owner: &ObjectId) -> Vec<XWaylandPopupAttachment> {
        let ids = self
            .popups
            .iter()
            .filter_map(|(id, state)| {
                (state
                    .attachment
                    .as_ref()
                    .map(|attachment| &attachment.owner)
                    == Some(owner))
                .then_some(*id)
            })
            .collect::<Vec<_>>();
        ids.into_iter().filter_map(|id| self.detach(id)).collect()
    }
}

impl RuntimeState {
    /// Start tracking a rootless override-redirect window. It stays outside
    /// the ECS view set until its Wayland surface, map state, and managed X11
    /// transient ancestor are all known.
    pub(crate) fn x11_popup_new(&mut self, x11: X11Surface) {
        self.xwayland_popups.observe(x11);
    }

    pub(crate) fn x11_popup_surface_associated(&mut self, x11: X11Surface, surface: WlSurface) {
        self.xwayland_popups.associate(x11, surface);
        self.reconcile_x11_popups();
    }

    pub(crate) fn x11_popup_mapped(&mut self, x11: X11Surface) {
        self.xwayland_popups.mark_mapped(x11);
        self.reconcile_x11_popups();
    }

    pub(crate) fn x11_popup_configured(&mut self, x11: X11Surface, above: Option<u32>) {
        let window_id = x11.window_id();
        self.xwayland_popups.observe(x11);
        self.reconcile_x11_popups();
        if let Some(owner) = self.xwayland_popups.restack(window_id, above) {
            self.restack_x11_popups_for_owner(&owner);
            self.refresh_x11_popup_owner_content(&owner);
        }
    }

    pub(crate) fn x11_popup_transient_for_changed(&mut self, x11: X11Surface) {
        self.xwayland_popups.observe(x11);
        self.reconcile_x11_popups();
    }

    pub(crate) fn x11_popup_unmapped(&mut self, x11: &X11Surface) {
        if let Some(attachment) = self.xwayland_popups.unmap(x11.window_id()) {
            self.unmap_x11_popup_window(&attachment.window);
            self.space.refresh(&self.popups);
            self.refresh_x11_popup_owner_content(&attachment.owner);
        }
    }

    pub(crate) fn x11_popup_destroyed(&mut self, x11: &X11Surface) {
        if let Some(attachment) = self.xwayland_popups.remove(x11.window_id()) {
            self.unmap_x11_popup_window(&attachment.window);
            self.space.refresh(&self.popups);
            self.refresh_x11_popup_owner_content(&attachment.owner);
        }
    }

    /// Drop an attached popup when its Wayland root is destroyed before XWM
    /// has emitted the matching X11 unmap/destroy event. The caller rebuilds
    /// the returned owner's scene table as part of normal surface teardown.
    pub(crate) fn x11_popup_surface_destroyed(&mut self, surface: &WlSurface) -> Option<WlSurface> {
        let attachment = self.xwayland_popups.surface_destroyed(&surface.id())?;
        let owner = self.mapped_surface_for_id(&attachment.owner);
        self.unmap_x11_popup_window(&attachment.window);
        self.space.refresh(&self.popups);
        owner
    }

    /// Return the root view that owns this detached X11 popup surface.
    pub(crate) fn x11_popup_owner_for_surface(&self, surface: &WlSurface) -> Option<WlSurface> {
        let owner = self.xwayland_popups.owner_for_surface(&surface.id())?;
        self.mapped_surface_for_id(&owner)
    }

    /// Flatten attached override-redirect trees into the owning view. The
    /// offsets are already relative to the view's logical origin, including
    /// any X11 frame geometry adjustment required by the surface tree.
    pub(crate) fn x11_popup_surface_trees_for_root(
        &self,
        root: &WlSurface,
    ) -> Vec<(WlSurface, (i32, i32))> {
        self.xwayland_popups
            .attachments_for_owner(&root.id())
            .into_iter()
            .map(|attachment| {
                let geometry = attachment.window.geometry();
                (
                    attachment.surface,
                    (
                        attachment.offset.x.saturating_sub(geometry.loc.x),
                        attachment.offset.y.saturating_sub(geometry.loc.y),
                    ),
                )
            })
            .collect()
    }

    pub(crate) fn x11_popup_surfaces_for_root(&self, root: &WlSurface) -> Vec<WlSurface> {
        self.xwayland_popups
            .attachments_for_owner(&root.id())
            .into_iter()
            .map(|attachment| attachment.surface)
            .collect()
    }

    /// Keep detached popup input windows at their owning root's logical
    /// position whenever layout moves a view. Their X11 global coordinates
    /// never become an independent layout coordinate system.
    pub(crate) fn relocate_x11_popups(&mut self) {
        for attachment in self.xwayland_popups.attachments() {
            self.place_x11_popup(&attachment);
        }
    }

    pub(crate) fn update_x11_popup_surface_states(&self) {
        for attachment in self.xwayland_popups.attachments() {
            if let Some(window) = self.x11_window_element(attachment.window.window_id()) {
                self.update_window_surface_state(&window);
            }
        }
    }

    /// A root-window raise must raise all of its detached X11 popups after it
    /// in the input space. Rendering order remains owned by its one ECS view.
    pub(crate) fn raise_x11_popups_for_root(&mut self, root: &WlSurface) {
        self.restack_x11_popups_for_owner(&root.id());
    }

    /// Toplevel teardown owns the parent scene tree, so dependent popup
    /// windows only need to leave Tensor's input/output space here.
    pub(crate) fn detach_x11_popups_for_owner(&mut self, owner: &ObjectId) {
        let attachments = self.xwayland_popups.detach_owner(owner);
        if attachments.is_empty() {
            return;
        }
        for attachment in attachments {
            self.unmap_x11_popup_window(&attachment.window);
        }
        self.space.refresh(&self.popups);
    }

    pub(crate) fn reconcile_x11_popups(&mut self) {
        let windows = self.xwayland_popups.ready_windows();
        for _ in 0..windows.len() {
            let mut changed = false;
            for window in &windows {
                changed |= self.reconcile_x11_popup(*window);
            }
            if !changed {
                break;
            }
        }
    }

    fn reconcile_x11_popup(&mut self, window_id: u32) -> bool {
        let Some(popup) = self.xwayland_popups.ready_window(window_id) else {
            return false;
        };
        let previous = self.xwayland_popups.attachment(window_id);
        let owner = self.x11_popup_owner(&popup);
        if !self
            .xwayland_popups
            .attachment_allowed(window_id, owner.is_some())
        {
            if let Some(attachment) = self.xwayland_popups.detach(window_id) {
                self.unmap_x11_popup_window(&attachment.window);
                self.space.refresh(&self.popups);
                self.refresh_x11_popup_owner_content(&attachment.owner);
                return true;
            }
            debug!(
                window = window_id,
                "XWayland popup has no managed transient owner"
            );
            return false;
        }
        let Some((owner_window, owner_surface)) = owner else {
            return false;
        };

        let offset = x11_popup_offset(&popup, &owner_window);
        let Some((attachment, changed)) =
            self.xwayland_popups
                .set_attachment(window_id, owner_surface.id(), offset)
        else {
            return false;
        };
        let needs_mapping = self.x11_window_element(window_id).is_none();
        if !changed && !needs_mapping {
            return false;
        }

        self.place_x11_popup(&attachment);
        self.restack_x11_popups_for_owner(&attachment.owner);
        self.space.refresh(&self.popups);
        if let Some(window) = self.x11_window_element(window_id) {
            self.update_window_surface_state(&window);
        }

        if let Some(previous_owner) = previous
            .as_ref()
            .map(|attachment| &attachment.owner)
            .filter(|owner| **owner != attachment.owner)
        {
            self.refresh_x11_popup_owner_content(previous_owner);
        }
        self.refresh_x11_popup_owner_content(&attachment.owner);
        true
    }

    fn x11_popup_owner(&self, popup: &X11Surface) -> Option<(X11Surface, WlSurface)> {
        let mut ancestor = popup.is_transient_for()?;
        let mut seen = HashSet::new();
        while seen.insert(ancestor) {
            if let Some(owner) = self.managed_x11_window(ancestor)
                && let Some(surface) = owner.wl_surface()
                && self.view_for_surface(&surface).is_some()
            {
                return Some((owner, surface));
            }
            ancestor = self.xwayland_popups.window(ancestor)?.is_transient_for()?;
        }
        None
    }

    fn mapped_window_for_surface_id(&self, surface: &ObjectId) -> Option<ProtocolWindow> {
        self.space
            .elements()
            .find(|window| {
                window
                    .wl_surface()
                    .as_deref()
                    .is_some_and(|candidate| candidate.id() == *surface)
            })
            .cloned()
    }

    fn mapped_surface_for_id(&self, surface: &ObjectId) -> Option<WlSurface> {
        self.mapped_window_for_surface_id(surface)
            .and_then(|window| window.wl_surface().map(|surface| surface.into_owned()))
    }

    fn x11_window_element(&self, window_id: u32) -> Option<ProtocolWindow> {
        self.space
            .elements()
            .find(|window| {
                window
                    .x11_surface()
                    .is_some_and(|surface| surface.window_id() == window_id)
            })
            .cloned()
    }

    fn place_x11_popup(&mut self, attachment: &XWaylandPopupAttachment) {
        let Some(owner) = self.mapped_window_for_surface_id(&attachment.owner) else {
            return;
        };
        let Some(owner_location) = self.space.element_location(&owner) else {
            return;
        };
        let location = owner_location + attachment.offset;
        if let Some(window) = self.x11_window_element(attachment.window.window_id()) {
            self.space.relocate_element(&window, location);
        } else {
            self.space.map_element(
                ProtocolWindow::new_x11(attachment.window.clone()),
                location,
                false,
            );
        }
    }

    fn unmap_x11_popup_window(&mut self, x11: &X11Surface) {
        if let Some(window) = self.x11_window_element(x11.window_id()) {
            self.space.unmap_elem(&window, &self.popups);
        }
    }

    fn restack_x11_popups_for_owner(&mut self, owner: &ObjectId) {
        let Some(mut reference) = self.mapped_window_for_surface_id(owner) else {
            return;
        };
        for attachment in self.xwayland_popups.attachments_for_owner(owner) {
            let Some(popup) = self.x11_window_element(attachment.window.window_id()) else {
                continue;
            };
            self.space.raise_element_above(&popup, &reference, false);
            reference = popup;
        }
    }

    fn refresh_x11_popup_owner_content(&mut self, owner: &ObjectId) {
        let Some(root) = self.mapped_surface_for_id(owner) else {
            return;
        };
        #[cfg(feature = "tty")]
        if self.update_surface_content(&root) {
            self.request_redraw_workspace();
        }
        #[cfg(not(feature = "tty"))]
        let _ = root;
    }
}

fn x11_popup_offset(popup: &X11Surface, owner: &X11Surface) -> Point<i32, Logical> {
    relative_popup_offset(popup.last_configure().loc, owner.last_configure().loc)
}

fn relative_popup_offset(
    popup_location: Point<i32, Logical>,
    owner_location: Point<i32, Logical>,
) -> Point<i32, Logical> {
    popup_location - owner_location
}

fn stack_above_index(stacking: &[u32], above: Option<u32>) -> usize {
    above
        .and_then(|sibling| stacking.iter().position(|id| *id == sibling))
        .map_or(stacking.len(), |index| index + 1)
}

#[cfg(test)]
mod tests {
    use super::{PopupLifecycle, relative_popup_offset, stack_above_index};

    #[test]
    fn x11_above_sibling_keeps_bottom_to_top_order() {
        let stacking = [4, 9, 16];

        assert_eq!(stack_above_index(&stacking, Some(9)), 2);
        assert_eq!(stack_above_index(&stacking, None), 3);
    }

    #[test]
    fn override_redirect_coordinates_become_a_relative_logical_offset() {
        let popup = (248, 189).into();
        let root = (192, 144).into();

        assert_eq!(relative_popup_offset(popup, root), (56, 45).into());
    }

    #[test]
    fn popup_needs_mapping_association_and_a_managed_transient_owner() {
        let mut lifecycle = PopupLifecycle::default();

        assert!(!lifecycle.attachment_allowed(true));
        lifecycle.mapped = true;
        assert!(!lifecycle.attachment_allowed(true));
        lifecycle.surface_associated = true;
        assert!(!lifecycle.attachment_allowed(false));
        assert!(lifecycle.attachment_allowed(true));
    }
}
