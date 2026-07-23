#[cfg(feature = "tty")]
mod output;
#[cfg(feature = "tty")]
mod presentation;
#[cfg(feature = "tty")]
mod surfaces;
#[cfg(feature = "tty")]
mod sync;
#[cfg(feature = "tty")]
mod tree;

use std::collections::HashMap;
#[cfg(feature = "tty")]
use std::collections::HashSet;

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
use tracing::warn;

use crate::{
    ecs::{CompositorWorld, ViewId, WorkspaceId},
    layout::{LayoutEngine, SizeConstraints},
    render::VulkanRenderer,
};
use tensor_util::Size;

use super::globals::ProtocolGlobals;
#[cfg(feature = "tty")]
use presentation::PendingPresentations;
#[cfg(feature = "tty")]
use surfaces::SurfaceBufferRegistry;
#[cfg(feature = "tty")]
pub(super) use sync::ExplicitSyncPoints;
#[cfg(feature = "tty")]
use sync::{PendingClientRelease, SurfaceSyncRegistry};

#[cfg(all(test, feature = "tty"))]
use crate::backend::BackendOutputEvent;
#[cfg(feature = "tty")]
use crate::backend::{BackendOutputId, OutputDescriptor, TtyBackend};

pub(crate) const DEFAULT_WORKSPACE: WorkspaceId = WorkspaceId::new(0);

