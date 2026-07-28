#[derive(Clone, Debug)]
struct ThumbnailSourceRequest {
    key: ThumbnailSourceKey,
    mime_type: Option<String>,
    priority: ThumbnailRequestPriority,
}

impl PriorityWorkerRequest for ThumbnailSourceRequest {
    type Key = ThumbnailSourceKey;

    fn key(&self) -> &Self::Key {
        &self.key
    }

    fn priority(&self) -> WorkerRequestPriority {
        self.priority.into()
    }
}

#[derive(Clone, Debug)]
struct ThumbnailSourceResult {
    key: ThumbnailSourceKey,
    source: Option<IconGpuSource>,
}

#[derive(Clone, Debug)]
enum ThumbnailResolveState {
    Ready(IconGpuSource),
    Pending,
    Failed,
}

#[derive(Clone, Debug)]
struct ThumbnailReadyEntry {
    source: IconGpuSource,
    bytes: usize,
    last_used_frame: u64,
}

struct ThumbnailSourceResolver {
    ready: HashMap<ThumbnailSourceKey, ThumbnailReadyEntry>,
    failed: HashSet<ThumbnailProbeCacheKey>,
    pending: HashMap<ThumbnailSourceKey, ThumbnailRequestPriority>,
    ready_frame: u64,
    ready_bytes: usize,
    ready_max_bytes: usize,
    request_tx: Option<Sender<ThumbnailSourceRequest>>,
    result_rx: Receiver<ThumbnailSourceResult>,
}
