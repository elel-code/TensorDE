//! Value-only compositor and workspace snapshots for local IPC consumers.

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
}
