//! Main-thread libinput owner driven by one-shot Compio fd completions.

use std::{
    collections::HashMap,
    io,
    os::fd::{AsFd, BorrowedFd, OwnedFd},
    path::Path,
};

use libinput::{
    AsRaw, DeviceCapability, Libinput, event,
    event::{
        EventTrait as _,
        gesture::{
            GestureEndEvent as _, GestureEventCoordinates as _, GestureEventTrait as _,
            GesturePinchEventTrait as _,
        },
        keyboard::KeyboardEventTrait as _,
        pointer::{PointerEventTrait as _, PointerScrollEvent as _},
        tablet_pad::{RingAxisSource, StripAxisSource, TabletPadEventTrait as _},
        tablet_tool::{
            ProximityState, TabletToolEventTrait as _, TabletToolType as LibinputToolType, TipState,
        },
    },
};
use rustix::fs::OFlags;
use tensor_event::{
    AbsoluteMotionEvent, AxisDirection, AxisSource, BackendInputEvent, DeviceCapabilities,
    DeviceChange, DeviceEvent, DeviceGroupId, DeviceId, KeyboardEvent, PointerAxisEvent,
    PointerButtonEvent, PointerGestureEvent, RelativeMotionEvent, TabletPadDescriptor,
    TabletPadEvent, TabletPadGroupDescriptor, TabletPadRingEvent, TabletPadStripEvent,
    TabletToolAxesEvent, TabletToolButtonEvent, TabletToolCapabilities, TabletToolDescriptor,
    TabletToolId, TabletToolProximityEvent, TabletToolTipEvent, TabletToolType,
};

mod raw;
mod tablet;

use tablet::{PendingEvents, ToolRegistry};

use super::session::SeatSession;

const MAX_EVENTS_PER_COMPLETION: usize = 256;
const MAX_PAD_GROUPS: usize = 8;
const MAX_PAD_BUTTONS: usize = 64;
const MAX_PAD_AXES: usize = 16;

#[derive(Debug)]
struct LibinputSessionInterface(SeatSession);

impl From<SeatSession> for LibinputSessionInterface {
    fn from(session: SeatSession) -> Self {
        Self(session)
    }
}

impl libinput::LibinputInterface for LibinputSessionInterface {
    fn open_restricted(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
        self.0
            .open(path, OFlags::from_bits_truncate(flags as u32))
            .map_err(|error| error.errno())
    }

    fn close_restricted(&mut self, fd: OwnedFd) {
        let _ = self.0.close(fd);
    }
}

#[derive(Debug)]
pub(crate) enum LibinputEvent {
    Device(DeviceEvent),
    Input(BackendInputEvent),
}

pub(super) struct LibinputSource {
    context: Libinput,
    device_ids: HashMap<usize, DeviceId>,
    device_groups: HashMap<usize, (DeviceGroupId, usize)>,
    next_device_id: u64,
    next_device_group_id: u64,
    dropped_events: u64,
    events_in_completion: usize,
    dropped_in_completion: u64,
    pending: PendingEvents,
    tools: ToolRegistry,
}

impl LibinputSource {
    pub(super) fn new(session: SeatSession, seat: &str, active: bool) -> Result<Self, ()> {
        let mut context = Libinput::new_with_udev(LibinputSessionInterface::from(session));
        context.udev_assign_seat(seat)?;
        if !active {
            context.suspend();
        }
        Ok(Self {
            context,
            device_ids: HashMap::new(),
            device_groups: HashMap::new(),
            next_device_id: 1,
            next_device_group_id: 1,
            dropped_events: 0,
            events_in_completion: 0,
            dropped_in_completion: 0,
            pending: PendingEvents::new(),
            tools: ToolRegistry::new(),
        })
    }

    pub(super) fn suspend(&mut self) {
        self.context.suspend();
    }

    pub(super) fn resume(&mut self) -> Result<(), ()> {
        self.context.resume()
    }

    pub(super) fn begin_drain(&mut self) -> io::Result<()> {
        self.context.dispatch()?;
        self.events_in_completion = 0;
        self.dropped_in_completion = 0;
        Ok(())
    }

