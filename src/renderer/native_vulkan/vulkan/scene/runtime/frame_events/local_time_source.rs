//! Capability-gated process-local civil-time publication.

use std::time::{Duration, Instant};

use crate::engine::scene::SceneLocalTime;

pub(super) struct SceneLocalTimeEventSource {
    enabled: bool,
    cached: Option<SceneLocalTime>,
    refresh_at: Option<Instant>,
}

impl SceneLocalTimeEventSource {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            cached: None,
            refresh_at: None,
        }
    }

    pub(super) fn capture(&mut self) -> Option<SceneLocalTime> {
        if !self.enabled {
            return None;
        }

        let now = Instant::now();
        if self.refresh_at.is_none_or(|refresh_at| now >= refresh_at) {
            self.refresh(now);
        }
        self.cached
    }

    fn refresh(&mut self, sampled_at: Instant) {
        let zoned = jiff::Zoned::now();
        self.cached = Some(SceneLocalTime::from_zoned(&zoned));
        let elapsed_nanos = u64::try_from(zoned.subsec_nanosecond()).unwrap_or(0);
        let whole_seconds = u64::try_from(zoned.second()).unwrap_or(0);
        let minute_nanos = 60_000_000_000_u64;
        let nanos_into_minute = whole_seconds
            .saturating_mul(1_000_000_000)
            .saturating_add(elapsed_nanos)
            .min(minute_nanos - 1);
        self.refresh_at = Some(
            sampled_at
                + Duration::from_nanos(minute_nanos.saturating_sub(nanos_into_minute).max(1)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_source_never_samples_local_time() {
        let mut source = SceneLocalTimeEventSource::new(false);

        assert_eq!(source.capture(), None);
        assert_eq!(source.cached, None);
        assert_eq!(source.refresh_at, None);
    }

    #[test]
    fn enabled_source_reuses_snapshot_until_minute_boundary() {
        let mut source = SceneLocalTimeEventSource::new(true);

        let first = source.capture();
        let refresh_at = source.refresh_at;
        let second = source.capture();

        assert!(first.is_some());
        assert_eq!(second, first);
        assert_eq!(source.refresh_at, refresh_at);
        assert!(refresh_at.is_some_and(|deadline| deadline > Instant::now()));
    }
}
