impl IconRasterCache {
    fn new(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            frame: 0,
            bytes: 0,
            max_bytes,
        }
    }

    fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    fn get(&mut self, key: &IconRasterCacheKey) -> Option<IconRaster> {
        let entry = self.entries.get_mut(key)?;
        entry.last_used_frame = self.frame;
        Some(entry.raster.clone())
    }

    fn contains(&self, key: &IconRasterCacheKey) -> bool {
        self.entries.contains_key(key)
    }

    #[cfg(test)]
    fn contains_icon_variant(&self, path: &Path) -> bool {
        self.entries
            .keys()
            .any(|key| key.stamp.is_none() && key.path == path)
    }

    fn get_closest_icon_variant(&mut self, requested: &IconRasterCacheKey) -> Option<IconRaster> {
        let key = self
            .entries
            .keys()
            .filter(|key| {
                key.stamp.is_none()
                    && key.path == requested.path
                    && key.style == requested.style
            })
            .min_by_key(|key| key.size_px.abs_diff(requested.size_px))
            .cloned()?;
        self.get(&key)
    }

    fn insert(&mut self, key: IconRasterCacheKey, raster: IconRaster) -> IconRaster {
        let bytes = raster.pixels.len();
        if let Some(old) = self.entries.insert(
            key.clone(),
            CachedIconRaster {
                raster: raster.clone(),
                bytes,
                last_used_frame: self.frame,
            },
        ) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
        }
        self.bytes += bytes;
        self.evict_if_needed(&key);
        raster
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn bytes(&self) -> usize {
        self.bytes
    }

    fn evict_if_needed(&mut self, protected: &IconRasterCacheKey) {
        while self.bytes > self.max_bytes && self.entries.len() > 1 {
            let Some(victim) = self
                .entries
                .iter()
                .filter(|(key, _)| *key != protected)
                .min_by_key(|(_, entry)| entry.last_used_frame)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&victim) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
            }
        }
    }
}
#[derive(Clone, Debug)]
struct IconRasterRequest {
    key: IconRasterCacheKey,
    priority: WorkerRequestPriority,
}
impl PriorityWorkerRequest for IconRasterRequest {
    type Key = IconRasterCacheKey;

    fn key(&self) -> &Self::Key {
        &self.key
    }

