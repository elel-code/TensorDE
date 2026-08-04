//! Capability-gated process-local civil-time publication.

use std::time::{Duration, Instant};

use crate::engine::scene::SceneLocalTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SceneLocalTimePrecision {
    Minute,
    Second,
}

pub(super) struct SceneLocalTimeEventSource {
    precision: Option<SceneLocalTimePrecision>,
    cached: Option<SceneLocalTime>,
    refresh_at: Option<Instant>,
}

impl SceneLocalTimeEventSource {
    pub(super) fn new(precision: Option<SceneLocalTimePrecision>) -> Self {
        Self {
            precision,
            cached: None,
            refresh_at: None,
        }
    }

    pub(super) fn capture(&mut self) -> Option<SceneLocalTime> {
        self.capture_at(Instant::now())
    }

    fn capture_at(&mut self, now: Instant) -> Option<SceneLocalTime> {
        let precision = self.precision?;

        if self.refresh_at.is_none_or(|refresh_at| now >= refresh_at) {
            self.refresh(now, precision);
        }
        self.cached
    }

    fn refresh(&mut self, sampled_at: Instant, precision: SceneLocalTimePrecision) {
        let zoned = jiff::Zoned::now();
        self.cached = Some(SceneLocalTime::from_zoned(&zoned));
        self.refresh_at = Some(
            sampled_at
                + until_next_boundary(
                    precision,
                    u64::try_from(zoned.second()).unwrap_or(0),
                    u64::try_from(zoned.subsec_nanosecond()).unwrap_or(0),
                ),
        );
    }
}

fn until_next_boundary(
    precision: SceneLocalTimePrecision,
    second: u64,
    nanosecond: u64,
) -> Duration {
    let nanosecond = nanosecond.min(999_999_999);
    let (period_nanos, elapsed_nanos) = match precision {
        SceneLocalTimePrecision::Minute => (
            60_000_000_000_u64,
            second
                .min(59)
                .saturating_mul(1_000_000_000)
                .saturating_add(nanosecond),
        ),
        SceneLocalTimePrecision::Second => (1_000_000_000_u64, nanosecond),
    };
    Duration::from_nanos(period_nanos - elapsed_nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_source_never_samples_local_time() {
        let mut source = SceneLocalTimeEventSource::new(None);

        assert_eq!(source.capture(), None);
        assert_eq!(source.cached, None);
        assert_eq!(source.refresh_at, None);
    }

    #[test]
    fn enabled_source_reuses_snapshot_until_its_deadline() {
        let mut source = SceneLocalTimeEventSource::new(Some(SceneLocalTimePrecision::Second));
        let now = Instant::now();
        let snapshot = SceneLocalTime {
            year: 2026,
            month: 8,
            day: 1,
            hour: 12,
            minute: 0,
            second: 0,
            weekday_sunday_zero: 6,
        };
        let refresh_at = now + Duration::from_secs(1);
        source.cached = Some(snapshot);
        source.refresh_at = Some(refresh_at);

        assert_eq!(source.capture_at(now), Some(snapshot));
        assert_eq!(source.cached, Some(snapshot));
        assert_eq!(source.refresh_at, Some(refresh_at));
    }

    #[test]
    fn precision_selects_the_requested_civil_time_boundary() {
        assert_eq!(
            until_next_boundary(SceneLocalTimePrecision::Minute, 23, 250_000_000),
            Duration::from_millis(36_750)
        );
        assert_eq!(
            until_next_boundary(SceneLocalTimePrecision::Second, 23, 250_000_000),
            Duration::from_millis(750)
        );
    }
}
