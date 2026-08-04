mod background_effect;
mod capture;
mod capture_shm;
mod client;
mod config;
mod display;
mod event_loop;
#[cfg(feature = "tty")]
mod input_method;
mod ipc_snapshot;
pub(in crate::protocol) mod layer;
#[cfg(feature = "tty")]
mod output;
mod output_config;
#[cfg(feature = "tty")]
mod output_helpers;
#[cfg(feature = "tty")]
mod output_topology;
mod output_values;
pub(super) mod popup;
#[cfg(feature = "tty")]
mod presentation;
mod protocol_side;
mod space;
mod surface_tree;
mod surfaces;
#[cfg(feature = "tty")]
mod sync;
#[cfg(feature = "tty")]
mod tree;
mod window;
mod workspace_host;
#[cfg(feature = "xwayland")]
mod xwayland;

pub(crate) use client::WaylandClientState;
pub(crate) use popup::{PopupGrab, PopupKind, PopupManager, find_popup_root_surface};
pub(crate) use protocol_side::{ObjectKey, ProtocolSideState};
pub(crate) use window::ProtocolWindow;
pub(crate) use workspace_host::{ViewWorkspaceError, WorkspaceHost};

use event_loop::EventLoopState;
use layer::LayerMaps;
#[cfg(feature = "tty")]
use layer::LayerSurface;
use output_values::{output_integer_scale, wayland_transform};
use space::WindowSpace;
#[cfg(test)]
pub(super) use surface_tree::OutputPresentationFeedback;

#[cfg(feature = "xwayland")]
use crate::protocol::xwayland::{X11Wm, XWaylandShellState};
use crate::{
    config::{CursorConfig, DebugConfig},
    protocol::serial::next_serial,
};
use std::collections::HashMap;
#[cfg(feature = "tty")]
use std::collections::HashSet;
use tracing::warn;
#[cfg(feature = "tty")]
use wayland_server::backend::GlobalId;
use wayland_server::{
    Display, DisplayHandle, Resource, backend::ObjectId, protocol::wl_surface::WlSurface,
};

use crate::protocol::seat::InputSeat;
use crate::{
    ecs::{CompositorWorld, ViewId, WorkspaceId},
    layout::{LayoutEngine, SizeConstraints},
    overview::OverviewOptions,
    render::VulkanRenderer,
    scene::SceneAppearance,
};
use tensor_protocol::SurfaceTransform;
use tensor_util::{OutputScale, Size};

#[cfg(feature = "tty")]
use super::cursor::CursorState;
use super::extensions::security_context::SecurityContextSubmitter;
use super::globals::{
    ProtocolGlobals,
    compositor::{CompositorState, get_parent, send_surface_state, with_states},
    xdg_shell::Toplevel,
};
#[cfg(feature = "tty")]
use presentation::PendingPresentations;
#[cfg(feature = "tty")]
use surfaces::SurfaceBufferRegistry;
#[cfg(feature = "tty")]
pub(crate) use surfaces::take_dnd_icon_surface_delta;
pub(crate) use surfaces::{
    apply_cursor_surface_delta, apply_surface_alpha, apply_surface_image_description,
    apply_surface_representation, destroy_surface_state, on_commit_surface_handler,
};
pub(in crate::protocol) use surfaces::{pending_buffer_logical_size, surface_has_buffer};
pub(in crate::protocol) use surfaces::{pending_surface_fourcc, surface_contains_point};
#[cfg(test)]
pub(crate) use surfaces::{test_surface_buffer, test_surface_tree_states};
#[cfg(feature = "tty")]
pub(super) use sync::ExplicitSyncPoints;
#[cfg(feature = "tty")]
use sync::{PendingClientRelease, SurfaceSyncRegistry};
#[cfg(feature = "tty")]
pub(in crate::protocol) use window::surface_tree_under;

#[cfg(all(test, feature = "tty"))]
use crate::backend::BackendOutputEvent;
#[cfg(feature = "tty")]
use crate::backend::{BackendOutputId, OutputDescriptor, TtyBackend};
#[cfg(feature = "tty")]
use crate::render::GpuFenceSubmitter;

/// First virtual desktop id (also the default active workspace at startup).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const DEFAULT_WORKSPACE: WorkspaceId = WorkspaceId::new(0);

#[cfg(feature = "tty")]
struct DeferredSurfaceSync {
    root: ObjectId,
    surface: WlSurface,
    points: Option<ExplicitSyncPoints>,
}

