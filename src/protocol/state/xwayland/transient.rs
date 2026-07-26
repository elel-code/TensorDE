use std::collections::{HashMap, HashSet};

use smithay::{wayland::seat::WaylandFocus, xwayland::X11Surface};
use tracing::{debug, warn};
use wayland_server::protocol::wl_surface::WlSurface;

use crate::ecs::{ViewId, ViewPlacement};
use tensor_util::Size;

use super::{super::RuntimeState, XWaylandWindowLifecycle};

/// Keeps normal X11 `WM_TRANSIENT_FOR` windows pending until their complete
/// rootless lifecycle and an owning managed X11 view are both available.
///
/// The registry intentionally does not own Smithay or renderer state. Its
/// windows become ordinary attached ECS views only after reconciliation.
#[derive(Debug, Default)]
pub(crate) struct XWaylandTransientRegistry {
    windows: HashMap<u32, X11Surface>,
}

impl XWaylandTransientRegistry {
    pub(super) fn observe(&mut self, window: X11Surface) {
        self.windows.insert(window.window_id(), window);
    }

    pub(super) fn remove(&mut self, window: u32) -> Option<X11Surface> {
        self.windows.remove(&window)
    }

    pub(super) fn window(&self, window: u32) -> Option<X11Surface> {
        self.windows.get(&window).cloned()
    }

