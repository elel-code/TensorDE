use super::entries::{
    Entry, read_entries_sync_cancellable, read_entry_batches_sync_cancellable, read_entry_sync,
};
use super::model::{DirectoryModel, DirectoryModelSignal};
use super::network::{
    NetworkScanError, is_network_path, read_network_entry_batches_sync_cancellable,
};
use super::pane::{Generation, PaneId, RequestSerial};
use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadMode {
    Load,
    Reload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshPair {
    pub old_path: PathBuf,
    pub entry: Option<Entry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryListerEvent {
    LoadingStarted {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        mode: LoadMode,
    },
    ItemsAdded {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        entries: Vec<Entry>,
    },
    ItemsDeleted {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        paths: Vec<PathBuf>,
    },
    ItemsRefreshed {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        pairs: Vec<RefreshPair>,
    },
    ListingRefreshed {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        entries: Arc<Vec<Entry>>,
    },
    ListingCompleted {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
    },
    CurrentDirectoryRemoved {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
    },
    Error {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        message: String,
    },
    NetworkAuthRequired {
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        uri: String,
        message: String,
        default_username: Option<String>,
        default_domain: Option<String>,
    },
}

impl DirectoryListerEvent {
    pub fn pane_id(&self) -> PaneId {
        match self {
            Self::LoadingStarted { pane_id, .. }
            | Self::ItemsAdded { pane_id, .. }
            | Self::ItemsDeleted { pane_id, .. }
            | Self::ItemsRefreshed { pane_id, .. }
            | Self::ListingRefreshed { pane_id, .. }
            | Self::ListingCompleted { pane_id, .. }
            | Self::CurrentDirectoryRemoved { pane_id, .. }
            | Self::Error { pane_id, .. }
            | Self::NetworkAuthRequired { pane_id, .. } => *pane_id,
        }
    }

    pub fn generation(&self) -> Generation {
        match self {
            Self::LoadingStarted { generation, .. }
            | Self::ItemsAdded { generation, .. }
            | Self::ItemsDeleted { generation, .. }
            | Self::ItemsRefreshed { generation, .. }
            | Self::ListingRefreshed { generation, .. }
            | Self::ListingCompleted { generation, .. }
            | Self::CurrentDirectoryRemoved { generation, .. }
            | Self::Error { generation, .. }
            | Self::NetworkAuthRequired { generation, .. } => *generation,
        }
    }

    pub fn request_serial(&self) -> RequestSerial {
        match self {
            Self::LoadingStarted { request_serial, .. }
            | Self::ItemsAdded { request_serial, .. }
            | Self::ItemsDeleted { request_serial, .. }
            | Self::ItemsRefreshed { request_serial, .. }
            | Self::ListingRefreshed { request_serial, .. }
            | Self::ListingCompleted { request_serial, .. }
            | Self::CurrentDirectoryRemoved { request_serial, .. }
            | Self::Error { request_serial, .. }
            | Self::NetworkAuthRequired { request_serial, .. } => *request_serial,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::LoadingStarted { path, .. }
            | Self::ItemsAdded { path, .. }
            | Self::ItemsDeleted { path, .. }
            | Self::ItemsRefreshed { path, .. }
            | Self::ListingRefreshed { path, .. }
            | Self::ListingCompleted { path, .. }
            | Self::CurrentDirectoryRemoved { path, .. }
            | Self::Error { path, .. }
            | Self::NetworkAuthRequired { path, .. } => path,
        }
    }

    pub fn matches_target(&self, pane_id: PaneId, generation: Generation, path: &Path) -> bool {
        self.pane_id() == pane_id && self.generation() == generation && self.path() == path
    }
}

#[derive(Debug)]
pub struct DirectoryLister {
    pane_id: PaneId,
    generation: Generation,
    path: PathBuf,
    request_serial: u64,
    active_listing: Option<ActiveListing>,
    watcher: Option<notify::RecommendedWatcher>,
    watcher_rx: Option<Receiver<notify::Result<Event>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveListing {
    request_serial: RequestSerial,
    received_items: bool,
}

#[derive(Debug)]
enum ListingReadError {
    Io(std::io::Error),
    Network(NetworkScanError),
}

impl std::fmt::Display for ListingReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Network(err) => write!(f, "{err}"),
        }
    }
}