    pub(super) fn next_event(&mut self) -> Option<LibinputEvent> {
        loop {
            if let Some(event) = self.pop_pending() {
                return self.accept_event(LibinputEvent::Input(event));
            }
            let Some(event) = self.next_mapped_event() else {
                self.report_dropped_events();
                return None;
            };
            if let Some(event) = self.accept_event(event) {
                return Some(event);
            }
        }
    }

    fn accept_event(&mut self, event: LibinputEvent) -> Option<LibinputEvent> {
        if self.events_in_completion < MAX_EVENTS_PER_COMPLETION {
            self.events_in_completion += 1;
            Some(event)
        } else {
            self.dropped_in_completion = self.dropped_in_completion.saturating_add(1);
            None
        }
    }

    fn next_mapped_event(&mut self) -> Option<LibinputEvent> {
        loop {
            match raw::next_event(&mut self.context)? {
                raw::Event::Standard(event) => {
                    if let Some(event) = self.map_event(event) {
                        return Some(event);
                    }
                }
                raw::Event::Dial(event) => {
                    if let Some(event) = self.map_pad_dial(event) {
                        return Some(event);
                    }
                }
            }
        }
    }

    fn map_event(&mut self, event: libinput::Event) -> Option<LibinputEvent> {
        match event {
            libinput::Event::Device(event) => match event {
                event::DeviceEvent::Added(event) => {
                    self.map_device(event.device(), DeviceChange::Added)
                }
                event::DeviceEvent::Removed(event) => {
                    self.map_device(event.device(), DeviceChange::Removed)
                }
                _ => None,
            },
            libinput::Event::Touch(
                event::TouchEvent::Down(_)
                | event::TouchEvent::Motion(_)
                | event::TouchEvent::Up(_),
            ) => Some(LibinputEvent::Input(BackendInputEvent::Activity)),
            libinput::Event::Touch(_) => None,
            libinput::Event::Keyboard(event::KeyboardEvent::Key(event)) => {
                let pressed = event.key_state() == event::keyboard::KeyState::Pressed;
                Some(LibinputEvent::Input(BackendInputEvent::Keyboard(
                    KeyboardEvent {
                        key: event.key(),
                        pressed,
                        time_ns: micros_to_nanos(event.time_usec()),
                    },
                )))
            }
            libinput::Event::Keyboard(_) => None,
            libinput::Event::Pointer(event) => self.map_pointer_event(event),
            libinput::Event::Gesture(event) => map_gesture_event(event),
            libinput::Event::Tablet(event) => self.map_tablet_tool_event(event),
            libinput::Event::TabletPad(event) => self.map_tablet_pad_event(event),
            _ => None,
        }
    }

    fn map_device(
        &mut self,
        device: libinput::Device,
        change: DeviceChange,
    ) -> Option<LibinputEvent> {
        let raw = device.as_raw() as usize;
        let raw_group = device.device_group().as_raw() as usize;
        let group = match change {
            DeviceChange::Added => {
                if let Some((id, members)) = self.device_groups.get_mut(&raw_group) {
                    *members = members.saturating_add(1);
                    *id
                } else {
                    let id = DeviceGroupId::new(self.next_device_group_id);
                    self.next_device_group_id =
                        self.next_device_group_id.checked_add(1).or_else(|| {
                            tracing::error!("libinput device-group identity space exhausted");
                            None
                        })?;
                    self.device_groups.insert(raw_group, (id, 1));
                    id
                }
            }
            DeviceChange::Removed => {
                let (id, remove) = self
                    .device_groups
                    .get_mut(&raw_group)
                    .map(|(id, members)| {
                        *members = members.saturating_sub(1);
                        (*id, *members == 0)
                    })
                    .unwrap_or_else(|| {
                        tracing::warn!("removed device from unknown libinput group");
                        (DeviceGroupId::new(0), false)
                    });
                if remove {
                    self.device_groups.remove(&raw_group);
                }
                id
            }
        };
        let id = match change {
            DeviceChange::Added => {
                if let Some(id) = self.device_ids.get(&raw).copied() {
                    id
                } else {
                    let id = DeviceId::new(self.next_device_id);
                    self.next_device_id = self.next_device_id.checked_add(1).or_else(|| {
                        tracing::error!("libinput device identity space exhausted");
                        None
                    })?;
                    self.device_ids.insert(raw, id);
                    id
                }
            }
            DeviceChange::Removed => {
                let id = self.device_ids.remove(&raw).unwrap_or_else(|| {
                    tracing::warn!(device = %device.sysname(), "removed unknown libinput device");
                    DeviceId::new(0)
                });
                self.tools.remove_device(id);
                id
            }
        };
        let capabilities = DeviceCapabilities {
            keyboard: device.has_capability(DeviceCapability::Keyboard),
            pointer: device.has_capability(DeviceCapability::Pointer),
            touch: device.has_capability(DeviceCapability::Touch),
            tablet: device.has_capability(DeviceCapability::TabletTool)
                || device.has_capability(DeviceCapability::TabletPad),
        };
        if change == DeviceChange::Added
            && device.has_capability(DeviceCapability::TabletPad)
            && !self.enqueue_pad_description(&device, id)
        {
            tracing::warn!(
                device = id.get(),
                "tablet pad topology exceeds fixed limits"
            );
        }
        Some(LibinputEvent::Device(DeviceEvent {
            id,
            group,
            bus_type: device.id_bustype(),
            vendor_id: device.id_vendor(),
            product_id: device.id_product(),
            capabilities,
            change,
        }))
    }

