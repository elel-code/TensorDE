impl ThumbnailSourceResolver {
    fn new() -> Self {
        Self::with_cache_root(default_thumbnail_cache_root())
    }

    fn with_cache_root(cache_root: PathBuf) -> Self {
        let (request_tx, request_rx) = mpsc::channel::<ThumbnailSourceRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ThumbnailSourceResult>();
        let request_tx = thread::Builder::new()
            .name("tensor-files-thumbnail-source".to_string())
            .spawn(move || thumbnail_source_worker(cache_root, request_rx, result_tx))
            .ok()
            .map(|_| request_tx);
        Self {
            ready: HashMap::new(),
            ready_sizes: HashMap::new(),
            failed: HashSet::new(),
            pending: HashMap::new(),
            ready_frame: 0,
            ready_bytes: 0,
            ready_max_bytes: THUMBNAIL_READY_CACHE_MAX_BYTES,
            request_tx,
            result_rx,
        }
    }

    fn resolve(
        &mut self,
        path: &Path,
        modified_secs: u64,
        mime_type: Option<String>,
        size_px: u16,
    ) -> ThumbnailResolveState {
        self.drain_results();
        // Keep the ready entry (FileManager-style cache hit). Removing here forced a
        // re-queue every frame and thrashed GPU uploads / flash of MIME icons.
        if let Some(source) = self.take_exact_ready(path, modified_secs, size_px) {
            return ThumbnailResolveState::Ready(source);
        }
        // Zoom / first-frame: paint a nearby size while the exact bucket loads.
        let closest = self.take_closest_ready(path, modified_secs, size_px);
        let shared_path = self
            .ready_sizes
            .get_key_value(path)
            .map(|(indexed_path, _)| Arc::clone(indexed_path))
            .unwrap_or_else(|| Arc::from(path));
        let key = ThumbnailSourceKey::thumbnail(
            Arc::clone(&shared_path),
            size_px,
            modified_secs,
        );
        let failure_key = ThumbnailProbeCacheKey::new(shared_path, modified_secs);
        if let Some(source) = closest {
            // Still ensure the exact size is queued as visible work.
            if !self.pending.contains_key(&key) && !self.failed.contains(&failure_key) {
                let _ = self.send_request(
                    key,
                    mime_type,
                    ThumbnailRequestPriority::Visible,
                    failure_key,
                );
            }
            return ThumbnailResolveState::Ready(source);
        }
        if self.failed.contains(&failure_key) {
            return ThumbnailResolveState::Failed;
        }
        match self.pending.get(&key).copied() {
            Some(ThumbnailRequestPriority::Visible) => return ThumbnailResolveState::Pending,
            Some(ThumbnailRequestPriority::Deferred) | None => {}
        }
        if self.send_request(
            key,
            mime_type,
            ThumbnailRequestPriority::Visible,
            failure_key,
        ) {
            ThumbnailResolveState::Pending
        } else {
            ThumbnailResolveState::Failed
        }
    }

    fn take_exact_ready(
        &mut self,
        path: &Path,
        modified_secs: u64,
        size_px: u16,
    ) -> Option<IconGpuSource> {
        let (indexed_path, stamps) = self.ready_sizes.get_key_value(path)?;
        let sizes = stamps.get(&modified_secs)?;
        if !sizes.contains(&size_px) {
            return None;
        }
        let key = ThumbnailSourceKey::thumbnail(
            Arc::clone(indexed_path),
            size_px,
            modified_secs,
        );
        self.touch_ready_source(&key)
    }

    fn take_closest_ready(
        &mut self,
        path: &Path,
        modified_secs: u64,
        size_px: u16,
    ) -> Option<IconGpuSource> {
        let (indexed_path, stamps) = self.ready_sizes.get_key_value(path)?;
        let sizes = stamps.get(&modified_secs)?;
        let lower = sizes.range(..=size_px).next_back().copied();
        let upper = sizes.range(size_px..).next().copied();
        let selected_size = match (lower, upper) {
            (Some(lower), Some(upper)) if size_px.abs_diff(lower) <= upper.abs_diff(size_px) => {
                lower
            }
            (Some(_), Some(upper)) => upper,
            (Some(lower), None) => lower,
            (None, Some(upper)) => upper,
            (None, None) => return None,
        };
        let key = ThumbnailSourceKey::thumbnail(
            Arc::clone(indexed_path),
            selected_size,
            modified_secs,
        );
        self.touch_ready_source(&key)
    }

    fn touch_ready_source(&mut self, key: &ThumbnailSourceKey) -> Option<IconGpuSource> {
        let entry = self.ready.get_mut(key)?;
        self.ready_frame = self.ready_frame.wrapping_add(1);
        entry.last_used_frame = self.ready_frame;
        Some(entry.source.clone())
    }

    /// Reuses only an already-completed preview without draining worker
    /// results or starting a new request. Dolphin keeps the model's current
    /// `iconPixmap` while its icon-size updater is paused and scales that
    /// pixmap in the visible widget.
    fn cached_or_closest_ready(
        &mut self,
        path: &Path,
        modified_secs: u64,
        size_px: u16,
    ) -> Option<IconGpuSource> {
        if let Some(source) = self.take_exact_ready(path, modified_secs, size_px) {
            return Some(source);
        }
        self.take_closest_ready(path, modified_secs, size_px)
    }

    fn queue_deferred(
        &mut self,
        path: &Path,
        modified_secs: u64,
        mime_type: Option<String>,
        size_px: u16,
    ) -> bool {
        self.drain_results();
        if self
            .ready_sizes
            .get(path)
            .and_then(|stamps| stamps.get(&modified_secs))
            .is_some_and(|sizes| sizes.contains(&size_px))
        {
            return false;
        }
        let shared_path = self
            .ready_sizes
            .get_key_value(path)
            .map(|(indexed_path, _)| Arc::clone(indexed_path))
            .unwrap_or_else(|| Arc::from(path));
        let key = ThumbnailSourceKey::thumbnail(
            Arc::clone(&shared_path),
            size_px,
            modified_secs,
        );
        let failure_key = ThumbnailProbeCacheKey::new(shared_path, modified_secs);
        if self.failed.contains(&failure_key) || self.pending.contains_key(&key) {
            return false;
        }
        self.send_request(
            key,
            mime_type,
            ThumbnailRequestPriority::Deferred,
            failure_key,
        )
    }

    fn send_request(
        &mut self,
        key: ThumbnailSourceKey,
        mime_type: Option<String>,
        priority: ThumbnailRequestPriority,
        failure_key: ThumbnailProbeCacheKey,
    ) -> bool {
        let Some(tx) = self.request_tx.as_ref() else {
            self.failed.insert(failure_key);
            return false;
        };
        if tx
            .send(ThumbnailSourceRequest {
                key: key.clone(),
                mime_type,
                priority,
            })
            .is_err()
        {
            self.failed.insert(failure_key);
            return false;
        }
        self.pending.insert(key, priority);
        true
    }

    fn drain_results(&mut self) -> usize {
        let (visible, deferred) = self.drain_results_by_priority();
        visible + deferred
    }

    fn drain_results_by_priority(&mut self) -> (usize, usize) {
        let mut visible = 0usize;
        let mut deferred = 0usize;
        while let Ok(result) = self.result_rx.try_recv() {
            match self
                .pending
                .remove(&result.key)
                .unwrap_or(ThumbnailRequestPriority::Deferred)
            {
                ThumbnailRequestPriority::Visible => visible += 1,
                ThumbnailRequestPriority::Deferred => deferred += 1,
            }
            if let Some(source) = result.source {
                self.insert_ready(result.key, source);
            } else if let Some(key) = ThumbnailProbeCacheKey::from_source_key(&result.key) {
                self.failed.insert(key);
            }
        }
        (visible, deferred)
    }

    fn insert_ready(&mut self, key: ThumbnailSourceKey, source: IconGpuSource) {
        let bytes = source.memory_bytes();
        self.ready_frame = self.ready_frame.wrapping_add(1);
        if let Some(old) = self.ready.insert(
            key.clone(),
            ThumbnailReadyEntry {
                source,
                bytes,
                last_used_frame: self.ready_frame,
            },
        ) {
            self.ready_bytes = self.ready_bytes.saturating_sub(old.bytes);
        }
        if let Some(stamp) = key.stamp {
            self.ready_sizes
                .entry(Arc::clone(&key.path))
                .or_default()
                .entry(stamp)
                .or_default()
                .insert(key.size_px);
        }
        self.ready_bytes += bytes;
        self.evict_ready_if_needed(&key);
        self.trim_failed(THUMBNAIL_FAILURE_CACHE_MAX_ENTRIES);
    }

    fn evict_ready_if_needed(&mut self, protected: &ThumbnailSourceKey) {
        while self.ready_bytes > self.ready_max_bytes && self.ready.len() > 1 {
            let Some(victim) = self
                .ready
                .iter()
                .filter(|(key, _)| *key != protected)
                .min_by_key(|(_, entry)| entry.last_used_frame)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.ready.remove(&victim) {
                self.ready_bytes = self.ready_bytes.saturating_sub(entry.bytes);
                self.remove_ready_size_index(&victim);
            }
        }
    }

    fn ready_len(&self) -> usize {
        self.ready.len()
    }

    fn ready_bytes(&self) -> usize {
        self.ready_bytes
    }

    /// Drop ready sizes ≤ resident GPU content size for path+mtime.
    ///
    /// Larger ready buckets are kept so zoom-in can upgrade the retained
    /// content texture instead of replaying the first-open resolution.
    #[cfg(test)]
    fn release_gpu_resident_content_upto(&mut self, keys: &[ThumbnailSourceKey]) {
        let targets = keys
            .iter()
            .filter_map(|k| k.stamp.map(|stamp| (k.path.as_ref(), stamp, k.size_px)))
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return;
        }
        let victims = self
            .ready
            .keys()
            .filter(|k| {
                k.stamp.is_some_and(|stamp| {
                    targets.iter().any(|(p, s, max_px)| {
                        *p == k.path.as_ref() && *s == stamp && k.size_px <= *max_px
                    })
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        for key in victims {
            if let Some(entry) = self.ready.remove(&key) {
                self.ready_bytes = self.ready_bytes.saturating_sub(entry.bytes);
                self.remove_ready_size_index(&key);
            }
        }
        self.shrink_maps_if_sparse();
    }

    /// Drop ready/failed/pending entries under `path` (directory navigate away).
    ///
    /// Mirrors FileManager killing preview jobs and clearing finished items when the
    /// model is emptied / items leave the view, so memory and stale failure
    /// markers do not accumulate across folders.
    #[cfg(test)]
    fn clear_path_prefix(&mut self, path: &Path) {
        self.ready.retain(|key, entry| {
            let keep = !key.path.starts_with(path);
            if !keep {
                self.ready_bytes = self.ready_bytes.saturating_sub(entry.bytes);
            }
            keep
        });
        self.rebuild_ready_size_index();
        self.failed.retain(|key| !key.path.starts_with(path));
        self.pending.retain(|key, _| !key.path.starts_with(path));
        self.shrink_maps_if_sparse();
    }

    /// Bound the permanent failure set so a long session cannot pin unbounded
    /// path strings after probing non-previewable files.
    fn trim_failed(&mut self, max_entries: usize) {
        if self.failed.len() <= max_entries {
            return;
        }
        // Failures are not LRU-tracked; drop an arbitrary excess. Keys are
        // (path, mtime) so a later mtime change re-probes correctly.
        let excess = self.failed.len() - max_entries;
        let drop_keys = self.failed.iter().take(excess).cloned().collect::<Vec<_>>();
        for key in drop_keys {
            self.failed.remove(&key);
        }
    }

    #[cfg(test)]
    fn shrink_maps_if_sparse(&mut self) {
        if self.ready.capacity() > self.ready.len().saturating_mul(2).max(64) {
            self.ready.shrink_to_fit();
        }
        if self.failed.capacity() > self.failed.len().saturating_mul(2).max(64) {
            self.failed.shrink_to_fit();
        }
        if self.pending.capacity() > self.pending.len().saturating_mul(2).max(64) {
            self.pending.shrink_to_fit();
        }
        if self.ready_sizes.capacity() > self.ready_sizes.len().saturating_mul(2).max(64) {
            self.ready_sizes.shrink_to_fit();
        }
    }

    fn remove_ready_size_index(&mut self, key: &ThumbnailSourceKey) {
        let Some(stamp) = key.stamp else {
            return;
        };
        let Some(stamps) = self.ready_sizes.get_mut(key.path.as_ref()) else {
            return;
        };
        if let Some(sizes) = stamps.get_mut(&stamp) {
            sizes.remove(&key.size_px);
            if sizes.is_empty() {
                stamps.remove(&stamp);
            }
        }
        if stamps.is_empty() {
            self.ready_sizes.remove(key.path.as_ref());
        }
    }

    #[cfg(test)]
    fn rebuild_ready_size_index(&mut self) {
        self.ready_sizes.clear();
        let keys = self.ready.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            let Some(stamp) = key.stamp else {
                continue;
            };
            self.ready_sizes
                .entry(key.path)
                .or_default()
                .entry(stamp)
                .or_default()
                .insert(key.size_px);
        }
    }

}
fn thumbnail_source_worker(
    cache_root: PathBuf,
    request_rx: Receiver<ThumbnailSourceRequest>,
    result_tx: Sender<ThumbnailSourceResult>,
) {
    let thumbnailers = ThumbnailerRegistry::shared_system();
    let mut queue = PriorityWorkerQueue::default();
    while let Some(request) = queue.next_request(&request_rx) {
        let source = thumbnail_source_for_request(&cache_root, thumbnailers, &request);
        if result_tx
            .send(ThumbnailSourceResult {
                key: request.key,
                source,
            })
            .is_err()
        {
            break;
        }
    }
}
fn thumbnail_source_for_request(
    cache_root: &Path,
    thumbnailers: &ThumbnailerRegistry,
    request: &ThumbnailSourceRequest,
) -> Option<IconGpuSource> {
    let freestanding = ThumbnailSize::for_source_px(request.key.size_px);
    thumbnail_request_from_source_request(request)
        .and_then(|thumbnail_request| {
            generate_thumbnail_with_external_thumbnailer_registry_size(
                cache_root,
                &thumbnail_request,
                thumbnailers,
                freestanding,
            )
            .ok()
            .flatten()
        })
        .map(|thumbnail| IconGpuSource::file(thumbnail.path().to_path_buf(), request.key.size_px))
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct FolderPreviewThumbnailSource {
    path: PathBuf,
    modified_secs: u64,
    mime_type: Option<String>,
}
#[derive(Clone, Debug)]
struct FolderPreviewReady {
    stamp: u64,
    size_px: u16,
    source: IconGpuSource,
}
#[derive(Clone, Debug)]
struct FolderPreviewReadyEntry {
    preview: FolderPreviewReady,
    bytes: usize,
    last_used_frame: u64,
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct FolderPreviewRoleKey {
    path: Arc<Path>,
    directory_modified_secs: u64,
    size_px: u16,
}
impl FolderPreviewRoleKey {
    fn new(path: impl Into<Arc<Path>>, directory_modified_secs: u64, size_px: u16) -> Self {
        Self {
            path: path.into(),
            directory_modified_secs,
            size_px,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct FolderPreviewRoleMetadata {
    stamp: u64,
    sources: Vec<FolderPreviewThumbnailSource>,
}
#[derive(Clone, Debug)]
struct FolderPreviewRoleRequest {
    key: FolderPreviewRoleKey,
    priority: ThumbnailRequestPriority,
}
impl PriorityWorkerRequest for FolderPreviewRoleRequest {
    type Key = FolderPreviewRoleKey;

    fn key(&self) -> &Self::Key {
        &self.key
    }

    fn priority(&self) -> WorkerRequestPriority {
        self.priority.into()
    }
}
#[derive(Clone, Debug)]
struct FolderPreviewRoleResult {
    key: FolderPreviewRoleKey,
    preview: Option<FolderPreviewReady>,
}
#[derive(Clone, Debug, Default)]
struct FolderPreviewRoleDrainStats {
    results: usize,
    applied: usize,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct FolderPreviewRoleUpdateStats {
    visible: usize,
    deferred: usize,
    queued: usize,
    ready: usize,
    failed: usize,
}
struct ShellFolderPreviewRoleRuntime {
    ready: HashMap<FolderPreviewRoleKey, FolderPreviewReadyEntry>,
    ready_sizes: HashMap<Arc<Path>, HashMap<u64, BTreeSet<u16>>>,
    failed: HashSet<FolderPreviewRoleKey>,
    pending: HashMap<FolderPreviewRoleKey, ThumbnailRequestPriority>,
    finished: HashSet<FolderPreviewRoleKey>,
    active: HashSet<FolderPreviewRoleKey>,
    frame: u64,
    ready_bytes: usize,
    ready_max_bytes: usize,
    request_tx: Option<Sender<FolderPreviewRoleRequest>>,
    result_rx: Receiver<FolderPreviewRoleResult>,
}
