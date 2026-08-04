use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use super::super::{
    entries::ItemId,
    model::DirectoryModel,
    pane::{Generation, PaneId},
};
use super::{
    ThumbnailRequest, ThumbnailRequestPriority, ThumbnailRequestQueue, ThumbnailerRegistry,
    cached_thumbnail_for_request, default_thumbnail_cache_root,
    generate_thumbnail_with_external_thumbnailer_registry, thumbnail_failure_is_cached,
    thumbnail_request_may_have_preview,
};

const THUMBNAIL_PROBE_WORKER_LIMIT: usize = 4;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ThumbnailWorkKey {
    pub pane_id: PaneId,
    pub generation: Generation,
    pub item_id: ItemId,
    pub modified_secs: u64,
    pub path_hash: u64,
    pub mime_hash: Option<u64>,
}

impl ThumbnailWorkKey {
    pub fn from_candidate(
        pane_id: PaneId,
        generation: Generation,
        candidate: &ThumbnailCandidate,
    ) -> Self {
        Self {
            pane_id,
            generation,
            item_id: candidate.item_id,
            modified_secs: candidate.modified_secs,
            path_hash: stable_hash(&candidate.path),
            mime_hash: candidate.mime_type.as_deref().map(stable_hash),
        }
    }

    pub fn from_request(request: &ThumbnailRequest) -> Self {
        Self {
            pane_id: request.pane_id(),
            generation: request.generation(),
            item_id: request.item_id(),
            modified_secs: request.modified_secs(),
            path_hash: stable_hash(request.path()),
            mime_hash: request.mime_type().map(stable_hash),
        }
    }
}