    fn enqueue(&mut self, event: BackendInputEvent) -> bool {
        self.pending.push(event)
    }

    fn pop_pending(&mut self) -> Option<BackendInputEvent> {
        self.pending.pop()
    }

    fn enqueue_pad_description(&mut self, device: &libinput::Device, id: DeviceId) -> bool {
        let counts = (
            usize::try_from(device.tablet_pad_number_of_buttons()).ok(),
            usize::try_from(device.tablet_pad_number_of_rings()).ok(),
            usize::try_from(device.tablet_pad_number_of_strips()).ok(),
            raw::pad_dial_count(device).map(|count| count as usize),
            usize::try_from(device.tablet_pad_number_of_mode_groups()).ok(),
        );
        let (Some(buttons), Some(rings), Some(strips), Some(dials), Some(groups)) = counts else {
            return false;
        };
        if buttons > MAX_PAD_BUTTONS
            || rings > MAX_PAD_AXES
            || strips > MAX_PAD_AXES
            || dials > MAX_PAD_AXES
            || groups == 0
            || groups > MAX_PAD_GROUPS
        {
            return false;
        }
        let mut descriptions = [None; MAX_PAD_GROUPS];
        for (index, description) in descriptions.iter_mut().enumerate().take(groups) {
            let Some(group) = device.tablet_pad_mode_group(index as u32) else {
                return false;
            };
            let mut button_mask = 0_u64;
            let mut ring_mask = 0_u16;
            let mut strip_mask = 0_u16;
            let mut dial_mask = 0_u16;
            for button in 0..buttons {
                button_mask |= u64::from(group.has_button(button as u32)) << button;
            }
            for ring in 0..rings {
                ring_mask |= u16::from(group.has_ring(ring as u32)) << ring;
            }
            for strip in 0..strips {
                strip_mask |= u16::from(group.has_strip(strip as u32)) << strip;
            }
            for dial in 0..dials {
                dial_mask |= u16::from(group.has_dial(dial as u32)) << dial;
            }
            *description = Some(TabletPadGroupDescriptor {
                device: id,
                index: index as u8,
                modes: group.number_of_modes().min(u32::from(u8::MAX)) as u8,
                current_mode: group.mode().min(u32::from(u8::MAX)) as u8,
                buttons: button_mask,
                rings: ring_mask,
                strips: strip_mask,
                dials: dial_mask,
                final_group: index + 1 == groups,
            });
        }
        if !self.pending.can_push(groups + 1) {
            return false;
        }
        let added = BackendInputEvent::TabletPad(TabletPadEvent::Added(TabletPadDescriptor {
            device: id,
            buttons: buttons as u8,
            rings: rings as u8,
            strips: strips as u8,
            dials: dials as u8,
            groups: groups as u8,
        }));
        assert!(self.enqueue(added), "preflighted tablet queue capacity");
        for descriptor in descriptions.into_iter().take(groups).flatten() {
            assert!(
                self.enqueue(BackendInputEvent::TabletPad(TabletPadEvent::Group(
                    descriptor
                ))),
                "preflighted tablet queue capacity"
            );
        }
        true
    }

