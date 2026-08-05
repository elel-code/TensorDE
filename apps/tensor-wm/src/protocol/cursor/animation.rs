use std::{
    io,
    os::fd::OwnedFd,
    time::{Duration, Instant},
};

use cursor_icon::CursorIcon;
use rustix::time::{
    Itimerspec, TimerfdClockId, TimerfdFlags, TimerfdTimerFlags, Timespec, timerfd_create,
    timerfd_settime,
};
use tensor_util::OutputScale;

use super::{CursorImage, CursorState};

pub(super) fn create_timer() -> Option<OwnedFd> {
    match timerfd_create(
        TimerfdClockId::Monotonic,
        TimerfdFlags::CLOEXEC | TimerfdFlags::NONBLOCK,
    ) {
        Ok(timer) => Some(timer),
        Err(error) => {
            tracing::warn!(%error, "cursor timerfd is unavailable");
            None
        }
    }
}

fn duration_timespec(duration: Duration) -> Timespec {
    Timespec {
        tv_sec: i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        tv_nsec: i64::from(duration.subsec_nanos()),
    }
}

impl CursorState {
    pub(super) fn select_named_frame(
        &mut self,
        icon: CursorIcon,
        scale: OutputScale,
        now: Instant,
    ) {
        let Some(Some(sequence)) = self.named_rasters.get_mut(&(icon, scale)) else {
            return;
        };
        let Some((current, remaining)) =
            sequence.frame_at(now.duration_since(self.animation_epoch))
        else {
            return;
        };
        sequence.current = current;
        self.arm_cursor_timer(now, remaining);
    }

    pub(crate) fn duplicate_cursor_timer_fd(&self) -> io::Result<Option<OwnedFd>> {
        self.cursor_timer
            .as_ref()
            .map(|timer| rustix::io::fcntl_dupfd_cloexec(timer, 0).map_err(io::Error::from))
            .transpose()
    }

    pub(crate) fn complete_cursor_timer(&mut self) -> io::Result<bool> {
        let Some(timer) = &self.cursor_timer else {
            return Ok(false);
        };
        let mut expirations = [0_u8; 8];
        let read = rustix::io::read(timer, &mut expirations).map_err(io::Error::from)?;
        if read != expirations.len() {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        self.cursor_timer_deadline = None;
        Ok(true)
    }

    pub(crate) fn cursor_timer_failed(&mut self) {
        self.cursor_timer = None;
        self.cursor_timer_deadline = None;
    }

    pub(crate) fn named_animation_will_change(&self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.animation_epoch);
        self.named_rasters.iter().any(|((icon, _), sequence)| {
            self.named_icon_is_active(*icon)
                && sequence.as_ref().is_some_and(|sequence| {
                    sequence
                        .frame_at(elapsed)
                        .is_some_and(|(current, _)| current != sequence.current)
                })
        })
    }

    pub(crate) fn advance_named_animation(&mut self, now: Instant) {
        let elapsed = now.duration_since(self.animation_epoch);
        let pointer = &self.image;
        let tablets = &self.tablets;
        let mut next = None;
        for ((icon, _), sequence) in &mut self.named_rasters {
            let active = matches!(pointer, CursorImage::Named(active) if active == icon)
                || tablets.iter().any(
                    |tablet| matches!(tablet.image, CursorImage::Named(active) if active == *icon),
                );
            if !active {
                continue;
            }
            let Some(sequence) = sequence else {
                continue;
            };
            let Some((current, remaining)) = sequence.frame_at(elapsed) else {
                continue;
            };
            sequence.current = current;
            next = Some(next.map_or(remaining, |current: Duration| current.min(remaining)));
        }
        if let Some(next) = next {
            self.arm_cursor_timer(now, next);
        }
    }

    fn named_icon_is_active(&self, icon: CursorIcon) -> bool {
        matches!(self.image, CursorImage::Named(active) if active == icon)
            || self
                .tablets
                .iter()
                .any(|tablet| matches!(tablet.image, CursorImage::Named(active) if active == icon))
    }

    pub(super) fn arm_cursor_timer(&mut self, now: Instant, delay: Duration) {
        let Some(timer) = &self.cursor_timer else {
            return;
        };
        let deadline = now.checked_add(delay).unwrap_or(now);
        if self
            .cursor_timer_deadline
            .is_some_and(|current| current <= deadline)
        {
            return;
        }
        let delay = deadline
            .saturating_duration_since(Instant::now())
            .max(Duration::from_nanos(1));
        let value = duration_timespec(delay);
        if let Err(error) = timerfd_settime(
            timer,
            TimerfdTimerFlags::empty(),
            &Itimerspec {
                it_interval: Timespec::default(),
                it_value: value,
            },
        ) {
            tracing::warn!(%error, "cursor timerfd could not be armed");
            self.cursor_timer = None;
            self.cursor_timer_deadline = None;
            return;
        }
        self.cursor_timer_deadline = Some(deadline);
    }

    pub(super) fn disarm_cursor_timer(&mut self) {
        let Some(timer) = &self.cursor_timer else {
            return;
        };
        if let Err(error) = timerfd_settime(
            timer,
            TimerfdTimerFlags::empty(),
            &Itimerspec {
                it_interval: Timespec::default(),
                it_value: Timespec::default(),
            },
        ) {
            tracing::warn!(%error, "cursor timerfd could not be disarmed");
            self.cursor_timer = None;
        }
        self.cursor_timer_deadline = None;
    }
}
