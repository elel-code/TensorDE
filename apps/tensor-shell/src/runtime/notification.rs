use wayland_client_runtime::{KeyState, KeyboardEvent, PointerEvent, PointerEventKind, SurfaceId};

use super::{BTN_LEFT, ShellRuntime, ShellRuntimeError};
use crate::notification_scene::{
    NotificationHit, NotificationInteraction, NotificationScene, NotificationSceneKind,
};
use crate::{CloseReason, NotificationId, ShellComponent};

impl ShellRuntime {
    pub(super) fn reconcile_notification_scenes(&mut self) -> Result<(), ShellRuntimeError> {
        let revision = self
            .notifications
            .store()
            .lock()
            .map(|store| store.revision())
            .map_err(|_| ShellRuntimeError::NotificationStorePoisoned)?;
        if revision == self.notification_revision {
            return Ok(());
        }
        self.notification_revision = revision;
        let surfaces = self
            .surface_keys
            .iter()
            .filter(|(_, key)| {
                matches!(
                    key.component,
                    ShellComponent::NotificationCenter | ShellComponent::NotificationPopups
                )
            })
            .map(|(surface, _)| *surface)
            .collect::<Vec<_>>();
        for surface in surfaces {
            self.refresh_notification_scene(surface)?;
            if self.configured_surfaces.contains(&surface) {
                self.present_surface(surface)?;
            }
        }
        Ok(())
    }

    pub(super) fn refresh_notification_scene(
        &mut self,
        surface: SurfaceId,
    ) -> Result<(), ShellRuntimeError> {
        let Some(key) = self.surface_keys.get(&surface).copied() else {
            self.notification_scenes.remove(&surface);
            self.notification_input.remove(&surface);
            return Ok(());
        };
        let kind = match key.component {
            ShellComponent::NotificationCenter => NotificationSceneKind::Center,
            ShellComponent::NotificationPopups => NotificationSceneKind::Popups,
            _ => {
                self.notification_scenes.remove(&surface);
                self.notification_input.remove(&surface);
                return Ok(());
            }
        };
        let extent = self
            .wayland
            .logical_size(surface)
            .ok_or(ShellRuntimeError::MissingLogicalExtent(surface))?;
        let scene = {
            let store = self
                .notifications
                .store()
                .lock()
                .map_err(|_| ShellRuntimeError::NotificationStorePoisoned)?;
            NotificationScene::from_store(extent, &store, kind)
        };
        self.notification_scenes.insert(surface, scene);
        let focused = self
            .notification_input
            .get(&surface)
            .and_then(|state| state.focused);
        let focus_is_valid = focused.is_none_or(|focus| {
            self.notification_scenes
                .get(&surface)
                .is_some_and(|scene| scene.has_focus(focus))
        });
        let state = self.notification_input.entry(surface).or_default();
        if !focus_is_valid {
            state.focused = None;
        }
        Ok(())
    }

