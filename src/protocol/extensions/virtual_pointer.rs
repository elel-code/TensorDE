use std::collections::HashSet;
use std::sync::Mutex;

use tracing::warn;

use tensor_event::{
    AbsoluteMotionEvent, AxisDirection, AxisSource, PointerAxisEvent, PointerButtonEvent,
    RelativeMotionEvent,
};
use wayland_protocols_wlr::virtual_pointer::v1::server::{
    zwlr_virtual_pointer_manager_v1, zwlr_virtual_pointer_v1,
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, New, Resource, WEnum,
    backend::ClientId,
    protocol::{wl_output::WlOutput, wl_pointer, wl_seat::WlSeat},
};
use zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1;
use zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1;

use crate::protocol::dispatch::{
    DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
};
use crate::protocol::state::RuntimeState;

const VERSION: u32 = 2;

pub struct VirtualPointerManagerState {
    virtual_pointers: HashSet<ZwlrVirtualPointerV1>,
}

pub struct VirtualPointerManagerGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct VirtualPointer {
    pointer: ZwlrVirtualPointerV1,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PendingAxisFrame {
    time_msec: Option<u32>,
    horizontal: Option<f64>,
    vertical: Option<f64>,
    horizontal_v120: Option<i32>,
    vertical_v120: Option<i32>,
    source: Option<AxisSource>,
    horizontal_stopped: bool,
    vertical_stopped: bool,
}

impl PendingAxisFrame {
    #[inline]
    fn set_time(&mut self, time_msec: u32) {
        self.time_msec.get_or_insert(time_msec);
    }

    #[inline]
    fn add_value(&mut self, axis: wl_pointer::Axis, value: f64) {
        let amount = match axis {
            wl_pointer::Axis::HorizontalScroll => &mut self.horizontal,
            wl_pointer::Axis::VerticalScroll => &mut self.vertical,
            _ => unreachable!(),
        };
        *amount = Some(amount.unwrap_or_default() + value);
    }

    #[inline]
    fn set_v120(&mut self, axis: wl_pointer::Axis, value: i32) {
        match axis {
            wl_pointer::Axis::HorizontalScroll => self.horizontal_v120 = Some(value),
            wl_pointer::Axis::VerticalScroll => self.vertical_v120 = Some(value),
            _ => unreachable!(),
        }
    }

    #[inline]
    fn stop(&mut self, axis: wl_pointer::Axis) {
        match axis {
            wl_pointer::Axis::HorizontalScroll => self.horizontal_stopped = true,
            wl_pointer::Axis::VerticalScroll => self.vertical_stopped = true,
            _ => unreachable!(),
        }
    }

    fn into_event(self) -> Option<PointerAxisEvent> {
        let time_msec = self.time_msec?;
        let source = self.source.unwrap_or_else(|| {
            warn!("virtual pointer axis frame has no source; using continuous");
            AxisSource::Continuous
        });
        Some(
            PointerAxisEvent::new(
                self.horizontal,
                self.vertical,
                self.horizontal_v120,
                self.vertical_v120,
                msec_to_nsec(time_msec),
                source,
                AxisDirection::Identical,
                AxisDirection::Identical,
            )
            .with_stops(self.horizontal_stopped, self.vertical_stopped),
        )
    }
}

#[derive(Debug)]
pub struct VirtualPointerUserData {
    #[allow(dead_code)]
    seat: Option<WlSeat>,
    #[allow(dead_code)]
    output: Option<WlOutput>,

    axis_frame: Mutex<Option<PendingAxisFrame>>,
}

impl VirtualPointer {
    fn data(&self) -> &VirtualPointerUserData {
        self.pointer.data().unwrap()
    }

    #[allow(dead_code)]
    pub fn seat(&self) -> Option<&WlSeat> {
        self.data().seat.as_ref()
    }

    #[allow(dead_code)]
    pub fn output(&self) -> Option<&WlOutput> {
        self.data().output.as_ref()
    }
}

impl VirtualPointerUserData {
    fn finish_axis_frame(&self) -> Option<PointerAxisEvent> {
        self.axis_frame.lock().unwrap().take()?.into_event()
    }

    fn update_axis_frame(
        &self,
        time_msec: Option<u32>,
        update: impl FnOnce(&mut PendingAxisFrame),
    ) {
        let mut pending = self.axis_frame.lock().unwrap();
        let frame = pending.get_or_insert_default();
        if let Some(time_msec) = time_msec {
            frame.set_time(time_msec);
        }
        update(frame);
    }
}

#[inline]
const fn msec_to_nsec(time_msec: u32) -> u64 {
    time_msec as u64 * 1_000_000
}

pub trait VirtualPointerHandler: 'static {
    fn virtual_pointer_manager_state(&mut self) -> &mut VirtualPointerManagerState;

    fn create_virtual_pointer(&mut self, pointer: VirtualPointer) {
        let _ = pointer;
    }
    fn destroy_virtual_pointer(&mut self, pointer: VirtualPointer) {
        let _ = pointer;
    }

