//! Tensor-owned staging xwayland-shell-v1 implementation.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use wayland_protocols::xwayland::shell::v1::server::{
    xwayland_shell_v1::{self, XwaylandShellV1},
    xwayland_surface_v1::{self, XwaylandSurfaceV1},
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, backend::GlobalId,
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    globals::compositor::{
        Cacheable, add_destruction_hook, add_pre_commit_hook, give_role, with_states,
    },
    state::RuntimeState,
    xwayland::XWaylandClientData,
};

const VERSION: u32 = 1;
const ROLE: &str = "xwayland_surface_v1";

#[derive(Debug)]
pub(crate) struct XWaylandShellState {
    _global: GlobalId,
    surfaces_by_serial: HashMap<u64, WlSurface>,
}

impl XWaylandShellState {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display
                .create_global::<RuntimeState, XwaylandShellV1, _>(VERSION, XWaylandGlobalData),
            surfaces_by_serial: HashMap::new(),
        }
    }

    fn take_surface(&mut self, serial: u64) -> Option<WlSurface> {
        self.surfaces_by_serial.remove(&serial)
    }

    fn remember_surface(&mut self, serial: u64, surface: WlSurface) {
        self.surfaces_by_serial.insert(serial, surface);
    }

    fn remove_surface(&mut self, surface: &WlSurface) {
        let id = surface.id();
        self.surfaces_by_serial
            .retain(|_, candidate| candidate.id() != id);
    }
}

#[derive(Clone, Copy, Debug)]
struct XWaylandGlobalData;

#[derive(Clone, Copy, Debug)]
struct XWaylandShellData;

#[derive(Debug)]
struct XWaylandSurfaceData {
    surface: WlSurface,
    associated: Arc<AtomicBool>,
}

#[derive(Clone, Copy, Debug, Default)]
struct XWaylandSurfaceCachedState {
    serial: Option<u64>,
}

impl Cacheable for XWaylandSurfaceCachedState {
    fn commit(&mut self, _display: &DisplayHandle) -> Self {
        std::mem::take(self)
    }

    fn merge_into(self, current: &mut Self, _display: &DisplayHandle) {
        *current = self;
    }
}

impl GlobalDispatch<XwaylandShellV1, XWaylandGlobalData> for RuntimeState {
    fn bind(
        _state: &mut Self,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<XwaylandShellV1>,
        _data: &XWaylandGlobalData,
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, XWaylandShellData);
    }

    fn can_view(client: Client, _data: &XWaylandGlobalData) -> bool {
        client.get_data::<XWaylandClientData>().is_some()
    }
}

impl Dispatch<XwaylandShellV1, XWaylandShellData> for RuntimeState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        resource: &XwaylandShellV1,
        request: xwayland_shell_v1::Request,
        _data: &XWaylandShellData,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            xwayland_shell_v1::Request::Destroy => {}
            xwayland_shell_v1::Request::GetXwaylandSurface { id, surface } => {
                if give_role(&surface, ROLE).is_err() {
                    resource.post_error(
                        xwayland_shell_v1::Error::Role,
                        "wl_surface already has a role",
                    );
                    return;
                }
                let associated = Arc::new(AtomicBool::new(false));
                let object = data_init.init(
                    id,
                    XWaylandSurfaceData {
                        surface: surface.clone(),
                        associated: Arc::clone(&associated),
                    },
                );
                add_pre_commit_hook::<RuntimeState, _>(&surface, move |state, _, surface| {
                    let serial = with_states(surface, |states| {
                        states
                            .cached_state
                            .get::<XWaylandSurfaceCachedState>()
                            .pending()
                            .serial
                            .take()
                    });
                    let Some(serial) = serial else {
                        return;
                    };
                    if associated.swap(true, Ordering::AcqRel) {
                        object.post_error(
                            xwayland_surface_v1::Error::AlreadyAssociated,
                            "wl_surface was already associated with an X11 window",
                        );
                        return;
                    }
                    state.xwayland_surface_serial_committed(serial, surface.clone());
                });
                add_destruction_hook::<RuntimeState, _>(&surface, |state, surface| {
                    state.xwayland_surface_destroyed(surface);
                });
            }
            _ => unreachable!(),
        }
    }
}

impl Dispatch<XwaylandSurfaceV1, XWaylandSurfaceData> for RuntimeState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        resource: &XwaylandSurfaceV1,
        request: xwayland_surface_v1::Request,
        data: &XWaylandSurfaceData,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            xwayland_surface_v1::Request::Destroy => {}
            xwayland_surface_v1::Request::SetSerial {
                serial_lo,
                serial_hi,
            } => {
                let serial = u64::from(serial_lo) | (u64::from(serial_hi) << 32);
                if serial == 0 {
                    resource.post_error(
                        xwayland_surface_v1::Error::InvalidSerial,
                        "xwayland-shell serial must be non-zero",
                    );
                    return;
                }
                if data.associated.load(Ordering::Acquire) {
                    resource.post_error(
                        xwayland_surface_v1::Error::AlreadyAssociated,
                        "wl_surface was already associated with an X11 window",
                    );
                    return;
                }
                with_states(&data.surface, |states| {
                    states
                        .cached_state
                        .get::<XWaylandSurfaceCachedState>()
                        .pending()
                        .serial = Some(serial);
                });
            }
            _ => unreachable!(),
        }
    }
}

impl RuntimeState {
    fn xwayland_surface_serial_committed(&mut self, serial: u64, surface: WlSurface) {
        let window = self
            .xwm
            .as_mut()
            .and_then(|xwm| xwm.take_unpaired_window(serial));
        if let Some(window) = window {
            self.associate_x11_surface(window, surface);
        } else {
            self.xwayland_shell_state.remember_surface(serial, surface);
        }
    }

    pub(crate) fn xwayland_window_serial_received(&mut self, window: u32, serial: u64) {
        if let Some(surface) = self.xwayland_shell_state.take_surface(serial) {
            self.associate_x11_surface(window, surface);
        } else if let Some(xwm) = self.xwm.as_mut() {
            xwm.remember_unpaired_window(serial, window);
        }
    }

    fn xwayland_surface_destroyed(&mut self, surface: &WlSurface) {
        self.xwayland_shell_state.remove_surface(surface);
        if let Some(xwm) = self.xwm.as_mut()
            && let Some(window) = xwm.window_for_wayland_surface(surface)
        {
            window.set_wl_surface(None);
        }
    }
}
