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
#[cfg(feature = "xwayland")]
mod xwayland;

use std::collections::HashMap;
#[cfg(feature = "tty")]
use std::collections::HashSet;

#[cfg(feature = "tty")]
use smithay::utils::SERIAL_COUNTER;
use smithay::{
    desktop::{PopupManager, Space, Window},
    input::{Seat, SeatState},
    output::Scale,
    reexports::{
        calloop::LoopHandle,
        wayland_server::{
            DisplayHandle, Resource,
            backend::{ClientData, ClientId, DisconnectReason, ObjectId},
            protocol::wl_surface::WlSurface,
        },
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
#[cfg(feature = "xwayland")]
use smithay::{wayland::xwayland_shell::XWaylandShellState, xwayland::X11Wm};
use tracing::warn;

use crate::{
    ecs::{CompositorWorld, ViewId, WorkspaceId},
    layout::{LayoutEngine, SizeConstraints},
    render::VulkanRenderer,
    scene::SceneAppearance,
};
use tensor_util::Size;

#[cfg(feature = "tty")]
use super::cursor::CursorState;
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
    #[cfg(feature = "xwayland")]
    pub(crate) xwayland_shell_state: XWaylandShellState,
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
    /// Per-CRTC redraw scheduler. Niri-style Idle/Queued/WaitingForVBlank so
    /// each output owns its own page-flip ring instead of sharing one global
    /// workspace submit path.
    #[cfg(feature = "tty")]
    redraw_states: HashMap<BackendOutputId, OutputRedrawState>,
    #[cfg(feature = "tty")]
    renderer_retry_scheduled: bool,
    #[cfg(feature = "tty")]
    pending_presentations: PendingPresentations,
    #[cfg(feature = "tty")]
    pub(crate) backend: Option<TtyBackend>,
    #[cfg(feature = "tty")]
    pub(crate) input_devices: HashMap<String, InputDeviceCapabilities>,
    #[cfg(feature = "tty")]
    pub(crate) cursor: CursorState,
    /// When true, every redraw path fans out to all CRTCs (debug only).
    #[cfg(feature = "tty")]
    force_full_redraw: bool,
    /// Emit per-submit timing at info level when enabled by config.
    #[cfg(feature = "tty")]
    frame_stats: bool,
    surface_views: HashMap<ObjectId, ViewId>,
    #[cfg(feature = "xwayland")]
    pub(crate) xwm: Option<X11Wm>,
    #[cfg(feature = "xwayland")]
    xwayland_windows: HashMap<u32, xwayland::XWaylandWindowLifecycle>,
    #[cfg(feature = "xwayland")]
    xwayland_popups: xwayland::XWaylandPopupRegistry,
    #[cfg(feature = "xwayland")]
    xwayland_transients: xwayland::XWaylandTransientRegistry,
    next_view_id: u64,
}

impl RuntimeState {
    pub(crate) fn with_appearance(
        display_handle: DisplayHandle,
        loop_handle: LoopHandle<'static, Self>,
        layout: LayoutEngine,
        appearance: SceneAppearance,
    ) -> Self {
        let compositor_state = CompositorState::new::<Self>(&display_handle);
        let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
        let shm_state = ShmState::new::<Self>(&display_handle, []);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&display_handle);
        let data_device_state = DataDeviceState::new::<Self>(&display_handle);
        let protocol_globals = ProtocolGlobals::new(&display_handle, &loop_handle);
        #[cfg(feature = "xwayland")]
        let xwayland_shell_state = XWaylandShellState::new::<Self>(&display_handle);
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
            #[cfg(feature = "xwayland")]
            xwayland_shell_state,
            seat,
            space: Space::default(),
            popups: PopupManager::default(),
            world: CompositorWorld::with_appearance(appearance),
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
            redraw_states: HashMap::new(),
            #[cfg(feature = "tty")]
            renderer_retry_scheduled: false,
            #[cfg(feature = "tty")]
            pending_presentations: PendingPresentations::default(),
            #[cfg(feature = "tty")]
            backend: None,
            #[cfg(feature = "tty")]
            input_devices: HashMap::new(),
            #[cfg(feature = "tty")]
            cursor: CursorState::default(),
            #[cfg(feature = "tty")]
            force_full_redraw: false,
            #[cfg(feature = "tty")]
            frame_stats: false,
            surface_views: HashMap::new(),
            #[cfg(feature = "xwayland")]
            xwm: None,
            #[cfg(feature = "xwayland")]
            xwayland_windows: HashMap::new(),
            #[cfg(feature = "xwayland")]
            xwayland_popups: xwayland::XWaylandPopupRegistry::default(),
            #[cfg(feature = "xwayland")]
            xwayland_transients: xwayland::XWaylandTransientRegistry::default(),
            next_view_id: 1,
        }
    }

    /// Apply value-only cursor and debug policy from the configuration boundary.
    pub(crate) fn apply_runtime_policy(
        &mut self,
        cursor: crate::config::CursorConfig,
        debug: crate::config::DebugConfig,
    ) {
        #[cfg(feature = "tty")]
        {
            self.cursor.configure(cursor.size, cursor.hide_when_typing);
            self.force_full_redraw = debug.force_full_redraw;
            self.frame_stats = debug.frame_stats;
        }
        #[cfg(not(feature = "tty"))]
        {
            let _ = (cursor, debug);
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

    /// Flush protocol events produced by non-Wayland event sources.
    ///
    /// Input, KMS, timers, and session events can enqueue client-visible
    /// state without making the Wayland display readable. The event-loop
    /// tail invokes this after every dispatch cycle so keyboard focus, keys,
    /// and frame callbacks are not stranded until a client happens to send
    /// another request.
    pub(crate) fn flush_wayland_clients(&mut self) {
        if let Err(error) = self.display_handle.flush_clients() {
            warn!(%error, "failed to flush pending Wayland client events");
        }
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
        let window = Window::new_wayland_window(surface);
        self.space.map_element(window.clone(), (0, 0), false);
        // Keep the initial focus decision separate from layout/configure
        // publication. XDG requires its first configure to be sent from the
        // initial surface commit, which the compositor handler performs.
        #[cfg(feature = "tty")]
        self.focus_mapped_window(window, SERIAL_COUNTER.next_serial());
        Some(view_id)
    }

    pub(crate) fn unregister_toplevel(&mut self, surface: &WlSurface) -> Option<ViewId> {
        let view_id = self.view_for_surface(surface)?;
        #[cfg(feature = "xwayland")]
        if !self.detach_x11_transient_views_for_owner(view_id) {
            warn!(
                view_id = view_id.get(),
                "refused to tear down a view with unresolved attachments"
            );
            return None;
        }
        #[cfg(feature = "tty")]
        let replacement = match self.world.focus_replacement_after_removal(view_id) {
            Ok(replacement) => replacement.and_then(|view_id| self.mapped_window_for_view(view_id)),
            Err(error) => {
                warn!(%error, view_id = view_id.get(), "failed to select focus after view teardown");
                None
            }
        };
        #[cfg(feature = "tty")]
        if self.world.is_focused(view_id) && replacement.is_none() {
            // Clear before dropping the old Smithay window so a final keyboard
            // leave and XDG deactivation can still reference a live root.
            self.clear_keyboard_focus_for_surface(surface);
            self.publish_window_activation(None);
        }
        #[cfg(feature = "xwayland")]
        self.detach_x11_popups_for_owner(&surface.id());
        let window = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(surface))
            .cloned();
        if let Some(window) = window {
            self.space.unmap_elem(&window);
        }

        let removed_view_id = self.surface_views.remove(&surface.id())?;
        debug_assert_eq!(
            removed_view_id, view_id,
            "surface view index changed during teardown"
        );
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
        #[cfg(feature = "tty")]
        if let Some(window) = replacement {
            // Niri and Hyprland both move focus as part of close-time state
            // reconciliation. Transfer directly rather than leaving the seat
            // and `Activated` state blank until another input event arrives.
            let _ = self.focus_mapped_window(window, SERIAL_COUNTER.next_serial());
        }
        self.reflow_default_workspace();
        Some(view_id)
    }

    #[cfg(feature = "tty")]
    fn clear_keyboard_focus_for_surface(&mut self, surface: &WlSurface) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        if keyboard
            .current_focus()
            .is_some_and(|focused| focused.targets_surface(surface))
        {
            keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
        }
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
        if !self.reflow_default_workspace_layout() {
            return false;
        }
        #[cfg(feature = "tty")]
        self.submit_default_workspace_frame();
        true
    }

    /// Relayout and reconfigure clients without submitting a frame.
    ///
    /// Callers that already force a multi-output redraw (for example
    /// `reflow_outputs`) use this to avoid a duplicate queue/drain cycle.
    pub(crate) fn reflow_default_workspace_layout(&mut self) -> bool {
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
                Some((window.clone(), geometry))
            })
            .collect::<Vec<_>>();

        for (window, geometry) in &windows {
            self.space
                .relocate_element(window, (geometry.x, geometry.y));
        }
        #[cfg(feature = "xwayland")]
        self.relocate_x11_popups();
        self.space.refresh();

        for (window, geometry) in windows {
            self.update_window_surface_state(&window);
            if let Some(toplevel) = window.toplevel().cloned() {
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
            #[cfg(feature = "xwayland")]
            if let Some(x11) = window.x11_surface() {
                xwayland::configure_x11_window(x11, geometry);
            }
        }
        #[cfg(feature = "xwayland")]
        self.update_x11_popup_surface_states();
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

    /// Value-only compositor snapshot for the IPC control surface.
    pub(crate) fn ipc_state_snapshot(&mut self) -> crate::ipc::StateSnapshot {
        crate::ipc::StateSnapshot {
            layout: self.layout.kind(),
            view_count: self.world.view_count(DEFAULT_WORKSPACE),
            output_count: self.output_count(),
            focused_view: self
                .world
                .focused_view(DEFAULT_WORKSPACE)
                .map(|view| view.get()),
        }
    }

    /// Value-only output topology for the IPC control surface.
    pub(crate) fn ipc_output_snapshots(&self) -> Vec<crate::ipc::OutputSnapshot> {
        let primary = self.default_workspace_area();
        let mut outputs = self
            .space
            .outputs()
            .filter_map(|output| {
                let geometry = self.space.output_geometry(output)?;
                let mode = output.current_mode()?;
                let scale = output.current_scale().fractional_scale();
                let logical = tensor_util::Rect::new(
                    geometry.loc.x,
                    geometry.loc.y,
                    u32::try_from(geometry.size.w).unwrap_or(0),
                    u32::try_from(geometry.size.h).unwrap_or(0),
                );
                Some(crate::ipc::OutputSnapshot {
                    name: output.name(),
                    x: geometry.loc.x,
                    y: geometry.loc.y,
                    width: geometry.size.w,
                    height: geometry.size.h,
                    scale,
                    mode_width: mode.size.w,
                    mode_height: mode.size.h,
                    refresh_millihertz: mode.refresh,
                    primary: primary == Some(logical),
                })
            })
            .collect::<Vec<_>>();
        outputs.sort_by(|left, right| {
            left.x
                .cmp(&right.x)
                .then(left.y.cmp(&right.y))
                .then(left.name.cmp(&right.name))
        });
        outputs
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
    /// True after at least one frame was accepted by atomic KMS. Used to skip
    /// empty secondary scanouts once the CRTC has a live page-flip ring.
    has_presented: bool,
}

/// Per-output redraw lifecycle, modeled on Niri's `RedrawState`.
///
/// A newly connected CRTC has no vblank until the first page flip lands. The
/// `Queued` state forces that first frame; subsequent damage either queues
/// immediately or latches `redraw_needed` while a flip is in flight.
#[cfg(feature = "tty")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputRedrawState {
    Idle,
    Queued,
    WaitingForVBlank { redraw_needed: bool },
}

#[cfg(feature = "tty")]
impl OutputRedrawState {
    const fn queue(self) -> Self {
        match self {
            Self::Idle | Self::Queued => Self::Queued,
            Self::WaitingForVBlank { .. } => Self::WaitingForVBlank {
                redraw_needed: true,
            },
        }
    }

    const fn is_queued(self) -> bool {
        matches!(self, Self::Queued)
    }

    const fn needs_gpu_retry(self) -> bool {
        matches!(
            self,
            Self::Queued
                | Self::WaitingForVBlank {
                    redraw_needed: true
                }
        )
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

#[cfg(all(test, feature = "tty"))]
mod tests;