impl From<std::io::Error> for ListingReadError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl DirectoryLister {
    pub fn new(pane_id: PaneId, path: PathBuf, generation: Generation) -> Self {
        Self {
            pane_id,
            generation,
            path,
            request_serial: 0,
            active_listing: None,
            watcher: None,
            watcher_rx: None,
        }
    }

    pub fn set_target(&mut self, pane_id: PaneId, path: PathBuf, generation: Generation) {
        self.pane_id = pane_id;
        self.generation = generation;
        self.path = path;
        self.active_listing = None;
        self.drop_watcher();
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    pub fn load_directory(&mut self, mode: LoadMode) -> DirectoryListerEvent {
        let serial = self.next_serial();
        self.active_listing = Some(ActiveListing {
            request_serial: serial,
            received_items: false,
        });
        self.loading_started(mode, serial)
    }

    pub fn read_listing(&mut self, mode: LoadMode) -> Vec<DirectoryListerEvent> {
        let serial = self.next_serial();
        read_listing_events(
            self.pane_id,
            self.generation,
            serial,
            self.path.clone(),
            mode,
        )
    }

    pub fn read_listing_events(
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        mode: LoadMode,
    ) -> Vec<DirectoryListerEvent> {
        read_listing_events(pane_id, generation, request_serial, path, mode)
    }

    pub fn read_listing_events_cancellable(
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        mode: LoadMode,
        is_cancelled: impl FnMut() -> bool,
    ) -> Option<Vec<DirectoryListerEvent>> {
        read_listing_events_cancellable(
            pane_id,
            generation,
            request_serial,
            path,
            mode,
            is_cancelled,
        )
    }

    pub fn read_listing_events_streaming_cancellable(
        pane_id: PaneId,
        generation: Generation,
        request_serial: RequestSerial,
        path: PathBuf,
        mode: LoadMode,
        is_cancelled: impl FnMut() -> bool,
        on_events: impl FnMut(Vec<DirectoryListerEvent>),
    ) -> Option<()> {
        read_listing_events_streaming_cancellable(
            pane_id,
            generation,
            request_serial,
            path,
            mode,
            is_cancelled,
            on_events,
        )
    }

    pub fn start_watcher(&mut self) -> Result<(), String> {
        self.drop_watcher();
        if is_network_path(&self.path) {
            return Ok(());
        }
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = tx.send(event);
        })
        .map_err(|err| err.to_string())?;
        watcher
            .watch(&self.path, RecursiveMode::NonRecursive)
            .map_err(|err| err.to_string())?;
        self.watcher = Some(watcher);
        self.watcher_rx = Some(rx);
        Ok(())
    }

    pub fn drain_watcher_events(&mut self) -> Vec<DirectoryListerEvent> {
        let Some(rx) = self.watcher_rx.take() else {
            return Vec::new();
        };
        let mut errors = Vec::new();
        let mut deltas = Vec::new();
        while let Ok(event) = rx.try_recv() {
            match event {
                Ok(event) => {
                    if let Some(delta) = WatcherDelta::from_notify_event(&self.path, event) {
                        deltas.push(delta);
                    }
                }
                Err(err) => errors.push(err.to_string()),
            }
        }
        self.watcher_rx = Some(rx);

        let mut events = errors
            .into_iter()
            .map(|message| {
                let serial = self.next_serial();
                DirectoryListerEvent::Error {
                    pane_id: self.pane_id,
                    generation: self.generation,
                    request_serial: serial,
                    path: self.path.clone(),
                    message,
                }
            })
            .collect::<Vec<_>>();
        events.extend(
            coalesce_watcher_deltas(&self.path, deltas)
                .into_iter()
                .map(|delta| {
                    let serial = self.next_serial();
                    self.event_for_classified_delta(delta, serial)
                }),
        );
        events
    }

    pub fn classify_watcher_delta(&mut self, delta: WatcherDelta) -> DirectoryListerEvent {
        let serial = self.next_serial();
        self.classify_delta_with_serial(delta, serial)
    }

    pub fn apply_event_to_model(
        &mut self,
        event: DirectoryListerEvent,
        model: &mut DirectoryModel,
    ) -> Vec<DirectoryModelSignal> {
        if self
            .active_listing
            .is_some_and(|listing| listing.request_serial < RequestSerial(self.request_serial))
        {
            self.active_listing = None;
        }
        if event.request_serial() < RequestSerial(self.request_serial) {
            return Vec::new();
        }

        match event {
            DirectoryListerEvent::LoadingStarted { request_serial, .. } => {
                self.active_listing = Some(ActiveListing {
                    request_serial,
                    received_items: false,
                });
                Vec::new()
            }
            DirectoryListerEvent::CurrentDirectoryRemoved { request_serial, .. }
            | DirectoryListerEvent::Error { request_serial, .. }
            | DirectoryListerEvent::NetworkAuthRequired { request_serial, .. } => {
                self.clear_active_listing_for_serial(request_serial);
                Vec::new()
            }
            DirectoryListerEvent::ItemsAdded {
                request_serial,
                path,
                entries,
                ..
            } if self.active_listing_is_waiting_for_first_items(request_serial) => {
                if let Some(listing) = &mut self.active_listing {
                    listing.received_items = true;
                }
                model.replace_listing(path, Arc::new(entries))
            }
            DirectoryListerEvent::ItemsAdded { path, entries, .. } if model.directory() != path => {
                model.replace_listing(path, Arc::new(entries))
            }
            DirectoryListerEvent::ItemsAdded { entries, .. } => model.apply_items_added(entries),
            DirectoryListerEvent::ItemsDeleted { paths, .. } => model.apply_items_deleted(&paths),
            DirectoryListerEvent::ItemsRefreshed { pairs, .. } => {
                model.apply_items_refreshed(pairs)
            }
            DirectoryListerEvent::ListingRefreshed {
                request_serial,
                path,
                entries,
                ..
            } => {
                self.clear_active_listing_for_serial(request_serial);
                model.replace_listing(path, entries)
            }
            DirectoryListerEvent::ListingCompleted {
                request_serial,
                path,
                ..
            } if self.active_listing_is_empty(request_serial) => {
                self.clear_active_listing_for_serial(request_serial);
                model.clear_for_directory(path)
            }
            DirectoryListerEvent::ListingCompleted {
                request_serial,
                path,
                ..
            } if model.directory() != path => {
                self.clear_active_listing_for_serial(request_serial);
                model.clear_for_directory(path)
            }
            DirectoryListerEvent::ListingCompleted { request_serial, .. } => {
                self.clear_active_listing_for_serial(request_serial);
                Vec::new()
            }
        }
    }

    fn active_listing_is_waiting_for_first_items(&self, request_serial: RequestSerial) -> bool {
        self.active_listing.is_some_and(|listing| {
            listing.request_serial == request_serial && !listing.received_items
        })
    }

    fn active_listing_is_empty(&self, request_serial: RequestSerial) -> bool {
        self.active_listing.is_some_and(|listing| {
            listing.request_serial == request_serial && !listing.received_items
        })
    }

    fn clear_active_listing_for_serial(&mut self, request_serial: RequestSerial) {
        if self
            .active_listing
            .is_some_and(|listing| listing.request_serial == request_serial)
        {
            self.active_listing = None;
        }
    }

    fn classify_delta_with_serial(
        &self,
        delta: WatcherDelta,
        serial: RequestSerial,
    ) -> DirectoryListerEvent {
        self.event_for_classified_delta(classify_watcher_delta(&self.path, delta), serial)
    }

    fn event_for_classified_delta(
        &self,
        delta: ClassifiedWatcherDelta,
        serial: RequestSerial,
    ) -> DirectoryListerEvent {
        match delta {
            ClassifiedWatcherDelta::ItemsAdded(paths) => {
                let entries = paths
                    .iter()
                    .filter_map(|path| read_entry_sync(&self.path, path).ok())
                    .collect();
                DirectoryListerEvent::ItemsAdded {
                    pane_id: self.pane_id,
                    generation: self.generation,
                    request_serial: serial,
                    path: self.path.clone(),
                    entries,
                }
            }
            ClassifiedWatcherDelta::ItemsDeleted(paths) => DirectoryListerEvent::ItemsDeleted {
                pane_id: self.pane_id,
                generation: self.generation,
                request_serial: serial,
                path: self.path.clone(),
                paths,
            },
            ClassifiedWatcherDelta::ItemsRefreshed(paths) => {
                let pairs = paths
                    .into_iter()
                    .map(|path| RefreshPair {
                        entry: read_entry_sync(&self.path, &path).ok(),
                        old_path: path,
                    })
                    .collect();
                DirectoryListerEvent::ItemsRefreshed {
                    pane_id: self.pane_id,
                    generation: self.generation,
                    request_serial: serial,
                    path: self.path.clone(),
                    pairs,
                }
            }
            ClassifiedWatcherDelta::Renamed { from, to } => {
                let entry = read_entry_sync(&self.path, &to).ok();
                DirectoryListerEvent::ItemsRefreshed {
                    pane_id: self.pane_id,
                    generation: self.generation,
                    request_serial: serial,
                    path: self.path.clone(),
                    pairs: vec![RefreshPair {
                        old_path: from,
                        entry,
                    }],
                }
            }
            ClassifiedWatcherDelta::FullReload => self.loading_started(LoadMode::Reload, serial),
            ClassifiedWatcherDelta::CurrentDirectoryRemoved => {
                DirectoryListerEvent::CurrentDirectoryRemoved {
                    pane_id: self.pane_id,
                    generation: self.generation,
                    request_serial: serial,
                    path: self.path.clone(),
                }
            }
        }
    }

    fn loading_started(&self, mode: LoadMode, serial: RequestSerial) -> DirectoryListerEvent {
        DirectoryListerEvent::LoadingStarted {
            pane_id: self.pane_id,
            generation: self.generation,
            request_serial: serial,
            path: self.path.clone(),
            mode,
        }
    }

    fn next_serial(&mut self) -> RequestSerial {
        self.request_serial += 1;
        RequestSerial(self.request_serial)
    }

    fn drop_watcher(&mut self) {
        self.watcher = None;
        self.watcher_rx = None;
    }
}

