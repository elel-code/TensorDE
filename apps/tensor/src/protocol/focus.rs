use std::sync::Mutex;

use wayland_server::protocol::wl_surface::WlSurface;

use super::{
    globals::compositor::{HookId, add_destruction_hook, remove_destruction_hook, with_states},
    state::RuntimeState,
};

#[derive(Debug, Default)]
struct KeyboardFocusHook(Mutex<Option<HookId>>);

pub(crate) fn install_keyboard_focus_hook(surface: &WlSurface) {
    let hook = add_destruction_hook::<RuntimeState, _>(surface, |state, surface| {
        state.clear_keyboard_focus_for_surface(surface);
    });
    let old = with_states(surface, |states| {
        states
            .data_map
            .get_or_insert(KeyboardFocusHook::default)
            .0
            .lock()
            .unwrap()
            .replace(hook)
    });
    if let Some(old) = old {
        remove_destruction_hook(surface, &old);
    }
}

pub(crate) fn remove_keyboard_focus_hook(surface: &WlSurface) {
    let hook = with_states(surface, |states| {
        states
            .data_map
            .get::<KeyboardFocusHook>()
            .and_then(|hook| hook.0.lock().unwrap().take())
    });
    if let Some(hook) = hook {
        remove_destruction_hook(surface, &hook);
    }
}
