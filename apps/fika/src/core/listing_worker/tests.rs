use super::*;
use std::fs;
use std::process;
use std::time::Duration;

#[test]
fn listing_requests_from_events_keeps_only_loading_events() {
    let first = listing_request(1, 1);
    let second = listing_request(2, 1);
    let events = [
        listing_started(&first),
        listing_completed(&first),
        listing_started(&second),
    ];

    assert_eq!(
        listing_requests_from_events(events.iter()),
        vec![first, second]
    );
}

#[test]
fn listing_worker_state_keeps_latest_pending_request_per_pane() {
    let mut state = ListingWorkerState::default();
    let old_first = listing_request(1, 1);
    let second = listing_request(2, 1);
    let new_first = listing_request(1, 2);

    state.schedule(old_first);
    state.schedule(second.clone());
    state.schedule(new_first.clone());

    assert_eq!(
        state.pop_batch().map(|batch| batch.requests),
        Some(vec![second])
    );
    assert_eq!(
        state.pop_batch().map(|batch| batch.requests),
        Some(vec![new_first])
    );
    assert_eq!(state.pop_batch(), None);
}

#[test]
fn listing_worker_state_batches_same_path_requests() {
    let mut state = ListingWorkerState::default();
    let first = listing_request_at(1, 1, "/tmp/fika-shared-listing");
    let different = listing_request_at(2, 1, "/tmp/fika-other-listing");
    let second = listing_request_at(3, 1, "/tmp/fika-shared-listing");

    state.schedule(first.clone());
    state.schedule(different.clone());
    state.schedule(second.clone());

    let shared_batch = state.pop_batch().unwrap();
    assert_eq!(shared_batch.path, PathBuf::from("/tmp/fika-shared-listing"));
    assert_eq!(shared_batch.requests, vec![first, second]);

    let different_batch = state.pop_batch().unwrap();
    assert_eq!(different_batch.requests, vec![different]);
    assert_eq!(state.pop_batch(), None);
}

#[test]
fn retarget_listing_events_preserves_shared_listing_entries() {
    let source = listing_request_at(1, 1, "/tmp/fika-shared-listing");
    let target = listing_request_at(2, 7, "/tmp/fika-shared-listing");
    let entries = Arc::new(vec![Entry::new(super::super::entries::EntryData {
        name: Arc::from("shared.txt"),
        name_width_units: 10,
        target_path: None,
        size_bytes: 4,
        modified_secs: None,
        metadata_complete: true,
        trash_original_path: None,
        trash_deletion_time: None,
        mime_type: None,
        mime_magic_checked: true,
        is_dir: false,
    })]);
    let events = vec![DirectoryListerEvent::ListingRefreshed {
        pane_id: source.pane_id,
        generation: source.generation,
        request_serial: source.request_serial,
        path: source.path.clone(),
        entries: Arc::clone(&entries),
    }];

    let retargeted = retarget_listing_events(&events, &target);

    let DirectoryListerEvent::ListingRefreshed {
        pane_id,
        generation,
        request_serial,
        path,
        entries: retargeted_entries,
    } = &retargeted[0]
    else {
        panic!("expected retargeted listing");
    };
    assert_eq!(*pane_id, target.pane_id);
    assert_eq!(*generation, target.generation);
    assert_eq!(*request_serial, target.request_serial);
    assert_eq!(path, &target.path);
    assert!(Arc::ptr_eq(&entries, retargeted_entries));
}

#[test]
fn listing_worker_state_drops_stale_results() {
    let mut state = ListingWorkerState::default();
    let old = listing_request(1, 1);
    let new = listing_request(1, 2);

    state.schedule(old.clone());
    let old_batch = listing_batch(vec![old.clone()]);
    let old_events = vec![listing_completed(&old)];
    assert!(state.publish_batch_if_current(&old_batch, &old_events));
    assert_eq!(state.results_by_pane.len(), 1);

    state.schedule(new.clone());
    assert!(state.results_by_pane.is_empty());
    assert!(!state.publish_batch_if_current(&old_batch, &old_events));
    assert!(state.drain_results().is_empty());

    let new_batch = listing_batch(vec![new.clone()]);
    let new_events = vec![listing_completed(&new)];
    assert!(state.publish_batch_if_current(&new_batch, &new_events));
    let results = state.drain_results();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0][0].request_serial(), RequestSerial(2));
}

