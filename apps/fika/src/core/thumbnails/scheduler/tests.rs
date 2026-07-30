use super::*;
use crate::core::entries::{Entry, EntryData};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn thumbnail_scheduler_queues_once_and_clears_pane_work() {
    let pane_id = PaneId(1);
    let generation = Generation(1);
    let mut scheduler = ThumbnailScheduler::new(PathBuf::from("/tmp/fika-thumbnail-scheduler"));
    let candidate = thumbnail_candidate(1, "image.png", ThumbnailRequestPriority::Visible);

    assert!(scheduler.queue_candidates(pane_id, generation, vec![candidate.clone()]));
    assert_eq!(scheduler.queued_len(), 1);
    assert_eq!(scheduler.seen_len(), 1);
    assert!(!scheduler.queue_candidates(pane_id, generation, vec![candidate]));
    assert_eq!(scheduler.queued_len(), 1);

    let request = scheduler.pop_next_request().unwrap();
    assert_eq!(request.pane_id(), pane_id);
    assert_eq!(request.generation(), generation);
    assert_eq!(request.item_id(), ItemId(1));
    assert_eq!(request.priority(), ThumbnailRequestPriority::Visible);

    scheduler.cancel_pane(pane_id);
    assert!(scheduler.is_empty());
    assert_eq!(scheduler.seen_len(), 0);
}

#[test]
fn thumbnail_scheduler_skips_incomplete_metadata_candidates() {
    let pane_id = PaneId(1);
    let generation = Generation(1);
    let mut scheduler =
        ThumbnailScheduler::new(PathBuf::from("/tmp/fika-thumbnail-incomplete-metadata"));
    let mut candidate = thumbnail_candidate(1, "image.png", ThumbnailRequestPriority::Visible);
    candidate.metadata_complete = false;

    assert!(!scheduler.queue_candidates(pane_id, generation, vec![candidate]));
    assert_eq!(scheduler.queued_len(), 0);
    assert_eq!(scheduler.seen_len(), 0);
}

#[test]
fn thumbnail_scheduler_skips_plain_text_candidates() {
    let pane_id = PaneId(1);
    let generation = Generation(1);
    let mut scheduler = ThumbnailScheduler::new(PathBuf::from("/tmp/fika-thumbnail-text"));
    let mut candidate = thumbnail_candidate(1, "notes.txt", ThumbnailRequestPriority::Visible);
    candidate.mime_type = Some("text/plain".to_string());

    assert!(!scheduler.queue_candidates(pane_id, generation, vec![candidate]));
    assert_eq!(scheduler.queued_len(), 0);
    assert_eq!(scheduler.seen_len(), 0);
}

#[test]
fn thumbnail_work_key_keeps_path_and_mime_identity_without_storing_paths() {
    let pane_id = PaneId(1);
    let generation = Generation(1);
    let image = thumbnail_candidate(1, "image.png", ThumbnailRequestPriority::Visible);
    let renamed = thumbnail_candidate(1, "renamed.png", ThumbnailRequestPriority::Visible);
    let mut retagged = image.clone();
    retagged.mime_type = Some("image/webp".to_string());

    assert_ne!(
        ThumbnailWorkKey::from_candidate(pane_id, generation, &image),
        ThumbnailWorkKey::from_candidate(pane_id, generation, &renamed)
    );
    assert_ne!(
        ThumbnailWorkKey::from_candidate(pane_id, generation, &image),
        ThumbnailWorkKey::from_candidate(pane_id, generation, &retagged)
    );
}

#[test]
fn thumbnail_scheduler_prunes_deferred_work_outside_current_resolve_set() {
    let pane_id = PaneId(1);
    let generation = Generation(1);
    let mut scheduler = ThumbnailScheduler::new(PathBuf::from("/tmp/fika-thumbnail-active"));
    let keep = thumbnail_candidate(1, "keep.png", ThumbnailRequestPriority::Deferred);
    let stale = thumbnail_candidate(2, "stale.png", ThumbnailRequestPriority::Deferred);

    assert!(scheduler.queue_candidates(pane_id, generation, vec![keep.clone(), stale.clone()]));
    let active_batch = scheduler.start_probe_batch(32).unwrap();
    assert_eq!(active_batch.requests.len(), 2);
    assert!(scheduler.is_empty());
    assert_eq!(scheduler.seen_len(), 2);

    assert!(!scheduler.queue_candidates(pane_id, generation, vec![keep.clone()]));
    assert_eq!(scheduler.seen_len(), 1);
    assert!(scheduler.contains_seen(&ThumbnailWorkKey::from_candidate(
        pane_id, generation, &keep
    )));
    assert!(!scheduler.contains_seen(&ThumbnailWorkKey::from_candidate(
        pane_id, generation, &stale
    )));

    assert!(scheduler.queue_candidates(pane_id, generation, vec![stale.clone()]));
    assert_eq!(scheduler.seen_len(), 1);
    assert!(scheduler.contains_seen(&ThumbnailWorkKey::from_candidate(
        pane_id, generation, &stale
    )));
    let requeued = scheduler.pop_next_request().unwrap();
    assert_eq!(requeued.item_id(), stale.item_id);
}