    fn map_pointer_event(&self, event: event::PointerEvent) -> Option<LibinputEvent> {
        let event = match event {
            event::PointerEvent::Motion(event) => {
                BackendInputEvent::PointerMotion(RelativeMotionEvent {
                    delta_x: event.dx(),
                    delta_y: event.dy(),
                    unaccelerated_x: event.dx_unaccelerated(),
                    unaccelerated_y: event.dy_unaccelerated(),
                    time_ns: micros_to_nanos(event.time_usec()),
                })
            }
            event::PointerEvent::MotionAbsolute(event) => {
                BackendInputEvent::PointerMotionAbsolute(AbsoluteMotionEvent {
                    x: event.absolute_x_transformed(1),
                    y: event.absolute_y_transformed(1),
                    time_ns: micros_to_nanos(event.time_usec()),
                })
            }
            event::PointerEvent::Button(event) => {
                BackendInputEvent::PointerButton(PointerButtonEvent {
                    button: event.button(),
                    pressed: event.button_state() == event::pointer::ButtonState::Pressed,
                    time_ns: micros_to_nanos(event.time_usec()),
                })
            }
            event::PointerEvent::ScrollWheel(event) => {
                let horizontal_v120 = event.has_axis(event::pointer::Axis::Horizontal).then(|| {
                    event
                        .scroll_value_v120(event::pointer::Axis::Horizontal)
                        .round() as i32
                });
                let vertical_v120 = event.has_axis(event::pointer::Axis::Vertical).then(|| {
                    event
                        .scroll_value_v120(event::pointer::Axis::Vertical)
                        .round() as i32
                });
                BackendInputEvent::PointerAxis(map_axis_event(
                    &event,
                    horizontal_v120,
                    vertical_v120,
                    AxisSource::Wheel,
                ))
            }
            event::PointerEvent::ScrollFinger(event) => BackendInputEvent::PointerAxis(
                map_axis_event(&event, None, None, AxisSource::Finger),
            ),
            event::PointerEvent::ScrollContinuous(event) => BackendInputEvent::PointerAxis(
                map_axis_event(&event, None, None, AxisSource::Continuous),
            ),
            _ => return None,
        };
        Some(LibinputEvent::Input(event))
    }