    fn priority(&self) -> WorkerRequestPriority {
        self.priority
    }
}
#[derive(Clone, Debug)]
struct IconRasterResult {
    key: IconRasterCacheKey,
    raster: Option<IconRaster>,
}
struct IconRasterResolver {
    pending: HashMap<IconRasterCacheKey, WorkerRequestPriority>,
    failed: HashSet<IconRasterCacheKey>,
    request_tx: Option<Sender<IconRasterRequest>>,
    result_rx: Receiver<IconRasterResult>,
}
impl IconRasterResolver {
    fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<IconRasterRequest>();
        let (result_tx, result_rx) = mpsc::channel::<IconRasterResult>();
        let request_tx = thread::Builder::new()
            .name("fika-wgpu-icon-raster".to_string())
            .spawn(move || icon_raster_worker(request_rx, result_tx))
            .ok()
            .map(|_| request_tx);
        Self {
            pending: HashMap::new(),
            failed: HashSet::new(),
            request_tx,
            result_rx,
        }
    }

    fn queue_visible(&mut self, key: IconRasterCacheKey) -> bool {
        self.queue(key, WorkerRequestPriority::Visible)
    }

    fn queue(&mut self, key: IconRasterCacheKey, priority: WorkerRequestPriority) -> bool {
        if self.failed.contains(&key) {
            return false;
        }
        match self.pending.get(&key).copied() {
            Some(WorkerRequestPriority::Visible) => return false,
            Some(WorkerRequestPriority::Deferred)
                if priority == WorkerRequestPriority::Deferred =>
            {
                return false;
            }
            Some(WorkerRequestPriority::Deferred) | None => {}
        }

        let Some(tx) = self.request_tx.as_ref() else {
            self.failed.insert(key);
            return false;
        };
        if tx
            .send(IconRasterRequest {
                key: key.clone(),
                priority,
            })
            .is_err()
        {
            self.failed.insert(key);
            return false;
        }
        self.pending.insert(key, priority);
        true
    }

    fn drain_results(&mut self, raster_cache: &mut IconRasterCache) -> usize {
        let (visible, deferred) = self.drain_results_by_priority(raster_cache);
        visible + deferred
    }

    fn drain_results_by_priority(
        &mut self,
        raster_cache: &mut IconRasterCache,
    ) -> (usize, usize) {
        let mut visible = 0usize;
        let mut deferred = 0usize;
        while let Ok(result) = self.result_rx.try_recv() {
            match self
                .pending
                .remove(&result.key)
                .unwrap_or(WorkerRequestPriority::Deferred)
            {
                WorkerRequestPriority::Visible => visible += 1,
                WorkerRequestPriority::Deferred => deferred += 1,
            }
            if let Some(raster) = result.raster {
                raster_cache.insert(result.key, raster);
            } else {
                self.failed.insert(result.key);
            }
        }
        (visible, deferred)
    }

    fn has_visible_pending(&mut self, raster_cache: &mut IconRasterCache) -> bool {
        self.drain_results(raster_cache);
        self.pending
            .values()
            .any(|priority| *priority == WorkerRequestPriority::Visible)
    }
}
fn icon_raster_worker(
    request_rx: Receiver<IconRasterRequest>,
    result_tx: Sender<IconRasterResult>,
) {
    let mut queue = PriorityWorkerQueue::default();
    while let Some(request) = queue.next_request(&request_rx) {
        let raster = rasterize_icon_for_cache_key(&request.key);
        if result_tx
            .send(IconRasterResult {
                key: request.key,
                raster,
            })
            .is_err()
        {
            break;
        }
    }
}
#[derive(Debug)]
struct IconRoleRasterCache {
    entries: HashMap<FileIconRoleCacheKey, CachedIconRaster>,
    frame: u64,
    bytes: usize,
    max_bytes: usize,
}
impl IconRoleRasterCache {
    fn new(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            frame: 0,
            bytes: 0,
            max_bytes,
        }
    }

    fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    fn get(&mut self, key: &FileIconRoleCacheKey) -> Option<IconRaster> {
        let entry = self.entries.get_mut(key)?;
        entry.last_used_frame = self.frame;
        Some(entry.raster.clone())
    }

    fn insert(&mut self, key: FileIconRoleCacheKey, raster: IconRaster) {
        let bytes = raster.pixels.len();
        if let Some(old) = self.entries.insert(
            key.clone(),
            CachedIconRaster {
                raster,
                bytes,
                last_used_frame: self.frame,
            },
        ) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
        }
        self.bytes += bytes;
        self.evict_if_needed(&key);
    }

    fn evict_if_needed(&mut self, protected: &FileIconRoleCacheKey) {
        while self.bytes > self.max_bytes && self.entries.len() > 1 {
            let Some(victim) = self
                .entries
                .iter()
                .filter(|(key, _)| *key != protected)
                .min_by_key(|(_, entry)| entry.last_used_frame)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&victim) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
            }
        }
    }
}
#[derive(Clone, Debug)]
struct ThumbnailRasterRequest {
    key: IconRasterCacheKey,
    mime_type: Option<String>,
    priority: ThumbnailRequestPriority,
}
impl PriorityWorkerRequest for ThumbnailRasterRequest {
    type Key = IconRasterCacheKey;

    fn key(&self) -> &Self::Key {
        &self.key
    }

    fn priority(&self) -> WorkerRequestPriority {
        self.priority.into()
    }
}
#[derive(Clone, Debug)]
struct ThumbnailRasterResult {
    key: IconRasterCacheKey,
    raster: Option<IconRaster>,
}
#[derive(Clone, Debug)]
enum ThumbnailResolveState {
    Ready(IconRaster),
    Pending,
    Failed,
}
#[derive(Clone, Debug)]
struct ThumbnailReadyEntry {
    raster: IconRaster,
    bytes: usize,
    last_used_frame: u64,
}
struct ThumbnailRasterResolver {
    ready: HashMap<IconRasterCacheKey, ThumbnailReadyEntry>,
    failed: HashSet<ThumbnailProbeCacheKey>,
    pending: HashMap<IconRasterCacheKey, ThumbnailRequestPriority>,
    ready_frame: u64,
    ready_bytes: usize,
    ready_max_bytes: usize,
    request_tx: Option<Sender<ThumbnailRasterRequest>>,
    result_rx: Receiver<ThumbnailRasterResult>,
}
