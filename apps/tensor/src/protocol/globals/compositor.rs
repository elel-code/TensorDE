//! Tensor-owned core Wayland surface, region, and subsurface state.
//!
//! The transaction and tree rules are derived from Smithay's compositor
//! implementation at commit c0aa71d. Smithay's copyright notice and MIT
//! terms are in `LICENSES/Smithay-MIT.txt`.

mod cache;
mod transaction;
mod tree;
mod wire;

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use tensor_util::{Point, Rect};
use wayland_server::{
    Client, DisplayHandle, Resource,
    backend::{ClientId, GlobalId},
    protocol::{
        wl_buffer, wl_callback, wl_compositor::WlCompositor, wl_output, wl_region,
        wl_subcompositor::WlSubcompositor, wl_surface::WlSurface,
    },
};

pub(in crate::protocol) use cache::{Cacheable, MultiCache, SurfaceDataMap};
pub(in crate::protocol) use transaction::Barrier;
use transaction::{Transaction, TransactionQueue};
#[cfg(feature = "tty")]
pub(in crate::protocol) use tree::remove_pre_commit_hook;
pub(in crate::protocol) use tree::{
    HookId, TraversalAction, add_blocker, add_destruction_hook, add_post_commit_hook,
    add_pre_commit_hook, get_parent, get_role, give_role, is_sync_subsurface,
    remove_destruction_hook, remove_post_commit_hook, with_states, with_surface_tree_downward,
    with_surface_tree_upward,
};
pub(in crate::protocol) use wire::SubsurfaceCachedState;

use crate::protocol::state::RuntimeState;

pub(in crate::protocol) const SUBSURFACE_ROLE: &str = "subsurface";

#[derive(Debug, Eq, PartialEq)]
pub(in crate::protocol) enum Damage {
    Surface(Rect),
    Buffer(Rect),
}

#[derive(Debug)]
pub(in crate::protocol) enum BufferAssignment {
    Removed,
    NewBuffer(wl_buffer::WlBuffer),
}

#[derive(Debug)]
pub(in crate::protocol) struct SurfaceAttributes {
    pub(in crate::protocol) buffer: Option<BufferAssignment>,
    pub(in crate::protocol) buffer_delta: Option<Point>,
    pub(in crate::protocol) buffer_scale: i32,
    pub(in crate::protocol) buffer_transform: wl_output::Transform,
    pub(in crate::protocol) opaque_region: Option<Arc<RegionAttributes>>,
    pub(in crate::protocol) input_region: Option<Arc<RegionAttributes>>,
    pub(in crate::protocol) damage: Vec<Damage>,
    pub(in crate::protocol) frame_callbacks: Vec<wl_callback::WlCallback>,
    client_scale: f64,
}

impl Default for SurfaceAttributes {
    fn default() -> Self {
        Self {
            buffer: None,
            buffer_delta: None,
            buffer_scale: 1,
            buffer_transform: wl_output::Transform::Normal,
            opaque_region: None,
            input_region: None,
            damage: Vec::new(),
            frame_callbacks: Vec::new(),
            client_scale: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::protocol) enum RectangleKind {
    Add,
    Subtract,
}

#[derive(Clone, Debug, Default)]
pub(in crate::protocol) struct RegionAttributes {
    pub(in crate::protocol) rects: Vec<(RectangleKind, Rect)>,
}

impl RegionAttributes {
    pub(in crate::protocol) fn contains(&self, point: (i32, i32)) -> bool {
        let mut contains = false;
        for (kind, rect) in &self.rects {
            let inside = point.0 >= rect.x
                && point.1 >= rect.y
                && point.0 < rect.right()
                && point.1 < rect.bottom();
            if inside {
                contains = matches!(kind, RectangleKind::Add);
            }
        }
        contains
    }
}

#[derive(Debug)]
pub(crate) struct SurfaceData {
    pub(in crate::protocol) role: Option<&'static str>,
    pub(in crate::protocol) data_map: SurfaceDataMap,
    pub(in crate::protocol) cached_state: MultiCache,
}

#[derive(Debug)]
pub(crate) struct CompositorClientState {
    queue: Mutex<Option<TransactionQueue>>,
    scale_bits: AtomicU64,
}

impl Default for CompositorClientState {
    fn default() -> Self {
        Self {
            queue: Mutex::new(None),
            scale_bits: AtomicU64::new(1.0f64.to_bits()),
        }
    }
}

impl CompositorClientState {
    pub(crate) fn set_client_scale(&self, scale: f64) {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        self.scale_bits.store(scale.to_bits(), Ordering::Release);
    }

    pub(crate) fn client_scale(&self) -> f64 {
        f64::from_bits(self.scale_bits.load(Ordering::Acquire))
    }

    fn take_ready(&self) -> Vec<Transaction> {
        self.queue
            .lock()
            .unwrap()
            .as_mut()
            .map(TransactionQueue::take_ready)
            .unwrap_or_default()
    }
}

#[derive(Debug)]
struct ClientEntry {
    state: Arc<CompositorClientState>,
    surfaces: usize,
}

#[derive(Debug)]
pub(crate) struct CompositorState {
    _compositor: GlobalId,
    _subcompositor: GlobalId,
    clients: HashMap<ClientId, ClientEntry>,
}

impl CompositorState {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _compositor: display
                .create_global::<RuntimeState, WlCompositor, _>(6, wire::CompositorGlobalData),
            _subcompositor: display.create_global::<RuntimeState, WlSubcompositor, _>(
                1,
                wire::SubcompositorGlobalData,
            ),
            clients: HashMap::new(),
        }
    }