    fn map_tablet_tool_event(&mut self, event: event::TabletToolEvent) -> Option<LibinputEvent> {
        if !self.pending.can_push(2) {
            tracing::warn!("tablet event expansion exceeds fixed pending capacity");
            return None;
        }
        let device = self.device_id(&event)?;
        let tool = event.tool();
        let tool_type = map_tool_type(tool.tool_type()?)?;
        let capabilities = map_tool_capabilities(&tool);
        let raw_tool = tool.as_raw() as usize;
        let (id, added) = self.tools.id_for(raw_tool, device)?;
        let descriptor = if added {
            Some(TabletToolDescriptor {
                id,
                device,
                hardware_serial: tool.serial(),
                hardware_id: tool.tool_id(),
                tool_type,
                capabilities,
            })
        } else {
            None
        };
        let mut queued_after_descriptor = false;
        let mapped = match event {
            event::TabletToolEvent::Proximity(event) => {
                let in_proximity = event.proximity_state() == ProximityState::In;
                let proximity = BackendInputEvent::TabletToolProximity(TabletToolProximityEvent {
                    id,
                    device,
                    x: finite_f32(event.x_transformed(1))?,
                    y: finite_f32(event.y_transformed(1))?,
                    in_proximity,
                    time_ns: micros_to_nanos(event.time_usec()),
                });
                if in_proximity {
                    let axes = map_tool_axes(id, &event, true);
                    if descriptor.is_some() {
                        assert!(self.enqueue(proximity), "preflighted tablet queue capacity");
                        assert!(
                            self.enqueue(BackendInputEvent::TabletToolAxes(axes)),
                            "preflighted tablet queue capacity"
                        );
                        queued_after_descriptor = true;
                    } else {
                        assert!(
                            self.enqueue(BackendInputEvent::TabletToolAxes(axes)),
                            "preflighted tablet queue capacity"
                        );
                    }
                } else if descriptor.is_some() {
                    assert!(self.enqueue(proximity), "preflighted tablet queue capacity");
                    queued_after_descriptor = true;
                }
                proximity
            }
            event::TabletToolEvent::Axis(event) => {
                BackendInputEvent::TabletToolAxes(map_tool_axes(id, &event, true))
            }
            event::TabletToolEvent::Tip(event) => {
                let tip = BackendInputEvent::TabletToolTip(TabletToolTipEvent {
                    id,
                    down: event.tip_state() == TipState::Down,
                    time_ns: micros_to_nanos(event.time_usec()),
                });
                let axes = map_tool_axes(id, &event, false);
                if axes.has_axes() {
                    let axes = BackendInputEvent::TabletToolAxes(axes);
                    if descriptor.is_some() {
                        assert!(self.enqueue(axes), "preflighted tablet queue capacity");
                        assert!(self.enqueue(tip), "preflighted tablet queue capacity");
                        queued_after_descriptor = true;
                    } else {
                        assert!(self.enqueue(tip), "preflighted tablet queue capacity");
                    }
                    axes
                } else {
                    tip
                }
            }
            event::TabletToolEvent::Button(event) => {
                BackendInputEvent::TabletToolButton(TabletToolButtonEvent {
                    id,
                    button: event.button(),
                    pressed: event.button_state() == event::pointer::ButtonState::Pressed,
                    time_ns: micros_to_nanos(event.time_usec()),
                })
            }
            _ => return None,
        };
        let mapped = if let Some(descriptor) = descriptor {
            if !queued_after_descriptor {
                assert!(self.enqueue(mapped), "preflighted tablet queue capacity");
            }
            BackendInputEvent::TabletToolAdded(descriptor)
        } else {
            mapped
        };
        Some(LibinputEvent::Input(mapped))
    }

    fn map_tablet_pad_event(&self, event: event::TabletPadEvent) -> Option<LibinputEvent> {
        let device = self.device_id(&event)?;
        let event = match event {
            event::TabletPadEvent::Button(event) => TabletPadEvent::Button {
                device,
                button: u8::try_from(event.button_number()).ok()?,
                mode_group: u8::try_from(event.mode_group().index()).ok()?,
                mode: u8::try_from(event.mode()).ok()?,
                pressed: event.button_state() == event::pointer::ButtonState::Pressed,
                time_ns: micros_to_nanos(event.time_usec()),
            },
            event::TabletPadEvent::Ring(event) => TabletPadEvent::Ring(TabletPadRingEvent {
                device,
                index: u8::try_from(event.number()).ok()?,
                mode_group: u8::try_from(event.mode_group().index()).ok()?,
                mode: u8::try_from(event.mode()).ok()?,
                position: (event.position() >= 0.0)
                    .then(|| finite_f32(event.position()))
                    .flatten(),
                finger: event.source() == RingAxisSource::Finger,
                time_ns: micros_to_nanos(event.time_usec()),
            }),
            event::TabletPadEvent::Strip(event) => TabletPadEvent::Strip(TabletPadStripEvent {
                device,
                index: u8::try_from(event.number()).ok()?,
                mode_group: u8::try_from(event.mode_group().index()).ok()?,
                mode: u8::try_from(event.mode()).ok()?,
                position: (event.position() >= 0.0)
                    .then(|| finite_f32(event.position()))
                    .flatten(),
                finger: event.source() == StripAxisSource::Finger,
                time_ns: micros_to_nanos(event.time_usec()),
            }),
            event::TabletPadEvent::Dial(event) => TabletPadEvent::Dial {
                device,
                index: u8::try_from(event.number()).ok()?,
                mode_group: u8::try_from(event.mode_group().index()).ok()?,
                mode: u8::try_from(event.mode()).ok()?,
                delta_v120: finite_i32(event.dial_v120())?,
                time_ns: micros_to_nanos(event.time_usec()),
            },
            event::TabletPadEvent::Key(_) => return None,
            _ => return None,
        };
        Some(LibinputEvent::Input(BackendInputEvent::TabletPad(event)))
    }

