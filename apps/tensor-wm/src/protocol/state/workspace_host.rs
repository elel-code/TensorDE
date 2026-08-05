//! Multi-workspace host for compositor policy.
//!
//! ECS already stores per-view [`WorkspaceId`]. This host owns the **active**
//! workspace, a configurable desktop pool, and mapping of protocol windows when the
//! user switches. Protocol (`ext-workspace`) and IPC read/write through here.

use tensor_util::Size;
use thiserror::Error;
use tracing::{debug, info};

use crate::config::WorkspaceConfig;
use crate::ecs::{ViewId, WorkspaceId};

use super::{ProtocolWindow, RuntimeState};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HiddenWorkspace {
    pub(crate) id: WorkspaceId,
    pub(crate) name: Box<str>,
    pub(crate) show_in_overview: bool,
    pub(crate) minimize_target: bool,
}

/// Compositor-owned workspace selection and pool.
#[derive(Debug)]
pub(crate) struct WorkspaceHost {
    active: WorkspaceId,
    /// Regular pool `0..regular_count` is the only pool advertised through
    /// ext-workspace and normal next/previous navigation.
    regular_count: u32,
    hidden: Box<[HiddenWorkspace]>,
    minimize_target: WorkspaceId,
}

#[derive(Debug, Error)]
pub(crate) enum ViewWorkspaceError {
    #[error("view {0:?} does not exist")]
    UnknownView(ViewId),
    #[error("workspace {index} is outside the regular workspace pool")]
    InvalidWorkspace { index: u32 },
    #[error("view {view:?} belongs to hidden workspace {workspace:?}")]
    HiddenWorkspace {
        view: ViewId,
        workspace: WorkspaceId,
    },
    #[error("overview selection is blocked by an active popup grab")]
    InteractionBlocked,
    #[error("view {0:?} is not minimized")]
    NotMinimized(ViewId),
    #[error("view lifecycle operation failed: {0}")]
    Lifecycle(#[from] crate::ecs::ViewLifecycleError),
}

impl Default for WorkspaceHost {
    fn default() -> Self {
        Self::from_config(&WorkspaceConfig::default())
    }
}

impl WorkspaceHost {
    pub(crate) fn from_config(config: &WorkspaceConfig) -> Self {
        let hidden = config
            .hidden
            .iter()
            .enumerate()
            .map(|(index, workspace)| {
                let offset = u32::try_from(index).expect("bounded hidden count fits u32");
                HiddenWorkspace {
                    id: WorkspaceId::new(
                        config
                            .regular_count
                            .checked_add(offset)
                            .expect("validated workspace IDs fit u32"),
                    ),
                    name: workspace.name.clone().into_boxed_str(),
                    show_in_overview: workspace.show_in_overview,
                    minimize_target: workspace.minimize_target,
                }
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let minimize_target = hidden
            .iter()
            .find(|workspace| workspace.minimize_target)
            .expect("validated workspace policy has one minimize target")
            .id;
        Self {
            active: WorkspaceId::new(0),
            regular_count: config.regular_count,
            hidden,
            minimize_target,
        }
    }

    pub(crate) fn active(&self) -> WorkspaceId {
        self.active
    }

    pub(crate) fn count(&self) -> u32 {
        self.regular_count
    }

    pub(crate) fn regular_ids(&self) -> impl Iterator<Item = WorkspaceId> + '_ {
        (0..self.regular_count).map(WorkspaceId::new)
    }

    pub(crate) fn hidden(&self) -> &[HiddenWorkspace] {
        &self.hidden
    }

    pub(crate) fn hidden_count(&self) -> usize {
        self.hidden.len()
    }

    pub(crate) fn minimize_target(&self) -> WorkspaceId {
        self.minimize_target
    }

    /// Activate by zero-based index. Returns `true` if the active id changed.
    pub(crate) fn activate_index(&mut self, index: u32) -> bool {
        if index >= self.regular_count {
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
        if id.get() >= self.regular_count {
            return false;
        }
        if self.active == id {
            return false;
        }
        self.active = id;
        true
    }

    pub(crate) fn cycle(&mut self, delta: i32) -> bool {
        let count = self.regular_count as i32;
        let cur = self.active.get() as i32;
        let next = ((cur + delta).rem_euclid(count)) as u32;
        self.activate_index(next)
    }
}

impl RuntimeState {
    pub(crate) fn configure_workspaces(&mut self, config: &WorkspaceConfig) {
        self.workspaces = WorkspaceHost::from_config(config);
        self.refresh_ext_workspace_protocol();
    }

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
    ) -> Result<ViewId, ViewWorkspaceError> {
        if workspace.get() >= self.workspaces.count() {
            return Err(ViewWorkspaceError::InvalidWorkspace {
                index: workspace.get(),
            });
        }
        let root = self
            .world
            .tiled_ancestor(view_id)
            .ok_or(ViewWorkspaceError::UnknownView(view_id))?;
        #[cfg(feature = "tty")]
        let focus_transfer = self.focus_transfer_before_family_move(root, workspace);
        match self.world.move_view(root, workspace) {
            Ok(()) => {
                debug!(
                    view = root.get(),
                    workspace = workspace.get(),
                    "view moved to workspace"
                );
                self.apply_workspace_visibility();
                let _ = self.reflow_active_workspace();
                #[cfg(feature = "tty")]
                self.finish_focus_transfer(focus_transfer);
                self.refresh_ext_workspace_protocol();
                Ok(root)
            }
            Err(error) => {
                tracing::warn!(%error, view = root.get(), "failed to move view");
                Err(error.into())
            }
        }
    }

    /// Select a stable overview view, activating its regular workspace or
    /// restoring its minimized family first.
    pub(crate) fn activate_view(&mut self, view_id: ViewId) -> Result<ViewId, ViewWorkspaceError> {
        if self.popup_grab.is_some() {
            return Err(ViewWorkspaceError::InteractionBlocked);
        }
        let root = self
            .world
            .tiled_ancestor(view_id)
            .ok_or(ViewWorkspaceError::UnknownView(view_id))?;
        let workspace = self
            .world
            .view_workspace(root)
            .ok_or(ViewWorkspaceError::UnknownView(view_id))?;
        if self.world.minimized_from(root).is_some() {
            self.restore_minimized_view(view_id, true)?;
        } else {
            if workspace.get() >= self.workspaces.count() {
                return Err(ViewWorkspaceError::HiddenWorkspace {
                    view: view_id,
                    workspace,
                });
            }
            let _ = self.activate_workspace(workspace);
        }

        if let Some(window) = self.mapped_window_for_view(view_id) {
            let _ = self.focus_mapped_window(window, crate::protocol::serial::next_serial());
        } else {
            self.world.focus_view(view_id)?;
        }
        Ok(root)
    }

    pub(crate) fn minimize_focused_view(&mut self) -> Option<ViewId> {
        let active = self.workspaces.active();
        let focused = self.world.focused_view(active)?;
        let view_id = self.world.tiled_ancestor(focused)?;
        #[cfg(feature = "tty")]
        let replacement = self
            .world
            .focus_replacement_after_family_removal(view_id)
            .ok()
            .flatten();
        #[cfg(feature = "tty")]
        let minimized_surface = self
            .mapped_window_for_view(view_id)
            .and_then(|window| window.wl_surface().map(|surface| surface.into_owned()));
        let target = self.workspaces.minimize_target();
        match self.world.minimize_view(view_id, target) {
            Ok(true) => {}
            Ok(false) => return None,
            Err(error) => {
                tracing::warn!(%error, view = view_id.get(), "failed to minimize view");
                return None;
            }
        }
        self.apply_workspace_visibility();
        let _ = self.reflow_active_workspace();
        #[cfg(feature = "tty")]
        if let Some(window) = replacement.and_then(|view| self.mapped_window_for_view(view)) {
            let _ = self.focus_mapped_window(window, crate::protocol::serial::next_serial());
        } else if let Some(surface) = minimized_surface {
            self.clear_keyboard_focus_for_surface(&surface);
            self.publish_window_activation(None);
        }
        Some(view_id)
    }

    #[cfg(feature = "tty")]
    fn focus_transfer_before_family_move(
        &mut self,
        root: ViewId,
        destination: WorkspaceId,
    ) -> Option<(
        Option<ViewId>,
        wayland_server::protocol::wl_surface::WlSurface,
    )> {
        let source = self.world.view_workspace(root)?;
        if source != self.workspaces.active() || source == destination {
            return None;
        }
        let focused = self.world.focused_view(source)?;
        if self.world.tiled_ancestor(focused) != Some(root) {
            return None;
        }
        let surface = self
            .mapped_window_for_view(focused)?
            .wl_surface()?
            .into_owned();
        let replacement = self
            .world
            .focus_replacement_after_family_removal(root)
            .ok()
            .flatten();
        Some((replacement, surface))
    }

    #[cfg(feature = "tty")]
    fn finish_focus_transfer(
        &mut self,
        transfer: Option<(
            Option<ViewId>,
            wayland_server::protocol::wl_surface::WlSurface,
        )>,
    ) {
        let Some((replacement, moved_surface)) = transfer else {
            return;
        };
        if let Some(window) = replacement.and_then(|view| self.mapped_window_for_view(view)) {
            let _ = self.focus_mapped_window(window, crate::protocol::serial::next_serial());
        } else {
            self.clear_keyboard_focus_for_surface(&moved_surface);
            self.publish_window_activation(None);
        }
    }

    pub(crate) fn restore_minimized_view(
        &mut self,
        requested: ViewId,
        follow: bool,
    ) -> Result<ViewId, ViewWorkspaceError> {
        let root = self
            .world
            .tiled_ancestor(requested)
            .ok_or(ViewWorkspaceError::UnknownView(requested))?;
        let origin = match self.world.restore_minimized_view(root) {
            Ok(Some(origin)) => origin,
            Ok(None) => return Err(ViewWorkspaceError::NotMinimized(requested)),
            Err(error) => {
                tracing::warn!(%error, view = root.get(), "failed to restore minimized view");
                return Err(error.into());
            }
        };
        if follow && origin != self.workspaces.active() {
            let _ = self.activate_workspace(origin);
        } else {
            self.apply_workspace_visibility();
            let _ = self.reflow_active_workspace();
        }
        #[cfg(feature = "tty")]
        if origin == self.workspaces.active()
            && let Some(window) = self.mapped_window_for_view(requested)
        {
            let _ = self.focus_mapped_window(window, crate::protocol::serial::next_serial());
        }
        Ok(root)
    }

    /// Show only windows belonging to the active workspace.
    pub(crate) fn apply_workspace_visibility(&mut self) {
        let active = self.workspaces.active();
        let windows: Vec<ProtocolWindow> = self.space.retained_elements().cloned().collect();
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
                self.space.hide_element(&window, &self.popups);
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
        let config = WorkspaceConfig {
            regular_count: 3,
            ..Default::default()
        };
        let mut host = WorkspaceHost::from_config(&config);
        assert_eq!(host.active().get(), 0);
        assert!(host.cycle(1));
        assert_eq!(host.active().get(), 1);
        assert!(host.activate_index(2));
        assert!(host.cycle(1));
        assert_eq!(host.active().get(), 0);
        assert_eq!(host.minimize_target().get(), 3);
        assert_eq!(host.hidden()[0].id, WorkspaceId::new(3));
    }

    #[test]
    fn activate_rejects_out_of_range() {
        let mut state = state();
        assert!(!state.activate_workspace_index(99));
        assert_eq!(state.active_workspace().get(), 0);
    }
}
