//! Tensor-owned core seat wire state.

use std::{collections::HashMap, sync::Arc};

use tensor_util::Point;
use wayland_server::{
    DisplayHandle, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::{
        wl_keyboard::WlKeyboard,
        wl_pointer::WlPointer,
        wl_seat::{self, WlSeat},
        wl_surface::WlSurface,
        wl_touch::WlTouch,
    },
};

use crate::protocol::seat::ModifiersState;
use crate::protocol::serial::Serial;
use crate::protocol::state::RuntimeState;

mod keyboard;
mod pointer;
mod wire;

pub(super) use keyboard::KeymapFile;
use pointer::PointerResource;
pub(super) use wire::SeatGlobalData;
use wire::{PointerData, SeatData};

pub(in crate::protocol) const CURSOR_IMAGE_ROLE: &str = "cursor_image";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SeatOwner {
    Primary,
    Transient(u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::protocol) struct CursorSurfaceState {
    pub(in crate::protocol) hotspot: Point,
    pub(in crate::protocol) outputs: Vec<crate::protocol::globals::output::OutputInstanceId>,
}

impl Default for CursorSurfaceState {
    fn default() -> Self {
        Self {
            hotspot: Point::default(),
            outputs: Vec::with_capacity(2),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SeatProtocol {
    _global: GlobalId,
    seats: Vec<Weak<WlSeat>>,
    keyboards: HashMap<ClientId, Vec<WlKeyboard>>,
    pointers: HashMap<ClientId, Vec<PointerResource>>,
    touches: HashMap<ClientId, Vec<WlTouch>>,
    keyboard_enabled: bool,
    pointer_enabled: bool,
    touch_enabled: bool,
    keymap: Option<Arc<KeymapFile>>,
    default_keymap: Option<Arc<KeymapFile>>,
    repeat_rate: i32,
    repeat_delay: i32,
    keyboard_focus: Option<ClientId>,
    keyboard_focus_surface: Option<Weak<WlSurface>>,
    keyboard_enter_serial: Option<Serial>,
    keyboard_modifiers: ModifiersState,
    pointer_focus: Option<ClientId>,
    pointer_focus_surface: Option<ObjectId>,
    pointer_enter_serial: Option<Serial>,
}

impl SeatProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, WlSeat, _>(9, SeatGlobalData::primary()),
            seats: Vec::new(),
            keyboards: HashMap::new(),
            pointers: HashMap::new(),
            touches: HashMap::new(),
            keyboard_enabled: false,
            pointer_enabled: false,
            touch_enabled: false,
            keymap: None,
            default_keymap: None,
            repeat_rate: 25,
            repeat_delay: 200,
            keyboard_focus: None,
            keyboard_focus_surface: None,
            keyboard_enter_serial: None,
            keyboard_modifiers: ModifiersState::default(),
            pointer_focus: None,
            pointer_focus_surface: None,
            pointer_enter_serial: None,
        }
    }

    fn capabilities(&self) -> wl_seat::Capability {
        let mut capabilities = wl_seat::Capability::empty();
        if self.pointer_enabled {
            capabilities |= wl_seat::Capability::Pointer;
        }
        if self.keyboard_enabled {
            capabilities |= wl_seat::Capability::Keyboard;
        }
        if self.touch_enabled {
            capabilities |= wl_seat::Capability::Touch;
        }
        capabilities
    }

    fn send_capabilities(&mut self) {
        let capabilities = self.capabilities();
        self.seats.retain(|seat| {
            let Ok(seat) = seat.upgrade() else {
                return false;
            };
            seat.capabilities(capabilities);
            true
        });
    }

    pub(crate) fn set_pointer_enabled(&mut self, enabled: bool) {
        if self.pointer_enabled != enabled {
            self.pointer_enabled = enabled;
            if !enabled {
                self.pointer_focus = None;
                self.pointer_focus_surface = None;
                self.pointer_enter_serial = None;
                for pointers in self.pointers.values_mut() {
                    for pointer in pointers {
                        pointer.reset_v120();
                    }
                }
            }
            self.send_capabilities();
        }
    }

    pub(crate) fn set_touch_enabled(&mut self, enabled: bool) {
        if self.touch_enabled != enabled {
            self.touch_enabled = enabled;
            self.send_capabilities();
        }
    }

    pub(super) fn insert_touch(&mut self, client: ClientId, touch: WlTouch) {
        self.touches.entry(client).or_default().push(touch);
    }

    pub(super) fn remove_touch(&mut self, client: &ClientId, touch: &WlTouch) {
        remove_resource(&mut self.touches, client, touch);
    }

    pub(crate) fn owns(&self, seat: &WlSeat) -> bool {
        self.owner(seat) == Some(SeatOwner::Primary)
    }

    pub(crate) fn owner(&self, seat: &WlSeat) -> Option<SeatOwner> {
        seat.data::<SeatData>().map(|data| data.owner)
    }

    pub(crate) fn owns_pointer(&self, pointer: &WlPointer) -> bool {
        self.pointer_enabled
            && pointer
                .data::<PointerData>()
                .is_some_and(|data| data.owner == SeatOwner::Primary)
    }

    pub(crate) const fn pointer_enter_serial(&self) -> Option<Serial> {
        self.pointer_enter_serial
    }

    pub(crate) const fn keyboard_enter_serial(&self) -> Option<Serial> {
        self.keyboard_enter_serial
    }

    pub(crate) fn pointer_may_set_cursor(
        &self,
        serial: Serial,
        pointer: &ObjectId,
        current_surface: bool,
    ) -> bool {
        self.pointer_enabled
            && self.pointer_received_enter(pointer, serial)
            && (current_surface
                || self
                    .pointer_focus_surface
                    .as_ref()
                    .is_some_and(|surface| surface.same_client_as(pointer)))
    }
}

fn remove_resource<R: Resource>(
    resources: &mut HashMap<ClientId, Vec<R>>,
    client: &ClientId,
    resource: &R,
) {
    let mut remove_client = false;
    if let Some(resources) = resources.get_mut(client) {
        if let Some(index) = resources
            .iter()
            .position(|candidate| candidate.id() == resource.id())
        {
            resources.swap_remove(index);
        }
        remove_client = resources.is_empty();
    }
    if remove_client {
        resources.remove(client);
    }
}