#[test]
fn listing_worker_state_coalesces_pending_streamed_items_added() {
    let mut state = ListingWorkerState::default();
    let request = listing_request(1, 1);
    let batch = listing_batch(vec![request.clone()]);

    state.schedule(request.clone());
    assert!(state.publish_batch_if_current(&batch, &[listing_items_added(&request, &["a.txt"])]));
    assert!(state.publish_batch_if_current(&batch, &[listing_items_added(&request, &["b.txt"])]));

    let results = state.drain_results();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].len(), 1);
    let DirectoryListerEvent::ItemsAdded { entries, .. } = &results[0][0] else {
        panic!("expected coalesced items");
    };
    assert_eq!(entry_names(entries), vec!["a.txt", "b.txt"]);
}

#[test]
fn listing_worker_state_reset_event_replaces_pending_streamed_items() {
    let mut state = ListingWorkerState::default();
    let request = listing_request(1, 1);
    let batch = listing_batch(vec![request.clone()]);
    let refreshed_entries = test_entries(&["fresh.txt"]);

    state.schedule(request.clone());
    assert!(
        state.publish_batch_if_current(&batch, &[listing_items_added(&request, &["stale.txt"])])
    );
    assert!(state.publish_batch_if_current(
        &batch,
        &[listing_refreshed(&request, Arc::clone(&refreshed_entries))]
    ));

    let results = state.drain_results();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].len(), 1);
    let DirectoryListerEvent::ListingRefreshed { entries, .. } = &results[0][0] else {
        panic!("expected refreshed listing");
    };
    assert!(Arc::ptr_eq(entries, &refreshed_entries));
}

#[test]
fn listing_worker_state_cancels_closed_pane_work() {
    let mut state = ListingWorkerState::default();
    let first = listing_request_at(1, 1, "/tmp/fika-shared-listing");
    let second = listing_request_at(2, 1, "/tmp/fika-shared-listing");
    state.schedule(first.clone());
    state.schedule(second.clone());

    let batch = listing_batch(vec![first.clone(), second.clone()]);
    let events = vec![listing_completed(&first)];
    assert!(state.publish_batch_if_current(&batch, &events));
    assert_eq!(state.results_by_pane.len(), 2);

    state.cancel_pane(first.pane_id);

    assert!(!state.latest_request_by_pane.contains_key(&first.pane_id));
    assert!(!state.results_by_pane.contains_key(&first.pane_id));
    assert!(
        state
            .pending
            .iter()
            .all(|pending| pending.pane_id != first.pane_id)
    );
    assert!(state.results_by_pane.contains_key(&second.pane_id));
}

#[test]
fn listing_worker_cache_serves_load_with_shared_entries() {
    let mut state = ListingWorkerState::default();
    let first = listing_request_at(1, 1, "/tmp/fika-cached-listing");
    let second = listing_request_at(2, 2, "/tmp/fika-cached-listing");
    let entries = test_entries(&["cached.txt"]);
    let events = vec![
        listing_refreshed(&first, Arc::clone(&entries)),
        listing_completed(&first),
    ];

    state.schedule(first.clone());
    assert!(state.publish_batch_if_current(&listing_batch(vec![first]), &events));

    let cached = state.cached_events_for(&second).expect("cache miss");
    let DirectoryListerEvent::ListingRefreshed {
        pane_id,
        request_serial,
        entries: cached_entries,
        ..
    } = &cached[0]
    else {
        panic!("expected cached listing refresh");
    };
    assert_eq!(*pane_id, second.pane_id);
    assert_eq!(*request_serial, second.request_serial);
    assert!(Arc::ptr_eq(&entries, cached_entries));
    assert!(matches!(
        cached[1],
        DirectoryListerEvent::ListingCompleted { .. }
    ));
}