    fn track_surface(&mut self, client: ClientId) -> Arc<CompositorClientState> {
        let entry = self.clients.entry(client).or_insert_with(|| ClientEntry {
            state: Arc::new(CompositorClientState::default()),
            surfaces: 0,
        });
        entry.surfaces = entry.surfaces.saturating_add(1);
        Arc::clone(&entry.state)
    }

    fn untrack_surface(&mut self, client: &ClientId) {
        let remove = self.clients.get_mut(client).is_some_and(|entry| {
            entry.surfaces = entry.surfaces.saturating_sub(1);
            entry.surfaces == 0
        });
        if remove {
            self.clients.remove(client);
        }
    }

    pub(crate) fn client_scale(&self, client: &Client) -> f64 {
        self.clients
            .get(&client.id())
            .map(|entry| entry.state.client_scale())
            .unwrap_or(1.0)
    }

    pub(crate) fn set_client_scale(&self, client: &Client, scale: f64) {
        if let Some(entry) = self.clients.get(&client.id()) {
            entry.state.set_client_scale(scale);
        }
    }

    fn client_state(&self, client: &ClientId) -> Option<Arc<CompositorClientState>> {
        self.clients
            .get(client)
            .map(|entry| Arc::clone(&entry.state))
    }
}

impl RuntimeState {
    pub(crate) fn client_scale(&self, client: &Client) -> f64 {
        self.compositor_state.client_scale(client)
    }

    pub(in crate::protocol) fn compositor_blocker_cleared(&mut self, client: &Client) {
        let Some(client_state) = self.compositor_state.client_state(&client.id()) else {
            return;
        };
        let display = self.display_handle.clone();
        for transaction in client_state.take_ready() {
            transaction.apply(&display, self);
        }
    }
}

pub(in crate::protocol) fn get_region_attributes(region: &wl_region::WlRegion) -> RegionAttributes {
    region
        .data::<wire::RegionData>()
        .expect("wl_region was not created by Tensor")
        .attributes
        .lock()
        .unwrap()
        .clone()
}

pub(in crate::protocol) fn send_surface_state(
    surface: &WlSurface,
    data: &SurfaceData,
    scale: i32,
    transform: wl_output::Transform,
) {
    if surface.version() < 6 {
        return;
    }
    let storage = data
        .data_map
        .get_or_insert(|| Mutex::new(SuggestedSurfaceState::default()));
    let mut storage = storage.lock().unwrap();
    if storage.scale != scale {
        surface.preferred_buffer_scale(scale);
        storage.scale = scale;
    }
    if storage.transform != transform {
        surface.preferred_buffer_transform(transform);
        storage.transform = transform;
    }
}

#[derive(Debug)]
struct SuggestedSurfaceState {
    scale: i32,
    transform: wl_output::Transform,
}

impl Default for SuggestedSurfaceState {
    fn default() -> Self {
        Self {
            scale: 1,
            transform: wl_output::Transform::Normal,
        }
    }
}