fn stable_hash(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailCandidate {
    pub item_id: ItemId,
    pub path: PathBuf,
    pub modified_secs: u64,
    pub metadata_complete: bool,
    pub mime_type: Option<String>,
    pub priority: ThumbnailRequestPriority,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailProbeResult {
    pub pane_id: PaneId,
    pub generation: Generation,
    pub item_id: ItemId,
    pub path: PathBuf,
    pub modified_secs: u64,
    pub thumbnail_path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct ThumbnailProbeBatch {
    pub cache_root: PathBuf,
    pub requests: Vec<ThumbnailRequest>,
    pub cancel_handle: ThumbnailProbeCancelHandle,
}

#[derive(Debug)]
pub struct ThumbnailScheduler {
    requests: ThumbnailRequestQueue,
    seen: HashSet<ThumbnailWorkKey>,
    probe_pending: bool,
    probe_cancel: Option<ThumbnailProbeCancelHandle>,
    cache_root: PathBuf,
}

impl Default for ThumbnailScheduler {
    fn default() -> Self {
        Self::new(default_thumbnail_cache_root())
    }
}

impl ThumbnailScheduler {
    pub fn new(cache_root: PathBuf) -> Self {
        Self {
            requests: ThumbnailRequestQueue::default(),
            seen: HashSet::new(),
            probe_pending: false,
            probe_cancel: None,
            cache_root,
        }
    }

    pub fn queue_candidates(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        candidates: impl IntoIterator<Item = ThumbnailCandidate>,
    ) -> bool {
        let mut keep = HashSet::new();
        let mut pending = Vec::new();
        for candidate in candidates {
            let (key, request) = thumbnail_candidate_request(pane_id, generation, candidate);
            let Some(request) = request else {
                continue;
            };
            keep.insert(key.clone());
            pending.push((key, request));
        }
        self.prune_generation_work(pane_id, generation, &keep);

        let mut queued = false;
        for (key, request) in pending {
            if self.seen.contains(&key) {
                if request.priority() == ThumbnailRequestPriority::Visible
                    && ((self.requests.contains(&request)
                        && self.requests.enqueue(request.clone()))
                        || self.cancel_active_deferred_request(&request)
                            && self.requests.enqueue(request))
                {
                    queued = true;
                }
                continue;
            }
            if self.requests.enqueue(request) {
                self.seen.insert(key);
                queued = true;
            }
        }
        queued
    }

    pub fn start_probe_batch(&mut self, batch_size: usize) -> Option<ThumbnailProbeBatch> {
        if self.probe_pending || self.requests.is_empty() {
            return None;
        }
        let requests = self.take_probe_batch(batch_size);
        if requests.is_empty() {
            return None;
        }
        let cancel_handle = ThumbnailProbeCancelHandle::from_requests(&requests);
        self.probe_cancel = Some(cancel_handle.clone());
        self.probe_pending = true;
        Some(ThumbnailProbeBatch {
            cache_root: self.cache_root.clone(),
            requests,
            cancel_handle,
        })
    }

    pub fn finish_probe_batch(&mut self) {
        self.probe_pending = false;
        self.probe_cancel = None;
    }

    pub fn cancel_pane(&mut self, pane_id: PaneId) {
        self.requests.cancel_pane(pane_id);
        if let Some(cancel) = &self.probe_cancel {
            cancel.cancel_matching(|key, _| key.pane_id == pane_id);
        }
        self.seen.retain(|key| key.pane_id != pane_id);
    }

    pub fn cancel_stale_pane_generations(&mut self, pane_id: PaneId, generation: Generation) {
        self.requests.cancel_stale_generations(pane_id, generation);
        if let Some(cancel) = &self.probe_cancel {
            cancel.cancel_matching(|key, _| key.pane_id == pane_id && key.generation != generation);
        }
        self.seen
            .retain(|key| key.pane_id != pane_id || key.generation == generation);
    }

    pub fn set_cache_root(&mut self, cache_root: PathBuf) {
        self.cache_root = cache_root;
    }

    pub fn queued_len(&self) -> usize {
        self.requests.len()
    }

    pub fn seen_len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    pub fn pop_next_request(&mut self) -> Option<ThumbnailRequest> {
        self.requests.pop_next()
    }

    pub fn contains_seen(&self, key: &ThumbnailWorkKey) -> bool {
        self.seen.contains(key)
    }

    fn cancel_active_deferred_request(&self, request: &ThumbnailRequest) -> bool {
        self.probe_cancel
            .as_ref()
            .is_some_and(|cancel| cancel.cancel_deferred_request(request))
    }

    fn take_probe_batch(&mut self, batch_size: usize) -> Vec<ThumbnailRequest> {
        let mut requests = Vec::new();
        while requests.len() < batch_size {
            let Some(request) = self.requests.pop_next() else {
                break;
            };
            requests.push(request);
        }
        requests
    }

    fn prune_generation_work(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        keep: &HashSet<ThumbnailWorkKey>,
    ) {
        let removed_queued = self.requests.cancel_matching(|request| {
            request.pane_id() == pane_id
                && request.generation() == generation
                && !keep.contains(&ThumbnailWorkKey::from_request(request))
        });
        for request in removed_queued {
            self.seen.remove(&ThumbnailWorkKey::from_request(&request));
        }

        if let Some(cancel) = &self.probe_cancel {
            for key in cancel.cancel_matching(|key, _| {
                key.pane_id == pane_id && key.generation == generation && !keep.contains(key)
            }) {
                self.seen.remove(&key);
            }
        }

        self.seen.retain(|key| {
            key.pane_id != pane_id || key.generation != generation || keep.contains(key)
        });
    }
}

#[derive(Clone, Debug)]
pub struct ThumbnailProbeCancelHandle {
    state: Arc<Mutex<ThumbnailProbeCancelState>>,
}

#[derive(Debug)]
struct ThumbnailProbeCancelState {
    active: HashMap<ThumbnailWorkKey, ThumbnailRequestPriority>,
    canceled: HashSet<ThumbnailWorkKey>,
}

impl ThumbnailProbeCancelHandle {
    pub fn from_requests(requests: &[ThumbnailRequest]) -> Self {
        Self {
            state: Arc::new(Mutex::new(ThumbnailProbeCancelState {
                active: requests
                    .iter()
                    .map(|request| (ThumbnailWorkKey::from_request(request), request.priority()))
                    .collect(),
                canceled: HashSet::new(),
            })),
        }
    }

    pub fn begin_request(&self, request: &ThumbnailRequest) -> bool {
        let key = ThumbnailWorkKey::from_request(request);
        let Ok(mut state) = self.state.lock() else {
            return true;
        };
        if state.canceled.remove(&key) {
            state.active.remove(&key);
            return false;
        }
        state.active.remove(&key);
        true
    }

    pub fn cancel_deferred_matching(
        &self,
        predicate: impl Fn(&ThumbnailWorkKey) -> bool,
    ) -> Vec<ThumbnailWorkKey> {
        self.cancel_matching(|key, priority| {
            *priority == ThumbnailRequestPriority::Deferred && predicate(key)
        })
    }

    pub fn cancel_matching(
        &self,
        predicate: impl Fn(&ThumbnailWorkKey, &ThumbnailRequestPriority) -> bool,
    ) -> Vec<ThumbnailWorkKey> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        let keys = state
            .active
            .iter()
            .filter(|(key, priority)| !state.canceled.contains(*key) && predicate(key, priority))
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in &keys {
            state.canceled.insert(key.clone());
        }
        keys
    }

    pub fn cancel_deferred_request(&self, request: &ThumbnailRequest) -> bool {
        let key = ThumbnailWorkKey::from_request(request);
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if state.active.get(&key) != Some(&ThumbnailRequestPriority::Deferred)
            || state.canceled.contains(&key)
        {
            return false;
        }
        state.canceled.insert(key);
        true
    }
}

pub fn thumbnail_candidate_failure_is_cached(
    cache_root: &Path,
    pane_id: PaneId,
    generation: Generation,
    candidate: ThumbnailCandidate,
) -> (ThumbnailWorkKey, Option<ThumbnailRequest>, bool) {
    let (key, request) = thumbnail_candidate_request(pane_id, generation, candidate);
    let failure_cached = request.as_ref().is_some_and(|request| {
        thumbnail_failure_is_cached(cache_root, request.uri(), request.modified_secs())
    });
    (key, request, failure_cached)
}

fn thumbnail_candidate_request(
    pane_id: PaneId,
    generation: Generation,
    candidate: ThumbnailCandidate,
) -> (ThumbnailWorkKey, Option<ThumbnailRequest>) {
    let key = ThumbnailWorkKey::from_candidate(pane_id, generation, &candidate);
    if !candidate.metadata_complete {
        return (key, None);
    }
    if !thumbnail_request_may_have_preview(&candidate.path, candidate.mime_type.as_deref()) {
        return (key, None);
    }
    let request = ThumbnailRequest::from_entry_metadata_with_mime(
        pane_id,
        generation,
        candidate.item_id,
        candidate.path,
        candidate.modified_secs,
        candidate.mime_type,
        candidate.priority,
    );
    (key, request)
}

pub fn thumbnail_probe_results_for_requests(
    cache_root: PathBuf,
    requests: Vec<ThumbnailRequest>,
    cancel_handle: ThumbnailProbeCancelHandle,
) -> Vec<ThumbnailProbeResult> {
    let thumbnailers = ThumbnailerRegistry::shared_system();
    thumbnail_probe_results_with_worker(
        requests,
        THUMBNAIL_PROBE_WORKER_LIMIT,
        Some(cancel_handle),
        |request| thumbnail_probe_result_for_request(&cache_root, thumbnailers, request),
    )
}

pub fn apply_thumbnail_probe_result_to_model(
    model: &mut DirectoryModel,
    result: ThumbnailProbeResult,
) -> bool {
    let Some(index) = model.index_of_id(result.item_id) else {
        return false;
    };
    if model.path_for_index(index).as_deref() != Some(result.path.as_path()) {
        return false;
    }
    if model.entries()[index].effective_modified_secs() != Some(result.modified_secs) {
        return false;
    }
    let signals = match result.thumbnail_path {
        Some(thumbnail_path) => model.set_thumbnail_path(result.item_id, Some(thumbnail_path)),
        None => model.set_thumbnail_failed(result.item_id, true),
    };
    !signals.is_empty()
}

fn thumbnail_probe_results_with_worker(
    requests: Vec<ThumbnailRequest>,
    worker_limit: usize,
    cancel_handle: Option<ThumbnailProbeCancelHandle>,
    worker: impl Fn(ThumbnailRequest) -> ThumbnailProbeResult + Send + Sync,
) -> Vec<ThumbnailProbeResult> {
    if requests.is_empty() {
        return Vec::new();
    }

    let worker_count = worker_limit.clamp(1, requests.len());
    let queue = Arc::new(Mutex::new(VecDeque::from(requests)));
    let results = Arc::new(Mutex::new(Vec::new()));

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let results = Arc::clone(&results);
            let cancel_handle = cancel_handle.clone();
            let worker = &worker;
            scope.spawn(move || {
                loop {
                    let request = queue.lock().ok().and_then(|mut queue| queue.pop_front());
                    let Some(request) = request else {
                        break;
                    };
                    if cancel_handle
                        .as_ref()
                        .is_some_and(|handle| !handle.begin_request(&request))
                    {
                        continue;
                    }
                    let result = worker(request);
                    if let Ok(mut results) = results.lock() {
                        results.push(result);
                    }
                }
            });
        }
    });

    Arc::try_unwrap(results)
        .ok()
        .and_then(|results| results.into_inner().ok())
        .unwrap_or_default()
}

fn thumbnail_probe_result_for_request(
    cache_root: &Path,
    thumbnailers: &ThumbnailerRegistry,
    request: ThumbnailRequest,
) -> ThumbnailProbeResult {
    let thumbnail = cached_thumbnail_for_request(cache_root, &request).or_else(|| {
        generate_thumbnail_with_external_thumbnailer_registry(cache_root, &request, thumbnailers)
            .ok()
            .flatten()
    });
    let thumbnail_path = thumbnail.map(|thumbnail| thumbnail.path().to_path_buf());
    ThumbnailProbeResult {
        pane_id: request.pane_id(),
        generation: request.generation(),
        item_id: request.item_id(),
        path: request.path().to_path_buf(),
        modified_secs: request.modified_secs(),
        thumbnail_path,
    }
}

#[cfg(test)]
mod tests;