#[test]
fn thumbnail_probe_result_applies_only_to_matching_model_item_and_path() {
    let pane_id = PaneId(1);
    let generation = Generation(1);
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp/fika-thumbnail-result"));
    model.replace_listing(
        PathBuf::from("/tmp/fika-thumbnail-result"),
        Arc::new(vec![test_entry("image.png")]),
    );
    let item_id = model.entries()[0].id;
    let thumbnail_path = PathBuf::from("/tmp/fika-thumbnail-cache/normal/image.png");

    assert!(!apply_thumbnail_probe_result_to_model(
        &mut model,
        ThumbnailProbeResult {
            pane_id,
            generation,
            item_id,
            path: PathBuf::from("/tmp/fika-thumbnail-result/other.png"),
            modified_secs: 42,
            thumbnail_path: Some(PathBuf::from("/tmp/wrong-path.png")),
        },
    ));
    assert!(model.entries()[0].thumbnail_path.is_none());

    assert!(!apply_thumbnail_probe_result_to_model(
        &mut model,
        ThumbnailProbeResult {
            pane_id,
            generation,
            item_id: ItemId(999),
            path: PathBuf::from("/tmp/fika-thumbnail-result/image.png"),
            modified_secs: 42,
            thumbnail_path: Some(PathBuf::from("/tmp/missing-item.png")),
        },
    ));
    assert!(model.entries()[0].thumbnail_path.is_none());

    assert!(apply_thumbnail_probe_result_to_model(
        &mut model,
        ThumbnailProbeResult {
            pane_id,
            generation,
            item_id,
            path: PathBuf::from("/tmp/fika-thumbnail-result/image.png"),
            modified_secs: 42,
            thumbnail_path: Some(thumbnail_path.clone()),
        },
    ));
    assert_eq!(
        model.entries()[0].thumbnail_path.as_deref(),
        Some(thumbnail_path.as_path())
    );
}

#[test]
fn thumbnail_probe_failure_marks_model_preview_finished() {
    let pane_id = PaneId(1);
    let generation = Generation(1);
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp/fika-thumbnail-failed"));
    model.replace_listing(
        PathBuf::from("/tmp/fika-thumbnail-failed"),
        Arc::new(vec![test_entry("image.png")]),
    );
    let item_id = model.entries()[0].id;

    assert!(apply_thumbnail_probe_result_to_model(
        &mut model,
        ThumbnailProbeResult {
            pane_id,
            generation,
            item_id,
            path: PathBuf::from("/tmp/fika-thumbnail-failed/image.png"),
            modified_secs: 42,
            thumbnail_path: None,
        },
    ));
    assert!(model.entries()[0].thumbnail_failed);
    assert!(model.entries()[0].thumbnail_path.is_none());
    assert!(!apply_thumbnail_probe_result_to_model(
        &mut model,
        ThumbnailProbeResult {
            pane_id,
            generation,
            item_id,
            path: PathBuf::from("/tmp/fika-thumbnail-failed/image.png"),
            modified_secs: 42,
            thumbnail_path: None,
        },
    ));
}

#[test]
fn stale_thumbnail_probe_result_does_not_update_model() {
    let pane_id = PaneId(1);
    let generation = Generation(1);
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp/fika-thumbnail-stale"));
    model.replace_listing(
        PathBuf::from("/tmp/fika-thumbnail-stale"),
        Arc::new(vec![test_entry("image.png")]),
    );
    let item_id = model.entries()[0].id;

    assert!(!apply_thumbnail_probe_result_to_model(
        &mut model,
        ThumbnailProbeResult {
            pane_id,
            generation,
            item_id,
            path: PathBuf::from("/tmp/fika-thumbnail-stale/image.png"),
            modified_secs: 41,
            thumbnail_path: Some(PathBuf::from("/tmp/stale.png")),
        },
    ));
    assert!(model.entries()[0].thumbnail_path.is_none());
    assert!(!model.entries()[0].thumbnail_failed);
}

