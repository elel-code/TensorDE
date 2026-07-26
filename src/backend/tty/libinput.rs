//! Main-thread libinput owner driven by one-shot Compio fd completions.

use std::{
    io,
    os::fd::{AsFd, BorrowedFd},
};

use input::{Libinput, event};
use smithay::backend::{
    input::InputEvent as SmithayInputEvent,
    libinput::{LibinputInputBackend, LibinputSessionInterface, PointerScrollAxis},
};

use super::session::SeatSession;

const MAX_EVENTS_PER_COMPLETION: usize = 256;

pub(super) type LibinputEvent = SmithayInputEvent<LibinputInputBackend>;

pub(super) struct LibinputSource {
    context: Libinput,
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
        for event in &mut self.context {
            let Some(event) = map_event(event) else {
                continue;
            };
            if self.events_in_completion < MAX_EVENTS_PER_COMPLETION {
                self.events_in_completion += 1;
                return Some(event);
            } else {
                self.dropped_in_completion = self.dropped_in_completion.saturating_add(1);
            }
        }
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
        None
    }
}

impl AsFd for LibinputSource {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.context.as_fd()
    }
}

fn map_event(event: input::Event) -> Option<LibinputEvent> {
    use event::EventTrait as _;

    match event {
        input::Event::Device(event) => match event {
            event::DeviceEvent::Added(event) => Some(SmithayInputEvent::DeviceAdded {
                device: event.device(),
            }),
            event::DeviceEvent::Removed(event) => Some(SmithayInputEvent::DeviceRemoved {
                device: event.device(),
            }),
            _ => None,
        },
        input::Event::Touch(event) => match event {
            event::TouchEvent::Down(event) => Some(SmithayInputEvent::TouchDown { event }),
            event::TouchEvent::Motion(event) => Some(SmithayInputEvent::TouchMotion { event }),
            event::TouchEvent::Up(event) => Some(SmithayInputEvent::TouchUp { event }),
            event::TouchEvent::Cancel(event) => Some(SmithayInputEvent::TouchCancel { event }),
            event::TouchEvent::Frame(event) => Some(SmithayInputEvent::TouchFrame { event }),
            _ => None,
        },
        input::Event::Keyboard(event::KeyboardEvent::Key(event)) => {
            Some(SmithayInputEvent::Keyboard { event })
        }
        input::Event::Keyboard(_) => None,
        input::Event::Pointer(event) => match event {
            event::PointerEvent::Motion(event) => Some(SmithayInputEvent::PointerMotion { event }),
            event::PointerEvent::MotionAbsolute(event) => {
                Some(SmithayInputEvent::PointerMotionAbsolute { event })
            }
            event::PointerEvent::ScrollWheel(event) => Some(SmithayInputEvent::PointerAxis {
                event: PointerScrollAxis::Wheel(event),
            }),
            event::PointerEvent::ScrollFinger(event) => Some(SmithayInputEvent::PointerAxis {
                event: PointerScrollAxis::Finger(event),
            }),
            event::PointerEvent::ScrollContinuous(event) => Some(SmithayInputEvent::PointerAxis {
                event: PointerScrollAxis::Continuous(event),
            }),
            event::PointerEvent::Button(event) => Some(SmithayInputEvent::PointerButton { event }),
            _ => None,
        },
        input::Event::Gesture(event) => match event {
            event::GestureEvent::Swipe(event::gesture::GestureSwipeEvent::Begin(event)) => {
                Some(SmithayInputEvent::GestureSwipeBegin { event })
            }
            event::GestureEvent::Swipe(event::gesture::GestureSwipeEvent::Update(event)) => {
                Some(SmithayInputEvent::GestureSwipeUpdate { event })
            }
            event::GestureEvent::Swipe(event::gesture::GestureSwipeEvent::End(event)) => {
                Some(SmithayInputEvent::GestureSwipeEnd { event })
            }
            event::GestureEvent::Pinch(event::gesture::GesturePinchEvent::Begin(event)) => {
                Some(SmithayInputEvent::GesturePinchBegin { event })
            }
            event::GestureEvent::Pinch(event::gesture::GesturePinchEvent::Update(event)) => {
                Some(SmithayInputEvent::GesturePinchUpdate { event })
            }
            event::GestureEvent::Pinch(event::gesture::GesturePinchEvent::End(event)) => {
                Some(SmithayInputEvent::GesturePinchEnd { event })
            }
            event::GestureEvent::Hold(event::gesture::GestureHoldEvent::Begin(event)) => {
                Some(SmithayInputEvent::GestureHoldBegin { event })
            }
            event::GestureEvent::Hold(event::gesture::GestureHoldEvent::End(event)) => {
                Some(SmithayInputEvent::GestureHoldEnd { event })
            }
            _ => None,
        },
        input::Event::Tablet(event) => match event {
            event::TabletToolEvent::Axis(event) => {
                Some(SmithayInputEvent::TabletToolAxis { event })
            }
            event::TabletToolEvent::Proximity(event) => {
                Some(SmithayInputEvent::TabletToolProximity { event })
            }
            event::TabletToolEvent::Tip(event) => Some(SmithayInputEvent::TabletToolTip { event }),
            event::TabletToolEvent::Button(event) => {
                Some(SmithayInputEvent::TabletToolButton { event })
            }
            _ => None,
        },
        input::Event::Switch(event::SwitchEvent::Toggle(event)) => {
            Some(SmithayInputEvent::SwitchToggle { event })
        }
        input::Event::Switch(_) => None,
        _ => None,
    }
}
