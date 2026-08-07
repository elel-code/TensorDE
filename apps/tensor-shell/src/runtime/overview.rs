use wayland_client_runtime::{KeyState, KeyboardEvent, PointerEvent, PointerEventKind, SurfaceId};

use super::{BTN_LEFT, ShellRuntime, ShellRuntimeError};
use crate::ShellComponent;
use crate::overview_scene::{OverviewHit, OverviewInteraction, OverviewScene};

const DRAG_THRESHOLD_SQUARED: f64 = 64.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OverviewAction {
    ActivateView(u64),
    SetWorkspace(u32),
    MoveViewToWorkspace { view: u64, index: u32 },
    CloseView(u64),
}

impl ShellRuntime {
    pub(super) fn reconcile_overview_service(&mut self) -> Result<(), ShellRuntimeError> {
        let (revision, snapshot) = self.overview.read();
        if revision == self.overview_revision {
            return Ok(());
        }
        self.overview_revision = revision;
        self.overview_snapshot = snapshot;
        let surfaces = self
            .surface_keys
            .iter()
            .filter(|(_, key)| key.component == ShellComponent::Overview)
            .map(|(surface, _)| *surface)
            .collect::<Vec<_>>();
        for surface in surfaces {
            self.refresh_overview_scene(surface)?;
            if self.configured_surfaces.contains(&surface) {
                self.present_surface(surface)?;
            }
        }
        Ok(())
    }

    pub(super) fn refresh_overview_scene(
        &mut self,
        surface: SurfaceId,
    ) -> Result<(), ShellRuntimeError> {
        let is_overview = self
            .surface_keys
            .get(&surface)
            .is_some_and(|key| key.component == ShellComponent::Overview);
        if !is_overview {
            self.overview_scenes.remove(&surface);
            self.overview_input.remove(&surface);
            return Ok(());
        }
        let extent = self
            .wayland
            .logical_size(surface)
            .ok_or(ShellRuntimeError::MissingLogicalExtent(surface))?;
        self.overview_scenes.insert(
            surface,
            OverviewScene::build(extent, &self.overview_snapshot),
        );
        self.overview_input.entry(surface).or_default();
        Ok(())
    }