    pub(super) fn handle_notification_pointer(
        &mut self,
        event: &PointerEvent,
    ) -> Result<(), ShellRuntimeError> {
        let Some(scene) = self.notification_scenes.get(&event.surface) else {
            return Ok(());
        };
        let hit = scene.hit_test(event.position);
        let state = self.notification_input.entry(event.surface).or_default();
        let previous = *state;
        let activation = match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                state.hovered = hit;
                None
            }
            PointerEventKind::Leave => {
                let focused = state.focused;
                *state = NotificationInteraction {
                    focused,
                    ..NotificationInteraction::default()
                };
                None
            }
            PointerEventKind::Press {
                button: BTN_LEFT, ..
            } => {
                state.hovered = hit;
                state.pressed = hit;
                state.focused = hit;
                None
            }
            PointerEventKind::Release {
                button: BTN_LEFT, ..
            } => {
                state.hovered = hit;
                let pressed = state.pressed.take();
                (pressed == hit).then_some(hit).flatten()
            }
            PointerEventKind::Press { .. }
            | PointerEventKind::Release { .. }
            | PointerEventKind::Axis { .. } => None,
        };
        if let Some(hit) = activation {
            self.activate_notification_hit(event.surface, hit)?;
            self.refresh_notification_scene(event.surface)?;
            self.present_surface(event.surface)?;
        } else if previous != *state {
            self.present_surface(event.surface)?;
        }
        Ok(())
    }

    pub(super) fn handle_notification_keyboard(
        &mut self,
        event: &KeyboardEvent,
    ) -> Result<(), ShellRuntimeError> {
        let KeyboardEvent::Key {
            surface,
            state,
            keysym,
            ..
        } = event
        else {
            return Ok(());
        };
        let Some(key) = self.surface_keys.get(surface).copied() else {
            return Ok(());
        };
        if key.component != ShellComponent::NotificationCenter {
            return Ok(());
        }
        let Some(action) = notification_keyboard_action(*keysym, *state) else {
            return Ok(());
        };
        if action == NotificationKeyboardAction::CloseCenter {
            self.model
                .set_visible(key.output, ShellComponent::NotificationCenter, false);
            self.present_configured_panels()?;
            return Ok(());
        }
        match action {
            NotificationKeyboardAction::NavigateForward
            | NotificationKeyboardAction::NavigateBackward => {
                let forward = action == NotificationKeyboardAction::NavigateForward;
                let state = self.notification_input.entry(*surface).or_default();
                let previous = state.focused;
                state.focused = self
                    .notification_scenes
                    .get(surface)
                    .and_then(|scene| scene.navigate_focus(previous, forward));
                if state.focused != previous {
                    self.present_surface(*surface)?;
                }
            }
            NotificationKeyboardAction::Activate => {
                let first_focus = self
                    .notification_scenes
                    .get(surface)
                    .and_then(NotificationScene::first_focus);
                let focused = self
                    .notification_input
                    .get(surface)
                    .and_then(|state| state.focused)
                    .or(first_focus);
                let Some(focused) = focused else {
                    return Ok(());
                };
                self.notification_input.entry(*surface).or_default().focused = Some(focused);
                self.activate_notification_hit(*surface, focused)?;
                self.refresh_notification_scene(*surface)?;
                self.present_surface(*surface)?;
            }
            NotificationKeyboardAction::Dismiss => {
                let Some(focused) = self
                    .notification_input
                    .get(surface)
                    .and_then(|state| state.focused)
                else {
                    return Ok(());
                };
                self.activate_notification_hit(
                    *surface,
                    NotificationHit::Dismiss(notification_hit_id(focused)),
                )?;
                self.refresh_notification_scene(*surface)?;
                self.present_surface(*surface)?;
            }
            NotificationKeyboardAction::CloseCenter => unreachable!(),
        }
        Ok(())
    }

    fn activate_notification_hit(
        &mut self,
        surface: SurfaceId,
        hit: NotificationHit,
    ) -> Result<(), ShellRuntimeError> {
        let Some(key) = self.surface_keys.get(&surface).copied() else {
            return Ok(());
        };
        match hit {
            NotificationHit::Dismiss(id) => {
                let closed = {
                    let mut store = self
                        .notifications
                        .store()
                        .lock()
                        .map_err(|_| ShellRuntimeError::NotificationStorePoisoned)?;
                    if key.component == ShellComponent::NotificationCenter {
                        store.dismiss_history(id);
                    }
                    store.close(
                        id,
                        CloseReason::DismissedByUser,
                        self.notifications.now_ms(),
                    )
                };
                if let Some(closed) = closed {
                    self.notifications.emit_closed(closed)?;
                }
            }
            NotificationHit::Action(id) => {
                let action_key = self
                    .notification_scenes
                    .get(&surface)
                    .and_then(|scene| scene.action_key(id))
                    .map(str::to_owned);
                if let Some(action_key) = action_key {
                    self.notifications.emit_action(id, action_key)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NotificationKeyboardAction {
    CloseCenter,
    NavigateForward,
    NavigateBackward,
    Activate,
    Dismiss,
}

fn notification_keyboard_action(
    keysym: u32,
    state: KeyState,
) -> Option<NotificationKeyboardAction> {
    match keysym {
        xkeysym::key::Escape if state == KeyState::Pressed => {
            Some(NotificationKeyboardAction::CloseCenter)
        }
        xkeysym::key::Tab | xkeysym::key::Down | xkeysym::key::Right
            if matches!(state, KeyState::Pressed | KeyState::Repeated) =>
        {
            Some(NotificationKeyboardAction::NavigateForward)
        }
        xkeysym::key::Up | xkeysym::key::Left
            if matches!(state, KeyState::Pressed | KeyState::Repeated) =>
        {
            Some(NotificationKeyboardAction::NavigateBackward)
        }
        xkeysym::key::Return | xkeysym::key::KP_Enter if state == KeyState::Pressed => {
            Some(NotificationKeyboardAction::Activate)
        }
        xkeysym::key::Delete | xkeysym::key::BackSpace if state == KeyState::Pressed => {
            Some(NotificationKeyboardAction::Dismiss)
        }
        _ => None,
    }
}

fn notification_hit_id(hit: NotificationHit) -> NotificationId {
    match hit {
        NotificationHit::Dismiss(id) | NotificationHit::Action(id) => id,
    }
}

#[cfg(test)]
mod tests {
    use super::{NotificationKeyboardAction, notification_keyboard_action};
    use wayland_client_runtime::KeyState;

    #[test]
    fn navigation_accepts_press_and_repeat_but_activation_does_not_repeat() {
        assert_eq!(
            notification_keyboard_action(xkeysym::key::Tab, KeyState::Pressed),
            Some(NotificationKeyboardAction::NavigateForward)
        );
        assert_eq!(
            notification_keyboard_action(xkeysym::key::Right, KeyState::Repeated),
            Some(NotificationKeyboardAction::NavigateForward)
        );
        assert_eq!(
            notification_keyboard_action(xkeysym::key::Left, KeyState::Repeated),
            Some(NotificationKeyboardAction::NavigateBackward)
        );
        assert_eq!(
            notification_keyboard_action(xkeysym::key::Return, KeyState::Repeated),
            None
        );
    }

    #[test]
    fn notification_shortcuts_map_to_modal_actions() {
        assert_eq!(
            notification_keyboard_action(xkeysym::key::Escape, KeyState::Pressed),
            Some(NotificationKeyboardAction::CloseCenter)
        );
        assert_eq!(
            notification_keyboard_action(xkeysym::key::KP_Enter, KeyState::Pressed),
            Some(NotificationKeyboardAction::Activate)
        );
        assert_eq!(
            notification_keyboard_action(xkeysym::key::BackSpace, KeyState::Pressed),
            Some(NotificationKeyboardAction::Dismiss)
        );
        assert_eq!(
            notification_keyboard_action(xkeysym::key::Delete, KeyState::Released),
            None
        );
    }
}
