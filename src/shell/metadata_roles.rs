use std::cell::RefCell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use fika_core::{
    Entry, EntryData, EntryMetadataRole, Generation, ItemId, MetadataRoleBatch,
    MetadataRoleCandidate, MetadataRolePriority, MetadataRoleRequest, MetadataRoleResult,
    MetadataRoleScheduler, PaneId, metadata_role_result_for_request,
    metadata_role_results_for_requests, mime_magic_resolution_required,
};

use crate::shell::file_item_view::{
    shell_file_manager_deferred_all_indexes, shell_file_manager_read_ahead_indexes,
    visible_layout_range_for_projection,
};
use crate::shell::metrics::{
    FILE_MANAGER_RESOLVE_ALL_ITEMS_LIMIT, METADATA_ROLE_BATCH_SIZE,
    METADATA_ROLE_READ_AHEAD_QUEUE_BUDGET_PER_FRAME,
};
use crate::shell::pane::{ShellPaneId, ShellPaneProjection};

#[derive(Default)]
pub(crate) struct MetadataRolePrewarmStats {
    pub(crate) visible: usize,
    pub(crate) deferred: usize,
    pub(crate) queued_snapshots: usize,
    pub(crate) batches_started: usize,
    pub(crate) results: usize,
    pub(crate) visible_results: usize,
    pub(crate) applied: usize,
    pub(crate) visible_applied: usize,
}

#[derive(Default)]
pub(crate) struct MetadataRoleSyncStats {
    pub(crate) visible: usize,
    pub(crate) resolved: usize,
    pub(crate) deferred: usize,
    pub(crate) applied: usize,
    pub(crate) resolve_us: u128,
    pub(crate) over_budget: bool,
}

pub(crate) struct ShellMetadataRoleRuntime {
    scheduler: RefCell<MetadataRoleScheduler>,
    tx: Option<Sender<MetadataRoleBatch>>,
    rx: Receiver<Vec<MetadataRoleResult>>,
}

impl ShellMetadataRoleRuntime {
    pub(crate) fn new() -> Self {
        let (tx, rx) = shell_metadata_role_channel();
        Self {
            scheduler: RefCell::new(MetadataRoleScheduler::default()),
            tx,
            rx,
        }
    }

