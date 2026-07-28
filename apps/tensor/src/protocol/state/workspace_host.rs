//! Multi-workspace host for compositor policy.
//!
//! ECS already stores per-view [`WorkspaceId`]. This host owns the **active**
//! workspace, a fixed desktop pool, and mapping of protocol windows when the
//! user switches. Protocol (`ext-workspace`) and IPC read/write through here.

use tensor_util::Size;
use tracing::{debug, info};

use crate::ecs::{ViewId, WorkspaceId};

use super::{ProtocolWindow, RuntimeState};

/// Default number of virtual desktops (1-based UX in IPC/protocol names).
pub(crate) const WORKSPACE_COUNT: u32 = 9;

/// Compositor-owned workspace selection and pool.
#[derive(Debug)]
pub(crate) struct WorkspaceHost {
    active: WorkspaceId,
    /// Fixed pool `0..count` always exists for docks / IPC.
    count: u32,
}

impl Default for WorkspaceHost {
    fn default() -> Self {
        Self {
            active: WorkspaceId::new(0),
            count: WORKSPACE_COUNT,
        }
    }
}

impl WorkspaceHost {
    pub(crate) fn active(&self) -> WorkspaceId {
        self.active
    }

    pub(crate) fn count(&self) -> u32 {
        self.count
    }

    pub(crate) fn ids(&self) -> impl Iterator<Item = WorkspaceId> + '_ {
        (0..self.count).map(WorkspaceId::new)
    }

    /// Activate by zero-based index. Returns `true` if the active id changed.
    pub(crate) fn activate_index(&mut self, index: u32) -> bool {
        if index >= self.count {
            return false;
        }
        let next = WorkspaceId::new(index);
        if self.active == next {
            return false;
        }
        self.active = next;
        true
    }

    pub(crate) fn activate_id(&mut self, id: WorkspaceId) -> bool {
        if id.get() >= self.count {
            return false;
        }
        if self.active == id {
            return false;
        }
        self.active = id;
        true
    }

    pub(crate) fn cycle(&mut self, delta: i32) -> bool {
        let count = self.count as i32;
        let cur = self.active.get() as i32;
        let next = ((cur + delta).rem_euclid(count)) as u32;
        self.activate_index(next)
    }
}

impl RuntimeState {
    pub(crate) fn active_workspace(&self) -> WorkspaceId {
        self.workspaces.active()
    }

    pub(crate) fn workspace_count(&self) -> u32 {
        self.workspaces.count()
    }

    /// Switch the visible workspace. Relayout + show/hide mapped windows.
    pub(crate) fn activate_workspace(&mut self, id: WorkspaceId) -> bool {
        if !self.workspaces.activate_id(id) {
            return false;
        }
        info!(workspace = id.get(), "workspace activated");
        self.apply_workspace_visibility();
        let _ = self.reflow_active_workspace();
        self.refresh_ext_workspace_protocol();
        true
    }

    pub(crate) fn activate_workspace_index(&mut self, index: u32) -> bool {
        if index >= self.workspaces.count() {
            return false;
        }
        self.activate_workspace(WorkspaceId::new(index))
    }

    pub(crate) fn cycle_workspace(&mut self, delta: i32) -> bool {
        if !self.workspaces.cycle(delta) {
            return false;
        }
        let id = self.workspaces.active();
        info!(workspace = id.get(), "workspace cycled");
        self.apply_workspace_visibility();
        let _ = self.reflow_active_workspace();
        self.refresh_ext_workspace_protocol();
        true
    }

    /// Move a view to another workspace (does not force activate).
    pub(crate) fn move_view_to_workspace(
        &mut self,
        view_id: ViewId,
        workspace: WorkspaceId,
    ) -> bool {
        if workspace.get() >= self.workspaces.count() {
            return false;
        }
        match self.world.move_view(view_id, workspace) {
            Ok(()) => {
                debug!(
                    view = view_id.get(),
                    workspace = workspace.get(),
                    "view moved to workspace"
                );
                self.apply_workspace_visibility();
                let _ = self.reflow_active_workspace();
                self.refresh_ext_workspace_protocol();
                true
            }
            Err(error) => {
                tracing::warn!(%error, view = view_id.get(), "failed to move view");
                false
            }
        }
    }