#[test]
fn thumbnail_probe_worker_limits_concurrent_requests() {
    #[derive(Default)]
    struct WorkerState {
        active: usize,
        max_active: usize,
        release: bool,
    }

    let pane_id = PaneId(1);
    let generation = Generation(1);
    let requests = (0..8)
        .map(|index| {
            ThumbnailRequest::from_entry_metadata_with_mime(
                pane_id,
                generation,
                ItemId(index),
                PathBuf::from(format!("/tmp/fika-thumbnail-worker-{index}.png")),
                42,
                Some("image/png".to_string()),
                ThumbnailRequestPriority::Visible,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let state = Arc::new((
        std::sync::Mutex::new(WorkerState::default()),
        std::sync::Condvar::new(),
    ));
    let worker_state = Arc::clone(&state);

    let worker = std::thread::spawn(move || {
        thumbnail_probe_results_with_worker(requests, 4, None, move |request| {
            let (lock, condvar) = &*worker_state;
            let mut state = lock.lock().unwrap();
            state.active += 1;
            state.max_active = state.max_active.max(state.active);
            condvar.notify_all();
            while !state.release {
                state = condvar.wait(state).unwrap();
            }
            state.active -= 1;
            drop(state);

            ThumbnailProbeResult {
                pane_id: request.pane_id(),
                generation: request.generation(),
                item_id: request.item_id(),
                path: request.path().to_path_buf(),
                modified_secs: request.modified_secs(),
                thumbnail_path: Some(PathBuf::from(format!(
                    "/tmp/fika-thumbnail-worker-result-{}.png",
                    request.item_id().0
                ))),
            }
        })
    });

    let (lock, condvar) = &*state;
    let mut state_guard = lock.lock().unwrap();
    while state_guard.max_active < 4 {
        let wait = condvar
            .wait_timeout(state_guard, Duration::from_secs(1))
            .unwrap();
        state_guard = wait.0;
        assert!(
            !wait.1.timed_out(),
            "thumbnail workers did not reach the configured concurrency limit"
        );
    }
    assert_eq!(state_guard.max_active, 4);
    state_guard.release = true;
    condvar.notify_all();
    drop(state_guard);

    let results = worker.join().unwrap();
    assert_eq!(results.len(), 8);
    assert_eq!(lock.lock().unwrap().max_active, 4);
}

#[test]
fn thumbnail_probe_worker_skips_cancelled_deferred_before_start() {
    #[derive(Default)]
    struct WorkerState {
        started: Vec<ItemId>,
        release: bool,
    }

    let pane_id = PaneId(1);
    let generation = Generation(1);
    let requests = vec![
        ThumbnailRequest::from_entry_metadata_with_mime(
            pane_id,
            generation,
            ItemId(1),
            PathBuf::from("/tmp/fika-thumbnail-worker-visible.png"),
            42,
            Some("image/png".to_string()),
            ThumbnailRequestPriority::Visible,
        )
        .unwrap(),
        ThumbnailRequest::from_entry_metadata_with_mime(
            pane_id,
            generation,
            ItemId(2),
            PathBuf::from("/tmp/fika-thumbnail-worker-deferred.png"),
            42,
            Some("image/png".to_string()),
            ThumbnailRequestPriority::Deferred,
        )
        .unwrap(),
    ];
    let cancel_handle = ThumbnailProbeCancelHandle::from_requests(&requests);
    let state = Arc::new((
        std::sync::Mutex::new(WorkerState::default()),
        std::sync::Condvar::new(),
    ));
    let worker_state = Arc::clone(&state);
    let worker_cancel = cancel_handle.clone();

    let worker = std::thread::spawn(move || {
        thumbnail_probe_results_with_worker(requests, 1, Some(worker_cancel), move |request| {
            let (lock, condvar) = &*worker_state;
            let mut state = lock.lock().unwrap();
            state.started.push(request.item_id());
            condvar.notify_all();
            while !state.release {
                state = condvar.wait(state).unwrap();
            }
            drop(state);

            ThumbnailProbeResult {
                pane_id: request.pane_id(),
                generation: request.generation(),
                item_id: request.item_id(),
                path: request.path().to_path_buf(),
                modified_secs: request.modified_secs(),
                thumbnail_path: Some(PathBuf::from(format!(
                    "/tmp/fika-thumbnail-worker-result-{}.png",
                    request.item_id().0
                ))),
            }
        })
    });

    let (lock, condvar) = &*state;
    let mut state_guard = lock.lock().unwrap();
    while state_guard.started.is_empty() {
        let wait = condvar
            .wait_timeout(state_guard, Duration::from_secs(1))
            .unwrap();
        state_guard = wait.0;
        assert!(
            !wait.1.timed_out(),
            "thumbnail worker did not start the first request"
        );
    }
    assert_eq!(state_guard.started, vec![ItemId(1)]);
    drop(state_guard);

    let canceled = cancel_handle.cancel_deferred_matching(|key| key.item_id == ItemId(2));
    assert_eq!(canceled.len(), 1);
    assert_eq!(canceled[0].item_id, ItemId(2));

    let mut state_guard = lock.lock().unwrap();
    state_guard.release = true;
    condvar.notify_all();
    drop(state_guard);

    let results = worker.join().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].item_id, ItemId(1));
    assert_eq!(lock.lock().unwrap().started, vec![ItemId(1)]);
}

fn thumbnail_candidate(
    item_id: u64,
    name: &str,
    priority: ThumbnailRequestPriority,
) -> ThumbnailCandidate {
    ThumbnailCandidate {
        item_id: ItemId(item_id),
        path: PathBuf::from(format!("/tmp/{name}")),
        modified_secs: 42,
        metadata_complete: true,
        mime_type: Some("image/png".to_string()),
        priority,
    }
}

fn test_entry(name: &str) -> Entry {
    Entry::new(EntryData {
        name: Arc::from(name),
        name_width_units: name.len() as u16,
        target_path: None,
        size_bytes: 0,
        modified_secs: Some(42),
        metadata_complete: true,
        mime_type: Some(Arc::from("image/png")),
        mime_magic_checked: true,
        trash_original_path: None,
        trash_deletion_time: None,
        is_dir: false,
    })
}
