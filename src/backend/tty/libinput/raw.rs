//! Narrow raw boundary for libinput event dequeue.
//!
//! `input` 0.10 exposes libinput 1.26 dial wrappers but omits dial events from
//! its top-level iterator. This module immediately converts that exceptional
//! path to a plain value beside the native libinput owner.

#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use libinput::{AsRaw, FromRaw as _, Libinput};

/// One tablet dial sample that `libinput::Event` cannot currently represent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DialEvent {
    pub device_raw: usize,
    pub index: u32,
    pub mode_group: u32,
    pub mode: u32,
    pub delta_v120: f64,
    pub time_usec: u64,
}

/// A standard safe wrapper event or the missing libinput 1.26 dial value.
#[derive(Debug)]
pub enum Event {
    Standard(libinput::Event),
    Dial(DialEvent),
}

/// Dequeue one event, preserving dial events and destroying unknown events.
pub fn next_event(context: &mut Libinput) -> Option<Event> {
    loop {
        let raw = unsafe { libinput::ffi::libinput_get_event(context.as_raw_mut()) };
        if raw.is_null() {
            return None;
        }
        let event_type = unsafe { libinput::ffi::libinput_event_get_type(raw) };
        if event_type == libinput::ffi::libinput_event_type_LIBINPUT_EVENT_TABLET_PAD_DIAL {
            let pad = unsafe { libinput::ffi::libinput_event_get_tablet_pad_event(raw) };
            let group = unsafe { libinput::ffi::libinput_event_tablet_pad_get_mode_group(pad) };
            let event = DialEvent {
                device_raw: unsafe { libinput::ffi::libinput_event_get_device(raw) } as usize,
                index: unsafe { libinput::ffi::libinput_event_tablet_pad_get_dial_number(pad) },
                mode_group: unsafe {
                    libinput::ffi::libinput_tablet_pad_mode_group_get_index(group)
                },
                mode: unsafe { libinput::ffi::libinput_event_tablet_pad_get_mode(pad) },
                delta_v120: unsafe {
                    libinput::ffi::libinput_event_tablet_pad_get_dial_delta_v120(pad)
                },
                time_usec: unsafe { libinput::ffi::libinput_event_tablet_pad_get_time_usec(pad) },
            };
            unsafe { libinput::ffi::libinput_event_destroy(raw) };
            return Some(Event::Dial(event));
        }
        if let Some(event) = unsafe { libinput::Event::try_from_raw(raw, context) } {
            return Some(Event::Standard(event));
        }
        unsafe { libinput::ffi::libinput_event_destroy(raw) };
    }
}

/// Number of dials reported by a pad, absent when called for another device.
pub fn pad_dial_count(device: &libinput::Device) -> Option<u32> {
    let count =
        unsafe { libinput::ffi::libinput_device_tablet_pad_get_num_dials(device.as_raw_mut()) };
    u32::try_from(count).ok()
}