#[test]
fn listing_worker_cache_serves_promoted_model_snapshot() {
    let mut state = ListingWorkerState::default();
    let request = listing_request_at(7, 1, "/tmp/fika-promoted-listing");
    let entries = test_entries(&["promoted.txt"]);

    assert!(state.cache_listing_snapshot(&request.path, Arc::clone(&entries)));

    let cached = state
        .schedule_or_cached(request.clone())
        .expect("promoted snapshot should be served from cache");
    assert!(state.pending.is_empty());
    let DirectoryListerEvent::ListingRefreshed {
        pane_id,
        request_serial,
        entries: cached_entries,
        ..
    } = &cached[0]
    else {
        panic!("expected cached listing refresh");
    };
    assert_eq!(*pane_id, request.pane_id);
    assert_eq!(*request_serial, request.request_serial);
    assert!(Arc::ptr_eq(cached_entries, &entries));
}

#[test]
fn listing_worker_reports_cache_entry_budget_before_snapshot_build() {
    let state = ListingWorkerState::default();

    assert!(state.can_cache_entry_count(10_000));
    assert!(!state.can_cache_entry_count(10_001));
}

#[test]
fn listing_worker_debug_snapshot_reports_uncached_large_directories() {
    let mut state = ListingWorkerState::default();

    assert!(state.record_uncached_directory(Path::new("/tmp/fika-large-listing"), 10_001));

    let snapshot = state.cache_debug_snapshot();
    assert_eq!(snapshot.stats().skipped_large_directories, 1);
    assert_eq!(snapshot.skipped_large_directories().len(), 1);
    assert_eq!(
        snapshot.skipped_large_directories()[0].path(),
        Path::new("/tmp/fika-large-listing")
    );
    assert_eq!(
        snapshot.skipped_large_directories()[0].entry_count(),
        10_001
    );
}

#[test]
fn listing_worker_cache_hit_does_not_schedule_background_reload() {
    let mut state = ListingWorkerState::default();
    let first = listing_request_at(1, 1, "/tmp/fika-cached-listing");
    let second = listing_request_at(2, 2, "/tmp/fika-cached-listing");
    let entries = test_entries(&["cached.txt"]);
    let events = vec![
        listing_refreshed(&first, Arc::clone(&entries)),
        listing_completed(&first),
    ];

    state.schedule(first.clone());
    let first_batch = state
        .pop_batch()
        .expect("scheduled listing should be pending");
    assert_eq!(first_batch.requests, vec![first]);
    assert!(state.publish_batch_if_current(&first_batch, &events));

    let cached = state
        .schedule_or_cached(second.clone())
        .expect("fresh cache should serve request directly");

    assert_eq!(cached.len(), 2);
    assert!(state.pending.is_empty());
    assert_eq!(
        state.latest_request_by_pane.get(&second.pane_id),
        Some(&second.key())
    );
}