    /// Show only windows belonging to the active workspace.
    pub(crate) fn apply_workspace_visibility(&mut self) {
        let active = self.workspaces.active();
        let windows: Vec<ProtocolWindow> = self.space.elements().cloned().collect();
        for window in windows {
            let Some(surface) = window.wl_surface() else {
                continue;
            };
            let Some(view_id) = self.view_for_surface(&surface) else {
                continue;
            };
            let on_active = self
                .world
                .view_workspace(view_id)
                .is_some_and(|ws| ws == active);
            if on_active {
                if self.space.element_geometry(&window).is_none() {
                    // Was unmapped; map at last known or origin until reflow.
                    let loc = self
                        .world
                        .geometry(view_id)
                        .map(|g| (g.x, g.y))
                        .unwrap_or((0, 0));
                    self.space.map_element(window, loc, false);
                }
            } else if self.space.element_geometry(&window).is_some() {
                self.space.unmap_elem(&window, &self.popups);
            }
        }
        self.space.refresh(&self.popups);
        #[cfg(feature = "tty")]
        self.refresh_input_method_popup_outputs();
        // Restore keyboard focus to the active workspace's focused view.
        #[cfg(feature = "tty")]
        if let Some(view_id) = self.world.focused_view(active)
            && let Some(window) = self.mapped_window_for_view(view_id)
        {
            self.focus_mapped_window(window, crate::protocol::serial::next_serial());
        }
    }

    /// Reflow the active workspace (layout + configure + redraw).
    pub(crate) fn reflow_active_workspace(&mut self) -> bool {
        if !self.reflow_active_workspace_layout() {
            return false;
        }
        #[cfg(feature = "tty")]
        self.submit_default_workspace_frame();
        true
    }

    pub(crate) fn reflow_active_workspace_layout(&mut self) -> bool {
        let workspace = self.workspaces.active();
        let Some(area) = self.default_workspace_area() else {
            return false;
        };
        self.world.arrange_workspace(workspace, self.layout, area);

        let windows = self
            .space
            .elements()
            .filter_map(|window| {
                let surface = window.wl_surface()?;
                let view_id = self.view_for_surface(&surface)?;
                let geometry = self.world.geometry(view_id)?;
                Some((window.clone(), geometry))
            })
            .collect::<Vec<_>>();

        for (window, geometry) in &windows {
            self.space
                .relocate_element(window, (geometry.x, geometry.y));
        }
        #[cfg(feature = "xwayland")]
        self.relocate_x11_popups();
        self.space.refresh(&self.popups);
        #[cfg(feature = "tty")]
        self.refresh_input_method_popup_outputs();

        for (window, geometry) in windows {
            self.update_window_surface_state(&window);
            if let Some(toplevel) = window.toplevel().cloned() {
                let size = Size::new(geometry.width, geometry.height);
                let bounds = Size::new(area.width, area.height);
                toplevel.set_layout(size, bounds);
                toplevel.send_pending_configure();
            }
            #[cfg(feature = "xwayland")]
            if let Some(x11) = window.x11_surface() {
                super::xwayland::configure_x11_window(x11, geometry);
            }
        }
        #[cfg(feature = "xwayland")]
        self.update_x11_popup_surface_states();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutEngine, LayoutKind};
    use wayland_server::Display;

    fn state() -> RuntimeState {
        let display = Display::<RuntimeState>::new().unwrap();
        RuntimeState::with_appearance(
            display,
            LayoutEngine::new(LayoutKind::Scrolling1D),
            crate::scene::SceneAppearance::default(),
        )
    }

    #[test]
    fn workspace_host_cycles_within_pool() {
        let mut host = WorkspaceHost::default();
        assert_eq!(host.active().get(), 0);
        assert!(host.cycle(1));
        assert_eq!(host.active().get(), 1);
        assert!(host.activate_index(8));
        assert!(host.cycle(1));
        assert_eq!(host.active().get(), 0);
    }

    #[test]
    fn activate_rejects_out_of_range() {
        let mut state = state();
        assert!(!state.activate_workspace_index(99));
        assert_eq!(state.active_workspace().get(), 0);
    }
}