fn read_listing_events(
    pane_id: PaneId,
    generation: Generation,
    request_serial: RequestSerial,
    path: PathBuf,
    mode: LoadMode,
) -> Vec<DirectoryListerEvent> {
    read_listing_events_cancellable(pane_id, generation, request_serial, path, mode, || false)
        .unwrap_or_default()
}

fn read_listing_events_cancellable(
    pane_id: PaneId,
    generation: Generation,
    request_serial: RequestSerial,
    path: PathBuf,
    mode: LoadMode,
    mut is_cancelled: impl FnMut() -> bool,
) -> Option<Vec<DirectoryListerEvent>> {
    if is_cancelled() {
        return None;
    }

    let result = match read_entries_for_listing(&path, &mut is_cancelled) {
        Ok(Some(entries)) => DirectoryListerEvent::ListingRefreshed {
            pane_id,
            generation,
            request_serial,
            path: path.clone(),
            entries: Arc::new(entries),
        },
        Ok(None) => return None,
        Err(err) => {
            listing_error_event(pane_id, generation, request_serial, path.clone(), mode, err)
        }
    };
    if is_cancelled() {
        return None;
    }
    Some(vec![
        result,
        DirectoryListerEvent::ListingCompleted {
            pane_id,
            generation,
            request_serial,
            path,
        },
    ])
}