pub(crate) struct RuntimeState {
    display: Option<Display<Self>>,
    pub(crate) display_handle: DisplayHandle,
    pub(crate) compositor_state: CompositorState,
    pub(crate) input_seat: InputSeat,
    pub(crate) protocol_globals: ProtocolGlobals,
    pub(crate) protocol_side: ProtocolSideState,
    #[cfg(feature = "xwayland")]
    pub(crate) xwayland_shell_state: XWaylandShellState,
    #[cfg(feature = "tty")]
    pub(crate) dnd_icon: super::dnd_icon::DndIconState,
    #[cfg(not(feature = "tty"))]
    pub(crate) dnd_icon: Option<WlSurface>,
    pub(crate) space: WindowSpace,
    pub(crate) popups: PopupManager,
    pub(crate) popup_grab: Option<PopupGrab>,
    layer_maps: LayerMaps,
    pub(crate) world: CompositorWorld,
    pub(crate) layout: LayoutEngine,
    pub(crate) overview_options: OverviewOptions,
    pub(crate) renderer: Option<VulkanRenderer>,
    pub(super) security_context_submitter: Option<SecurityContextSubmitter>,
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
    gpu_fence_submitter: Option<GpuFenceSubmitter>,
    #[cfg(feature = "tty")]
    pending_presentations: PendingPresentations,
    #[cfg(feature = "tty")]
    pub(crate) backend: Option<TtyBackend>,
    #[cfg(feature = "tty")]
    /// Physical devices discovered by the input adapter (value-only caps).
    pub(crate) input_devices: HashMap<tensor_event::DeviceId, InputDeviceCapabilities>,
    #[cfg(feature = "tty")]
    pub(crate) cursor: CursorState,
    /// When true, every redraw path fans out to all CRTCs (debug only).
    #[cfg(feature = "tty")]
    force_full_redraw: bool,
    /// Emit per-submit timing at info level when enabled by config.
    #[cfg(feature = "tty")]
    frame_stats: bool,
    /// On-demand layer surface that last received click or new-map focus.
    #[cfg(feature = "tty")]
    layer_shell_on_demand_focus: Option<LayerSurface>,
    surface_views: HashMap<ObjectId, ViewId>,
    #[cfg(feature = "xwayland")]
    pub(crate) xwm: Option<X11Wm>,
    #[cfg(feature = "xwayland")]
    xwayland_process: Option<xwayland::XWaylandProcess>,
    #[cfg(feature = "xwayland")]
    xwayland_windows: HashMap<u32, xwayland::XWaylandWindowLifecycle>,
    #[cfg(feature = "xwayland")]
    xwayland_popups: xwayland::XWaylandPopupRegistry,
    #[cfg(feature = "xwayland")]
    xwayland_transients: xwayland::XWaylandTransientRegistry,
    next_view_id: u64,
    /// Tensor-owned event bus (phase rings + worker inject). Reactor-agnostic.
    event_loop: EventLoopState,
    /// Active regular workspace plus configured regular/hidden topology.
    workspaces: WorkspaceHost,
}

impl RuntimeState {
    pub(crate) fn with_appearance(
        display: Display<Self>,
        layout: LayoutEngine,
        appearance: SceneAppearance,
    ) -> Self {
        let display_handle = display.handle();
        let compositor_state = CompositorState::new(&display_handle);
        let protocol_globals = ProtocolGlobals::new(&display_handle);
        #[cfg(feature = "xwayland")]
        let xwayland_shell_state = XWaylandShellState::new(&display_handle);
        Self {
            display: Some(display),
            display_handle,
            compositor_state,
            input_seat: InputSeat::default(),
            protocol_globals,
            protocol_side: ProtocolSideState::default(),
            #[cfg(feature = "xwayland")]
            xwayland_shell_state,
            #[cfg(feature = "tty")]
            dnd_icon: super::dnd_icon::DndIconState::default(),
            #[cfg(not(feature = "tty"))]
            dnd_icon: None,
            space: WindowSpace::default(),
            popups: PopupManager::default(),
            popup_grab: None,
            layer_maps: LayerMaps::default(),
            world: CompositorWorld::with_appearance(appearance),
            layout,
            overview_options: OverviewOptions::default(),
            renderer: None,
            security_context_submitter: None,
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
            gpu_fence_submitter: None,
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
            #[cfg(feature = "tty")]
            layer_shell_on_demand_focus: None,
            surface_views: HashMap::new(),
            #[cfg(feature = "xwayland")]
            xwm: None,
            #[cfg(feature = "xwayland")]
            xwayland_process: None,
            #[cfg(feature = "xwayland")]
            xwayland_windows: HashMap::new(),
            #[cfg(feature = "xwayland")]
            xwayland_popups: xwayland::XWaylandPopupRegistry::default(),
            #[cfg(feature = "xwayland")]
            xwayland_transients: xwayland::XWaylandTransientRegistry::default(),
            next_view_id: 1,
            event_loop: EventLoopState::new(),
            workspaces: WorkspaceHost::default(),
        }
    }

