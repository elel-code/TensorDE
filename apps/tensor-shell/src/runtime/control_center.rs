use wayland_client_runtime::{KeyState, KeyboardEvent, PointerEvent, PointerEventKind, SurfaceId};

use tensor_dbus::freedesktop::mpris::MprisAction;

use super::{BTN_LEFT, ShellRuntime, ShellRuntimeError};
use crate::ShellComponent;
use crate::control_center_scene::{
    ControlCenterHit, ControlCenterInteraction, ControlCenterScene, ControlCenterSnapshot,
};
use crate::session_lock_service::SessionAction;

impl ShellRuntime {
    pub(super) fn reconcile_control_center_scenes(&mut self) -> Result<(), ShellRuntimeError> {
        let (power_revision, power) = self.power.read();
        let (action_revision, action) = self.session_lock.action_read();
        let (notification_revision, do_not_disturb) = self.notification_state()?;
        let media_revision = self.media_revision;
        let media = self.media_snapshot.clone();
        let media_action = self.media_action;
        let (network_revision, network, network_action) = self.network.read();
        let revisions = (
            power_revision,
            action_revision,
            notification_revision,
            media_revision,
            network_revision,
        );
        if revisions == self.control_center_revisions {
            return Ok(());
        }
        self.control_center_revisions = revisions;
        let surfaces = self
            .surface_keys
            .iter()
            .filter(|(_, key)| key.component == ShellComponent::ControlCenter)
            .map(|(surface, _)| *surface)
            .collect::<Vec<_>>();
        for surface in surfaces {
            self.build_control_center_scene(
                surface,
                ControlCenterSnapshot {
                    power: &power,
                    media: &media,
                    media_action,
                    network: &network,
                    network_action,
                    do_not_disturb,
                    session_action: action,
                },
            )?;
            if self.configured_surfaces.contains(&surface) {
                self.present_surface(surface)?;
            }
        }
        Ok(())
    }

    pub(super) fn refresh_control_center_scene(
        &mut self,
        surface: SurfaceId,
    ) -> Result<(), ShellRuntimeError> {
        let is_control_center = self
            .surface_keys
            .get(&surface)
            .is_some_and(|key| key.component == ShellComponent::ControlCenter);
        if !is_control_center {
            self.control_center_scenes.remove(&surface);
            self.control_center_input.remove(&surface);
            return Ok(());
        }
        let (_, power) = self.power.read();
        let (_, action) = self.session_lock.action_read();
        let (_, do_not_disturb) = self.notification_state()?;
        let media = self.media_snapshot.clone();
        let media_action = self.media_action;
        let (_, network, network_action) = self.network.read();
        self.build_control_center_scene(
            surface,
            ControlCenterSnapshot {
                power: &power,
                media: &media,
                media_action,
                network: &network,
                network_action,
                do_not_disturb,
                session_action: action,
            },
        )
    }

    fn build_control_center_scene(
        &mut self,
        surface: SurfaceId,
        snapshot: ControlCenterSnapshot<'_>,
    ) -> Result<(), ShellRuntimeError> {
        let extent = self
            .wayland
            .logical_size(surface)
            .ok_or(ShellRuntimeError::MissingLogicalExtent(surface))?;
        self.control_center_scenes
            .insert(surface, ControlCenterScene::build(extent, snapshot));
        let focused = self
            .control_center_input
            .get(&surface)
            .and_then(|state| state.focused);
        let focus_is_valid = focused.is_none_or(|focus| {
            self.control_center_scenes
                .get(&surface)
                .is_some_and(|scene| scene.has_focus(focus))
        });
        let state = self.control_center_input.entry(surface).or_default();
        if !focus_is_valid {
            state.focused = None;
        }
        Ok(())
    }

    fn notification_state(&self) -> Result<(u64, bool), ShellRuntimeError> {
        self.notifications
            .store()
            .lock()
            .map(|store| (store.revision(), store.do_not_disturb()))
            .map_err(|_| ShellRuntimeError::NotificationStorePoisoned)
    }

    pub(super) fn handle_control_center_pointer(
        &mut self,
        event: &PointerEvent,
    ) -> Result<(), ShellRuntimeError> {
        let Some(scene) = self.control_center_scenes.get(&event.surface) else {
            return Ok(());
        };
        let hit = scene.hit_test(event.position);
        let state = self.control_center_input.entry(event.surface).or_default();
        let previous = *state;
        let activation = match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                state.hovered = hit;
                None
            }
            PointerEventKind::Leave => {
                let focused = state.focused;
                *state = ControlCenterInteraction {
                    focused,
                    ..ControlCenterInteraction::default()
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
            self.activate_control_center_hit(event.surface, hit)?;
        } else if previous != *state {
            self.present_surface(event.surface)?;
        }
        Ok(())
    }

