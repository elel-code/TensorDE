use crate::ecs::{MinimizedFrom, ViewId, WorkspaceId};

use super::{CompositorWorld, ViewLifecycleError, Workspace};

impl CompositorWorld {
    pub fn minimize_view(
        &mut self,
        view_id: ViewId,
        hidden_workspace: WorkspaceId,
    ) -> Result<bool, ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        if self.world.get::<MinimizedFrom>(entity).is_some() {
            return Ok(false);
        }
        let origin = self
            .world
            .get::<Workspace>(entity)
            .expect("every view has a workspace")
            .id;
        if origin == hidden_workspace {
            return Ok(false);
        }
        self.move_view(view_id, hidden_workspace)?;
        let entity = self.entity_for(view_id)?;
        self.world.entity_mut(entity).insert(MinimizedFrom(origin));
        Ok(true)
    }

    pub fn restore_minimized_view(
        &mut self,
        view_id: ViewId,
    ) -> Result<Option<WorkspaceId>, ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        let Some(origin) = self.world.get::<MinimizedFrom>(entity).copied() else {
            return Ok(None);
        };
        self.move_view(view_id, origin.0)?;
        Ok(Some(origin.0))
    }

    pub fn minimized_from(&self, view_id: ViewId) -> Option<WorkspaceId> {
        let entity = self.view_entities.get(&view_id).copied()?;
        self.world.get::<MinimizedFrom>(entity).map(|state| state.0)
    }

    /// Number of minimized root families, independent of attached dialogs or
    /// other views stored on the same hidden workspace.
    pub fn minimized_count(&self) -> usize {
        self.view_entities
            .values()
            .filter(|entity| self.world.get::<MinimizedFrom>(**entity).is_some())
            .count()
    }
}
