impl ThumbnailRasterResolver {
    fn new() -> Self {
        Self::with_cache_root(default_thumbnail_cache_root())
    }

    fn with_cache_root(cache_root: PathBuf) -> Self {
        let (request_tx, request_rx) = mpsc::channel::<ThumbnailRasterRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ThumbnailRasterResult>();
        let request_tx = thread::Builder::new()
            .name("fika-wgpu-thumbnail-raster".to_string())
            .spawn(move || thumbnail_raster_worker(cache_root, request_rx, result_tx))
            .ok()
            .map(|_| request_tx);
        Self {
            ready: HashMap::new(),
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
        let key = IconRasterCacheKey::thumbnail(path.to_path_buf(), size_px, modified_secs);
        let failure_key = ThumbnailProbeCacheKey::new(path.to_path_buf(), modified_secs);
        // Keep the ready entry (Dolphin-style cache hit). Removing here forced a
        // re-queue every frame and thrashed GPU uploads / flash of MIME icons.
        if let Some(entry) = self.ready.get_mut(&key) {
            self.ready_frame = self.ready_frame.wrapping_add(1);
            entry.last_used_frame = self.ready_frame;
            return ThumbnailResolveState::Ready(entry.raster.clone());
        }
        // Zoom / first-frame: paint a nearby size while the exact bucket loads.
        if let Some(raster) = self.take_closest_ready(path, modified_secs, size_px) {
            // Still ensure the exact size is queued as visible work.
            if !self.pending.contains_key(&key) && !self.failed.contains(&failure_key) {
                let _ = self.send_request(
                    key,
                    mime_type,
                    ThumbnailRequestPriority::Visible,
                    failure_key,
                );
            }
            return ThumbnailResolveState::Ready(raster);
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

    fn take_closest_ready(
        &mut self,
        path: &Path,
        modified_secs: u64,
        size_px: u16,
    ) -> Option<IconRaster> {
        let key = self
            .ready
            .keys()
            .filter(|key| {
                key.path.as_path() == path
                    && key.stamp == Some(modified_secs)
                    && key.style == IconRasterStyle::Original
            })
            .min_by_key(|key| key.size_px.abs_diff(size_px))
            .cloned()?;
        let entry = self.ready.get_mut(&key)?;
        self.ready_frame = self.ready_frame.wrapping_add(1);
        entry.last_used_frame = self.ready_frame;
        Some(entry.raster.clone())
    }

    fn queue_deferred(
        &mut self,
        path: &Path,
        modified_secs: u64,
        mime_type: Option<String>,
        size_px: u16,
    ) -> bool {
        self.drain_results();
        let key = IconRasterCacheKey::thumbnail(path.to_path_buf(), size_px, modified_secs);
        let failure_key = ThumbnailProbeCacheKey::new(path.to_path_buf(), modified_secs);
        if self.ready.contains_key(&key)
            || self.failed.contains(&failure_key)
            || self.pending.contains_key(&key)
        {
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
        key: IconRasterCacheKey,
        mime_type: Option<String>,
        priority: ThumbnailRequestPriority,
        failure_key: ThumbnailProbeCacheKey,
    ) -> bool {
        let Some(tx) = self.request_tx.as_ref() else {
            self.failed.insert(failure_key);
            return false;
        };
        if tx
            .send(ThumbnailRasterRequest {
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
            if let Some(raster) = result.raster {
                self.insert_ready(result.key, raster);
            } else if let Some(key) = ThumbnailProbeCacheKey::from_raster_key(&result.key) {
                self.failed.insert(key);
            }
        }
        (visible, deferred)
    }

    fn insert_ready(&mut self, key: IconRasterCacheKey, raster: IconRaster) {
        let bytes = raster.pixels.len();
        self.ready_frame = self.ready_frame.wrapping_add(1);
        if let Some(old) = self.ready.insert(
            key.clone(),
            ThumbnailReadyEntry {
                raster,
                bytes,
                last_used_frame: self.ready_frame,
            },
        ) {
            self.ready_bytes = self.ready_bytes.saturating_sub(old.bytes);
        }
        self.ready_bytes += bytes;
        self.evict_ready_if_needed(&key);
        self.trim_failed(THUMBNAIL_FAILURE_CACHE_MAX_ENTRIES);
    }

    fn evict_ready_if_needed(&mut self, protected: &IconRasterCacheKey) {
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
    /// Larger ready buckets are kept so zoom-in can upgrade the size-free
    /// content GPU slot instead of replaying the first-open resolution.
    fn release_gpu_resident_content_upto(&mut self, keys: &[IconRasterCacheKey]) {
        let targets = keys
            .iter()
            .filter_map(|k| k.stamp.map(|stamp| (k.path.as_path(), stamp, k.size_px)))
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
                        *p == k.path.as_path() && *s == stamp && k.size_px <= *max_px
                    })
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        for key in victims {
            if let Some(entry) = self.ready.remove(&key) {
                self.ready_bytes = self.ready_bytes.saturating_sub(entry.bytes);
            }
        }
        self.shrink_maps_if_sparse();
    }

    /// Drop ready/failed/pending entries under `path` (directory navigate away).
    ///
    /// Mirrors Dolphin killing preview jobs and clearing finished items when the
    /// model is emptied / items leave the view, so memory and stale failure
    /// markers do not accumulate across folders.
    fn clear_path_prefix(&mut self, path: &Path) {
        self.ready.retain(|key, entry| {
            let keep = !key.path.starts_with(path);
            if !keep {
                self.ready_bytes = self.ready_bytes.saturating_sub(entry.bytes);
            }
            keep
        });
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
    }

    fn has_visible_pending(&self) -> bool {
        self.pending
            .values()
            .any(|priority| *priority == ThumbnailRequestPriority::Visible)
    }
}
fn thumbnail_raster_worker(
    cache_root: PathBuf,
    request_rx: Receiver<ThumbnailRasterRequest>,
    result_tx: Sender<ThumbnailRasterResult>,
) {
    let thumbnailers = ThumbnailerRegistry::shared_system();
    let mut queue = PriorityWorkerQueue::default();
    while let Some(request) = queue.next_request(&request_rx) {
        let raster = thumbnail_raster_for_request(&cache_root, thumbnailers, &request);
        if result_tx
            .send(ThumbnailRasterResult {
                key: request.key,
                raster,
            })
            .is_err()
        {
            break;
        }
    }
}
fn thumbnail_raster_for_request(
    cache_root: &Path,
    thumbnailers: &ThumbnailerRegistry,
    request: &ThumbnailRasterRequest,
) -> Option<IconRaster> {
    let freestanding = ThumbnailSize::for_raster_px(request.key.size_px);
    thumbnail_request_from_raster_request(request)
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
        .and_then(|thumbnail| rasterize_icon(thumbnail.path(), request.key.size_px as u32))
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
    raster: IconRaster,
}
#[derive(Clone, Debug)]
struct FolderPreviewReadyEntry {
    preview: FolderPreviewReady,
    bytes: usize,
    last_used_frame: u64,
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct FolderPreviewRoleKey {
    path: PathBuf,
    directory_modified_secs: u64,
    size_px: u16,
}
impl FolderPreviewRoleKey {
    fn new(path: PathBuf, directory_modified_secs: u64, size_px: u16) -> Self {
        Self {
            path,
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
    changes: Vec<FolderPreviewRoleChange>,
}
#[derive(Clone, Debug)]
struct FolderPreviewRoleChange {
    key: FolderPreviewRoleKey,
    previous: Option<FolderPreviewReady>,
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