    pub(super) fn handle_control_center_keyboard(
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
        if key.component != ShellComponent::ControlCenter {
            return Ok(());
        }
        let Some(action) = control_center_keyboard_action(*keysym, *state) else {
            return Ok(());
        };
        if action == ControlCenterKeyboardAction::Close {
            self.model
                .set_visible(key.output, ShellComponent::ControlCenter, false);
            self.present_configured_panels()?;
            return Ok(());
        }
        match action {
            ControlCenterKeyboardAction::NavigateForward
            | ControlCenterKeyboardAction::NavigateBackward => {
                let forward = action == ControlCenterKeyboardAction::NavigateForward;
                let state = self.control_center_input.entry(*surface).or_default();
                let previous = state.focused;
                state.focused = self
                    .control_center_scenes
                    .get(surface)
                    .and_then(|scene| scene.navigate_focus(previous, forward));
                if state.focused != previous {
                    self.present_surface(*surface)?;
                }
            }
            ControlCenterKeyboardAction::Activate => {
                let first_focus = self
                    .control_center_scenes
                    .get(surface)
                    .and_then(ControlCenterScene::first_focus);
                let focused = self
                    .control_center_input
                    .get(surface)
                    .and_then(|state| state.focused)
                    .or(first_focus);
                let Some(focused) = focused else {
                    return Ok(());
                };
                self.control_center_input
                    .entry(*surface)
                    .or_default()
                    .focused = Some(focused);
                self.activate_control_center_hit(*surface, focused)?;
            }
            ControlCenterKeyboardAction::Close => unreachable!(),
        }
        Ok(())
    }

    fn activate_control_center_hit(
        &mut self,
        surface: SurfaceId,
        hit: ControlCenterHit,
    ) -> Result<(), ShellRuntimeError> {
        match hit {
            ControlCenterHit::Network => self.network.request_toggle()?,
            ControlCenterHit::Lock => self.session_lock.request_action(SessionAction::Lock)?,
            ControlCenterHit::Suspend => {
                self.session_lock.request_action(SessionAction::Suspend)?
            }
            ControlCenterHit::DoNotDisturb => {
                let now_ms = self.notifications.now_ms();
                {
                    let mut store = self
                        .notifications
                        .store()
                        .lock()
                        .map_err(|_| ShellRuntimeError::NotificationStorePoisoned)?;
                    let enabled = !store.do_not_disturb();
                    store.set_do_not_disturb(enabled, now_ms);
                }
                self.refresh_control_center_scene(surface)?;
                self.present_surface(surface)?;
                return Ok(());
            }
            ControlCenterHit::Previous => self.media.request(MprisAction::Previous)?,
            ControlCenterHit::PlayPause => self.media.request(MprisAction::PlayPause)?,
            ControlCenterHit::Next => self.media.request(MprisAction::Next)?,
        }
        if matches!(
            hit,
            ControlCenterHit::Network
                | ControlCenterHit::Previous
                | ControlCenterHit::PlayPause
                | ControlCenterHit::Next
        ) {
            self.present_surface(surface)?;
            return Ok(());
        }
        if let Some(key) = self.surface_keys.get(&surface).copied() {
            self.model
                .set_visible(key.output, ShellComponent::ControlCenter, false);
            self.present_configured_panels()?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlCenterKeyboardAction {
    Close,
    NavigateForward,
    NavigateBackward,
    Activate,
}

fn control_center_keyboard_action(
    keysym: u32,
    state: KeyState,
) -> Option<ControlCenterKeyboardAction> {
    match keysym {
        xkeysym::key::Escape if state == KeyState::Pressed => {
            Some(ControlCenterKeyboardAction::Close)
        }
        xkeysym::key::Tab | xkeysym::key::Down | xkeysym::key::Right
            if matches!(state, KeyState::Pressed | KeyState::Repeated) =>
        {
            Some(ControlCenterKeyboardAction::NavigateForward)
        }
        xkeysym::key::Up | xkeysym::key::Left
            if matches!(state, KeyState::Pressed | KeyState::Repeated) =>
        {
            Some(ControlCenterKeyboardAction::NavigateBackward)
        }
        xkeysym::key::Return | xkeysym::key::KP_Enter | xkeysym::key::space
            if state == KeyState::Pressed =>
        {
            Some(ControlCenterKeyboardAction::Activate)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{ControlCenterKeyboardAction, control_center_keyboard_action};
    use wayland_client_runtime::KeyState;

    #[test]
    fn control_center_navigation_repeats_but_activation_does_not() {
        assert_eq!(
            control_center_keyboard_action(xkeysym::key::Tab, KeyState::Repeated),
            Some(ControlCenterKeyboardAction::NavigateForward)
        );
        assert_eq!(
            control_center_keyboard_action(xkeysym::key::Up, KeyState::Pressed),
            Some(ControlCenterKeyboardAction::NavigateBackward)
        );
        assert_eq!(
            control_center_keyboard_action(xkeysym::key::space, KeyState::Pressed),
            Some(ControlCenterKeyboardAction::Activate)
        );
        assert_eq!(
            control_center_keyboard_action(xkeysym::key::Return, KeyState::Repeated),
            None
        );
    }

    #[test]
    fn escape_is_a_press_only_close_action() {
        assert_eq!(
            control_center_keyboard_action(xkeysym::key::Escape, KeyState::Pressed),
            Some(ControlCenterKeyboardAction::Close)
        );
        assert_eq!(
            control_center_keyboard_action(xkeysym::key::Escape, KeyState::Released),
            None
        );
    }
}
