use super::cache::{DirectoryCache, DirectoryCacheDebugSnapshot};
use super::directory::{DirectoryLister, DirectoryListerEvent, LoadMode, RefreshPair};
use super::entries::Entry;
use super::pane::{Generation, PaneId, PaneState, RequestSerial};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListingRequestKey {
    pub generation: Generation,
    pub request_serial: RequestSerial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadingPaneState {
    pub key: ListingRequestKey,
    pub started_at: Instant,
    pub previous_summary: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListingRequest {
    pub pane_id: PaneId,
    pub generation: Generation,
    pub request_serial: RequestSerial,
    pub path: PathBuf,
    pub mode: LoadMode,
}

impl ListingRequest {
    pub fn from_event(event: &DirectoryListerEvent) -> Option<Self> {
        let DirectoryListerEvent::LoadingStarted {
            pane_id,
            generation,
            request_serial,
            path,
            mode,
        } = event
        else {
            return None;
        };
        Some(Self {
            pane_id: *pane_id,
            generation: *generation,
            request_serial: *request_serial,
            path: path.clone(),
            mode: *mode,
        })
    }

    pub fn key(&self) -> ListingRequestKey {
        ListingRequestKey {
            generation: self.generation,
            request_serial: self.request_serial,
        }
    }
}

pub fn listing_requests_from_events<'a>(
    events: impl IntoIterator<Item = &'a DirectoryListerEvent>,
) -> Vec<ListingRequest> {
    events
        .into_iter()
        .filter_map(ListingRequest::from_event)
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ListingBatch {
    path: PathBuf,
    mode: LoadMode,
    requests: Vec<ListingRequest>,
}

impl ListingBatch {
    fn read_events_cancellable(&self, state: &Arc<(Mutex<ListingWorkerState>, Condvar)>) {
        let Some(request) = self.requests.first() else {
            return;
        };
        let _ = DirectoryLister::read_listing_events_streaming_cancellable(
            request.pane_id,
            request.generation,
            request.request_serial,
            self.path.clone(),
            self.mode,
            || listing_batch_cancelled(state, self),
            |events| {
                let (lock, _) = &**state;
                let mut guard = lock.lock().expect("listing worker state poisoned");
                guard.publish_batch_if_current(self, &events);
            },
        );
    }
}

#[derive(Debug, Default)]
struct ListingWorkerState {
    pending: VecDeque<ListingRequest>,
    latest_request_by_pane: HashMap<PaneId, ListingRequestKey>,
    results_by_pane: BTreeMap<PaneId, Vec<DirectoryListerEvent>>,
    cache: DirectoryCache,
    result_notifier: Option<mpsc::Sender<()>>,
    shutdown: bool,
}

impl ListingWorkerState {
    fn schedule(&mut self, request: ListingRequest) {
        if request.mode == LoadMode::Reload {
            self.cache.mark_stale(&request.path);
        }
        self.pending
            .retain(|pending| pending.pane_id != request.pane_id);
        self.latest_request_by_pane
            .insert(request.pane_id, request.key());
        self.results_by_pane.remove(&request.pane_id);
        self.pending.push_back(request);
    }

    fn cancel_pane(&mut self, pane_id: PaneId) {
        self.pending.retain(|pending| pending.pane_id != pane_id);
        self.latest_request_by_pane.remove(&pane_id);
        self.results_by_pane.remove(&pane_id);
    }

    fn mark_cache_stale(&mut self, path: &Path) {
        self.cache.mark_stale(path);
    }

    fn apply_cache_items_added(&mut self, path: &Path, entries: &[Entry]) -> bool {
        let applied = self.cache.apply_items_added(path, entries);
        if !applied {
            self.cache.mark_stale(path);
        }
        applied
    }

    fn apply_cache_items_deleted(&mut self, path: &Path, paths: &[PathBuf]) -> bool {
        let applied = self.cache.apply_items_deleted(path, paths);
        if !applied {
            self.cache.mark_stale(path);
        }
        applied
    }

    fn apply_cache_items_refreshed(&mut self, path: &Path, pairs: &[RefreshPair]) -> bool {
        let applied = self.cache.apply_items_refreshed(path, pairs);
        if !applied {
            self.cache.mark_stale(path);
        }
        applied
    }

    fn remove_cached_directory(&mut self, path: &Path) {
        self.cache.remove(path);
    }

    fn cache_listing_snapshot(&mut self, path: &Path, entries: Arc<Vec<Entry>>) -> bool {
        self.cache.insert_fresh(path, entries).is_some()
    }

    fn can_cache_entry_count(&self, entry_count: usize) -> bool {
        self.cache.can_store_entry_count(entry_count)
    }

    fn record_uncached_directory(&mut self, path: &Path, entry_count: usize) -> bool {
        self.cache.record_uncached_directory(path, entry_count)
    }

    fn cache_debug_snapshot(&self) -> DirectoryCacheDebugSnapshot {
        self.cache.debug_snapshot()
    }

    fn cached_events_for(&mut self, request: &ListingRequest) -> Option<Vec<DirectoryListerEvent>> {
        if request.mode != LoadMode::Load {
            return None;
        }
        let snapshot = self.cache.get_fresh(&request.path)?;
        Some(vec![
            DirectoryListerEvent::ListingRefreshed {
                pane_id: request.pane_id,
                generation: request.generation,
                request_serial: request.request_serial,
                path: request.path.clone(),
                entries: Arc::clone(snapshot.entries()),
            },
            DirectoryListerEvent::ListingCompleted {
                pane_id: request.pane_id,
                generation: request.generation,
                request_serial: request.request_serial,
                path: request.path.clone(),
            },
        ])
    }

    fn schedule_or_cached(&mut self, request: ListingRequest) -> Option<Vec<DirectoryListerEvent>> {
        if let Some(events) = self.cached_events_for(&request) {
            self.pending
                .retain(|pending| pending.pane_id != request.pane_id);
            self.latest_request_by_pane
                .insert(request.pane_id, request.key());
            self.results_by_pane.remove(&request.pane_id);
            return Some(events);
        }

        self.schedule(request);
        None
    }

    fn pop_batch(&mut self) -> Option<ListingBatch> {
        while let Some(leader) = self.pending.pop_front() {
            if !self.is_current(&leader) {
                continue;
            }

            let path = leader.path.clone();
            let mode = leader.mode;
            let mut requests = vec![leader];
            let mut index = 0;
            while index < self.pending.len() {
                let Some(pending) = self.pending.get(index) else {
                    break;
                };
                if !self.is_current(pending) {
                    self.pending.remove(index);
                    continue;
                }
                if pending.path == path && pending.mode == mode {
                    if let Some(request) = self.pending.remove(index) {
                        requests.push(request);
                    }
                    continue;
                }
                index += 1;
            }

            return Some(ListingBatch {
                path,
                mode,
                requests,
            });
        }
        None
    }

    fn is_current(&self, request: &ListingRequest) -> bool {
        self.latest_request_by_pane
            .get(&request.pane_id)
            .is_some_and(|key| *key == request.key())
    }

    fn publish_batch_if_current(
        &mut self,
        batch: &ListingBatch,
        events: &[DirectoryListerEvent],
    ) -> bool {
        if self.shutdown {
            return false;
        }
        let mut published = false;
        for request in &batch.requests {
            if !self.is_current(request) {
                continue;
            }
            append_listing_results_for_pane(
                self.results_by_pane.entry(request.pane_id).or_default(),
                retarget_listing_events(events, request),
            );
            published = true;
        }
        if published && let Some(entries) = listing_refreshed_entries(events) {
            self.cache.insert_fresh(&batch.path, entries);
        }
        if let Some(notifier) = &self.result_notifier {
            let _ = notifier.send(());
        }
        published
    }

    fn drain_results(&mut self) -> Vec<Vec<DirectoryListerEvent>> {
        std::mem::take(&mut self.results_by_pane)
            .into_values()
            .collect()
    }
}

pub struct ListingWorker {
    state: Arc<(Mutex<ListingWorkerState>, Condvar)>,
    handle: Option<JoinHandle<()>>,
}

impl ListingWorker {
    pub fn new() -> Self {
        Self::with_optional_result_notifier(None)
    }

    pub fn with_result_notifier(result_notifier: mpsc::Sender<()>) -> Self {
        Self::with_optional_result_notifier(Some(result_notifier))
    }

    fn with_optional_result_notifier(result_notifier: Option<mpsc::Sender<()>>) -> Self {
        let state = Arc::new((Mutex::new(ListingWorkerState::default()), Condvar::new()));
        {
            let (lock, _) = &*state;
            lock.lock()
                .expect("listing worker state poisoned")
                .result_notifier = result_notifier;
        }
        let worker_state = Arc::clone(&state);
        let handle = std::thread::spawn(move || listing_worker_loop(worker_state));
        Self {
            state,
            handle: Some(handle),
        }
    }

    pub fn schedule_all(&self, requests: Vec<ListingRequest>) {
        if requests.is_empty() {
            return;
        }
        let (lock, cvar) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        if state.shutdown {
            return;
        }
        for request in requests {
            state.schedule(request);
        }
        cvar.notify_one();
    }

    pub fn schedule_or_cached(&self, request: ListingRequest) -> Option<Vec<DirectoryListerEvent>> {
        let (lock, cvar) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        if state.shutdown {
            return None;
        }
        let cached_events = state.schedule_or_cached(request);
        if cached_events.is_none() {
            cvar.notify_one();
        }
        cached_events
    }

    pub fn mark_cache_stale(&self, path: &Path) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        state.mark_cache_stale(path);
    }

    pub fn apply_cache_event(&self, event: &DirectoryListerEvent) -> bool {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        match event {
            DirectoryListerEvent::ItemsAdded { path, entries, .. } => {
                state.apply_cache_items_added(path, entries)
            }
            DirectoryListerEvent::ItemsDeleted { path, paths, .. } => {
                state.apply_cache_items_deleted(path, paths)
            }
            DirectoryListerEvent::ItemsRefreshed { path, pairs, .. } => {
                state.apply_cache_items_refreshed(path, pairs)
            }
            DirectoryListerEvent::CurrentDirectoryRemoved { path, .. } => {
                state.remove_cached_directory(path);
                true
            }
            DirectoryListerEvent::LoadingStarted { mode, path, .. }
                if *mode == LoadMode::Reload =>
            {
                state.mark_cache_stale(path);
                false
            }
            _ => false,
        }
    }

    pub fn remove_cached_directory(&self, path: &Path) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        state.remove_cached_directory(path);
    }

    pub fn cache_listing_snapshot(&self, path: &Path, entries: Arc<Vec<Entry>>) -> bool {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        if state.shutdown {
            return false;
        }
        state.cache_listing_snapshot(path, entries)
    }

    pub fn can_cache_entry_count(&self, entry_count: usize) -> bool {
        let (lock, _) = &*self.state;
        let state = lock.lock().expect("listing worker state poisoned");
        !state.shutdown && state.can_cache_entry_count(entry_count)
    }

    pub fn record_uncached_directory(&self, path: &Path, entry_count: usize) -> bool {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        if state.shutdown {
            return false;
        }
        state.record_uncached_directory(path, entry_count)
    }

    pub fn cache_debug_snapshot(&self) -> DirectoryCacheDebugSnapshot {
        let (lock, _) = &*self.state;
        lock.lock()
            .expect("listing worker state poisoned")
            .cache_debug_snapshot()
    }

    pub fn cancel_pane(&self, pane_id: PaneId) {
        let (lock, cvar) = &*self.state;
        let mut state = lock.lock().expect("listing worker state poisoned");
        state.cancel_pane(pane_id);
        cvar.notify_one();
    }

    pub fn drain_results(&self) -> Vec<Vec<DirectoryListerEvent>> {
        let (lock, _) = &*self.state;
        lock.lock()
            .expect("listing worker state poisoned")
            .drain_results()
    }

    pub fn pending_count(&self) -> usize {
        let (lock, _) = &*self.state;
        lock.lock()
            .expect("listing worker state poisoned")
            .pending
            .len()
    }
}

