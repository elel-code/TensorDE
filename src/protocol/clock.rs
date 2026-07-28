use std::time::Duration;

use rustix::time::{ClockId, clock_gettime};

pub(super) const MONOTONIC_CLOCK_ID: u32 = ClockId::Monotonic as u32;

#[inline]
#[cfg_attr(not(feature = "tty"), allow(dead_code))]
pub(super) fn monotonic_now() -> Duration {
    let time = clock_gettime(ClockId::Monotonic);
    debug_assert!(time.tv_sec >= 0);
    debug_assert!(time.tv_nsec >= 0);
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_time_does_not_move_backwards() {
        let first = monotonic_now();
        let second = monotonic_now();
        assert!(second >= first);
        assert_eq!(MONOTONIC_CLOCK_ID, ClockId::Monotonic as u32);
    }
}