    pub(super) fn windows(&self) -> Vec<X11Surface> {
        let mut windows = self.windows.values().cloned().collect::<Vec<_>>();
        windows.sort_unstable_by_key(X11Surface::window_id);
        windows
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransientReconciliation {
    Ignore,
    Hold,
    Attach,
    RestoreTiled,
    RemoveUnowned,
}

fn transient_reconciliation(
    lifecycle: XWaylandWindowLifecycle,
    has_transient_owner: bool,
    owner_is_managed: bool,
    has_view: bool,
) -> TransientReconciliation {
    if !lifecycle.protocol_ready() {
        return TransientReconciliation::Ignore;
    }
    if !has_transient_owner {
        return TransientReconciliation::RestoreTiled;
    }
    if owner_is_managed {
        return TransientReconciliation::Attach;
    }
    if has_view {
        TransientReconciliation::RemoveUnowned
    } else {
        TransientReconciliation::Hold
    }
}

impl RuntimeState {
    pub(crate) fn x11_window_new(&mut self, x11: X11Surface) {
        if x11.is_transient_for().is_some() {
            self.xwayland_transients.observe(x11);
        }
    }

    /// Reconcile a normal X11 window whose `WM_TRANSIENT_FOR` property may
    /// have changed after map/association. An unresolved owner is held outside
    /// the scene; it is never converted into a global X11 placement fallback.
    pub(crate) fn x11_transient_for_changed(&mut self, x11: X11Surface) {
        if x11.is_transient_for().is_some() {
            self.xwayland_transients.observe(x11);
            self.reconcile_x11_transients();
        } else {
            self.xwayland_transients.remove(x11.window_id());
            self.restore_x11_tiled_window(x11);
        }
    }

    pub(crate) fn reconcile_x11_transients(&mut self) {
        let windows = self.xwayland_transients.windows();
        for _ in 0..windows.len() {
            let mut changed = false;
            for x11 in &windows {
                changed |= self.reconcile_x11_transient(x11.clone());
            }
            if !changed {
                break;
            }
        }
    }

    /// Apply a dialog resize request as an update to its attached-view size
    /// policy. Position requests remain intentionally ignored.
    pub(crate) fn x11_transient_configure_requested(
        &mut self,
        x11: &X11Surface,
        width: Option<u32>,
        height: Option<u32>,
    ) -> bool {
        let Some(surface) = x11.wl_surface() else {
            return false;
        };
        let Some(view_id) = self.view_for_surface(&surface) else {
            return false;
        };
        let Some(ViewPlacement::Attached {
            owner,
            preferred_size,
        }) = self.world.view_placement(view_id)
        else {
            return false;
        };
        let requested = Size::new(
            width.unwrap_or(preferred_size.width).max(1),
            height.unwrap_or(preferred_size.height).max(1),
        );
        match self.world.set_view_placement(
            view_id,
            ViewPlacement::Attached {
                owner,
                preferred_size: requested,
            },
        ) {
            Ok(_) => {
                let _ = self.reflow_default_workspace();
                true
            }
            Err(error) => {
                warn!(
                    %error,
                    window = x11.window_id(),
                    "failed to update XWayland transient dialog size"
                );
                true
            }
        }
    }

    pub(crate) fn x11_normal_hints_changed(&mut self, x11: X11Surface) {
        let Some(surface) = x11.wl_surface() else {
            return;
        };
        if self.update_x11_constraints(&surface, &x11) {
            let _ = self.reflow_default_workspace();
        }
    }

    /// Drop child views before their owner leaves ECS, then leave their X11
    /// lifecycle pending. This makes an owner disappearing before its client
    /// dialogs a safe, invisible state rather than a dangling attachment.
    pub(crate) fn detach_x11_transient_views_for_owner(&mut self, owner: ViewId) -> bool {
        for child in self.world.attached_children(owner) {
            let Some((surface, window_id)) = self.x11_surface_for_view(child) else {
                warn!(
                    child = child.get(),
                    owner = owner.get(),
                    "attached view had no XWayland surface during owner teardown"
                );
                return false;
            };
            if let Some(lifecycle) = self.xwayland_windows.get_mut(&window_id) {
                lifecycle.mark_unregistered();
            }
            if self.unregister_toplevel(&surface).is_none() {
                warn!(
                    child = child.get(),
                    owner = owner.get(),
                    "failed to detach XWayland transient before owner teardown"
                );
                return false;
            }
        }
        self.world.attached_children(owner).is_empty()
    }

    fn reconcile_x11_transient(&mut self, x11: X11Surface) -> bool {
        let window_id = x11.window_id();
        let Some(lifecycle) = self.xwayland_windows.get(&window_id).copied() else {
            return false;
        };
        let Some(surface) = x11.wl_surface() else {
            return false;
        };
        let has_transient_owner = x11.is_transient_for().is_some();
        let owner = has_transient_owner
            .then(|| self.x11_transient_owner(&x11))
            .flatten();
        match transient_reconciliation(
            lifecycle,
            has_transient_owner,
            owner.is_some(),
            self.view_for_surface(&surface).is_some(),
        ) {
            TransientReconciliation::Ignore | TransientReconciliation::Hold => return false,
            TransientReconciliation::RestoreTiled => {
                self.xwayland_transients.remove(window_id);
                return self.restore_x11_tiled_window(x11);
            }
            TransientReconciliation::RemoveUnowned => {
                return self.remove_unowned_x11_transient(&x11);
            }
            TransientReconciliation::Attach => {}
        }
        let owner = owner.expect("attached XWayland transient has a managed owner");
        let placement = ViewPlacement::Attached {
            owner,
            preferred_size: x11_transient_preferred_size(&x11),
        };
        let (view_id, newly_registered) = match self.view_for_surface(&surface) {
            Some(view_id) => (view_id, false),
            None if lifecycle.should_register() => {
                let Some(view_id) =
                    self.register_x11_window_with_placement(x11.clone(), Some(placement))
                else {
                    return false;
                };
                if let Some(lifecycle) = self.xwayland_windows.get_mut(&window_id) {
                    lifecycle.mark_registered();
                }
                (view_id, true)
            }
            None => return false,
        };

        let changed = match self.world.set_view_placement(view_id, placement) {
            Ok(changed) => changed,
            Err(error) => {
                warn!(%error, window = window_id, "failed to attach XWayland transient dialog");
                return false;
            }
        };
        if changed {
            let _ = self.reflow_default_workspace();
        }
        if changed || newly_registered {
            self.reconcile_x11_popups();
        }
        changed || newly_registered
    }

    pub(super) fn restore_x11_tiled_window(&mut self, x11: X11Surface) -> bool {
        let window_id = x11.window_id();
        let Some(lifecycle) = self.xwayland_windows.get(&window_id).copied() else {
            return false;
        };
        if !lifecycle.protocol_ready() {
            return false;
        }
        let Some(surface) = x11.wl_surface() else {
            return false;
        };
        let (view_id, newly_registered) = match self.view_for_surface(&surface) {
            Some(view_id) => (view_id, false),
            None if lifecycle.should_register() => {
                let Some(view_id) = self.register_x11_window(x11.clone()) else {
                    return false;
                };
                if let Some(lifecycle) = self.xwayland_windows.get_mut(&window_id) {
                    lifecycle.mark_registered();
                }
                (view_id, true)
            }
            None => return false,
        };
        let changed = match self.world.set_view_placement(view_id, ViewPlacement::Tiled) {
            Ok(changed) => changed,
            Err(error) => {
                warn!(%error, window = window_id, "failed to restore XWayland tiled placement");
                return false;
            }
        };
        if changed {
            let _ = self.reflow_default_workspace();
        }
        if changed || newly_registered {
            self.reconcile_x11_popups();
        }
        changed || newly_registered
    }

    fn remove_unowned_x11_transient(&mut self, x11: &X11Surface) -> bool {
        let Some(surface) = x11.wl_surface() else {
            return false;
        };
        if self.view_for_surface(&surface).is_none() {
            return false;
        }
        if let Some(lifecycle) = self.xwayland_windows.get_mut(&x11.window_id()) {
            lifecycle.mark_unregistered();
        }
        let removed = self.unregister_toplevel(&surface).is_some();
        if removed {
            debug!(
                window = x11.window_id(),
                "held XWayland transient dialog until its owner becomes managed"
            );
        }
        removed
    }

    fn x11_transient_owner(&self, x11: &X11Surface) -> Option<ViewId> {
        let mut ancestor = x11.is_transient_for()?;
        let mut seen = HashSet::new();
        while seen.insert(ancestor) {
            if let Some(owner) = self.managed_x11_window(ancestor)
                && let Some(surface) = owner.wl_surface()
                && let Some(view_id) = self.view_for_surface(&surface)
            {
                return Some(view_id);
            }
            ancestor = self
                .xwayland_transients
                .window(ancestor)?
                .is_transient_for()?;
        }
        None
    }

    fn x11_surface_for_view(&self, view_id: ViewId) -> Option<(WlSurface, u32)> {
        self.space.elements().find_map(|window| {
            let x11 = window.x11_surface()?;
            let surface = window.wl_surface()?.into_owned();
            (self.view_for_surface(&surface) == Some(view_id)).then_some((surface, x11.window_id()))
        })
    }
}

fn x11_transient_preferred_size(x11: &X11Surface) -> Size {
    let size = x11.geometry().size;
    Size::new(logical_extent(size.w), logical_extent(size.h))
}

fn logical_extent(value: i32) -> u32 {
    u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::{
        TransientReconciliation, XWaylandWindowLifecycle, logical_extent, transient_reconciliation,
        x11_transient_preferred_size,
    };

    #[test]
    fn non_positive_x11_extents_stay_representable_for_attached_layout() {
        assert_eq!(logical_extent(-4), 1);
        assert_eq!(logical_extent(0), 1);
        assert_eq!(logical_extent(480), 480);
    }

    #[test]
    fn preferred_size_helper_is_kept_linked_to_x11_geometry_contract() {
        let _ =
            x11_transient_preferred_size as fn(&smithay::xwayland::X11Surface) -> tensor_util::Size;
    }

    #[test]
    fn unresolved_owner_never_falls_back_to_a_tiled_x11_view() {
        let mut lifecycle = XWaylandWindowLifecycle::default();
        lifecycle.map_requested();
        lifecycle.surface_associated();

        assert_eq!(
            transient_reconciliation(lifecycle, true, false, false),
            TransientReconciliation::Hold
        );
        assert_eq!(
            transient_reconciliation(lifecycle, true, false, true),
            TransientReconciliation::RemoveUnowned
        );
        assert_eq!(
            transient_reconciliation(lifecycle, true, true, false),
            TransientReconciliation::Attach
        );
    }

    #[test]
    fn removed_transient_property_restores_the_ordinary_tiled_role() {
        let mut lifecycle = XWaylandWindowLifecycle::default();
        lifecycle.map_requested();
        lifecycle.surface_associated();

        assert_eq!(
            transient_reconciliation(lifecycle, false, false, false),
            TransientReconciliation::RestoreTiled
        );
        assert_eq!(
            transient_reconciliation(XWaylandWindowLifecycle::default(), false, false, false,),
            TransientReconciliation::Ignore
        );
    }
}
