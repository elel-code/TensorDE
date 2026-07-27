//! Main-thread libinput owner driven by one-shot Compio fd completions.

use std::{
    collections::HashMap,
    io,
    os::fd::{AsFd, BorrowedFd, OwnedFd},
    path::Path,
};

use input::{
    AsRaw, DeviceCapability, Libinput, event,
    event::{
        EventTrait as _,
        keyboard::KeyboardEventTrait as _,
        pointer::{PointerEventTrait as _, PointerScrollEvent as _},
    },
};
use rustix::fs::OFlags;
use tensor_host::AxisSource;
use tensor_input::{
    AbsoluteMotionEvent, AxisDirection, BackendInputEvent, DeviceCapabilities, DeviceChange,
    DeviceEvent, DeviceId, KeyboardEvent, PointerAxisEvent, PointerButtonEvent,
    RelativeMotionEvent,
};

use super::session::SeatSession;

const MAX_EVENTS_PER_COMPLETION: usize = 256;

#[derive(Debug)]
struct LibinputSessionInterface(SeatSession);

impl From<SeatSession> for LibinputSessionInterface {
    fn from(session: SeatSession) -> Self {
        Self(session)
    }
}

impl input::LibinputInterface for LibinputSessionInterface {
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
    Device {
        event: DeviceEvent,
        /// The protocol tablet adapter still needs the libinput device object.
        tablet: Option<input::Device>,
    },
    Input(BackendInputEvent),
    Tablet {
        device: DeviceId,
        event: event::TabletToolEvent,
    },
}

pub(super) struct LibinputSource {
    context: Libinput,
    device_ids: HashMap<usize, DeviceId>,
    next_device_id: u64,
    dropped_events: u64,
    events_in_completion: usize,
    dropped_in_completion: u64,
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
            next_device_id: 1,
            dropped_events: 0,
            events_in_completion: 0,
            dropped_in_completion: 0,
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
            let Some(event) = self.context.next() else {
                self.report_dropped_events();
                return None;
            };
            let Some(event) = self.map_event(event) else {
                continue;
            };
            if self.events_in_completion < MAX_EVENTS_PER_COMPLETION {
                self.events_in_completion += 1;
                return Some(event);
            } else {
                self.dropped_in_completion = self.dropped_in_completion.saturating_add(1);
            }
        }
    }

    fn map_event(&mut self, event: input::Event) -> Option<LibinputEvent> {
        match event {
            input::Event::Device(event) => match event {
                event::DeviceEvent::Added(event) => {
                    self.map_device(event.device(), DeviceChange::Added)
                }
                event::DeviceEvent::Removed(event) => {
                    self.map_device(event.device(), DeviceChange::Removed)
                }
                _ => None,
            },
            input::Event::Touch(
                event::TouchEvent::Down(_)
                | event::TouchEvent::Motion(_)
                | event::TouchEvent::Up(_),
            ) => Some(LibinputEvent::Input(BackendInputEvent::Activity)),
            input::Event::Touch(_) => None,
            input::Event::Keyboard(event::KeyboardEvent::Key(event)) => {
                let pressed = event.key_state() == event::keyboard::KeyState::Pressed;
                Some(LibinputEvent::Input(BackendInputEvent::Keyboard(
                    KeyboardEvent {
                        key: event.key(),
                        pressed,
                        time_ns: micros_to_nanos(event.time_usec()),
                    },
                )))
            }
            input::Event::Keyboard(_) => None,
            input::Event::Pointer(event) => self.map_pointer_event(event),
            input::Event::Tablet(event) => {
                let device = event.device();
                let raw = device.as_raw() as usize;
                let Some(device) = self.device_ids.get(&raw).copied() else {
                    tracing::warn!(
                        device = %device.sysname(),
                        "ignored tablet event from an unknown libinput device"
                    );
                    return None;
                };
                Some(LibinputEvent::Tablet { device, event })
            }
            _ => None,
        }
    }

    fn map_device(&mut self, device: input::Device, change: DeviceChange) -> Option<LibinputEvent> {
        let raw = device.as_raw() as usize;
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
            DeviceChange::Removed => self.device_ids.remove(&raw).unwrap_or_else(|| {
                tracing::warn!(device = %device.sysname(), "removed unknown libinput device");
                DeviceId::new(0)
            }),
        };
        let capabilities = DeviceCapabilities {
            keyboard: device.has_capability(DeviceCapability::Keyboard),
            pointer: device.has_capability(DeviceCapability::Pointer),
            touch: device.has_capability(DeviceCapability::Touch),
            tablet: device.has_capability(DeviceCapability::TabletTool),
        };
        let tablet = capabilities.tablet.then_some(device);
        Some(LibinputEvent::Device {
            event: DeviceEvent {
                id,
                capabilities,
                change,
            },
            tablet,
        })
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