    fn on_virtual_pointer_motion(&mut self, event: RelativeMotionEvent);
    fn on_virtual_pointer_motion_absolute(&mut self, event: AbsoluteMotionEvent);
    fn on_virtual_pointer_button(&mut self, event: PointerButtonEvent);
    fn on_virtual_pointer_axis(&mut self, event: PointerAxisEvent);
}

impl VirtualPointerManagerState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: wayland_server::GlobalDispatch<
                ZwlrVirtualPointerManagerV1,
                VirtualPointerManagerGlobalData,
            >,
        D: Dispatch<ZwlrVirtualPointerManagerV1, VirtualPointerManagerGlobalData>,
        D: Dispatch<ZwlrVirtualPointerV1, VirtualPointerUserData>,
        D: VirtualPointerHandler,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        let global_data = VirtualPointerManagerGlobalData {
            filter: Box::new(filter),
        };
        display.create_global::<D, ZwlrVirtualPointerManagerV1, _>(VERSION, global_data);
        Self {
            virtual_pointers: HashSet::new(),
        }
    }
}

impl<D> GlobalDispatchDelegate<ZwlrVirtualPointerManagerV1, D> for VirtualPointerManagerGlobalData
where
    D: Dispatch<ZwlrVirtualPointerManagerV1, VirtualPointerManagerGlobalData>,
    D: 'static,
{
    fn bind(
        &self,
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrVirtualPointerManagerV1>,
        data_init: &mut DataInit<'_, D>,
    ) {
        // Manager objects carry the same global data type so can_view stays consistent;
        // filter is only used at GlobalDispatchDelegate::can_view.
        data_init.init(
            resource,
            VirtualPointerManagerGlobalData {
                filter: Box::new(|_| true),
            },
        );
    }

    fn can_view(&self, client: &Client) -> bool {
        (self.filter)(client)
    }
}

impl<D> DispatchDelegate<ZwlrVirtualPointerManagerV1, D> for VirtualPointerManagerGlobalData
where
    D: Dispatch<ZwlrVirtualPointerV1, VirtualPointerUserData>,
    D: VirtualPointerHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        _resource: &ZwlrVirtualPointerManagerV1,
        request: <ZwlrVirtualPointerManagerV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        let (id, seat, output) = match request {
            zwlr_virtual_pointer_manager_v1::Request::CreateVirtualPointer { seat, id } => {
                (id, seat, None)
            }
            zwlr_virtual_pointer_manager_v1::Request::CreateVirtualPointerWithOutput {
                seat,
                output,
                id,
            } => (id, seat, output),
            zwlr_virtual_pointer_manager_v1::Request::Destroy => return,
            _ => unreachable!(),
        };

        let pointer = data_init.init(
            id,
            VirtualPointerUserData {
                seat,
                output,
                axis_frame: Mutex::new(None),
            },
        );
        state
            .virtual_pointer_manager_state()
            .virtual_pointers
            .insert(pointer.clone());
        state.create_virtual_pointer(VirtualPointer { pointer });
    }
}

