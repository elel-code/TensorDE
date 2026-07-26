use std::borrow::Cow;

use smithay::{
    backend::input::KeyState,
    input::{
        Seat,
        keyboard::{KeyboardTarget, KeysymHandle, ModifiersState},
    },
    utils::{IsAlive, Serial},
    wayland::seat::WaylandFocus,
};
use wayland_server::protocol::wl_surface::WlSurface;

#[cfg(feature = "xwayland")]
use smithay::xwayland::X11Surface;

use super::state::{PopupKind, RuntimeState};

/// Keyboard focus keeps X11's ICCCM focus handshake intact while retaining
/// normal Wayland surfaces as the common protocol target.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum KeyboardFocusTarget {
    Wayland(WlSurface),
    #[cfg(feature = "xwayland")]
    X11(Box<X11Surface>),
}

impl KeyboardFocusTarget {
    #[cfg(feature = "tty")]
    pub(crate) fn targets_surface(&self, surface: &WlSurface) -> bool {
        self.wl_surface()
            .is_some_and(|focused| focused.as_ref() == surface)
    }

    fn target(&self) -> &dyn KeyboardTarget<RuntimeState> {
        match self {
            Self::Wayland(surface) => surface,
            #[cfg(feature = "xwayland")]
            Self::X11(surface) => surface.as_ref(),
        }
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
            Self::Wayland(surface) => surface.alive(),
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
        self.target().enter(seat, state, keys, serial);
    }

    fn leave(&self, seat: &Seat<RuntimeState>, state: &mut RuntimeState, serial: Serial) {
        self.target().leave(seat, state, serial);
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
        self.target().key(seat, state, key, key_state, serial, time);
    }

    fn modifiers(
        &self,
        seat: &Seat<RuntimeState>,
        state: &mut RuntimeState,
        modifiers: ModifiersState,
        serial: Serial,
    ) {
        self.target().modifiers(seat, state, modifiers, serial);
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