    fn map_pad_dial(&self, raw: raw::DialEvent) -> Option<LibinputEvent> {
        let device = self.device_ids.get(&raw.device_raw).copied()?;
        let event = TabletPadEvent::Dial {
            device,
            index: u8::try_from(raw.index).ok()?,
            mode_group: u8::try_from(raw.mode_group).ok()?,
            mode: u8::try_from(raw.mode).ok()?,
            delta_v120: finite_i32(raw.delta_v120)?,
            time_ns: micros_to_nanos(raw.time_usec),
        };
        Some(LibinputEvent::Input(BackendInputEvent::TabletPad(event)))
    }

    fn device_id(&self, event: &impl event::EventTrait) -> Option<DeviceId> {
        self.device_ids
            .get(&(event.device().as_raw() as usize))
            .copied()
    }

    fn report_dropped_events(&mut self) {
        if self.dropped_in_completion > 0 {
            self.dropped_events = self
                .dropped_events
                .saturating_add(self.dropped_in_completion);
            tracing::warn!(
                dropped = self.dropped_in_completion,
                dropped_total = self.dropped_events,
                "libinput completion batch exceeded its fixed capacity"
            );
            self.dropped_in_completion = 0;
        }
    }
}

fn map_tool_axes(
    id: TabletToolId,
    event: &impl event::tablet_tool::TabletToolEventTrait,
    final_frame: bool,
) -> TabletToolAxesEvent {
    let changed = |present: bool, value: f64| present.then(|| finite_f32(value)).flatten();
    TabletToolAxesEvent::new(
        id,
        micros_to_nanos(event.time_usec()),
        changed(event.x_has_changed(), event.x_transformed(1)),
        changed(event.y_has_changed(), event.y_transformed(1)),
        changed(event.pressure_has_changed(), event.pressure()),
        changed(event.distance_has_changed(), event.distance()),
        changed(event.tilt_x_has_changed(), event.tilt_x()),
        changed(event.tilt_y_has_changed(), event.tilt_y()),
        changed(event.rotation_has_changed(), event.rotation()),
        changed(event.slider_has_changed(), event.slider_position()),
        event.wheel_has_changed().then(|| {
            (
                finite_f32(event.wheel_delta()).unwrap_or_default(),
                event
                    .wheel_delta_discrete()
                    .round()
                    .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16,
            )
        }),
        final_frame,
    )
}

fn map_tool_type(tool_type: LibinputToolType) -> Option<TabletToolType> {
    Some(match tool_type {
        LibinputToolType::Pen => TabletToolType::Pen,
        LibinputToolType::Eraser => TabletToolType::Eraser,
        LibinputToolType::Brush => TabletToolType::Brush,
        LibinputToolType::Pencil => TabletToolType::Pencil,
        LibinputToolType::Airbrush => TabletToolType::Airbrush,
        LibinputToolType::Mouse => TabletToolType::Mouse,
        LibinputToolType::Lens => TabletToolType::Lens,
        _ => return None,
    })
}

fn map_tool_capabilities(tool: &event::tablet_tool::TabletTool) -> TabletToolCapabilities {
    let mut bits = 0;
    bits |= u8::from(tool.has_tilt()) * TabletToolCapabilities::TILT;
    bits |= u8::from(tool.has_pressure()) * TabletToolCapabilities::PRESSURE;
    bits |= u8::from(tool.has_distance()) * TabletToolCapabilities::DISTANCE;
    bits |= u8::from(tool.has_rotation()) * TabletToolCapabilities::ROTATION;
    bits |= u8::from(tool.has_slider()) * TabletToolCapabilities::SLIDER;
    bits |= u8::from(tool.has_wheel()) * TabletToolCapabilities::WHEEL;
    TabletToolCapabilities::from_bits(bits)
}

