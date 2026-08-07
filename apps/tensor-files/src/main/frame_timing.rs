use std::time::Instant;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FrameTiming {
    enabled: bool,
    total_us: u128,
}

impl FrameTiming {
    pub(crate) const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            total_us: 0,
        }
    }

    pub(crate) fn start(self) -> Option<Instant> {
        self.enabled.then(Instant::now)
    }

    pub(crate) fn record(&mut self, started_at: Option<Instant>) {
        if let Some(started_at) = started_at {
            self.total_us = self
                .total_us
                .saturating_add(started_at.elapsed().as_micros());
        }
    }

    pub(crate) const fn total_us(self) -> u128 {
        self.total_us
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn disabled_timing_does_not_sample_or_accumulate() {
        let mut timing = FrameTiming::new(false);
        assert!(timing.start().is_none());
        timing.record(None);
        assert_eq!(timing.total_us(), 0);
    }

    #[test]
    fn enabled_timing_records_a_sample() {
        let mut timing = FrameTiming::new(true);
        let started_at = Some(Instant::now() - Duration::from_millis(1));
        timing.record(started_at);
        assert!(timing.total_us() >= 1_000);
    }
}