#[cfg(feature = "tty")]
struct DeferredSurfaceSync {
    root: ObjectId,
    surface: WlSurface,
    points: Option<ExplicitSyncPoints>,
}

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
    surface_buffers: SurfaceBufferRegistry,
    #[cfg(feature = "tty")]
    surface_sync: SurfaceSyncRegistry,
    #[cfg(feature = "tty")]
    pending_client_releases: Vec<PendingClientRelease>,
    #[cfg(feature = "tty")]
    pub(super) pending_content_repaints: HashSet<ViewId>,
    #[cfg(feature = "tty")]
    pending_surface_sync: HashMap<ObjectId, DeferredSurfaceSync>,
    #[cfg(feature = "tty")]
    outputs: HashMap<BackendOutputId, ManagedOutput>,
    #[cfg(feature = "tty")]
    repaint_pending: HashSet<BackendOutputId>,
    #[cfg(feature = "tty")]
    renderer_retry_scheduled: bool,
    #[cfg(feature = "tty")]
    pending_presentations: PendingPresentations,
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
            surface_buffers: SurfaceBufferRegistry::default(),
            #[cfg(feature = "tty")]
            surface_sync: SurfaceSyncRegistry::default(),
            #[cfg(feature = "tty")]
            pending_client_releases: Vec::new(),
            #[cfg(feature = "tty")]
            pending_content_repaints: HashSet::new(),
            #[cfg(feature = "tty")]
            pending_surface_sync: HashMap::new(),
            #[cfg(feature = "tty")]
            outputs: HashMap::new(),
            #[cfg(feature = "tty")]
            repaint_pending: HashSet::new(),
            #[cfg(feature = "tty")]
            renderer_retry_scheduled: false,
            #[cfg(feature = "tty")]
            pending_presentations: PendingPresentations::default(),
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
        #[cfg(feature = "tty")]
        {
            let render_node = renderer.selected().render_node;
            let main_device =
                smithay::reexports::rustix::fs::makedev(render_node.major(), render_node.minor());
            let formats = renderer.client_import_formats();
            if let Err(error) =
                self.protocol_globals
                    .install_dmabuf(&self.display_handle, main_device, formats)
            {
                warn!(%error, "failed to build the linux-dmabuf feedback table");
            }
        }
        self.renderer = Some(renderer);
    }

    pub(crate) fn renderer(&self) -> Option<&VulkanRenderer> {
        self.renderer.as_ref()
    }

    #[cfg(feature = "tty")]
    pub(crate) fn install_backend(&mut self, backend: TtyBackend) {
        assert!(
            self.backend.is_none(),
            "tty backend was installed more than once"
        );
        let device = backend.syncobj_device();
        self.protocol_globals
            .update_syncobj(&self.display_handle, device);
        self.backend = Some(backend);
    }

    #[cfg(feature = "tty")]
    pub(crate) fn refresh_syncobj_device(&mut self) {
        let device = self.backend.as_ref().and_then(TtyBackend::syncobj_device);
        self.protocol_globals
            .update_syncobj(&self.display_handle, device);
        self.flush_client_releases();
    }

    pub(crate) fn register_toplevel(&mut self, surface: ToplevelSurface) -> Option<ViewId> {
        #[cfg(feature = "tty")]
        if self
            .surface_buffers
            .register_view_root(surface.wl_surface().id())
            .is_none()
        {
            warn!("surface identity space is exhausted; rejecting new toplevel");
            return None;
        }
        let view_id = self.allocate_view_id();
        self.world
            .spawn_view(view_id, DEFAULT_WORKSPACE)
            .expect("monotonic view IDs must be unique");
        self.surface_views
            .insert(surface.wl_surface().id(), view_id);
        self.space
            .map_element(Window::new_wayland_window(surface), (0, 0), false);
        Some(view_id)
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
        #[cfg(feature = "tty")]
        self.discard_deferred_view_sync(&surface.id());
        #[cfg(feature = "tty")]
        let removal = self.surface_buffers.remove_view_tree(&surface.id());
        #[cfg(feature = "tty")]
        for surface_id in removal.surfaces {
            if let Some(sync) = self.surface_sync.remove(surface_id) {
                self.finish_surface_sync(surface_id, sync.release);
            }
        }
        #[cfg(feature = "tty")]
        self.release_client_buffers(removal.released_buffers);
        #[cfg(feature = "tty")]
        self.flush_client_releases();
        #[cfg(feature = "tty")]
        self.pending_content_repaints.remove(&view_id);
        if let Err(error) = self.world.remove_view(view_id) {
            warn!(%error, view_id = view_id.get(), "Wayland view was missing from ECS");
        }
        self.reflow_default_workspace();
        Some(view_id)
    }

    pub(crate) fn view_for_surface(&self, surface: &WlSurface) -> Option<ViewId> {
        self.surface_views.get(&surface.id()).copied()
    }

    #[cfg(all(test, feature = "tty"))]
    pub(crate) fn surface_tree_member_count(&self, root: &WlSurface) -> usize {
        self.surface_buffers.view_member_count(&root.id())
    }

    #[cfg(feature = "tty")]
    pub(crate) fn allocate_client_buffer_id(&mut self) -> Option<crate::ecs::SurfaceBufferId> {
        self.surface_buffers.allocate_buffer_id_for_import()
    }

    #[cfg(feature = "tty")]
    pub(crate) fn register_imported_client_buffer(
        &mut self,
        object: smithay::reexports::wayland_server::backend::ObjectId,
        id: crate::ecs::SurfaceBufferId,
        size: tensor_util::Size,
    ) -> bool {
        self.surface_buffers
            .register_imported_buffer(object, id, size)
    }

    #[cfg(feature = "tty")]
    pub(crate) fn buffer_destroyed(
        &mut self,
        object: &smithay::reexports::wayland_server::backend::ObjectId,
    ) {
        let released = self.surface_buffers.buffer_destroyed(object);
        self.release_client_buffers(released);
    }

    #[cfg(feature = "tty")]
    pub(crate) fn release_client_buffers(
        &mut self,
        ids: impl IntoIterator<Item = crate::ecs::SurfaceBufferId>,
    ) {
        if let Some(renderer) = self.renderer.as_mut() {
            for id in ids {
                renderer.release_client_image(id);
            }
        }
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
    descriptor: OutputDescriptor,
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
        backend::allocator::{Format as DrmFormat, Fourcc, Modifier},
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
            native_format: crate::render::OutputFormat {
                format: DrmFormat {
                    code: Fourcc::Xrgb8888,
                    modifier: Modifier::from(9),
                },
                plane_count: 1,
            },
            scale: tensor_util::OutputScale::ONE,
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

    fn output_geometry(
        state: &RuntimeState,
        name: &str,
    ) -> smithay::utils::Rectangle<i32, smithay::utils::Logical> {
        let output = state
            .space
            .outputs()
            .find(|output| output.name() == name)
            .unwrap();
        state.space.output_geometry(output).unwrap()
    }

    #[test]
    fn output_events_keep_smithay_space_stable_across_hotplug() {
        let display = Display::<RuntimeState>::new().unwrap();
        let mut state = RuntimeState::new(
            display.handle(),
            LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        );

        state
            .apply_backend_output_events([
                BackendOutputEvent::Connected(descriptor(2, "DP-2", 2560)),
                BackendOutputEvent::Connected(descriptor(1, "DP-1", 1920)),
            ])
            .unwrap();
        assert_eq!(state.output_count(), 2);
        assert_eq!(output_location(&state, "DP-1"), 0);
        assert_eq!(output_location(&state, "DP-2"), 1920);

        state
            .apply_backend_output_events([BackendOutputEvent::Changed(descriptor(1, "DP-1", 1280))])
            .unwrap();
        assert_eq!(output_location(&state, "DP-2"), 1280);

        state
            .apply_backend_output_events([BackendOutputEvent::Disconnected(BackendOutputId {
                device_id: 1,
                connector_id: 1,
            })])
            .unwrap();
        assert_eq!(state.output_count(), 1);
        assert_eq!(output_location(&state, "DP-2"), 0);
    }

    #[test]
    fn fractional_output_scale_controls_logical_reflow() {
        let display = Display::<RuntimeState>::new().unwrap();
        let mut state = RuntimeState::new(
            display.handle(),
            LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        );
        let mut first = descriptor(1, "DP-1", 1920);
        first.scale = tensor_util::OutputScale::from_f64(1.25).unwrap();
        let second = descriptor(2, "DP-2", 1920);

        state
            .apply_backend_output_events([
                BackendOutputEvent::Connected(first),
                BackendOutputEvent::Connected(second),
            ])
            .unwrap();

        assert_eq!(output_geometry(&state, "DP-1").size, (1536, 864).into());
        assert_eq!(output_location(&state, "DP-2"), 1536);
        let first = state
            .space
            .outputs()
            .find(|output| output.name() == "DP-1")
            .unwrap();
        assert_eq!(first.current_scale().fractional_scale(), 1.25);
        assert_eq!(first.current_scale().integer_scale(), 2);
    }
}