impl Default for ListingWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ListingWorker {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.state;
        if let Ok(mut state) = lock.lock() {
            state.shutdown = true;
            state.pending.clear();
            state.results_by_pane.clear();
            cvar.notify_one();
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn listing_batch_cancelled(
    state: &Arc<(Mutex<ListingWorkerState>, Condvar)>,
    batch: &ListingBatch,
) -> bool {
    let (lock, _) = &**state;
    let guard = lock.lock().expect("listing worker state poisoned");
    guard.shutdown
        || !batch
            .requests
            .iter()
            .any(|request| guard.is_current(request))
}

fn retarget_listing_events(
    events: &[DirectoryListerEvent],
    target: &ListingRequest,
) -> Vec<DirectoryListerEvent> {
    events
        .iter()
        .map(|event| retarget_listing_event(event, target))
        .collect()
}

fn retarget_listing_event(
    event: &DirectoryListerEvent,
    target: &ListingRequest,
) -> DirectoryListerEvent {
    match event {
        DirectoryListerEvent::LoadingStarted { .. } => DirectoryListerEvent::LoadingStarted {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            mode: target.mode,
        },
        DirectoryListerEvent::ItemsAdded { entries, .. } => DirectoryListerEvent::ItemsAdded {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            entries: entries.clone(),
        },
        DirectoryListerEvent::ItemsDeleted { paths, .. } => DirectoryListerEvent::ItemsDeleted {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            paths: paths.clone(),
        },
        DirectoryListerEvent::ItemsRefreshed { pairs, .. } => {
            DirectoryListerEvent::ItemsRefreshed {
                pane_id: target.pane_id,
                generation: target.generation,
                request_serial: target.request_serial,
                path: target.path.clone(),
                pairs: pairs.clone(),
            }
        }
        DirectoryListerEvent::ListingRefreshed { entries, .. } => {
            DirectoryListerEvent::ListingRefreshed {
                pane_id: target.pane_id,
                generation: target.generation,
                request_serial: target.request_serial,
                path: target.path.clone(),
                entries: Arc::clone(entries),
            }
        }
        DirectoryListerEvent::ListingCompleted { .. } => DirectoryListerEvent::ListingCompleted {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
        },
        DirectoryListerEvent::CurrentDirectoryRemoved { .. } => {
            DirectoryListerEvent::CurrentDirectoryRemoved {
                pane_id: target.pane_id,
                generation: target.generation,
                request_serial: target.request_serial,
                path: target.path.clone(),
            }
        }
        DirectoryListerEvent::Error { message, .. } => DirectoryListerEvent::Error {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            message: message.clone(),
        },
        DirectoryListerEvent::NetworkAuthRequired {
            uri,
            message,
            default_username,
            default_domain,
            ..
        } => DirectoryListerEvent::NetworkAuthRequired {
            pane_id: target.pane_id,
            generation: target.generation,
            request_serial: target.request_serial,
            path: target.path.clone(),
            uri: uri.clone(),
            message: message.clone(),
            default_username: default_username.clone(),
            default_domain: default_domain.clone(),
        },
    }
}

fn listing_refreshed_entries(events: &[DirectoryListerEvent]) -> Option<Arc<Vec<Entry>>> {
    events.iter().find_map(|event| {
        if let DirectoryListerEvent::ListingRefreshed { entries, .. } = event {
            Some(Arc::clone(entries))
        } else {
            None
        }
    })
}

fn append_listing_results_for_pane(
    results: &mut Vec<DirectoryListerEvent>,
    events: Vec<DirectoryListerEvent>,
) {
    for event in events {
        match event {
            DirectoryListerEvent::ListingRefreshed { .. }
            | DirectoryListerEvent::CurrentDirectoryRemoved { .. }
            | DirectoryListerEvent::Error { .. }
            | DirectoryListerEvent::NetworkAuthRequired { .. } => {
                results.clear();
                results.push(event);
            }
            DirectoryListerEvent::ItemsAdded { entries, .. } if entries.is_empty() => {}
            DirectoryListerEvent::ItemsAdded {
                pane_id,
                generation,
                request_serial,
                path,
                mut entries,
            } => {
                if let Some(DirectoryListerEvent::ItemsAdded {
                    pane_id: last_pane_id,
                    generation: last_generation,
                    request_serial: last_request_serial,
                    path: last_path,
                    entries: last_entries,
                }) = results.last_mut()
                    && *last_pane_id == pane_id
                    && *last_generation == generation
                    && *last_request_serial == request_serial
                    && *last_path == path
                {
                    last_entries.append(&mut entries);
                } else {
                    results.push(DirectoryListerEvent::ItemsAdded {
                        pane_id,
                        generation,
                        request_serial,
                        path,
                        entries,
                    });
                }
            }
            DirectoryListerEvent::ItemsDeleted { paths, .. } if paths.is_empty() => {}
            DirectoryListerEvent::ItemsRefreshed { pairs, .. } if pairs.is_empty() => {}
            event => results.push(event),
        }
    }
}

pub fn update_loading_state_for_event(
    loading_panes: &mut HashMap<PaneId, LoadingPaneState>,
    pane: Option<&PaneState>,
    event: &DirectoryListerEvent,
    now: Instant,
    previous_summary: Option<String>,
) {
    match event {
        DirectoryListerEvent::LoadingStarted {
            pane_id,
            generation,
            request_serial,
            ..
        } => {
            if pane.is_some_and(|pane| {
                event.matches_target(pane.id, pane.generation, &pane.current_dir)
            }) {
                loading_panes.insert(
                    *pane_id,
                    LoadingPaneState {
                        key: ListingRequestKey {
                            generation: *generation,
                            request_serial: *request_serial,
                        },
                        started_at: now,
                        previous_summary,
                    },
                );
            } else {
                loading_panes.remove(pane_id);
            }
        }
        DirectoryListerEvent::ListingCompleted {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::Error {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::NetworkAuthRequired {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::CurrentDirectoryRemoved {
            pane_id,
            generation,
            request_serial,
            ..
        } => {
            let key = ListingRequestKey {
                generation: *generation,
                request_serial: *request_serial,
            };
            if loading_panes
                .get(pane_id)
                .is_some_and(|state| state.key == key)
            {
                loading_panes.remove(pane_id);
            }
        }
        DirectoryListerEvent::ListingRefreshed {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::ItemsAdded {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::ItemsDeleted {
            pane_id,
            generation,
            request_serial,
            ..
        }
        | DirectoryListerEvent::ItemsRefreshed {
            pane_id,
            generation,
            request_serial,
            ..
        } => {
            let Some(pane) = pane else {
                loading_panes.remove(pane_id);
                return;
            };
            if pane.generation != *generation
                || loading_panes
                    .get(pane_id)
                    .is_some_and(|state| state.key.request_serial < *request_serial)
            {
                loading_panes.remove(pane_id);
            }
        }
    }
}

fn listing_worker_loop(state: Arc<(Mutex<ListingWorkerState>, Condvar)>) {
    loop {
        let batch = {
            let (lock, cvar) = &*state;
            let mut guard = lock.lock().expect("listing worker state poisoned");
            while guard.pending.is_empty() && !guard.shutdown {
                guard = cvar.wait(guard).expect("listing worker state poisoned");
            }
            if guard.shutdown {
                return;
            }
            guard
                .pop_batch()
                .expect("pending listing request disappeared")
        };

        batch.read_events_cancellable(&state);
    }
}

#[cfg(test)]
#[path = "listing_worker/tests.rs"]
mod tests;
