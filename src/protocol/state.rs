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
#[cfg(feature = "tty")]
use tracing::info;
use tracing::warn;

use crate::{
    ecs::{CompositorWorld, ViewId, WorkspaceId},
    layout::{LayoutEngine, LayoutKind},
};

#[cfg(feature = "tty")]
use crate::backend::{BackendOutputEvent, BackendOutputId, OutputDescriptor, TtyBackend};

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
    outputs: HashMap<BackendOutputId, ManagedOutput>,
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
            outputs: HashMap::new(),
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

    pub(crate) fn output_count(&self) -> usize {
        self.space.outputs().count()
    }

    #[cfg(feature = "tty")]
    pub(crate) fn dispatch_udev_event(&mut self, event: smithay::backend::udev::UdevEvent) {
        let Some(mut backend) = self.backend.take() else {
            return;
        };
        backend.handle_udev_event(event);
        self.apply_backend_output_events(backend.take_output_events());
        self.backend = Some(backend);
    }

    #[cfg(feature = "tty")]
    pub(crate) fn dispatch_session_event(&mut self, event: smithay::backend::session::Event) {
        let Some(mut backend) = self.backend.take() else {
            return;
        };
        backend.handle_session_event(event);
        self.apply_backend_output_events(backend.take_output_events());
        self.backend = Some(backend);
    }

    #[cfg(feature = "tty")]
    pub(crate) fn apply_backend_output_events(
        &mut self,
        events: impl IntoIterator<Item = BackendOutputEvent>,
    ) {
        for event in events {
            match event {
                BackendOutputEvent::Connected(descriptor) => self.connect_output(descriptor),
                BackendOutputEvent::Changed(descriptor) => self.change_output(descriptor),
                BackendOutputEvent::Disconnected(id) => self.disconnect_output(id),
            }
        }
    }

    #[cfg(feature = "tty")]
    fn connect_output(&mut self, descriptor: OutputDescriptor) {
        if self.outputs.contains_key(&descriptor.id) {
            self.change_output(descriptor);
            return;
        }
        info!(
            output = descriptor.name,
            device_id = descriptor.id.device_id,
            connector_id = descriptor.id.connector_id,
            crtc = descriptor.crtc,
            "Smithay output connected"
        );
        let output = smithay::output::Output::new(
            descriptor.name.clone(),
            smithay::output::PhysicalProperties {
                size: descriptor.physical_size.into(),
                subpixel: descriptor.subpixel,
                make: "Unknown".to_owned(),
                model: descriptor.name,
                serial_number: "Unknown".to_owned(),
            },
        );
        for mode in &descriptor.modes {
            output.add_mode(*mode);
        }
        output.set_preferred(descriptor.preferred_mode);
        output.change_current_state(
            Some(descriptor.preferred_mode),
            None,
            None,
            Some((0, 0).into()),
        );
        let global = output.create_global::<Self>(&self.display_handle);
        self.space.map_output(&output, (0, 0));
        self.outputs
            .insert(descriptor.id, ManagedOutput { output, global });
        self.reflow_outputs();
    }

    #[cfg(feature = "tty")]
    fn change_output(&mut self, descriptor: OutputDescriptor) {
        info!(
            output = descriptor.name,
            device_id = descriptor.id.device_id,
            connector_id = descriptor.id.connector_id,
            crtc = descriptor.crtc,
            "Smithay output modes changed"
        );
        let Some(managed) = self.outputs.get(&descriptor.id) else {
            self.connect_output(descriptor);
            return;
        };
        for mode in managed.output.modes() {
            managed.output.delete_mode(mode);
        }
        for mode in &descriptor.modes {
            managed.output.add_mode(*mode);
        }
        managed.output.set_preferred(descriptor.preferred_mode);
        managed
            .output
            .change_current_state(Some(descriptor.preferred_mode), None, None, None);
        self.reflow_outputs();
    }

    #[cfg(feature = "tty")]
    fn disconnect_output(&mut self, id: BackendOutputId) {
        let Some(managed) = self.outputs.remove(&id) else {
            return;
        };
        self.space.unmap_output(&managed.output);
        self.display_handle.remove_global::<Self>(managed.global);
        self.reflow_outputs();
        info!(
            device_id = id.device_id,
            connector_id = id.connector_id,
            "Smithay output disconnected"
        );
    }

    #[cfg(feature = "tty")]
    fn reflow_outputs(&mut self) {
        let mut outputs = self.outputs.iter().collect::<Vec<_>>();
        outputs.sort_by_key(|(id, _)| (id.device_id, id.connector_id));
        let mut x = 0;
        for (_, managed) in outputs {
            managed
                .output
                .change_current_state(None, None, None, Some((x, 0).into()));
            self.space.map_output(&managed.output, (x, 0));
            x += managed
                .output
                .current_mode()
                .map(|mode| mode.size.w)
                .unwrap_or(0);
        }
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
struct ManagedOutput {
    output: smithay::output::Output,
    global: smithay::reexports::wayland_server::backend::GlobalId,
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

#[cfg(all(test, feature = "tty"))]
mod tests {
    use smithay::{
        output::{Mode, Subpixel},
        reexports::wayland_server::Display,
    };

    use super::*;

    fn descriptor(connector_id: u32, name: &str, width: i32) -> OutputDescriptor {
        let mode = Mode {
            size: (width, 1080).into(),
            refresh: 60_000,
        };
        OutputDescriptor {
            id: BackendOutputId {
                device_id: 1,
                connector_id,
            },
            name: name.to_owned(),
            physical_size: (600, 340),
            subpixel: Subpixel::HorizontalRgb,
            modes: vec![mode],
            preferred_mode: mode,
            crtc: connector_id,
        }
    }

    fn output_location(state: &RuntimeState, name: &str) -> i32 {
        state
            .space
            .outputs()
            .find(|output| output.name() == name)
            .unwrap()
            .current_location()
            .x
    }

    #[test]
    fn output_events_keep_smithay_space_stable_across_hotplug() {
        let display = Display::<RuntimeState>::new().unwrap();
        let mut state = RuntimeState::new(display.handle(), LayoutKind::Scrolling1D);

        state.apply_backend_output_events([
            BackendOutputEvent::Connected(descriptor(2, "DP-2", 2560)),
            BackendOutputEvent::Connected(descriptor(1, "DP-1", 1920)),
        ]);
        assert_eq!(state.output_count(), 2);
        assert_eq!(output_location(&state, "DP-1"), 0);
        assert_eq!(output_location(&state, "DP-2"), 1920);

        state.apply_backend_output_events([BackendOutputEvent::Changed(descriptor(
            1, "DP-1", 1280,
        ))]);
        assert_eq!(output_location(&state, "DP-2"), 1280);

        state.apply_backend_output_events([BackendOutputEvent::Disconnected(BackendOutputId {
            device_id: 1,
            connector_id: 1,
        })]);
        assert_eq!(state.output_count(), 1);
        assert_eq!(output_location(&state, "DP-2"), 0);
    }
}