    /// Apply value-only cursor and debug policy from the configuration boundary.
    pub(crate) fn apply_runtime_policy(&mut self, cursor: CursorConfig, debug: DebugConfig) {
        #[cfg(feature = "tty")]
        {
            let (released, cursor_changed) = self.cursor.configure_from(cursor);
            self.release_client_buffers(released);
            if cursor_changed {
                self.request_redraw_all();
            }
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
            let main_device = rustix::fs::makedev(render_node.major(), render_node.minor());
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
    pub(crate) fn install_gpu_fence_submitter(&mut self, submitter: GpuFenceSubmitter) {
        assert!(
            self.gpu_fence_submitter.is_none(),
            "GPU fence submitter was installed more than once"
        );
        self.gpu_fence_submitter = Some(submitter);
    }

    /// Flush client-visible protocol after non-Wayland sources (see `event_loop`).
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
        self.refresh_drm_lease_device();
    }
    #[cfg(feature = "tty")]
    pub(crate) fn refresh_syncobj_device(&mut self) {
        let device = self.backend.as_ref().and_then(TtyBackend::syncobj_device);
        self.protocol_globals
            .update_syncobj(&self.display_handle, device);
        self.flush_client_releases();
    }

    pub(crate) fn register_toplevel(&mut self, surface: Toplevel) -> Option<ViewId> {
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
        let workspace = self.workspaces.active();
        self.world
            .spawn_view(view_id, workspace)
            .expect("monotonic view IDs must be unique");
        self.surface_views
            .insert(surface.wl_surface().id(), view_id);
        let window = ProtocolWindow::new_wayland(surface);
        self.space.map_element(window.clone(), (0, 0), false);
        if let Some(toplevel) = window.toplevel() {
            self.publish_foreign_toplevel_from_surface(toplevel.wl_surface());
        }
        // Keep the initial focus decision separate from layout/configure
        // publication. XDG requires its first configure to be sent from the
        // initial surface commit, which the compositor handler performs.
        #[cfg(feature = "tty")]
        self.focus_mapped_window(window, next_serial());
        self.refresh_ext_workspace_protocol();
        Some(view_id)
    }

    pub(crate) fn unregister_toplevel(&mut self, surface: &WlSurface) -> Option<ViewId> {
        let view_id = self.view_for_surface(surface)?;
        self.close_foreign_toplevel(surface);
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
            // Clear before dropping the old protocol window so a final keyboard
            // leave and XDG deactivation can still reference a live root.
            self.clear_keyboard_focus_for_surface(surface);
            self.publish_window_activation(None);
        }
        #[cfg(feature = "xwayland")]
        self.detach_x11_popups_for_owner(&surface.id());
        let window = self
            .space
            .retained_elements()
            .find(|window| window.wl_surface().as_deref() == Some(surface))
            .cloned();
        if let Some(window) = window {
            self.space.unmap_elem(&window, &self.popups);
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
            let _ = self.focus_mapped_window(window, next_serial());
        }
        self.refresh_ext_workspace_protocol();
        self.reflow_default_workspace();
        Some(view_id)
    }

    pub(crate) fn clear_keyboard_focus_for_surface(&mut self, surface: &WlSurface) {
        if self.input_seat.keyboard_focus() == Some(surface) {
            self.set_keyboard_focus(None, next_serial());
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
        object: ObjectId,
        id: crate::ecs::SurfaceBufferId,
        size: tensor_util::Size,
    ) -> bool {
        self.surface_buffers
            .register_imported_buffer(object, id, size)
    }

    #[cfg(feature = "tty")]
    pub(crate) fn buffer_destroyed(&mut self, object: &ObjectId) {
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
        // Historical name: reflows the **active** virtual desktop.
        self.reflow_active_workspace()
    }

    /// Relayout and reconfigure clients without submitting a frame.
    pub(crate) fn reflow_default_workspace_layout(&mut self) -> bool {
        self.reflow_active_workspace_layout()
    }

    /// Push active workspace state to bound `ext-workspace` clients.
    pub(crate) fn refresh_ext_workspace_protocol(&mut self) {
        use crate::protocol::extensions::ext_workspace::WorkspaceProtocolSnapshot;
        let snapshot = WorkspaceProtocolSnapshot {
            active: self.workspaces.active(),
            count: self.workspaces.count(),
        };
        self.protocol_globals.ext_workspace().refresh(&snapshot);
    }

    pub(crate) fn update_surface_scale(&self, surface: &WlSurface) {
        #[cfg(feature = "tty")]
        let root = self
            .owning_view_root(surface)
            .unwrap_or_else(|| self.protocol_role_root(surface));
        #[cfg(not(feature = "tty"))]
        let root = self.protocol_role_root(surface);
        let (scale, transform) = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(&root))
            .map(|window| self.window_output_state(window))
            .unwrap_or((OutputScale::ONE, SurfaceTransform::Normal));
        with_states(surface, |states| {
            send_surface_state(
                surface,
                states,
                output_integer_scale(scale),
                wayland_transform(transform),
            );
        });
        self.protocol_globals
            .set_preferred_fractional_scale(surface, scale);
    }