impl<D> DispatchDelegate<ZwlrVirtualPointerV1, D> for VirtualPointerUserData
where
    D: VirtualPointerHandler,
    D: 'static,
{
    fn request(
        &self,
        handler: &mut D,
        _client: &Client,
        resource: &ZwlrVirtualPointerV1,
        request: <ZwlrVirtualPointerV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_virtual_pointer_v1::Request::Motion { time, dx, dy } => {
                handler.on_virtual_pointer_motion(RelativeMotionEvent {
                    delta_x: dx,
                    delta_y: dy,
                    unaccelerated_x: dx,
                    unaccelerated_y: dy,
                    time_ns: msec_to_nsec(time),
                });
            }
            zwlr_virtual_pointer_v1::Request::MotionAbsolute {
                time,
                x,
                y,
                x_extent,
                y_extent,
            } => {
                handler.on_virtual_pointer_motion_absolute(AbsoluteMotionEvent {
                    x: f64::from(x) / f64::from(x_extent),
                    y: f64::from(y) / f64::from(y_extent),
                    time_ns: msec_to_nsec(time),
                });
            }
            zwlr_virtual_pointer_v1::Request::Button {
                time,
                button,
                state,
            } => {
                let pressed = !matches!(state, WEnum::Value(wl_pointer::ButtonState::Released));
                handler.on_virtual_pointer_button(PointerButtonEvent {
                    button,
                    pressed,
                    time_ns: msec_to_nsec(time),
                });
            }
            zwlr_virtual_pointer_v1::Request::Axis { time, axis, value } => {
                let axis = match axis {
                    WEnum::Value(axis @ wl_pointer::Axis::VerticalScroll)
                    | WEnum::Value(axis @ wl_pointer::Axis::HorizontalScroll) => axis,
                    _ => {
                        warn!("Axis: invalid axis");
                        resource.post_error(
                            zwlr_virtual_pointer_v1::Error::InvalidAxis,
                            "invalid axis",
                        );
                        return;
                    }
                };
                self.update_axis_frame(Some(time), |frame| frame.add_value(axis, value));
            }
            zwlr_virtual_pointer_v1::Request::Frame => {
                if let Some(event) = self.finish_axis_frame() {
                    handler.on_virtual_pointer_axis(event);
                }
            }
            zwlr_virtual_pointer_v1::Request::AxisSource { axis_source } => {
                let axis_source = match axis_source {
                    WEnum::Value(wl_pointer::AxisSource::Wheel) => AxisSource::Wheel,
                    WEnum::Value(wl_pointer::AxisSource::Finger) => AxisSource::Finger,
                    WEnum::Value(wl_pointer::AxisSource::Continuous) => AxisSource::Continuous,
                    WEnum::Value(wl_pointer::AxisSource::WheelTilt) => AxisSource::WheelTilt,
                    _ => {
                        warn!("AxisSource: invalid axis source");
                        resource.post_error(
                            zwlr_virtual_pointer_v1::Error::InvalidAxisSource,
                            "invalid axis source",
                        );
                        return;
                    }
                };
                self.update_axis_frame(None, |frame| frame.source = Some(axis_source));
            }
            zwlr_virtual_pointer_v1::Request::AxisStop { time, axis } => {
                let axis = match axis {
                    WEnum::Value(axis @ wl_pointer::Axis::VerticalScroll)
                    | WEnum::Value(axis @ wl_pointer::Axis::HorizontalScroll) => axis,
                    _ => {
                        warn!("AxisStop: invalid axis");
                        resource.post_error(
                            zwlr_virtual_pointer_v1::Error::InvalidAxis,
                            "invalid axis",
                        );
                        return;
                    }
                };
                self.update_axis_frame(Some(time), |frame| frame.stop(axis));
            }
            zwlr_virtual_pointer_v1::Request::AxisDiscrete {
                time,
                axis,
                value,
                discrete,
            } => {
                let axis = match axis {
                    WEnum::Value(axis @ wl_pointer::Axis::VerticalScroll)
                    | WEnum::Value(axis @ wl_pointer::Axis::HorizontalScroll) => axis,
                    _ => {
                        warn!("AxisDiscrete: invalid axis");
                        resource.post_error(
                            zwlr_virtual_pointer_v1::Error::InvalidAxis,
                            "invalid axis",
                        );
                        return;
                    }
                };
                self.update_axis_frame(Some(time), |frame| {
                    frame.add_value(axis, value);
                    frame.set_v120(axis, discrete.saturating_mul(120));
                });
            }
            zwlr_virtual_pointer_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, handler: &mut D, _client: ClientId, resource: &ZwlrVirtualPointerV1) {
        let pointer = VirtualPointer {
            pointer: resource.clone(),
        };
        handler.destroy_virtual_pointer(pointer);
        handler
            .virtual_pointer_manager_state()
            .virtual_pointers
            .remove(resource);
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ZwlrVirtualPointerManagerV1,
    VirtualPointerManagerGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ZwlrVirtualPointerManagerV1,
    VirtualPointerManagerGlobalData
);
delegate_dispatch!(RuntimeState, ZwlrVirtualPointerV1, VirtualPointerUserData);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_source_before_timed_value_survives_the_frame() {
        let mut frame = PendingAxisFrame {
            source: Some(AxisSource::Finger),
            ..PendingAxisFrame::default()
        };
        frame.add_value(wl_pointer::Axis::VerticalScroll, 1.25);
        frame.set_time(42);
        frame.add_value(wl_pointer::Axis::VerticalScroll, -0.5);
        frame.set_time(99);

        let event = frame.into_event().unwrap();
        assert_eq!(event.source, AxisSource::Finger);
        assert_eq!(event.vertical(), Some(0.75));
        assert_eq!(event.horizontal(), None);
        assert_eq!(event.time_ns, 42_000_000);
    }

    #[test]
    fn axis_stop_and_v120_remain_independent_values() {
        let mut frame = PendingAxisFrame {
            source: Some(AxisSource::Wheel),
            ..PendingAxisFrame::default()
        };
        frame.set_time(7);
        frame.set_v120(wl_pointer::Axis::HorizontalScroll, -240);
        frame.stop(wl_pointer::Axis::VerticalScroll);

        let event = frame.into_event().unwrap();
        assert_eq!(event.horizontal_v120(), Some(-240));
        assert_eq!(event.vertical(), None);
        assert!(!event.horizontal_stopped());
        assert!(event.vertical_stopped());
    }

    #[test]
    fn source_only_frame_does_not_fabricate_an_axis_timestamp() {
        let frame = PendingAxisFrame {
            source: Some(AxisSource::Continuous),
            ..PendingAxisFrame::default()
        };

        assert_eq!(frame.into_event(), None);
    }
}
