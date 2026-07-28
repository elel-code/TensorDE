use std::collections::VecDeque;

const EVENT_ZOOM: &str = "zoom";
const EVENT_CONTENT_SCROLL: &str = "content-scroll";
const EVENT_PLACES_SCROLL: &str = "places-scroll";
const EVENT_PATH_CHANGE: &str = "path-change";
const EVENT_DIRECTORY_RELOAD: &str = "directory-reload";
const EVENT_MIME_METADATA: &str = "mime-metadata";
const EVENT_ICON_RESOLVE: &str = "icon-resolve";
const EVENT_THUMBNAIL: &str = "thumbnail";
const EVENT_FOLDER_PREVIEW: &str = "folder-preview";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellFrameLatencyCounters {
    pub(crate) zoom_changes: u64,
    pub(crate) content_scroll_changes: u64,
    pub(crate) places_scroll_changes: u64,
    pub(crate) path_changes: u64,
    pub(crate) directory_reloads: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellFrameLatencyAsyncResults {
    pub(crate) metadata_applied: u64,
    pub(crate) icon_resolve_results: u64,
    pub(crate) thumbnail_results: u64,
    pub(crate) folder_preview_results: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellFrameLatencyReport {
    pub(crate) event: &'static str,
    pub(crate) count: u64,
    pub(crate) requested_after_frame: u64,
    pub(crate) presented_frame: u64,
    pub(crate) frames: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ShellFrameLatencyMarker {
    event: &'static str,
    count: u64,
    requested_after_frame: u64,
}

#[derive(Debug, Default)]
pub(crate) struct ShellFrameLatencyTracker {
    last_counters: Option<ShellFrameLatencyCounters>,
    pending: VecDeque<ShellFrameLatencyMarker>,
}

impl ShellFrameLatencyTracker {
    pub(crate) fn observe_scene_counters(
        &mut self,
        counters: ShellFrameLatencyCounters,
        frame_count: u64,
    ) {
        let Some(previous) = self.last_counters.replace(counters) else {
            return;
        };

        self.record_counter_delta(
            EVENT_ZOOM,
            previous.zoom_changes,
            counters.zoom_changes,
            frame_count,
        );
        self.record_counter_delta(
            EVENT_CONTENT_SCROLL,
            previous.content_scroll_changes,
            counters.content_scroll_changes,
            frame_count,
        );
        self.record_counter_delta(
            EVENT_PLACES_SCROLL,
            previous.places_scroll_changes,
            counters.places_scroll_changes,
            frame_count,
        );
        self.record_counter_delta(
            EVENT_PATH_CHANGE,
            previous.path_changes,
            counters.path_changes,
            frame_count,
        );
        self.record_counter_delta(
            EVENT_DIRECTORY_RELOAD,
            previous.directory_reloads,
            counters.directory_reloads,
            frame_count,
        );
    }

    pub(crate) fn observe_async_results(
        &mut self,
        results: ShellFrameLatencyAsyncResults,
        frame_count: u64,
    ) {
        self.record_count(EVENT_MIME_METADATA, results.metadata_applied, frame_count);
        self.record_count(
            EVENT_ICON_RESOLVE,
            results.icon_resolve_results,
            frame_count,
        );
        self.record_count(EVENT_THUMBNAIL, results.thumbnail_results, frame_count);
        self.record_count(
            EVENT_FOLDER_PREVIEW,
            results.folder_preview_results,
            frame_count,
        );
    }

    pub(crate) fn drain_presented(&mut self, presented_frame: u64) -> Vec<ShellFrameLatencyReport> {
        let mut reports = Vec::with_capacity(self.pending.len());
        while let Some(marker) = self.pending.pop_front() {
            reports.push(ShellFrameLatencyReport {
                event: marker.event,
                count: marker.count,
                requested_after_frame: marker.requested_after_frame,
                presented_frame,
                frames: presented_frame.saturating_sub(marker.requested_after_frame),
            });
        }
        reports
    }

    fn record_counter_delta(
        &mut self,
        event: &'static str,
        previous: u64,
        current: u64,
        frame_count: u64,
    ) {
        self.record_count(event, current.saturating_sub(previous), frame_count);
    }

    fn record_count(&mut self, event: &'static str, count: u64, frame_count: u64) {
        if count == 0 {
            return;
        }
        self.pending.push_back(ShellFrameLatencyMarker {
            event,
            count,
            requested_after_frame: frame_count,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_scene_observation_seeds_without_report() {
        let mut tracker = ShellFrameLatencyTracker::default();

        tracker.observe_scene_counters(
            ShellFrameLatencyCounters {
                zoom_changes: 2,
                content_scroll_changes: 3,
                ..ShellFrameLatencyCounters::default()
            },
            7,
        );

        assert!(tracker.drain_presented(8).is_empty());
    }

    #[test]
    fn scene_counter_deltas_report_frame_latency() {
        let mut tracker = ShellFrameLatencyTracker::default();
        tracker.observe_scene_counters(ShellFrameLatencyCounters::default(), 4);

        tracker.observe_scene_counters(
            ShellFrameLatencyCounters {
                zoom_changes: 1,
                content_scroll_changes: 2,
                places_scroll_changes: 1,
                path_changes: 1,
                directory_reloads: 1,
            },
            4,
        );

        let reports = tracker.drain_presented(5);
        assert_eq!(
            reports,
            vec![
                ShellFrameLatencyReport {
                    event: EVENT_ZOOM,
                    count: 1,
                    requested_after_frame: 4,
                    presented_frame: 5,
                    frames: 1,
                },
                ShellFrameLatencyReport {
                    event: EVENT_CONTENT_SCROLL,
                    count: 2,
                    requested_after_frame: 4,
                    presented_frame: 5,
                    frames: 1,
                },
                ShellFrameLatencyReport {
                    event: EVENT_PLACES_SCROLL,
                    count: 1,
                    requested_after_frame: 4,
                    presented_frame: 5,
                    frames: 1,
                },
                ShellFrameLatencyReport {
                    event: EVENT_PATH_CHANGE,
                    count: 1,
                    requested_after_frame: 4,
                    presented_frame: 5,
                    frames: 1,
                },
                ShellFrameLatencyReport {
                    event: EVENT_DIRECTORY_RELOAD,
                    count: 1,
                    requested_after_frame: 4,
                    presented_frame: 5,
                    frames: 1,
                },
            ]
        );
    }

    #[test]
    fn async_result_batches_report_loaded_asset_latency() {
        let mut tracker = ShellFrameLatencyTracker::default();

        tracker.observe_async_results(
            ShellFrameLatencyAsyncResults {
                metadata_applied: 1,
                icon_resolve_results: 2,
                thumbnail_results: 4,
                folder_preview_results: 5,
            },
            10,
        );

        let events = tracker
            .drain_presented(12)
            .into_iter()
            .map(|report| (report.event, report.count, report.frames))
            .collect::<Vec<_>>();
        assert_eq!(
            events,
            vec![
                (EVENT_MIME_METADATA, 1, 2),
                (EVENT_ICON_RESOLVE, 2, 2),
                (EVENT_THUMBNAIL, 4, 2),
                (EVENT_FOLDER_PREVIEW, 5, 2),
            ]
        );
    }

    #[test]
    fn zero_async_results_do_not_report() {
        let mut tracker = ShellFrameLatencyTracker::default();

        tracker.observe_async_results(ShellFrameLatencyAsyncResults::default(), 1);

        assert!(tracker.drain_presented(2).is_empty());
    }
}