fn read_entries_for_listing(
    path: &Path,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<Option<Vec<Entry>>, ListingReadError> {
    if is_network_path(path) {
        let mut entries = Vec::new();
        let Some(()) =
            read_entry_batches_for_listing(path, usize::MAX, &mut is_cancelled, |mut batch| {
                entries.append(&mut batch);
            })?
        else {
            return Ok(None);
        };
        Ok(Some(entries))
    } else {
        read_entries_sync_cancellable(path, is_cancelled).map_err(ListingReadError::Io)
    }
}

fn read_entry_batches_for_listing(
    path: &Path,
    batch_size: usize,
    is_cancelled: impl FnMut() -> bool,
    on_batch: impl FnMut(Vec<Entry>),
) -> Result<Option<()>, ListingReadError> {
    if is_network_path(path) {
        read_network_entry_batches_sync_cancellable(path, batch_size, is_cancelled, on_batch)
            .map_err(ListingReadError::Network)
    } else {
        read_entry_batches_sync_cancellable(path, batch_size, is_cancelled, on_batch)
            .map_err(ListingReadError::Io)
    }
}

fn listing_error_event(
    pane_id: PaneId,
    generation: Generation,
    request_serial: RequestSerial,
    path: PathBuf,
    mode: LoadMode,
    error: ListingReadError,
) -> DirectoryListerEvent {
    if let ListingReadError::Network(NetworkScanError::AuthenticationRequired {
        uri,
        message,
        default_username,
        default_domain,
    }) = error
    {
        return DirectoryListerEvent::NetworkAuthRequired {
            pane_id,
            generation,
            request_serial,
            path,
            uri,
            message,
            default_username,
            default_domain,
        };
    }

    if mode == LoadMode::Reload && !is_network_path(&path) && !path.exists() {
        DirectoryListerEvent::CurrentDirectoryRemoved {
            pane_id,
            generation,
            request_serial,
            path,
        }
    } else {
        DirectoryListerEvent::Error {
            pane_id,
            generation,
            request_serial,
            path,
            message: error.to_string(),
        }
    }
}

fn read_listing_events_streaming_cancellable(
    pane_id: PaneId,
    generation: Generation,
    request_serial: RequestSerial,
    path: PathBuf,
    mode: LoadMode,
    mut is_cancelled: impl FnMut() -> bool,
    mut on_events: impl FnMut(Vec<DirectoryListerEvent>),
) -> Option<()> {
    const LISTING_BATCH_SIZE: usize = 512;
    const MAXIMUM_UPDATE_INTERVAL: Duration = Duration::from_millis(2000);

    if is_cancelled() {
        return None;
    }

    let mut pending_entries = Vec::new();
    let mut pending_started_at: Option<Instant> = None;
    let mut dispatch_pending =
        |pending_entries: &mut Vec<Entry>, pending_started_at: &mut Option<Instant>| {
            if pending_entries.is_empty() {
                return;
            }
            on_events(vec![DirectoryListerEvent::ItemsAdded {
                pane_id,
                generation,
                request_serial,
                path: path.clone(),
                entries: std::mem::take(pending_entries),
            }]);
            *pending_started_at = None;
        };

    let result =
        read_entry_batches_for_listing(&path, LISTING_BATCH_SIZE, &mut is_cancelled, |entries| {
            if pending_entries.is_empty() {
                pending_started_at = Some(Instant::now());
            }
            pending_entries.extend(entries);
            if pending_started_at
                .is_some_and(|started| started.elapsed() >= MAXIMUM_UPDATE_INTERVAL)
            {
                dispatch_pending(&mut pending_entries, &mut pending_started_at);
            }
        });

    match result {
        Ok(Some(())) => {
            if is_cancelled() {
                return None;
            }
            dispatch_pending(&mut pending_entries, &mut pending_started_at);
            on_events(vec![DirectoryListerEvent::ListingCompleted {
                pane_id,
                generation,
                request_serial,
                path,
            }]);
            Some(())
        }
        Ok(None) => None,
        Err(err) => {
            if is_cancelled() {
                return None;
            }
            let event = listing_error_event(pane_id, generation, request_serial, path, mode, err);
            on_events(vec![event]);
            Some(())
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatcherDelta {
    pub kind: WatcherDeltaKind,
    pub paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatcherDeltaKind {
    Create,
    Remove,
    Rename,
    Modify,
    Rescan,
}

impl WatcherDelta {
    pub fn from_notify_event(root: &Path, event: Event) -> Option<Self> {
        let kind = match event.kind {
            EventKind::Access(_) | EventKind::Other => return None,
            EventKind::Create(
                CreateKind::Any | CreateKind::File | CreateKind::Folder | CreateKind::Other,
            ) => WatcherDeltaKind::Create,
            EventKind::Remove(
                RemoveKind::Any | RemoveKind::File | RemoveKind::Folder | RemoveKind::Other,
            ) => WatcherDeltaKind::Remove,
            EventKind::Modify(ModifyKind::Name(
                RenameMode::Any
                | RenameMode::Both
                | RenameMode::From
                | RenameMode::To
                | RenameMode::Other,
            )) => WatcherDeltaKind::Rename,
            EventKind::Modify(
                ModifyKind::Any | ModifyKind::Data(_) | ModifyKind::Metadata(_) | ModifyKind::Other,
            ) => WatcherDeltaKind::Modify,
            _ => WatcherDeltaKind::Rescan,
        };
        let paths = event
            .paths
            .into_iter()
            .filter(|path| path == root || path.parent() == Some(root))
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return None;
        }
        if paths.len() == 1
            && paths[0] == root
            && matches!(kind, WatcherDeltaKind::Create | WatcherDeltaKind::Modify)
        {
            return None;
        }
        Some(Self { kind, paths })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClassifiedWatcherDelta {
    ItemsAdded(Vec<PathBuf>),
    ItemsDeleted(Vec<PathBuf>),
    ItemsRefreshed(Vec<PathBuf>),
    Renamed { from: PathBuf, to: PathBuf },
    FullReload,
    CurrentDirectoryRemoved,
}

pub fn classify_watcher_delta(root: &Path, delta: WatcherDelta) -> ClassifiedWatcherDelta {
    if delta.paths.iter().any(|path| path == root) && matches!(delta.kind, WatcherDeltaKind::Remove)
    {
        return ClassifiedWatcherDelta::CurrentDirectoryRemoved;
    }

    let child_paths = delta
        .paths
        .into_iter()
        .filter(|path| path.parent() == Some(root))
        .collect::<Vec<_>>();
    if child_paths.is_empty() {
        return ClassifiedWatcherDelta::FullReload;
    }

    match delta.kind {
        WatcherDeltaKind::Create => ClassifiedWatcherDelta::ItemsAdded(child_paths),
        WatcherDeltaKind::Remove => ClassifiedWatcherDelta::ItemsDeleted(child_paths),
        WatcherDeltaKind::Modify => ClassifiedWatcherDelta::ItemsRefreshed(child_paths),
        WatcherDeltaKind::Rename if child_paths.len() == 2 => ClassifiedWatcherDelta::Renamed {
            from: child_paths[0].clone(),
            to: child_paths[1].clone(),
        },
        WatcherDeltaKind::Rename => ClassifiedWatcherDelta::FullReload,
        WatcherDeltaKind::Rescan => ClassifiedWatcherDelta::FullReload,
    }
}

fn coalesce_watcher_deltas(
    root: &Path,
    deltas: impl IntoIterator<Item = WatcherDelta>,
) -> Vec<ClassifiedWatcherDelta> {
    let mut coalesced = Vec::new();
    for delta in deltas {
        push_coalesced_watcher_delta(&mut coalesced, classify_watcher_delta(root, delta));
        if matches!(
            coalesced.as_slice(),
            [ClassifiedWatcherDelta::CurrentDirectoryRemoved]
        ) {
            break;
        }
    }
    coalesced
}

fn push_coalesced_watcher_delta(
    coalesced: &mut Vec<ClassifiedWatcherDelta>,
    delta: ClassifiedWatcherDelta,
) {
    match delta {
        ClassifiedWatcherDelta::CurrentDirectoryRemoved => {
            coalesced.clear();
            coalesced.push(ClassifiedWatcherDelta::CurrentDirectoryRemoved);
        }
        ClassifiedWatcherDelta::FullReload => {
            if !matches!(
                coalesced.as_slice(),
                [ClassifiedWatcherDelta::CurrentDirectoryRemoved]
            ) {
                coalesced.clear();
                coalesced.push(ClassifiedWatcherDelta::FullReload);
            }
        }
        ClassifiedWatcherDelta::ItemsAdded(paths) => {
            if matches!(
                coalesced.as_slice(),
                [ClassifiedWatcherDelta::FullReload]
                    | [ClassifiedWatcherDelta::CurrentDirectoryRemoved]
            ) {
                return;
            }
            if let Some(ClassifiedWatcherDelta::ItemsAdded(existing)) = coalesced.last_mut() {
                extend_unique_paths(existing, paths);
            } else {
                coalesced.push(ClassifiedWatcherDelta::ItemsAdded(unique_paths(paths)));
            }
        }
        ClassifiedWatcherDelta::ItemsDeleted(paths) => {
            if matches!(
                coalesced.as_slice(),
                [ClassifiedWatcherDelta::FullReload]
                    | [ClassifiedWatcherDelta::CurrentDirectoryRemoved]
            ) {
                return;
            }
            if let Some(ClassifiedWatcherDelta::ItemsDeleted(existing)) = coalesced.last_mut() {
                extend_unique_paths(existing, paths);
            } else {
                coalesced.push(ClassifiedWatcherDelta::ItemsDeleted(unique_paths(paths)));
            }
        }
        ClassifiedWatcherDelta::ItemsRefreshed(paths) => {
            if matches!(
                coalesced.as_slice(),
                [ClassifiedWatcherDelta::FullReload]
                    | [ClassifiedWatcherDelta::CurrentDirectoryRemoved]
            ) {
                return;
            }
            if let Some(ClassifiedWatcherDelta::ItemsRefreshed(existing)) = coalesced.last_mut() {
                extend_unique_paths(existing, paths);
            } else {
                coalesced.push(ClassifiedWatcherDelta::ItemsRefreshed(unique_paths(paths)));
            }
        }
        ClassifiedWatcherDelta::Renamed { from, to } => {
            if !matches!(
                coalesced.as_slice(),
                [ClassifiedWatcherDelta::FullReload]
                    | [ClassifiedWatcherDelta::CurrentDirectoryRemoved]
            ) {
                coalesced.push(ClassifiedWatcherDelta::Renamed { from, to });
            }
        }
    }
}

fn unique_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    extend_unique_paths(&mut unique, paths);
    unique
}

fn extend_unique_paths(target: &mut Vec<PathBuf>, paths: Vec<PathBuf>) {
    for path in paths {
        if !target.iter().any(|existing| existing == &path) {
            target.push(path);
        }
    }
}

pub fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut cursor = Some(path);
    while let Some(path) = cursor {
        if path.exists() {
            return Some(path.to_path_buf());
        }
        cursor = path.parent();
    }
    None
}

#[cfg(test)]
#[path = "directory/tests.rs"]
mod tests;