    pub(crate) fn prewarm(
        &self,
        projections: &[ShellPaneProjection<'_>],
        generation: Generation,
    ) -> MetadataRolePrewarmStats {
        let mut stats = MetadataRolePrewarmStats::default();
        for projection in projections {
            let visible = metadata_role_candidates_for_visible_projection(projection);
            stats.visible += visible.len();
            if self.scheduler.borrow_mut().queue_candidates(
                core_pane_id_for_shell_pane(projection.geometry.kind),
                generation,
                visible,
            ) {
                stats.queued_snapshots += 1;
            }

            let deferred = metadata_role_candidates_for_deferred_projection(projection);
            stats.deferred += deferred.len();
            if !deferred.is_empty()
                && self.scheduler.borrow_mut().queue_candidates_with_priority(
                    core_pane_id_for_shell_pane(projection.geometry.kind),
                    generation,
                    deferred,
                    MetadataRolePriority::Deferred,
                )
            {
                stats.queued_snapshots += 1;
            }
        }
        if let Some(batch) = self
            .scheduler
            .borrow_mut()
            .start_role_batch(METADATA_ROLE_BATCH_SIZE)
        {
            if self.tx.as_ref().is_some_and(|tx| tx.send(batch).is_ok()) {
                stats.batches_started += 1;
            } else {
                self.scheduler.borrow_mut().finish_role_batch();
            }
        }
        stats
    }

    pub(crate) fn drain_ready_results(
        &self,
    ) -> (
        MetadataRolePrewarmStats,
        Vec<(MetadataRoleResult, MetadataRolePriority)>,
    ) {
        let mut stats = MetadataRolePrewarmStats::default();
        let mut ready = Vec::new();
        while let Ok(results) = self.rx.try_recv() {
            let priorities = {
                let mut scheduler = self.scheduler.borrow_mut();
                let priorities = results
                    .iter()
                    .map(|result| {
                        scheduler
                            .priority_for_result(result)
                            .unwrap_or(MetadataRolePriority::Deferred)
                    })
                    .collect::<Vec<_>>();
                scheduler.finish_role_batch_with_results(&results);
                priorities
            };
            stats.results += results.len();
            stats.visible_results += priorities
                .iter()
                .filter(|priority| **priority == MetadataRolePriority::Visible)
                .count();
            ready.extend(results.into_iter().zip(priorities));
        }
        (stats, ready)
    }

    pub(crate) fn resolve_visible_synchronously(
        &self,
        projections: &[ShellPaneProjection<'_>],
        generation: Generation,
        budget: Duration,
    ) -> (MetadataRoleSyncStats, Vec<MetadataRoleResult>) {
        let mut requests = Vec::new();
        for projection in projections {
            let pane_id = core_pane_id_for_shell_pane(projection.geometry.kind);
            requests.extend(
                metadata_role_candidates_for_visible_projection(projection)
                    .into_iter()
                    .filter_map(|candidate| {
                        MetadataRoleRequest::from_candidate(pane_id, generation, candidate)
                    }),
            );
        }

        let mut stats = MetadataRoleSyncStats {
            visible: requests.len(),
            ..MetadataRoleSyncStats::default()
        };
        let started = Instant::now();
        let mut results = Vec::with_capacity(requests.len());
        for request in requests {
            if started.elapsed() >= budget {
                stats.over_budget = true;
                break;
            }
            results.push(metadata_role_result_for_request(request));
        }
        stats.resolved = results.len();
        stats.deferred = stats.visible.saturating_sub(stats.resolved);
        stats.resolve_us = started.elapsed().as_micros();
        (stats, results)
    }

    pub(crate) fn has_visible_pending(&self) -> bool {
        self.scheduler.borrow().has_visible_pending()
    }

    pub(crate) fn cancel_pane(&self, pane: ShellPaneId) {
        self.scheduler
            .borrow_mut()
            .cancel_pane(core_pane_id_for_shell_pane(pane));
    }
}

fn shell_metadata_role_channel() -> (
    Option<Sender<MetadataRoleBatch>>,
    Receiver<Vec<MetadataRoleResult>>,
) {
    let (request_tx, request_rx) = mpsc::channel::<MetadataRoleBatch>();
    let (result_tx, result_rx) = mpsc::channel::<Vec<MetadataRoleResult>>();
    let request_tx = thread::Builder::new()
        .name("fika-wgpu-metadata-role".to_string())
        .spawn(move || shell_metadata_role_worker(request_rx, result_tx))
        .ok()
        .map(|_| request_tx);
    (request_tx, result_rx)
}

fn shell_metadata_role_worker(
    request_rx: Receiver<MetadataRoleBatch>,
    result_tx: Sender<Vec<MetadataRoleResult>>,
) {
    while let Ok(batch) = request_rx.recv() {
        let results = metadata_role_results_for_requests(batch.requests);
        if result_tx.send(results).is_err() {
            break;
        }
    }
}

fn metadata_role_candidates_for_visible_projection(
    projection: &ShellPaneProjection<'_>,
) -> Vec<MetadataRoleCandidate> {
    projection
        .visible_items
        .iter()
        .filter_map(|item| {
            let entry_index = projection
                .view
                .filtered_indexes
                .get(item.layout.model_index)
                .copied()?;
            let entry = projection.view.entries.get(entry_index)?;
            shell_metadata_role_candidate(projection.view.path, entry_index, entry)
        })
        .collect()
}

fn metadata_role_candidates_for_deferred_projection(
    projection: &ShellPaneProjection<'_>,
) -> Vec<MetadataRoleCandidate> {
    let item_count = projection.view.filtered_entry_count();
    metadata_deferred_layout_indexes(
        visible_layout_range_for_projection(projection),
        item_count,
        projection.visible_items.len(),
    )
    .into_iter()
    .filter_map(|layout_index| {
        let entry_index = projection
            .view
            .filtered_indexes
            .get(layout_index)
            .copied()?;
        let entry = projection.view.entries.get(entry_index)?;
        shell_metadata_role_candidate(projection.view.path, entry_index, entry)
    })
    .take(METADATA_ROLE_READ_AHEAD_QUEUE_BUDGET_PER_FRAME)
    .collect()
}

fn metadata_deferred_layout_indexes(
    visible_range: Option<Range<usize>>,
    item_count: usize,
    maximum_visible_items: usize,
) -> Vec<usize> {
    if item_count <= FILE_MANAGER_RESOLVE_ALL_ITEMS_LIMIT {
        return shell_file_manager_deferred_all_indexes(visible_range, item_count);
    }
    let Some(visible_range) = visible_range else {
        return Vec::new();
    };
    shell_file_manager_read_ahead_indexes(visible_range, item_count, maximum_visible_items)
}

pub(crate) fn core_pane_id_for_shell_pane(pane: ShellPaneId) -> PaneId {
    PaneId(pane.index() as u64 + 1)
}

pub(crate) fn shell_pane_id_for_core_pane(pane: PaneId) -> Option<ShellPaneId> {
    match pane.0 {
        1 => Some(ShellPaneId::SLOT_0),
        2 => Some(ShellPaneId::SLOT_1),
        _ => None,
    }
}

pub(crate) fn shell_metadata_item_id(entry_index: usize) -> ItemId {
    ItemId(entry_index as u64 + 1)
}

pub(crate) fn shell_metadata_entry_index(item_id: ItemId) -> Option<usize> {
    item_id
        .0
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
}

pub(crate) fn shell_entry_path(directory: &Path, entry: &Entry) -> PathBuf {
    entry
        .target_path
        .clone()
        .unwrap_or_else(|| directory.join(entry.name.as_ref()))
}

pub(crate) fn shell_metadata_role_candidate(
    directory: &Path,
    entry_index: usize,
    entry: &Entry,
) -> Option<MetadataRoleCandidate> {
    if !mime_magic_resolution_required(
        entry.is_dir,
        entry.size_bytes,
        entry.mime_type.as_deref(),
        entry.mime_magic_checked,
    ) {
        return None;
    }
    Some(MetadataRoleCandidate {
        item_id: shell_metadata_item_id(entry_index),
        path: shell_entry_path(directory, entry),
        size_bytes: entry.size_bytes,
        modified_secs: entry.modified_secs,
        mime_type: entry.mime_type.as_ref().map(|mime| mime.to_string()),
        mime_magic_checked: entry.mime_magic_checked,
    })
}

pub(crate) fn entry_with_metadata_role(entry: &Entry, role: EntryMetadataRole) -> Entry {
    Entry::new(EntryData {
        name: entry.name.clone(),
        name_width_units: entry.name_width_units,
        target_path: entry.target_path.clone(),
        size_bytes: role.size_bytes,
        modified_secs: role.modified_secs,
        metadata_complete: true,
        mime_type: role.mime_type,
        mime_magic_checked: role.mime_magic_checked,
        trash_original_path: entry.trash_original_path.clone(),
        trash_deletion_time: entry.trash_deletion_time.clone(),
        is_dir: entry.is_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_deferred_indexes_for_small_directory_exclude_visible_items() {
        let indexes = metadata_deferred_layout_indexes(Some(4..7), 10, 3);

        assert_eq!(indexes, vec![0, 1, 2, 3, 7, 8, 9]);
    }

    #[test]
    fn metadata_deferred_indexes_for_large_directory_follow_file_manager_order() {
        let indexes = metadata_deferred_layout_indexes(
            Some(4..7),
            FILE_MANAGER_RESOLVE_ALL_ITEMS_LIMIT + 1,
            3,
        );

        assert_eq!(&indexes[..6], &[7, 8, 9, 10, 11, 12]);
        assert!(!indexes.iter().any(|index| (4..7).contains(index)));
    }

    #[test]
    fn metadata_deferred_indexes_for_large_directory_require_visible_range() {
        let indexes =
            metadata_deferred_layout_indexes(None, FILE_MANAGER_RESOLVE_ALL_ITEMS_LIMIT + 1, 3);

        assert!(indexes.is_empty());
    }
}
