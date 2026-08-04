//! Value-only compositor and workspace snapshots for local IPC consumers.

use crate::{
    ecs::OverviewViewKind,
    ipc::{
        MAX_OVERVIEW_VIEWS, OverviewGeometrySnapshot, OverviewSnapshot, OverviewViewKindSnapshot,
        OverviewViewSnapshot, OverviewWorkspaceSnapshot,
    },
};

use super::RuntimeState;

impl RuntimeState {
    pub(crate) fn ipc_state_snapshot(&mut self) -> crate::ipc::StateSnapshot {
        let active = self.workspaces.active();
        crate::ipc::StateSnapshot {
            layout: self.layout.kind(),
            view_count: self.world.view_count(active),
            output_count: self.output_count(),
            focused_view: self.world.focused_view(active).map(|view| view.get()),
            workspace: active.get(),
            workspace_count: self.workspaces.count(),
            hidden_workspace_count: self.workspaces.hidden_count(),
            minimized_count: self.world.minimized_count(),
        }
    }

    pub(crate) fn ipc_workspace_snapshots(&mut self) -> Vec<crate::ipc::WorkspaceSnapshot> {
        let active = self.workspaces.active();
        let mut snapshots = self
            .workspaces
            .regular_ids()
            .map(|id| {
                let index = id.get();
                crate::ipc::WorkspaceSnapshot {
                    index,
                    name: (index + 1).to_string(),
                    active: id == active,
                    hidden: false,
                    show_in_overview: true,
                    minimize_target: false,
                    view_count: self.world.view_count(id),
                    focused_view: self.world.focused_view(id).map(|view| view.get()),
                }
            })
            .collect::<Vec<_>>();
        snapshots.extend(self.workspaces.hidden().iter().map(|workspace| {
            crate::ipc::WorkspaceSnapshot {
                index: workspace.id.get(),
                name: workspace.name.to_string(),
                active: false,
                hidden: true,
                show_in_overview: workspace.show_in_overview,
                minimize_target: workspace.minimize_target,
                view_count: self.world.view_count(workspace.id),
                focused_view: self.world.focused_view(workspace.id).map(|view| view.get()),
            }
        }));
        snapshots
    }

    pub(crate) fn ipc_overview_snapshot(&mut self) -> OverviewSnapshot {
        let active_workspace = self.workspaces.active().get();
        let mut topology = self
            .workspaces
            .regular_ids()
            .map(|id| (id, (id.get() + 1).to_string(), false, false))
            .collect::<Vec<_>>();
        topology.extend(
            self.workspaces
                .hidden()
                .iter()
                .filter(|workspace| workspace.show_in_overview)
                .map(|workspace| {
                    (
                        workspace.id,
                        workspace.name.to_string(),
                        true,
                        workspace.minimize_target,
                    )
                }),
        );

        let mut emitted = 0;
        let mut truncated = false;
        let mut workspaces = Vec::with_capacity(topology.len());
        for (id, name, hidden, minimize_target) in topology {
            let mut inventory = self.world.overview_views(id);
            let view_count = inventory.len();
            let available = MAX_OVERVIEW_VIEWS.saturating_sub(emitted);
            if inventory.len() > available {
                inventory.truncate(available);
                truncated = true;
            }
            emitted += inventory.len();
            let views = inventory
                .into_iter()
                .map(|view| {
                    let foreign_toplevel_identifier = self
                        .retained_window_for_view(view.id)
                        .and_then(|window| window.wl_surface().map(|surface| surface.into_owned()))
                        .and_then(|surface| self.foreign_toplevel_identifier(&surface));
                    OverviewViewSnapshot {
                        id: view.id.get(),
                        root: view.root.get(),
                        foreign_toplevel_identifier,
                        geometry: view.geometry.map(|geometry| OverviewGeometrySnapshot {
                            x: geometry.x,
                            y: geometry.y,
                            width: geometry.width,
                            height: geometry.height,
                        }),
                        focused: view.focused,
                        kind: match view.kind {
                            OverviewViewKind::Tiled => OverviewViewKindSnapshot::Tiled,
                            OverviewViewKind::Floating => OverviewViewKindSnapshot::Floating,
                            OverviewViewKind::Attached => OverviewViewKindSnapshot::Attached,
                        },
                        stacking_order: view.stacking_order,
                    }
                })
                .collect();
            workspaces.push(OverviewWorkspaceSnapshot {
                index: id.get(),
                name,
                hidden,
                minimize_target,
                view_count,
                views,
            });
        }
        OverviewSnapshot {
            active_workspace,
            truncated,
            workspaces,
        }
    }
}