    fn protocol_role_root(&self, surface: &WlSurface) -> WlSurface {
        let mut tree_root = surface.clone();
        while let Some(parent) = get_parent(&tree_root) {
            tree_root = parent;
        }
        if let Some(popup) = self.popups.find_popup(&tree_root)
            && let Ok(root) = find_popup_root_surface(&popup)
        {
            return root;
        }
        if let Some(parent) = self.protocol_globals.input_method.popup_parent(&tree_root) {
            return self.protocol_role_root(&parent);
        }
        tree_root
    }

    fn update_window_surface_state(&self, window: &ProtocolWindow) {
        let (scale, transform) = self.window_output_state(window);
        window.with_surfaces(&self.popups, |surface, states| {
            send_surface_state(
                surface,
                states,
                output_integer_scale(scale),
                wayland_transform(transform),
            );
            self.protocol_globals
                .set_preferred_fractional_scale(surface, scale);
        });
    }

    fn window_output_state(&self, window: &ProtocolWindow) -> (OutputScale, SurfaceTransform) {
        self.space
            .outputs_for_element(window)
            .filter_map(|output| {
                let geometry = self.space.output_geometry(output)?;
                Some((geometry.loc.x, geometry.loc.y, output.name(), output))
            })
            .min_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then(left.1.cmp(&right.1))
                    .then(left.2.cmp(right.2))
            })
            .map(|(_, _, _, output)| {
                let snapshot = output.snapshot();
                (snapshot.scale, snapshot.transform)
            })
            .unwrap_or((OutputScale::ONE, SurfaceTransform::Normal))
    }

    pub(crate) fn default_workspace_area(&self) -> Option<tensor_util::Rect> {
        self.space
            .outputs()
            .filter_map(|output| {
                let geometry = self.space.output_geometry(output)?;
                #[cfg(feature = "tty")]
                let area = self.exclusive_workspace_area(output, geometry)?;
                #[cfg(not(feature = "tty"))]
                let area = {
                    let width = u32::try_from(geometry.size.w).ok()?;
                    let height = u32::try_from(geometry.size.h).ok()?;
                    (width > 0 && height > 0).then(|| {
                        tensor_util::Rect::new(geometry.loc.x, geometry.loc.y, width, height)
                    })?
                };
                Some((geometry.loc.x, geometry.loc.y, output.name(), area))
            })
            .min_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then(left.1.cmp(&right.1))
                    .then(left.2.cmp(right.2))
            })
            .map(|(_, _, _, geometry)| geometry)
    }

    pub(crate) fn view_count(&mut self) -> usize {
        self.world.view_count(self.workspaces.active())
    }

    /// Value-only output topology for the IPC control surface.
    pub(crate) fn ipc_output_snapshots(&self) -> Vec<crate::ipc::OutputSnapshot> {
        let primary = self.default_workspace_area();
        let mut outputs = self
            .space
            .outputs()
            .filter_map(|output| {
                let geometry = self.space.output_geometry(output)?;
                let snapshot = output.snapshot();
                let mode = snapshot.mode?;
                let scale = snapshot.scale.as_f64();
                let logical = tensor_util::Rect::new(
                    geometry.loc.x,
                    geometry.loc.y,
                    u32::try_from(geometry.size.w).unwrap_or(0),
                    u32::try_from(geometry.size.h).unwrap_or(0),
                );
                Some(crate::ipc::OutputSnapshot {
                    name: output.name().to_owned(),
                    x: geometry.loc.x,
                    y: geometry.loc.y,
                    width: geometry.size.w,
                    height: geometry.size.h,
                    scale,
                    mode_width: mode.width,
                    mode_height: mode.height,
                    refresh_millihertz: mode.refresh_millihertz,
                    primary: primary == Some(logical),
                    enabled: true,
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

pub(crate) fn xdg_size_constraints(min_size: Size, max_size: Size) -> SizeConstraints {
    SizeConstraints::new(
        Size::new(min_size.width.max(1), min_size.height.max(1)),
        (max_size.width > 0).then_some(max_size.width),
        (max_size.height > 0).then_some(max_size.height),
    )
}

#[cfg(feature = "tty")]
struct ManagedOutput {
    output: crate::protocol::globals::output::Output,
    global: GlobalId,
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

    #[cfg(test)]
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

/// Physical device capability bits owned by `tensor-event`.
#[cfg(feature = "tty")]
pub(crate) type InputDeviceCapabilities = tensor_event::DeviceCapabilities;

#[cfg(all(test, feature = "tty"))]
mod tests;
