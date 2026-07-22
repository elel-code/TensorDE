use std::collections::HashMap;

use smithay::{
    desktop::{PopupManager, Space, Window},
    input::{Seat, SeatState},
    reexports::wayland_server::{
        DisplayHandle, Resource,
        backend::{ClientData, ClientId, DisconnectReason, ObjectId},
        protocol::wl_surface::WlSurface,
    },
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        seat::WaylandFocus,
        selection::data_device::DataDeviceState,
        shell::xdg::{ToplevelSurface, XdgShellState},
        shm::ShmState,
    },
};
use tracing::warn;

use crate::{
    ecs::{CompositorWorld, ViewId, WorkspaceId},
    layout::{LayoutEngine, LayoutKind},
};

#[cfg(feature = "tty")]
use crate::backend::TtyBackend;

pub(crate) const DEFAULT_WORKSPACE: WorkspaceId = WorkspaceId::new(0);

pub(crate) struct RuntimeState {
    pub(crate) display_handle: DisplayHandle,
    pub(crate) compositor_state: CompositorState,
    pub(crate) xdg_shell_state: XdgShellState,
    pub(crate) shm_state: ShmState,
    pub(crate) output_manager_state: OutputManagerState,
    pub(crate) seat_state: SeatState<Self>,
    pub(crate) data_device_state: DataDeviceState,
    pub(crate) seat: Seat<Self>,
    pub(crate) space: Space<Window>,
    pub(crate) popups: PopupManager,
    pub(crate) world: CompositorWorld,
    pub(crate) layout: LayoutEngine,
    #[cfg(feature = "tty")]
    pub(crate) backend: Option<TtyBackend>,
    #[cfg(feature = "tty")]
    pub(crate) input_devices: HashMap<String, InputDeviceCapabilities>,
    surface_views: HashMap<ObjectId, ViewId>,
    next_view_id: u64,
}

impl RuntimeState {
    pub(crate) fn new(display_handle: DisplayHandle, layout: LayoutKind) -> Self {
        let compositor_state = CompositorState::new::<Self>(&display_handle);
        let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
        let shm_state = ShmState::new::<Self>(&display_handle, []);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&display_handle);
        let data_device_state = DataDeviceState::new::<Self>(&display_handle);
        let mut seat_state = SeatState::new();
        let seat = seat_state.new_wl_seat(&display_handle, "tensor");

        Self {
            display_handle,
            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            seat,
            space: Space::default(),
            popups: PopupManager::default(),
            world: CompositorWorld::new(),
            layout: LayoutEngine::new(layout),
            #[cfg(feature = "tty")]
            backend: None,
            #[cfg(feature = "tty")]
            input_devices: HashMap::new(),
            surface_views: HashMap::new(),
            next_view_id: 1,
        }
    }

    pub(crate) fn register_toplevel(&mut self, surface: ToplevelSurface) -> ViewId {
        let view_id = self.allocate_view_id();
        self.world
            .spawn_view(view_id, DEFAULT_WORKSPACE)
            .expect("monotonic view IDs must be unique");
        self.surface_views
            .insert(surface.wl_surface().id(), view_id);
        self.space
            .map_element(Window::new_wayland_window(surface), (0, 0), false);
        view_id
    }

    pub(crate) fn unregister_toplevel(&mut self, surface: &WlSurface) -> Option<ViewId> {
        let window = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(surface))
            .cloned();
        if let Some(window) = window {
            self.space.unmap_elem(&window);
        }

        let view_id = self.surface_views.remove(&surface.id())?;
        if let Err(error) = self.world.remove_view(view_id) {
            warn!(%error, view_id = view_id.get(), "Wayland view was missing from ECS");
        }
        Some(view_id)
    }

    pub(crate) fn view_for_surface(&self, surface: &WlSurface) -> Option<ViewId> {
        self.surface_views.get(&surface.id()).copied()
    }

    pub(crate) fn view_count(&mut self) -> usize {
        self.world.view_count(DEFAULT_WORKSPACE)
    }

    fn allocate_view_id(&mut self) -> ViewId {
        let view_id = ViewId::new(self.next_view_id);
        self.next_view_id = self
            .next_view_id
            .checked_add(1)
            .expect("compositor exhausted the stable view ID space");
        view_id
    }
}

#[cfg(feature = "tty")]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct InputDeviceCapabilities {
    pub(crate) keyboard: bool,
    pub(crate) pointer: bool,
    pub(crate) touch: bool,
}

#[derive(Debug, Default)]
pub(crate) struct WaylandClientState {
    pub(crate) compositor_state: CompositorClientState,
}

impl ClientData for WaylandClientState {
    fn initialized(&self, _client_id: ClientId) {}

    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