fn finite_f32(value: f64) -> Option<f32> {
    value.is_finite().then_some(value as f32)
}

fn finite_i32(value: f64) -> Option<i32> {
    value.is_finite().then(|| {
        value
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
    })
}

fn map_gesture_event(event: event::GestureEvent) -> Option<LibinputEvent> {
    let event = match event {
        event::GestureEvent::Swipe(event) => match event {
            event::gesture::GestureSwipeEvent::Begin(event) => PointerGestureEvent::SwipeBegin {
                fingers: gesture_fingers(&event)?,
                time_ns: micros_to_nanos(event.time_usec()),
            },
            event::gesture::GestureSwipeEvent::Update(event) => PointerGestureEvent::SwipeUpdate {
                delta_x: event.dx(),
                delta_y: event.dy(),
                time_ns: micros_to_nanos(event.time_usec()),
            },
            event::gesture::GestureSwipeEvent::End(event) => PointerGestureEvent::SwipeEnd {
                cancelled: event.cancelled(),
                time_ns: micros_to_nanos(event.time_usec()),
            },
            _ => return None,
        },
        event::GestureEvent::Pinch(event) => match event {
            event::gesture::GesturePinchEvent::Begin(event) => PointerGestureEvent::PinchBegin {
                fingers: gesture_fingers(&event)?,
                time_ns: micros_to_nanos(event.time_usec()),
            },
            event::gesture::GesturePinchEvent::Update(event) => PointerGestureEvent::PinchUpdate {
                delta_x: event.dx(),
                delta_y: event.dy(),
                scale: event.scale(),
                rotation: event.angle_delta(),
                time_ns: micros_to_nanos(event.time_usec()),
            },
            event::gesture::GesturePinchEvent::End(event) => PointerGestureEvent::PinchEnd {
                cancelled: event.cancelled(),
                time_ns: micros_to_nanos(event.time_usec()),
            },
            _ => return None,
        },
        event::GestureEvent::Hold(event) => match event {
            event::gesture::GestureHoldEvent::Begin(event) => PointerGestureEvent::HoldBegin {
                fingers: gesture_fingers(&event)?,
                time_ns: micros_to_nanos(event.time_usec()),
            },
            event::gesture::GestureHoldEvent::End(event) => PointerGestureEvent::HoldEnd {
                cancelled: event.cancelled(),
                time_ns: micros_to_nanos(event.time_usec()),
            },
            _ => return None,
        },
        _ => return None,
    };
    Some(LibinputEvent::Input(BackendInputEvent::PointerGesture(
        event,
    )))
}

fn gesture_fingers(event: &impl event::gesture::GestureEventTrait) -> Option<u32> {
    u32::try_from(event.finger_count())
        .ok()
        .filter(|fingers| *fingers != 0)
}

impl AsFd for LibinputSource {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.context.as_fd()
    }
}

#[inline]
fn micros_to_nanos(time_usec: u64) -> u64 {
    time_usec.saturating_mul(1_000)
}

fn map_axis_event<E>(
    event: &E,
    horizontal_v120: Option<i32>,
    vertical_v120: Option<i32>,
    source: AxisSource,
) -> PointerAxisEvent
where
    E: event::EventTrait + event::pointer::PointerEventTrait + event::pointer::PointerScrollEvent,
{
    let horizontal = event
        .has_axis(event::pointer::Axis::Horizontal)
        .then(|| event.scroll_value(event::pointer::Axis::Horizontal));
    let vertical = event
        .has_axis(event::pointer::Axis::Vertical)
        .then(|| event.scroll_value(event::pointer::Axis::Vertical));
    let direction = if event.device().config_scroll_natural_scroll_enabled() {
        AxisDirection::Inverted
    } else {
        AxisDirection::Identical
    };
    PointerAxisEvent::new(
        horizontal,
        vertical,
        horizontal_v120,
        vertical_v120,
        micros_to_nanos(event.time_usec()),
        source,
        direction,
        direction,
    )
    .with_stops(
        source == AxisSource::Finger && horizontal == Some(0.0),
        source == AxisSource::Finger && vertical == Some(0.0),
    )
}
