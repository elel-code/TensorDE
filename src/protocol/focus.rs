use std::{borrow::Cow, sync::Mutex};

use smithay::{
    backend::input::KeyState,
    input::{
        Seat,
        keyboard::{KeyboardTarget, KeysymHandle, ModifiersState},
    },
    utils::{IsAlive, Serial},
    wayland::seat::WaylandFocus,
};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

#[cfg(feature = "xwayland")]
use smithay::xwayland::X11Surface;

use super::{
    globals::compositor::{HookId, add_destruction_hook, remove_destruction_hook, with_states},
    state::{PopupKind, RuntimeState},
};

mod surface;
pub(crate) use surface::SurfaceFocusTarget;

/// Keyboard focus keeps X11's ICCCM focus handshake intact while retaining
/// normal Wayland surfaces as the common protocol target.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum KeyboardFocusTarget {
    Wayland(WlSurface),
    #[cfg(feature = "xwayland")]
    X11(Box<X11Surface>),
}

impl KeyboardFocusTarget {
    pub(crate) fn targets_surface(&self, surface: &WlSurface) -> bool {
        self.wl_surface()
            .is_some_and(|focused| focused.as_ref() == surface)
    }
}

impl From<WlSurface> for KeyboardFocusTarget {
    fn from(surface: WlSurface) -> Self {
        Self::Wayland(surface)
    }
}

impl From<PopupKind> for KeyboardFocusTarget {
    fn from(popup: PopupKind) -> Self {
        Self::Wayland(popup.wl_surface().clone())
    }
}

impl From<KeyboardFocusTarget> for WlSurface {
    fn from(target: KeyboardFocusTarget) -> Self {
        target
            .wl_surface()
            .map(Cow::into_owned)
            .expect("a mapped keyboard target must retain its Wayland surface")
    }
}

#[cfg(feature = "xwayland")]
impl From<X11Surface> for KeyboardFocusTarget {
    fn from(surface: X11Surface) -> Self {
        Self::X11(Box::new(surface))
    }
}

impl IsAlive for KeyboardFocusTarget {
    fn alive(&self) -> bool {
        match self {
            Self::Wayland(surface) => surface.is_alive(),
            #[cfg(feature = "xwayland")]
            Self::X11(surface) => surface.alive(),
        }
    }
}

impl KeyboardTarget<RuntimeState> for KeyboardFocusTarget {
    fn enter(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        keys: Vec<KeysymHandle<'_>>,
        serial: Serial,
    ) {
        match self {
            Self::Wayland(surface) => {
                state
                    .protocol_globals
                    .seat
                    .keyboard_enter(surface, &keys, serial);
                install_keyboard_focus_hook(surface);
            }
            #[cfg(feature = "xwayland")]
            Self::X11(surface) => surface.enter(seat, state, keys, serial),
        }
    }

    fn leave(&self, seat: &Seat<RuntimeState>, state: &mut RuntimeState, serial: Serial) {
        match self {
            Self::Wayland(surface) => {
                state.protocol_globals.seat.keyboard_leave(surface, serial);
                remove_keyboard_focus_hook(surface);
            }
            #[cfg(feature = "xwayland")]
            Self::X11(surface) => surface.leave(seat, state, serial),
        }
    }

    fn key(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        key: KeysymHandle<'_>,
        key_state: KeyState,
        serial: Serial,
        time: u32,
    ) {
        match self {
            Self::Wayland(_) => state
                .protocol_globals
                .seat
                .key(&key, key_state, serial, time),
            #[cfg(feature = "xwayland")]
            Self::X11(surface) => surface.key(seat, state, key, key_state, serial, time),
        }
    }

    fn modifiers(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        modifiers: ModifiersState,
        serial: Serial,
    ) {
        match self {
            Self::Wayland(_) => state.protocol_globals.seat.modifiers(modifiers, serial),
            #[cfg(feature = "xwayland")]
            Self::X11(surface) => surface.modifiers(seat, state, modifiers, serial),
        }
    }
}

#[derive(Debug, Default)]
struct KeyboardFocusHook(Mutex<Option<HookId>>);

fn install_keyboard_focus_hook(surface: &WlSurface) {
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

fn remove_keyboard_focus_hook(surface: &WlSurface) {
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

impl WaylandFocus for KeyboardFocusTarget {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        match self {
            Self::Wayland(surface) => Some(Cow::Borrowed(surface)),
            #[cfg(feature = "xwayland")]
            Self::X11(surface) => surface.wl_surface().map(Cow::Owned),
        }
    }
}
