//! Tensor-owned layer-surface handle and double-buffered state.

use std::{
    cell::{Cell, RefCell},
    ops::{BitOr, BitOrAssign},
    rc::Rc,
    sync::atomic::{AtomicU32, Ordering},
};

use smithay::utils::{Logical, Size};
use wayland_protocols_wlr::layer_shell::v1::server::zwlr_layer_surface_v1::{
    self, ZwlrLayerSurfaceV1,
};
use wayland_server::{Resource, backend::ObjectId, protocol::wl_surface::WlSurface};

const MAX_PENDING_CONFIGURES: usize = 16;
static NEXT_CONFIGURE_SERIAL: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::protocol) enum Layer {
    #[default]
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::protocol) enum KeyboardInteractivity {
    #[default]
    None,
    Exclusive,
    OnDemand,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::protocol) enum ExclusiveZone {
    Exclusive(u32),
    #[default]
    Neutral,
    DontCare,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::protocol) struct Margins {
    pub(in crate::protocol) top: i32,
    pub(in crate::protocol) right: i32,
    pub(in crate::protocol) bottom: i32,
    pub(in crate::protocol) left: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(in crate::protocol) struct Anchor(u8);

impl Anchor {
    pub(in crate::protocol) const TOP: Self = Self(1);
    pub(in crate::protocol) const BOTTOM: Self = Self(2);
    pub(in crate::protocol) const LEFT: Self = Self(4);
    pub(in crate::protocol) const RIGHT: Self = Self(8);
    const ALL: u8 = 15;

    pub(in crate::protocol) const fn empty() -> Self {
        Self(0)
    }

    pub(in crate::protocol) const fn from_bits(bits: u32) -> Option<Self> {
        if bits <= Self::ALL as u32 {
            Some(Self(bits as u8))
        } else {
            None
        }
    }

    pub(in crate::protocol) const fn bits(self) -> u8 {
        self.0
    }

    pub(in crate::protocol) const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub(in crate::protocol) const fn anchored_horizontally(self) -> bool {
        self.contains(Self::LEFT) && self.contains(Self::RIGHT)
    }

    pub(in crate::protocol) const fn anchored_vertically(self) -> bool {
        self.contains(Self::TOP) && self.contains(Self::BOTTOM)
    }

    pub(in crate::protocol) const fn complement(self) -> Self {
        Self(!self.0 & Self::ALL)
    }
}

impl BitOr for Anchor {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Anchor {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::protocol) struct LayerSurfaceState {
    pub(in crate::protocol) size: Size<i32, Logical>,
    pub(in crate::protocol) anchor: Anchor,
    pub(in crate::protocol) exclusive_zone: ExclusiveZone,
    pub(in crate::protocol) exclusive_edge: Option<Anchor>,
    pub(in crate::protocol) margin: Margins,
    pub(in crate::protocol) keyboard_interactivity: KeyboardInteractivity,
    pub(in crate::protocol) layer: Layer,
}

impl LayerSurfaceState {
    fn initial(layer: Layer) -> Self {
        Self {
            size: Size::default(),
            anchor: Anchor::empty(),
            exclusive_zone: ExclusiveZone::Neutral,
            exclusive_edge: None,
            margin: Margins::default(),
            keyboard_interactivity: KeyboardInteractivity::None,
            layer,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Configure {
    serial: u32,
    size: Size<i32, Logical>,
    generation: u64,
}

#[derive(Debug)]
struct ConfigureQueue {
    pending: [Option<Configure>; MAX_PENDING_CONFIGURES],
    head: usize,
    len: usize,
}

impl ConfigureQueue {
    fn new() -> Self {
        Self {
            pending: [None; MAX_PENDING_CONFIGURES],
            head: 0,
            len: 0,
        }
    }

    fn push(&mut self, configure: Configure) {
        if self.len == MAX_PENDING_CONFIGURES {
            self.pending[self.head] = None;
            self.head = (self.head + 1) % MAX_PENDING_CONFIGURES;
            self.len -= 1;
        }
        let tail = (self.head + self.len) % MAX_PENDING_CONFIGURES;
        self.pending[tail] = Some(configure);
        self.len += 1;
    }

    fn ack(&mut self, serial: u32) -> Option<Configure> {
        let offset = (0..self.len).find(|offset| {
            let index = (self.head + offset) % MAX_PENDING_CONFIGURES;
            self.pending[index].is_some_and(|configure| configure.serial == serial)
        })?;
        let configure = self.pending[(self.head + offset) % MAX_PENDING_CONFIGURES]?;
        for _ in 0..=offset {
            self.pending[self.head] = None;
            self.head = (self.head + 1) % MAX_PENDING_CONFIGURES;
            self.len -= 1;
        }
        Some(configure)
    }
}

#[derive(Clone, Debug)]
pub(in crate::protocol) struct LayerSurface(Rc<LayerSurfaceInner>);

#[derive(Debug)]
struct LayerSurfaceInner {
    wl_surface: WlSurface,
    protocol: ZwlrLayerSurfaceV1,
    _namespace: String,
    view_id: u64,
    initial: LayerSurfaceState,
    pending: Cell<LayerSurfaceState>,
    current: Cell<LayerSurfaceState>,
    mapped: Cell<bool>,
    closed: Cell<bool>,
    generation: Cell<u64>,
    configure_ready: Cell<bool>,
    initial_configure_sent: Cell<bool>,
    pending_server_size: Cell<Option<Size<i32, Logical>>>,
    last_sent_size: Cell<Option<Size<i32, Logical>>>,
    last_acked: Cell<Option<Configure>>,
    configures: RefCell<ConfigureQueue>,
}

impl PartialEq for LayerSurface {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for LayerSurface {}

impl LayerSurface {
    pub(in crate::protocol) fn new(
        wl_surface: WlSurface,
        protocol: ZwlrLayerSurfaceV1,
        layer: Layer,
        namespace: String,
        view_id: u64,
    ) -> Self {
        let initial = LayerSurfaceState::initial(layer);
        Self(Rc::new(LayerSurfaceInner {
            wl_surface,
            protocol,
            _namespace: namespace,
            view_id,
            initial,
            pending: Cell::new(initial),
            current: Cell::new(initial),
            mapped: Cell::new(false),
            closed: Cell::new(false),
            generation: Cell::new(1),
            configure_ready: Cell::new(false),
            initial_configure_sent: Cell::new(false),
            pending_server_size: Cell::new(None),
            last_sent_size: Cell::new(None),
            last_acked: Cell::new(None),
            configures: RefCell::new(ConfigureQueue::new()),
        }))
    }

    pub(in crate::protocol) fn protocol_id(&self) -> ObjectId {
        self.0.protocol.id()
    }

    pub(in crate::protocol) fn wl_surface(&self) -> &WlSurface {
        &self.0.wl_surface
    }

    pub(in crate::protocol) fn alive(&self) -> bool {
        !self.0.closed.get() && self.0.wl_surface.is_alive() && self.0.protocol.is_alive()
    }

    pub(in crate::protocol) fn current(&self) -> LayerSurfaceState {
        self.0.current.get()
    }

    pub(in crate::protocol) fn mapped(&self) -> bool {
        self.0.mapped.get()
    }

    pub(in crate::protocol) fn layer(&self) -> Layer {
        self.current().layer
    }

    pub(in crate::protocol) fn can_receive_keyboard_focus(&self) -> bool {
        matches!(
            self.current().keyboard_interactivity,
            KeyboardInteractivity::Exclusive | KeyboardInteractivity::OnDemand
        )
    }

    pub(in crate::protocol) fn view_id(&self) -> u64 {
        self.0.view_id
    }

    pub(in crate::protocol) fn update_pending(&self, update: impl FnOnce(&mut LayerSurfaceState)) {
        if self.0.closed.get() {
            return;
        }
        let mut pending = self.0.pending.get();
        update(&mut pending);
        self.0.pending.set(pending);
    }

    pub(in crate::protocol) fn commit(&self, has_buffer: bool) -> bool {
        if self.0.closed.get() {
            return false;
        }
        let pending = self.0.pending.get();
        if pending.size.w == 0 && !pending.anchor.anchored_horizontally() {
            self.0.protocol.post_error(
                zwlr_layer_surface_v1::Error::InvalidSize,
                "width 0 requires left and right anchors",
            );
            return false;
        }
        if pending.size.h == 0 && !pending.anchor.anchored_vertically() {
            self.0.protocol.post_error(
                zwlr_layer_surface_v1::Error::InvalidSize,
                "height 0 requires top and bottom anchors",
            );
            return false;
        }
        if pending
            .exclusive_edge
            .is_some_and(|edge| !pending.anchor.contains(edge))
        {
            self.0.protocol.post_error(
                zwlr_layer_surface_v1::Error::InvalidExclusiveEdge,
                "exclusive edge is not one of the surface anchors",
            );
            return false;
        }
        if has_buffer
            && self
                .0
                .last_acked
                .get()
                .is_none_or(|configure| configure.generation != self.0.generation.get())
        {
            self.0.protocol.post_error(
                zwlr_layer_surface_v1::Error::InvalidSurfaceState,
                "a configure from the current mapping must be acknowledged before attaching a buffer",
            );
            return false;
        }

        if self.0.mapped.get() && !has_buffer {
            self.reset_for_remap();
            return true;
        }
        self.0.current.set(pending);
        self.0.mapped.set(has_buffer);
        if !has_buffer {
            self.0.configure_ready.set(true);
        }
        true
    }

    fn reset_for_remap(&self) {
        self.0.pending.set(self.0.initial);
        self.0.current.set(self.0.initial);
        self.0.mapped.set(false);
        self.0
            .generation
            .set(self.0.generation.get().wrapping_add(1));
        self.0.configure_ready.set(false);
        self.0.initial_configure_sent.set(false);
        self.0.pending_server_size.set(None);
        self.0.last_sent_size.set(None);
        self.0.last_acked.set(None);
    }

    pub(in crate::protocol) fn set_pending_server_size(&self, size: Size<i32, Logical>) -> bool {
        let previous = self
            .0
            .pending_server_size
            .get()
            .or(self.0.last_sent_size.get());
        self.0.pending_server_size.set(Some(size));
        previous != Some(size)
    }

    pub(in crate::protocol) fn initial_configure_sent(&self) -> bool {
        self.0.initial_configure_sent.get()
    }

    pub(in crate::protocol) fn send_pending_configure(&self) -> Option<u32> {
        if self.0.closed.get() || !self.0.configure_ready.get() {
            return None;
        }
        let size = self
            .0
            .pending_server_size
            .take()
            .or(self.0.last_sent_size.get())
            .unwrap_or_default();
        if self.0.initial_configure_sent.get() && self.0.last_sent_size.get() == Some(size) {
            return None;
        }
        let serial = NEXT_CONFIGURE_SERIAL
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |serial| {
                Some(serial.wrapping_add(1).max(1))
            })
            .expect("configure serial update cannot fail");
        self.0.configures.borrow_mut().push(Configure {
            serial,
            size,
            generation: self.0.generation.get(),
        });
        self.0.initial_configure_sent.set(true);
        self.0.last_sent_size.set(Some(size));
        self.0
            .protocol
            .configure(serial, size.w.max(0) as u32, size.h.max(0) as u32);
        Some(serial)
    }

    pub(in crate::protocol) fn ack_configure(&self, serial: u32) -> bool {
        let Some(configure) = self.0.configures.borrow_mut().ack(serial) else {
            return false;
        };
        if configure.generation == self.0.generation.get() {
            self.0.last_acked.set(Some(configure));
        }
        true
    }

    pub(in crate::protocol) fn close(&self) {
        if !self.0.closed.replace(true) && self.0.protocol.is_alive() {
            self.0.protocol.closed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configure_queue_is_fixed_and_ack_consumes_older_entries() {
        let mut queue = ConfigureQueue::new();
        for serial in 1..=(MAX_PENDING_CONFIGURES as u32 + 1) {
            queue.push(Configure {
                serial,
                size: Size::from((serial as i32, 1)),
                generation: 1,
            });
        }
        assert_eq!(queue.len, MAX_PENDING_CONFIGURES);
        assert!(queue.ack(1).is_none());
        assert_eq!(queue.ack(3).map(|configure| configure.serial), Some(3));
        assert_eq!(queue.len, MAX_PENDING_CONFIGURES - 2);
    }

    #[test]
    fn anchor_rejects_unknown_bits_and_detects_opposite_edges() {
        assert!(Anchor::from_bits(16).is_none());
        assert!((Anchor::LEFT | Anchor::RIGHT).anchored_horizontally());
        assert!((Anchor::TOP | Anchor::BOTTOM).anchored_vertically());
        assert!(!Anchor::TOP.anchored_horizontally());
    }
}
