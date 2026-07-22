use std::collections::HashMap;

use smithay::{
    desktop::{PopupManager, Space, Window},
    input::{Seat, SeatState},
    output::Scale,
    reexports::wayland_server::{
        DisplayHandle, Resource,
        backend::{ClientData, ClientId, DisconnectReason, ObjectId},
        protocol::wl_surface::WlSurface,
    },
    wayland::{
        compositor::{
            CompositorClientState, CompositorState, get_parent, send_surface_state, with_states,
        },
        fractional_scale::with_fractional_scale,
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
    layout::{LayoutEngine, SizeConstraints},
    render::VulkanRenderer,
};
use tensor_util::Size;

#[cfg(feature = "tty")]
use crate::render::RenderOutputId;
#[cfg(feature = "tty")]
use tensor_util::Rect;

use super::globals::ProtocolGlobals;

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
    pub(crate) protocol_globals: ProtocolGlobals,
    pub(crate) seat: Seat<Self>,
    pub(crate) space: Space<Window>,
    pub(crate) popups: PopupManager,
    pub(crate) world: CompositorWorld,
    pub(crate) layout: LayoutEngine,
    pub(crate) renderer: Option<VulkanRenderer>,
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
    pub(crate) fn new(display_handle: DisplayHandle, layout: LayoutEngine) -> Self {
        let compositor_state = CompositorState::new::<Self>(&display_handle);
        let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
        let shm_state = ShmState::new::<Self>(&display_handle, []);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&display_handle);
        let data_device_state = DataDeviceState::new::<Self>(&display_handle);
        let protocol_globals = ProtocolGlobals::new(&display_handle);
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
            protocol_globals,
            seat,
            space: Space::default(),
            popups: PopupManager::default(),
            world: CompositorWorld::new(),
            layout,
            renderer: None,
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

    pub(crate) fn install_renderer(&mut self, renderer: VulkanRenderer) {
        assert!(
            self.renderer.is_none(),
            "renderer was installed more than once"
        );
        self.renderer = Some(renderer);
    }

    pub(crate) fn renderer(&self) -> Option<&VulkanRenderer> {
        self.renderer.as_ref()
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
        self.reflow_default_workspace();
        Some(view_id)
    }

    pub(crate) fn view_for_surface(&self, surface: &WlSurface) -> Option<ViewId> {
        self.surface_views.get(&surface.id()).copied()
    }

    pub(crate) fn update_toplevel_constraints(
        &mut self,
        surface: &WlSurface,
        constraints: SizeConstraints,
    ) -> bool {
        let Some(view_id) = self.view_for_surface(surface) else {
            return false;
        };
        match self.world.set_view_constraints(view_id, constraints) {
            Ok(changed) => changed,
            Err(error) => {
                warn!(%error, view_id = view_id.get(), "failed to update XDG size constraints");
                false
            }
        }
    }

    pub(crate) fn reflow_default_workspace(&mut self) -> bool {
        let Some(area) = self.default_workspace_area() else {
            return false;
        };
        self.world
            .arrange_workspace(DEFAULT_WORKSPACE, self.layout, area);

        let windows = self
            .space
            .elements()
            .filter_map(|window| {
                let surface = window.wl_surface()?;
                let view_id = self.view_for_surface(&surface)?;
                let geometry = self.world.geometry(view_id)?;
                Some((window.clone(), window.toplevel().cloned(), geometry))
            })
            .collect::<Vec<_>>();

        for (window, _, geometry) in &windows {
            self.space
                .relocate_element(window, (geometry.x, geometry.y));
        }
        self.space.refresh();

        for (window, toplevel, geometry) in windows {
            self.update_window_surface_state(&window);
            if let Some(toplevel) = toplevel {
                let size = (
                    i32::try_from(geometry.width).unwrap_or(i32::MAX),
                    i32::try_from(geometry.height).unwrap_or(i32::MAX),
                )
                    .into();
                let bounds = (
                    i32::try_from(area.width).unwrap_or(i32::MAX),
                    i32::try_from(area.height).unwrap_or(i32::MAX),
                )
                    .into();
                toplevel.with_pending_state(|state| {
                    state.size = Some(size);
                    state.bounds = Some(bounds);
                });
                toplevel.send_pending_configure();
            }
        }
        #[cfg(feature = "tty")]
        self.submit_default_workspace_frame();
        true
    }

    #[cfg(feature = "tty")]
    fn submit_default_workspace_frame(&mut self) {
        let Some(scene) = self.world.extract_scene(DEFAULT_WORKSPACE) else {
            return;
        };
        let Some((output_id, _)) = self.outputs.iter().find(|(_, managed)| {
            let Some(geometry) = self.space.output_geometry(&managed.output) else {
                return false;
            };
            geometry.loc.x == scene.viewport.x
                && geometry.loc.y == scene.viewport.y
                && geometry.size.w == i32::try_from(scene.viewport.width).unwrap_or(i32::MAX)
                && geometry.size.h == i32::try_from(scene.viewport.height).unwrap_or(i32::MAX)
        }) else {
            return;
        };
        let output_id = *output_id;
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        match renderer.submit_scene(
            RenderOutputId {
                device_id: output_id.device_id,
                connector_id: output_id.connector_id,
            },
            scene,
        ) {
            Ok(frame) => info!(
                output_device = output_id.device_id,
                output_connector = output_id.connector_id,
                serial = frame.serial,
                timeline = frame.timeline_value,
                damage_regions = frame.damage.regions().len(),
                descriptor_offset = frame.descriptors.offset,
                descriptor_bytes = frame.descriptors.size,
                scene_nodes = frame.scene.nodes().len(),
                damage_empty = frame.damage.is_empty(),
                frame_output_device = frame.output.device_id,
                frame_output_connector = frame.output.connector_id,
                viewport = ?frame.viewport,
                "renderer frame boundary submitted"
            ),
            Err(error) => warn!(
                output_device = output_id.device_id,
                output_connector = output_id.connector_id,
                %error,
                "renderer frame boundary failed"
            ),
        }
    }

    pub(crate) fn update_surface_scale(&self, surface: &WlSurface) {
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        let (scale, transform) = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(&root))
            .map(|window| self.window_output_state(window))
            .unwrap_or((Scale::Integer(1), smithay::utils::Transform::Normal));
        with_states(surface, |states| {
            send_surface_state(surface, states, scale.integer_scale(), transform);
            with_fractional_scale(states, |fractional| {
                fractional.set_preferred_scale(scale.fractional_scale());
            });
        });
    }

    fn update_window_surface_state(&self, window: &Window) {
        let (scale, transform) = self.window_output_state(window);
        window.with_surfaces(|surface, states| {
            send_surface_state(surface, states, scale.integer_scale(), transform);
            with_fractional_scale(states, |fractional| {
                fractional.set_preferred_scale(scale.fractional_scale());
            });
        });
    }

    fn window_output_state(&self, window: &Window) -> (Scale, smithay::utils::Transform) {
        self.space
            .outputs_for_element(window)
            .into_iter()
            .filter_map(|output| {
                let geometry = self.space.output_geometry(&output)?;
                Some((geometry.loc.x, geometry.loc.y, output.name(), output))
            })
            .min_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then(left.1.cmp(&right.1))
                    .then(left.2.cmp(&right.2))
            })
            .map(|(_, _, _, output)| (output.current_scale(), output.current_transform()))
            .unwrap_or((Scale::Integer(1), smithay::utils::Transform::Normal))
    }

    fn default_workspace_area(&self) -> Option<tensor_util::Rect> {
        self.space
            .outputs()
            .filter_map(|output| {
                let geometry = self.space.output_geometry(output)?;
                let width = u32::try_from(geometry.size.w).ok()?;
                let height = u32::try_from(geometry.size.h).ok()?;
                (width > 0 && height > 0).then(|| {
                    (
                        geometry.loc.x,
                        geometry.loc.y,
                        output.name(),
                        tensor_util::Rect::new(geometry.loc.x, geometry.loc.y, width, height),
                    )
                })
            })
            .min_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then(left.1.cmp(&right.1))
                    .then(left.2.cmp(&right.2))
            })
            .map(|(_, _, _, geometry)| geometry)
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
                model: descriptor.name.clone(),
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
        if let Some(renderer) = self.renderer.as_mut() {
            let viewport = Rect::new(
                0,
                0,
                u32::try_from(descriptor.preferred_mode.size.w).unwrap_or(0),
                u32::try_from(descriptor.preferred_mode.size.h).unwrap_or(0),
            );
            if let Err(error) = renderer.register_output(
                RenderOutputId {
                    device_id: descriptor.id.device_id,
                    connector_id: descriptor.id.connector_id,
                },
                viewport,
            ) {
                warn!(%error, output = descriptor.name, "failed to register renderer output");
            }
        }
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
        if let Some(renderer) = self.renderer.as_mut() {
            let viewport = Rect::new(
                0,
                0,
                u32::try_from(descriptor.preferred_mode.size.w).unwrap_or(0),
                u32::try_from(descriptor.preferred_mode.size.h).unwrap_or(0),
            );
            if let Err(error) = renderer.register_output(
                RenderOutputId {
                    device_id: descriptor.id.device_id,
                    connector_id: descriptor.id.connector_id,
                },
                viewport,
            ) {
                warn!(%error, output = descriptor.name, "failed to resize renderer output");
            }
        }
        self.reflow_outputs();
    }

    #[cfg(feature = "tty")]
    fn disconnect_output(&mut self, id: BackendOutputId) {
        let Some(managed) = self.outputs.remove(&id) else {
            return;
        };
        self.space.unmap_output(&managed.output);
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.unregister_output(RenderOutputId {
                device_id: id.device_id,
                connector_id: id.connector_id,
            });
        }
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
        self.reflow_default_workspace();
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

pub(crate) fn xdg_size_constraints(
    min_size: smithay::utils::Size<i32, smithay::utils::Logical>,
    max_size: smithay::utils::Size<i32, smithay::utils::Logical>,
) -> SizeConstraints {
    SizeConstraints::new(
        Size::new(minimum_axis(min_size.w), minimum_axis(min_size.h)),
        maximum_axis(max_size.w),
        maximum_axis(max_size.h),
    )
}

fn minimum_axis(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(0).max(1)
}

fn maximum_axis(value: i32) -> Option<u32> {
    u32::try_from(value).ok().filter(|value| *value > 0)
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
        let mut state = RuntimeState::new(
            display.handle(),
            LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        );

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
