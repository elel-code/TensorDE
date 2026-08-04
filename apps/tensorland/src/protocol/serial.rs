use std::sync::atomic::{AtomicU32, Ordering};

/// Wayland serial allocated on the compositor thread.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Serial(u32);

impl From<u32> for Serial {
    #[inline]
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<Serial> for u32 {
    #[inline]
    fn from(value: Serial) -> Self {
        value.0
    }
}

static NEXT_SERIAL: AtomicU32 = AtomicU32::new(1);

#[inline]
pub(crate) fn next_serial() -> Serial {
    let serial = NEXT_SERIAL
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.wrapping_add(1).max(1))
        })
        .unwrap_or(1);
    Serial(serial.max(1))
}
