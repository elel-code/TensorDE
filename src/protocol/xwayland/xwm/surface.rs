use std::sync::{Arc, Mutex};

use smithay::utils::{IsAlive, Logical, Rectangle, Size};
use wayland_server::protocol::wl_surface::WlSurface;
use x11rb::{
    CURRENT_TIME,
    connection::Connection,
    properties::WmSizeHints,
    protocol::xproto::{
        Atom, AtomEnum, ConfigureWindowAux, ConnectionExt as _, InputFocus, PropMode,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

use super::{Atoms, WmWindowProperty};

#[derive(Debug)]
struct SurfaceState {
    geometry: Rectangle<i32, Logical>,
    wl_surface: Option<WlSurface>,
    transient_for: Option<u32>,
    min_size: Option<Size<i32, Logical>>,
    max_size: Option<Size<i32, Logical>>,
    override_redirect: bool,
    mapped: bool,
    activated: bool,
    net_state: Vec<Atom>,
    alive: bool,
}

#[derive(Clone)]
pub(crate) struct X11Surface {
    connection: Arc<RustConnection>,
    window: u32,
    state: Arc<Mutex<SurfaceState>>,
    net_wm_state: Atom,
    net_wm_state_focused: Atom,
}

impl PartialEq for X11Surface {
    fn eq(&self, other: &Self) -> bool {
        self.window == other.window && Arc::ptr_eq(&self.connection, &other.connection)
    }
}

impl Eq for X11Surface {}

impl std::fmt::Debug for X11Surface {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("X11Surface")
            .field("window", &self.window)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl X11Surface {
    pub(super) fn new(
        connection: Arc<RustConnection>,
        window: u32,
        override_redirect: bool,
        geometry: Rectangle<i32, Logical>,
        net_wm_state: Atom,
        net_wm_state_focused: Atom,
    ) -> Self {
        Self {
            connection,
            window,
            state: Arc::new(Mutex::new(SurfaceState {
                geometry,
                wl_surface: None,
                transient_for: None,
                min_size: None,
                max_size: None,
                override_redirect,
                mapped: false,
                activated: false,
                net_state: Vec::new(),
                alive: true,
            })),
            net_wm_state,
            net_wm_state_focused,
        }
    }

    pub(crate) const fn window_id(&self) -> u32 {
        self.window
    }

    pub(crate) fn alive(&self) -> bool {
        self.state.lock().unwrap().alive
    }

    pub(crate) fn is_override_redirect(&self) -> bool {
        self.state.lock().unwrap().override_redirect
    }

    pub(crate) fn geometry(&self) -> Rectangle<i32, Logical> {
        Rectangle::from_size(self.state.lock().unwrap().geometry.size)
    }

    pub(crate) fn bbox(&self) -> Rectangle<i32, Logical> {
        self.geometry()
    }

    pub(crate) fn last_configure(&self) -> Rectangle<i32, Logical> {
        self.state.lock().unwrap().geometry
    }

    pub(crate) fn update_geometry(&self, geometry: Rectangle<i32, Logical>) {
        self.state.lock().unwrap().geometry = geometry;
    }

    pub(crate) fn wl_surface(&self) -> Option<WlSurface> {
        self.state.lock().unwrap().wl_surface.clone()
    }

    pub(crate) fn set_wl_surface(&self, surface: Option<WlSurface>) {
        self.state.lock().unwrap().wl_surface = surface;
    }

    pub(crate) fn is_transient_for(&self) -> Option<u32> {
        self.state.lock().unwrap().transient_for
    }

    pub(crate) fn min_size(&self) -> Option<Size<i32, Logical>> {
        self.state.lock().unwrap().min_size
    }

    pub(crate) fn max_size(&self) -> Option<Size<i32, Logical>> {
        self.state.lock().unwrap().max_size
    }

    pub(crate) fn is_activated(&self) -> bool {
        self.state.lock().unwrap().activated
    }

    pub(crate) fn set_activated(&self, activated: bool) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = self.state.lock().unwrap();
        if state.activated == activated {
            return Ok(());
        }
        state.activated = activated;
        state
            .net_state
            .retain(|atom| *atom != self.net_wm_state_focused);
        if activated {
            state.net_state.push(self.net_wm_state_focused);
        }
        let net_state = state.net_state.clone();
        drop(state);
        self.connection.change_property32(
            PropMode::REPLACE,
            self.window,
            self.net_wm_state,
            AtomEnum::ATOM,
            &net_state,
        )?;
        if activated {
            self.connection
                .set_input_focus(InputFocus::PARENT, self.window, CURRENT_TIME)?;
        }
        self.connection.flush()?;
        Ok(())
    }

    pub(crate) fn set_mapped(&self, mapped: bool) -> Result<(), Box<dyn std::error::Error>> {
        if mapped {
            self.connection.map_window(self.window)?;
        } else {
            self.connection.unmap_window(self.window)?;
        }
        self.mark_mapped(mapped);
        self.connection.flush()?;
        Ok(())
    }

    pub(crate) fn configure(
        &self,
        geometry: Option<Rectangle<i32, Logical>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let geometry = geometry.unwrap_or_else(|| self.last_configure());
        let width = u32::try_from(geometry.size.w.max(1)).unwrap_or(u32::MAX);
        let height = u32::try_from(geometry.size.h.max(1)).unwrap_or(u32::MAX);
        self.connection.configure_window(
            self.window,
            &ConfigureWindowAux::new()
                .x(geometry.loc.x)
                .y(geometry.loc.y)
                .width(width)
                .height(height)
                .border_width(0),
        )?;
        self.update_geometry(geometry);
        self.connection.flush()?;
        Ok(())
    }

    pub(super) fn mark_mapped(&self, mapped: bool) {
        self.state.lock().unwrap().mapped = mapped;
    }

    pub(super) fn mark_destroyed(&self) {
        let mut state = self.state.lock().unwrap();
        state.alive = false;
        state.mapped = false;
        state.wl_surface = None;
    }

    pub(super) fn refresh_properties(
        &self,
        atoms: &Atoms,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.refresh_transient_for()?;
        self.refresh_normal_hints()?;
        self.refresh_net_state(atoms)?;
        Ok(())
    }

    pub(super) fn refresh_property(
        &self,
        atom: Atom,
        atoms: &Atoms,
    ) -> Result<Option<WmWindowProperty>, Box<dyn std::error::Error>> {
        if atom == u32::from(AtomEnum::WM_TRANSIENT_FOR) {
            self.refresh_transient_for()?;
            Ok(Some(WmWindowProperty::TransientFor))
        } else if atom == u32::from(AtomEnum::WM_NORMAL_HINTS) {
            self.refresh_normal_hints()?;
            Ok(Some(WmWindowProperty::NormalHints))
        } else if atom == atoms._NET_WM_STATE {
            self.refresh_net_state(atoms)?;
            Ok(None)
        } else {
            Ok(None)
        }
    }

    fn refresh_transient_for(&self) -> Result<(), Box<dyn std::error::Error>> {
        let property = self
            .connection
            .get_property(
                false,
                self.window,
                AtomEnum::WM_TRANSIENT_FOR,
                AtomEnum::WINDOW,
                0,
                1,
            )?
            .reply()?;
        self.state.lock().unwrap().transient_for =
            property.value32().and_then(|mut values| values.next());
        Ok(())
    }

    fn refresh_normal_hints(&self) -> Result<(), Box<dyn std::error::Error>> {
        let hints = WmSizeHints::get_normal_hints(&*self.connection, self.window)?.reply()?;
        let (min_size, max_size) = hints.map_or((None, None), |hints| {
            (
                hints.min_size.map(|(w, h)| (w, h).into()),
                hints.max_size.map(|(w, h)| (w, h).into()),
            )
        });
        let mut state = self.state.lock().unwrap();
        state.min_size = min_size;
        state.max_size = max_size;
        Ok(())
    }

    fn refresh_net_state(&self, atoms: &Atoms) -> Result<(), Box<dyn std::error::Error>> {
        let property = self
            .connection
            .get_property(
                false,
                self.window,
                atoms._NET_WM_STATE,
                AtomEnum::ATOM,
                0,
                u32::MAX,
            )?
            .reply()?;
        let net_state: Vec<Atom> = property
            .value32()
            .map(Iterator::collect)
            .unwrap_or_default();
        let mut state = self.state.lock().unwrap();
        state.activated = net_state.contains(&atoms._NET_WM_STATE_FOCUSED);
        state.net_state = net_state;
        Ok(())
    }
}

impl IsAlive for X11Surface {
    fn alive(&self) -> bool {
        X11Surface::alive(self)
    }
}