#[test]
fn listing_worker_reloads_when_fresh_cache_metadata_is_stale() {
    let root = temp_root("fresh-cache-stale");
    let request = listing_request_at(1, 1, root.to_str().unwrap());
    let entries = test_entries(&["cached.txt"]);
    let mut state = ListingWorkerState::default();
    assert!(state.cache_listing_snapshot(&root, Arc::clone(&entries)));

    std::thread::sleep(Duration::from_millis(20));
    fs::write(root.join("new.txt"), b"changed").unwrap();

    assert!(state.schedule_or_cached(request.clone()).is_none());
    assert_eq!(state.pending.len(), 1);
    assert_eq!(state.pending[0], request);
    assert!(state.cache.get(&root).is_none());
    assert_eq!(state.cache.cached_entry_count(), 0);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn listing_worker_cache_ignores_reload_and_can_remove_directory() {
    let mut state = ListingWorkerState::default();
    let first = listing_request_at(1, 1, "/tmp/fika-cached-listing");
    let mut reload = listing_request_at(2, 2, "/tmp/fika-cached-listing");
    reload.mode = LoadMode::Reload;
    let entries = test_entries(&["cached.txt"]);
    let events = vec![
        listing_refreshed(&first, Arc::clone(&entries)),
        listing_completed(&first),
    ];

    state.schedule(first.clone());
    assert!(state.publish_batch_if_current(&listing_batch(vec![first]), &events));

    assert!(state.cached_events_for(&reload).is_none());
    state.schedule(reload);
    assert!(
        state
            .cache
            .get(Path::new("/tmp/fika-cached-listing"))
            .is_none()
    );
    assert!(
        state
            .cached_events_for(&listing_request_at(3, 3, "/tmp/fika-cached-listing"))
            .is_none()
    );

    state.remove_cached_directory(Path::new("/tmp/fika-cached-listing"));
    assert!(
        state
            .cache
            .get(Path::new("/tmp/fika-cached-listing"))
            .is_none()
    );
}

#[test]
fn listing_worker_cache_applies_incremental_delta_for_next_load() {
    let root = temp_root("incremental-cache");
    let next = listing_request_at(2, 2, root.to_str().unwrap());
    let mut state = ListingWorkerState::default();
    assert!(state.cache_listing_snapshot(&root, test_entries(&["a.txt"])));

    fs::write(root.join("b.txt"), b"b").unwrap();
    let added = test_entries(&["b.txt"]);
    assert!(state.apply_cache_items_added(&root, added.as_slice()));

    let cached = state
        .schedule_or_cached(next.clone())
        .expect("incremental cache should serve next load");
    assert!(state.pending.is_empty());
    let DirectoryListerEvent::ListingRefreshed {
        entries: cached_entries,
        request_serial,
        ..
    } = &cached[0]
    else {
        panic!("expected cached listing refresh");
    };
    assert_eq!(*request_serial, next.request_serial);
    assert_eq!(entry_names(cached_entries), vec!["a.txt", "b.txt"]);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn listing_batch_cancelled_only_when_all_requests_are_stale() {
    let mut state = ListingWorkerState::default();
    let first = listing_request_at(1, 1, "/tmp/fika-shared-listing");
    let second = listing_request_at(2, 1, "/tmp/fika-shared-listing");
    state.schedule(first.clone());
    state.schedule(second.clone());
    let batch = listing_batch(vec![first.clone(), second.clone()]);
    let shared = Arc::new((Mutex::new(state), Condvar::new()));

    {
        let (lock, _) = &*shared;
        lock.lock()
            .expect("listing worker state poisoned")
            .cancel_pane(first.pane_id);
    }
    assert!(!listing_batch_cancelled(&shared, &batch));

    {
        let (lock, _) = &*shared;
        lock.lock()
            .expect("listing worker state poisoned")
            .cancel_pane(second.pane_id);
    }
    assert!(listing_batch_cancelled(&shared, &batch));
}

#[test]
fn loading_state_tracks_current_request_and_ignores_stale_events() {
    let mut controller =
        super::super::pane::PaneController::new(PathBuf::from("/tmp/fika-loading"));
    let pane_id = controller.focused().unwrap();
    let start = controller.reload(pane_id).unwrap();
    let mut loading = HashMap::new();
    let now = Instant::now();

    update_loading_state_for_event(
        &mut loading,
        controller.pane(pane_id),
        &start,
        now,
        Some("2 folders, 3 files".to_string()),
    );
    assert_eq!(
        loading.get(&pane_id).map(|state| state.key),
        Some(ListingRequestKey {
            generation: start.generation(),
            request_serial: start.request_serial(),
        })
    );
    assert_eq!(
        loading
            .get(&pane_id)
            .and_then(|state| state.previous_summary.as_deref()),
        Some("2 folders, 3 files")
    );

    let stale = DirectoryListerEvent::ListingCompleted {
        pane_id,
        generation: start.generation(),
        request_serial: RequestSerial(start.request_serial().0 + 1),
        path: start.path().to_path_buf(),
    };
    update_loading_state_for_event(&mut loading, controller.pane(pane_id), &stale, now, None);
    assert!(loading.contains_key(&pane_id));

    let completed = DirectoryListerEvent::ListingCompleted {
        pane_id,
        generation: start.generation(),
        request_serial: start.request_serial(),
        path: start.path().to_path_buf(),
    };
    update_loading_state_for_event(
        &mut loading,
        controller.pane(pane_id),
        &completed,
        now,
        None,
    );
    assert!(!loading.contains_key(&pane_id));
}

#[test]
fn loading_state_rejects_stale_started_event_for_old_generation() {
    let mut controller =
        super::super::pane::PaneController::new(PathBuf::from("/tmp/fika-loading-a"));
    let pane_id = controller.focused().unwrap();
    let stale = controller.reload(pane_id).unwrap();
    controller.load(pane_id, PathBuf::from("/tmp/fika-loading-b"));
    let mut loading = HashMap::new();

    update_loading_state_for_event(
        &mut loading,
        controller.pane(pane_id),
        &stale,
        Instant::now(),
        None,
    );

    assert!(loading.is_empty());
}

fn listing_request(pane: u64, serial: u64) -> ListingRequest {
    listing_request_at(pane, serial, &format!("/tmp/fika-listing-{pane}"))
}

fn listing_request_at(pane: u64, serial: u64, path: &str) -> ListingRequest {
    ListingRequest {
        pane_id: PaneId(pane),
        generation: Generation(1),
        request_serial: RequestSerial(serial),
        path: PathBuf::from(path),
        mode: LoadMode::Load,
    }
}

fn listing_batch(requests: Vec<ListingRequest>) -> ListingBatch {
    ListingBatch {
        path: requests[0].path.clone(),
        mode: requests[0].mode,
        requests,
    }
}

fn listing_started(request: &ListingRequest) -> DirectoryListerEvent {
    DirectoryListerEvent::LoadingStarted {
        pane_id: request.pane_id,
        generation: request.generation,
        request_serial: request.request_serial,
        path: request.path.clone(),
        mode: request.mode,
    }
}

fn listing_completed(request: &ListingRequest) -> DirectoryListerEvent {
    DirectoryListerEvent::ListingCompleted {
        pane_id: request.pane_id,
        generation: request.generation,
        request_serial: request.request_serial,
        path: request.path.clone(),
    }
}

fn listing_refreshed(request: &ListingRequest, entries: Arc<Vec<Entry>>) -> DirectoryListerEvent {
    DirectoryListerEvent::ListingRefreshed {
        pane_id: request.pane_id,
        generation: request.generation,
        request_serial: request.request_serial,
        path: request.path.clone(),
        entries,
    }
}

fn listing_items_added(request: &ListingRequest, names: &[&str]) -> DirectoryListerEvent {
    DirectoryListerEvent::ItemsAdded {
        pane_id: request.pane_id,
        generation: request.generation,
        request_serial: request.request_serial,
        path: request.path.clone(),
        entries: test_entries(names).as_ref().clone(),
    }
}

fn test_entries(names: &[&str]) -> Arc<Vec<Entry>> {
    Arc::new(
        names
            .iter()
            .map(|name| {
                Entry::new(super::super::entries::EntryData {
                    name: Arc::from(*name),
                    name_width_units: name.len() as u16,
                    target_path: None,
                    size_bytes: 0,
                    modified_secs: None,
                    metadata_complete: true,
                    trash_original_path: None,
                    trash_deletion_time: None,
                    mime_type: None,
                    mime_magic_checked: true,
                    is_dir: false,
                })
            })
            .collect(),
    )
}

fn entry_names(entries: &[Entry]) -> Vec<String> {
    entries.iter().map(|entry| entry.name.to_string()).collect()
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("fika-listing-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}