    pub(super) fn handle_overview_pointer(
        &mut self,
        event: &PointerEvent,
    ) -> Result<(), ShellRuntimeError> {
        let Some(scene) = self.overview_scenes.get(&event.surface) else {
            return Ok(());
        };
        let hit = scene.hit_test(event.position);
        let state = self.overview_input.entry(event.surface).or_default();
        let previous = *state;
        let action = match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                state.hovered = hit;
                if let (Some(OverviewHit::View(_)), Some(origin)) =
                    (state.pressed, state.press_position)
                    && !state.dragging
                    && moved_far_enough(origin, event.position)
                {
                    state.dragging = true;
                }
                state.drop_workspace = match (state.pressed, state.dragging) {
                    (Some(OverviewHit::View(view)), true) => {
                        scene.drop_workspace(view, event.position)
                    }
                    _ => None,
                };
                None
            }
            PointerEventKind::Leave => {
                *state = OverviewInteraction::default();
                None
            }
            PointerEventKind::Press {
                button: BTN_LEFT, ..
            } => {
                state.hovered = hit;
                state.pressed = hit;
                state.press_position = hit.map(|_| event.position);
                state.dragging = false;
                state.drop_workspace = None;
                None
            }
            PointerEventKind::Release {
                button: BTN_LEFT, ..
            } => {
                state.hovered = hit;
                let pressed = state.pressed.take();
                let dragging = state.dragging;
                let drop_workspace = state.drop_workspace.take();
                state.press_position = None;
                state.dragging = false;
                if dragging {
                    match (pressed, drop_workspace) {
                        (Some(OverviewHit::View(view)), Some(index)) => {
                            Some(OverviewAction::MoveViewToWorkspace { view, index })
                        }
                        _ => None,
                    }
                } else {
                    click_action(pressed, hit)
                }
            }
            PointerEventKind::Press { .. }
            | PointerEventKind::Release { .. }
            | PointerEventKind::Axis { .. } => None,
        };
        if let Some(action) = action {
            self.execute_overview_action(event.surface, action)?;
        } else if previous != *state {
            self.present_surface(event.surface)?;
        }
        Ok(())
    }

    pub(super) fn handle_overview_keyboard(
        &mut self,
        event: &KeyboardEvent,
    ) -> Result<(), ShellRuntimeError> {
        let KeyboardEvent::Key {
            surface,
            state: KeyState::Pressed,
            keysym: xkeysym::key::Escape,
            ..
        } = event
        else {
            return Ok(());
        };
        let Some(key) = self.surface_keys.get(surface).copied() else {
            return Ok(());
        };
        if key.component == ShellComponent::Overview {
            self.model
                .set_visible(key.output, ShellComponent::Overview, false);
            self.present_configured_panels()?;
        }
        Ok(())
    }

    fn execute_overview_action(
        &mut self,
        surface: SurfaceId,
        action: OverviewAction,
    ) -> Result<(), ShellRuntimeError> {
        let dismiss = match action {
            OverviewAction::ActivateView(view) => {
                self.overview
                    .activate_view(view)
                    .map_err(|error| ShellRuntimeError::OverviewCommand(error.to_string()))?;
                true
            }
            OverviewAction::SetWorkspace(index) => {
                self.overview
                    .set_workspace(index)
                    .map_err(|error| ShellRuntimeError::OverviewCommand(error.to_string()))?;
                true
            }
            OverviewAction::MoveViewToWorkspace { view, index } => {
                self.overview
                    .move_view_to_workspace(view, index)
                    .map_err(|error| ShellRuntimeError::OverviewCommand(error.to_string()))?;
                false
            }
            OverviewAction::CloseView(view) => {
                self.overview
                    .close_view(view)
                    .map_err(|error| ShellRuntimeError::OverviewCommand(error.to_string()))?;
                false
            }
        };
        if dismiss && let Some(key) = self.surface_keys.get(&surface).copied() {
            self.model
                .set_visible(key.output, ShellComponent::Overview, false);
            self.present_configured_panels()?;
        }
        Ok(())
    }
}

fn moved_far_enough(origin: (f64, f64), current: (f64, f64)) -> bool {
    if !origin.0.is_finite()
        || !origin.1.is_finite()
        || !current.0.is_finite()
        || !current.1.is_finite()
    {
        return false;
    }
    let dx = current.0 - origin.0;
    let dy = current.1 - origin.1;
    dx.mul_add(dx, dy * dy) >= DRAG_THRESHOLD_SQUARED
}

fn click_action(
    pressed: Option<OverviewHit>,
    released: Option<OverviewHit>,
) -> Option<OverviewAction> {
    if pressed != released {
        return None;
    }
    match pressed? {
        OverviewHit::View(view) => Some(OverviewAction::ActivateView(view)),
        OverviewHit::CloseView(view) => Some(OverviewAction::CloseView(view)),
        OverviewHit::Workspace(index) => Some(OverviewAction::SetWorkspace(index)),
    }
}

#[cfg(test)]
mod tests {
    use super::{OverviewAction, click_action, moved_far_enough};
    use crate::overview_scene::OverviewHit;

    #[test]
    fn escape_keysym_is_the_only_modal_close_shortcut() {
        assert_ne!(xkeysym::key::Escape, xkeysym::key::Return);
    }

    #[test]
    fn overview_clicks_and_drag_release_lower_to_distinct_actions() {
        assert_eq!(
            click_action(Some(OverviewHit::View(7)), Some(OverviewHit::View(7))),
            Some(OverviewAction::ActivateView(7))
        );
        assert_eq!(
            click_action(
                Some(OverviewHit::CloseView(7)),
                Some(OverviewHit::CloseView(7))
            ),
            Some(OverviewAction::CloseView(7))
        );
        assert_eq!(
            click_action(Some(OverviewHit::View(7)), Some(OverviewHit::Workspace(2))),
            None
        );
        assert!(moved_far_enough((0.0, 0.0), (8.0, 0.0)));
        assert!(!moved_far_enough((0.0, 0.0), (7.0, 0.0)));
    }
}
