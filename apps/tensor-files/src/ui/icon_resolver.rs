use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    mpsc::{self, Receiver, Sender},
};
use std::thread;

#[path = "icon_resolver/name_interner.rs"]
mod name_interner;
use crate::ui::icon_roles::{
    FileIconKind, FileIconPathCacheKey, FileIconProfile, FileIconRoleCacheKey, NamedIconFallback,
    file_icon_path_cache_key_with_stamp, file_icon_profile, icon_cache_size,
};
use crate::ui::role_worker_queue::{
    PriorityWorkerQueue, PriorityWorkerRequest, WorkerRequestPriority,
};
use crate::{Entry, IconEmblemMask, IconThemeResolver, file_icon_snapshot};
use name_interner::IconNameInterner;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedFileIcon {
    pub(crate) path: Option<Arc<Path>>,
}

pub(crate) struct FileIconResolver {
    cached: HashMap<FileIconPathCacheKey, ResolvedFileIcon>,
    cached_role_source: HashMap<FileIconRoleCacheKey, ResolvedFileIcon>,
    pending: HashMap<FileIconPathCacheKey, IconResolvePriority>,
    fast_theme: IconThemeResolver,
    fast_profiles: HashMap<FileIconRoleCacheKey, FileIconProfile>,
    named_icon_names: IconNameInterner,
    emblem_cache: HashMap<PathBuf, IconEmblemCacheEntry>,
    request_tx: Option<Sender<IconResolveRequest>>,
    result_rx: Receiver<IconResolveResult>,
}

const FILE_MANAGER_VISIBLE_ICON_PREWARM_SIZES: &[u16] = &[
    16, 22, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208, 224, 240, 256,
];
const FILE_ICON_EMBLEM_CACHE_MAX_ENTRIES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IconEmblemFingerprint {
    is_dir: bool,
    size_bytes: u64,
    modified_secs: Option<u64>,
    metadata_complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IconEmblemCacheEntry {
    fingerprint: IconEmblemFingerprint,
    mask: IconEmblemMask,
}

#[derive(Clone, Debug)]
struct IconResolveRequest {
    key: FileIconPathCacheKey,
}

impl PriorityWorkerRequest for IconResolveRequest {
    type Key = FileIconPathCacheKey;

    fn key(&self) -> &Self::Key {
        &self.key
    }

    fn priority(&self) -> WorkerRequestPriority {
        WorkerRequestPriority::Deferred
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum IconResolvePriority {
    Deferred,
}

#[derive(Clone, Debug)]
struct IconResolveResult {
    key: FileIconPathCacheKey,
    icon: ResolvedFileIcon,
}

impl FileIconResolver {
    pub(crate) fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<IconResolveRequest>();
        let (result_tx, result_rx) = mpsc::channel::<IconResolveResult>();
        let request_tx = thread::Builder::new()
            .name("tensor-files-icon-resolver".to_string())
            .spawn(move || icon_resolve_worker(request_rx, result_tx))
            .ok()
            .map(|_| request_tx);
        let mut resolver = Self {
            cached: HashMap::new(),
            cached_role_source: HashMap::new(),
            pending: HashMap::new(),
            fast_theme: IconThemeResolver::default(),
            fast_profiles: HashMap::new(),
            named_icon_names: IconNameInterner::default(),
            emblem_cache: HashMap::with_capacity(256),
            request_tx,
            result_rx,
        };
        resolver.prewarm_common_visible_roles();
        resolver
    }

    pub(crate) fn icon_emblem_mask_for_entry(
        &mut self,
        path: &Path,
        entry: &Entry,
    ) -> IconEmblemMask {
        let fingerprint = IconEmblemFingerprint {
            is_dir: entry.is_dir,
            size_bytes: entry.size_bytes,
            modified_secs: entry.modified_secs,
            metadata_complete: entry.metadata_complete,
        };
        if let Some(cached) = self.emblem_cache.get(path)
            && cached.fingerprint == fingerprint
        {
            return cached.mask;
        }

        let mask = crate::icon_emblem_mask_for_path(path);
        if self.emblem_cache.len() >= FILE_ICON_EMBLEM_CACHE_MAX_ENTRIES
            && !self.emblem_cache.contains_key(path)
        {
            self.emblem_cache.clear();
        }
        self.emblem_cache.insert(
            path.to_path_buf(),
            IconEmblemCacheEntry { fingerprint, mask },
        );
        mask
    }

    fn prewarm_common_visible_roles(&mut self) {
        let roles = [
            FileIconKind::Directory,
            FileIconKind::File { extension: None },
            FileIconKind::PreliminaryFile { extension: None },
            FileIconKind::Mime {
                mime: Arc::from(tensor_files_core::GENERIC_BINARY_MIME),
            },
            FileIconKind::Mime {
                mime: Arc::from("text/plain"),
            },
        ];

        for size_px in FILE_MANAGER_VISIBLE_ICON_PREWARM_SIZES {
            for kind in roles.iter().cloned() {
                self.resolve_key_fast(FileIconPathCacheKey {
                    role: FileIconRoleCacheKey { kind },
                    size_px: *size_px,
                });
            }
        }
    }

    pub(crate) fn resolve_entry_visible(
        &mut self,
        directory: &Path,
        entry: &Entry,
        icon_size: f32,
    ) -> (ResolvedFileIcon, bool) {
        let path = directory.join(entry.name.as_ref());
        let key = file_icon_path_cache_key_with_stamp(
            &path,
            entry.is_dir,
            entry.mime_type.clone(),
            entry.mime_magic_checked,
            entry.modified_secs,
            icon_size,
        );
        self.resolve_path_cache_key_visible(key)
    }

    /// Resolve a precomputed visible key synchronously, matching FileManager's
    /// `updateVisibleIcons()` pass. Deferred resolution is reserved for
    /// off-screen/read-ahead roles; painting a generic fallback here can leave
    /// a resident role-size variant showing the wrong icon until the directory
    /// is entered again.
    pub(crate) fn resolve_path_cache_key_visible(
        &mut self,
        key: FileIconPathCacheKey,
    ) -> (ResolvedFileIcon, bool) {
        (self.resolve_key_fast(key), false)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn resolve_entry_visible_fast(
        &mut self,
        directory: &Path,
        entry: &Entry,
        icon_size: f32,
    ) -> ResolvedFileIcon {
        let path = directory.join(entry.name.as_ref());
        let key = file_icon_path_cache_key_with_stamp(
            &path,
            entry.is_dir,
            entry.mime_type.clone(),
            entry.mime_magic_checked,
            entry.modified_secs,
            icon_size,
        );
        self.resolve_key_fast(key)
    }

    pub(crate) fn resolve_named(
        &mut self,
        icon_name: &str,
        fallback: NamedIconFallback,
        icon_size: f32,
    ) -> Option<ResolvedFileIcon> {
        let key = self.named_path_cache_key(icon_name, fallback, icon_size)?;
        self.resolve_key(key, IconResolvePriority::Deferred)
    }

    pub(crate) fn resolve_named_fast(
        &mut self,
        icon_name: &str,
        fallback: NamedIconFallback,
        icon_size: f32,
    ) -> Option<ResolvedFileIcon> {
        let key = self.named_path_cache_key(icon_name, fallback, icon_size)?;
        Some(self.resolve_key_fast(key))
    }

    pub(crate) fn intern_named_icon_name(&mut self, icon_name: &str) -> Option<Arc<str>> {
        let icon_name = icon_name.trim();
        (!icon_name.is_empty()).then(|| self.named_icon_names.intern(icon_name))
    }

    fn named_path_cache_key(
        &mut self,
        icon_name: &str,
        fallback: NamedIconFallback,
        icon_size: f32,
    ) -> Option<FileIconPathCacheKey> {
        Some(FileIconPathCacheKey {
            role: FileIconRoleCacheKey {
                kind: FileIconKind::Named {
                    icon_name: self.intern_named_icon_name(icon_name)?,
                    fallback,
                },
            },
            size_px: icon_cache_size(icon_size),
        })
    }

    pub(crate) fn resolve_named_exact_fast(
        &mut self,
        icon_name: &str,
        icon_size: f32,
    ) -> Option<Arc<Path>> {
        let icon_name = icon_name.trim();
        if icon_name.is_empty() {
            return None;
        }
        self.fast_theme.find(icon_name, icon_cache_size(icon_size))
    }

    pub(crate) fn resolve_path_cache_key(
        &mut self,
        key: FileIconPathCacheKey,
    ) -> Option<ResolvedFileIcon> {
        self.resolve_key(key, IconResolvePriority::Deferred)
    }

    /// Reads only the role snapshot that was already committed before the
    /// current scroll transaction. Completed worker messages intentionally
    /// remain queued until the visible-range updater is unpaused at settle.
    pub(crate) fn cached_path_cache_key(
        &self,
        key: &FileIconPathCacheKey,
    ) -> Option<ResolvedFileIcon> {
        self.cached.get(key).cloned()
    }

    /// Returns the already-prewarmed preliminary file icon without touching
    /// the icon theme on the frame thread. Dolphin keeps showing preliminary
    /// icons while its visible-range role updater is paused during scrolling.
    pub(crate) fn cached_preliminary_file_icon(
        &self,
        size_px: u16,
    ) -> Option<(FileIconRoleCacheKey, ResolvedFileIcon)> {
        let role = FileIconRoleCacheKey {
            kind: FileIconKind::PreliminaryFile { extension: None },
        };
        let key = FileIconPathCacheKey {
            role: role.clone(),
            size_px,
        };
        self.cached.get(&key).cloned().map(|icon| (role, icon))
    }

    pub(crate) fn resolve_path_cache_key_fast(
        &mut self,
        key: FileIconPathCacheKey,
    ) -> ResolvedFileIcon {
        self.resolve_key_fast(key)
    }

    /// Matches Dolphin's `iconName` + QPixmapCache path during an icon-size
    /// transaction. A semantic role committed before the transaction is
    /// resolved synchronously at the target size; a newly visible role stays
    /// preliminary until the paused visible-role updater resumes.
    pub(crate) fn resolve_path_cache_key_for_icon_size_change(
        &mut self,
        key: FileIconPathCacheKey,
    ) -> Option<ResolvedFileIcon> {
        if let Some(icon) = self.cached.get(&key) {
            return Some(icon.clone());
        }
        if self.cached_role_source.contains_key(&key.role) {
            return Some(self.resolve_key_fast(key));
        }
        let _ = self.resolve_key(key, IconResolvePriority::Deferred);
        None
    }

    fn resolve_key(
        &mut self,
        key: FileIconPathCacheKey,
        priority: IconResolvePriority,
    ) -> Option<ResolvedFileIcon> {
        if let Some(icon) = self.cached.get(&key) {
            return Some(icon.clone());
        }

        let should_send = self
            .pending
            .get(&key)
            .is_none_or(|queued_priority| priority > *queued_priority);
        if should_send {
            self.pending.insert(key.clone(), priority);
            if self
                .request_tx
                .as_ref()
                .is_none_or(|tx| tx.send(IconResolveRequest { key }).is_err())
            {
                self.pending.clear();
            }
        }
        None
    }

    fn resolve_key_fast(&mut self, key: FileIconPathCacheKey) -> ResolvedFileIcon {
        if let Some(icon) = self.cached.get(&key) {
            return icon.clone();
        }

        let profile = self
            .fast_profiles
            .entry(key.role.clone())
            .or_insert_with(|| {
                file_icon_profile(&key.role.kind, tensor_files_core::MimeDatabase::shared())
            });
        let icon = file_icon_snapshot(profile, key.size_px, &mut self.fast_theme);
        self.pending.remove(&key);
        self.remember_role_source(&key.role, &icon);
        self.cached.insert(key, icon.clone());
        icon
    }

    fn remember_role_source(&mut self, role: &FileIconRoleCacheKey, icon: &ResolvedFileIcon) {
        if icon.path.is_some() || !self.cached_role_source.contains_key(role) {
            self.cached_role_source.insert(role.clone(), icon.clone());
        }
    }

    pub(crate) fn drain_results(&mut self) -> usize {
        let (visible, deferred) = self.drain_results_by_priority();
        visible + deferred
    }

    pub(crate) fn drain_results_by_priority(&mut self) -> (usize, usize) {
        let visible = 0usize;
        let mut deferred = 0usize;
        while let Ok(result) = self.result_rx.try_recv() {
            if self.pending.remove(&result.key).is_none() {
                continue;
            }
            deferred += 1;
            self.remember_role_source(&result.key.role, &result.icon);
            self.cached.insert(result.key, result.icon);
        }
        (visible, deferred)
    }
}

fn icon_resolve_worker(
    request_rx: Receiver<IconResolveRequest>,
    result_tx: Sender<IconResolveResult>,
) {
    let mut theme = IconThemeResolver::default();
    let mime = tensor_files_core::MimeDatabase::shared();
    let mut roles = HashMap::<FileIconRoleCacheKey, FileIconProfile>::new();
    let mut queue = PriorityWorkerQueue::default();
    while let Some(request) = queue.next_request(&request_rx) {
        if result_tx
            .send(resolve_icon_request(request, &mut theme, mime, &mut roles))
            .is_err()
        {
            return;
        }
    }
}

fn resolve_icon_request(
    request: IconResolveRequest,
    theme: &mut IconThemeResolver,
    mime: &tensor_files_core::MimeDatabase,
    roles: &mut HashMap<FileIconRoleCacheKey, FileIconProfile>,
) -> IconResolveResult {
    let profile = roles
        .entry(request.key.role.clone())
        .or_insert_with(|| file_icon_profile(&request.key.role.kind, mime));
    let icon = file_icon_snapshot(profile, request.key.size_px, theme);
    IconResolveResult {
        key: request.key,
        icon,
    }
}

#[cfg(test)]
pub(crate) struct FileIconResolverTestHarness {
    pub(crate) resolver: FileIconResolver,
    request_rx: Receiver<IconResolveRequest>,
    result_tx: Sender<IconResolveResult>,
}

#[cfg(test)]
impl FileIconResolverTestHarness {
    pub(crate) fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<IconResolveRequest>();
        let (result_tx, result_rx) = mpsc::channel::<IconResolveResult>();
        Self {
            resolver: FileIconResolver {
                cached: HashMap::new(),
                cached_role_source: HashMap::new(),
                pending: HashMap::new(),
                fast_theme: IconThemeResolver::default(),
                fast_profiles: HashMap::new(),
                named_icon_names: IconNameInterner::default(),
                emblem_cache: HashMap::new(),
                request_tx: Some(request_tx),
                result_rx,
            },
            request_rx,
            result_tx,
        }
    }

    pub(crate) fn next_request_key(&mut self) -> Option<FileIconPathCacheKey> {
        self.request_rx.try_recv().ok().map(|request| request.key)
    }

    pub(crate) fn complete(&self, key: FileIconPathCacheKey, path: Option<PathBuf>) {
        let _ = self.result_tx.send(IconResolveResult {
            key,
            icon: ResolvedFileIcon {
                path: path.map(Arc::<Path>::from),
            },
        });
    }
}

#[cfg(test)]
#[path = "icon_resolver/drain_tests.rs"]
mod drain_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_scroll_lookup_does_not_publish_completed_worker_result() {
        let (_request_tx, request_rx) = mpsc::channel::<IconResolveRequest>();
        let (result_tx, result_rx) = mpsc::channel::<IconResolveResult>();
        drop(request_rx);
        let key = FileIconPathCacheKey {
            role: FileIconRoleCacheKey {
                kind: FileIconKind::Mime {
                    mime: Arc::from("text/plain"),
                },
            },
            size_px: 32,
        };
        let expected = ResolvedFileIcon {
            path: Some(Arc::<Path>::from(Path::new("/theme/text-plain.svg"))),
        };
        let mut resolver = FileIconResolver {
            cached: HashMap::new(),
            cached_role_source: HashMap::new(),
            pending: HashMap::from([(key.clone(), IconResolvePriority::Deferred)]),
            fast_theme: IconThemeResolver::default(),
            fast_profiles: HashMap::new(),
            named_icon_names: IconNameInterner::default(),
            emblem_cache: HashMap::new(),
            request_tx: None,
            result_rx,
        };
        result_tx
            .send(IconResolveResult {
                key: key.clone(),
                icon: expected.clone(),
            })
            .unwrap();

        assert_eq!(resolver.cached_path_cache_key(&key), None);
        assert_eq!(resolver.drain_results(), 1);
        assert_eq!(resolver.cached_path_cache_key(&key), Some(expected));
    }

    #[test]
    fn named_desktop_icons_resolve_synchronously_on_first_visible_frame() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let root = std::env::temp_dir().join(format!(
            "tensor-files-named-visible-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("OCS Desktop.desktop");
        fs::write(
            &path,
            "[Desktop Entry]\nName=OCS\nType=Application\nIcon=folder\nExec=true\n",
        )
        .unwrap();

        let (request_tx, _request_rx) = mpsc::channel::<IconResolveRequest>();
        let (_result_tx, result_rx) = mpsc::channel::<IconResolveResult>();
        let mut resolver = FileIconResolver {
            cached: HashMap::new(),
            cached_role_source: HashMap::new(),
            pending: HashMap::new(),
            fast_theme: IconThemeResolver::default(),
            fast_profiles: HashMap::new(),
            named_icon_names: IconNameInterner::default(),
            emblem_cache: HashMap::new(),
            request_tx: Some(request_tx),
            result_rx,
        };

        let entry = Entry::new(tensor_files_core::EntryData {
            name: Arc::from("OCS Desktop.desktop"),
            name_width_units: 0,
            target_path: None,
            size_bytes: 64,
            modified_secs: Some(1),
            metadata_complete: true,
            mime_type: Some(Arc::from("application/x-desktop")),
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        });
        // First visible frame must not fall back to generic File — that would
        // paint the wrong resident Role variant until re-enter.
        let (icon, deferred) = resolver.resolve_entry_visible(&root, &entry, 48.0);
        assert!(
            !deferred,
            "Named desktop icons must sync-resolve on first paint"
        );
        assert!(
            icon.path.is_some(),
            "Icon=folder should resolve via theme on first frame"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn warm_theme_snapshot_reuses_shared_path_storage() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let root = std::env::temp_dir().join(format!(
            "tensor-files-theme-path-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let icon_path = root.join("exact.svg");
        fs::write(&icon_path, b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
        let key = FileIconPathCacheKey {
            role: FileIconRoleCacheKey {
                kind: FileIconKind::Named {
                    icon_name: Arc::from(icon_path.to_string_lossy().as_ref()),
                    fallback: NamedIconFallback::Application,
                },
            },
            size_px: 64,
        };
        let mut resolver = FileIconResolverTestHarness::new().resolver;
        let first = resolver
            .resolve_path_cache_key_fast(key.clone())
            .path
            .expect("absolute theme icon should resolve");
        let second = resolver
            .resolve_path_cache_key_fast(key)
            .path
            .expect("warm theme icon should resolve");
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(resolver.fast_theme.path_cache.len(), 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn emblem_cache_reuses_warm_path_state_and_rekeys_metadata_changes() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let root = std::env::temp_dir().join(format!(
            "tensor-files-emblem-cache-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("document.txt");
        fs::write(&path, b"document").unwrap();
        let entry = Entry::new(tensor_files_core::EntryData {
            name: Arc::from("document.txt"),
            name_width_units: 0,
            target_path: None,
            size_bytes: 8,
            modified_secs: Some(1),
            metadata_complete: true,
            mime_type: Some(Arc::from("text/plain")),
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        });
        let mut resolver = FileIconResolverTestHarness::new().resolver;

        let first = resolver.icon_emblem_mask_for_entry(&path, &entry);
        let second = resolver.icon_emblem_mask_for_entry(&path, &entry);
        assert_eq!(first, second);
        assert_eq!(resolver.emblem_cache.len(), 1);

        let changed = Entry::new(tensor_files_core::EntryData {
            modified_secs: Some(2),
            ..(*entry).clone()
        });
        let third = resolver.icon_emblem_mask_for_entry(&path, &changed);
        assert_eq!(third, first);
        assert_eq!(resolver.emblem_cache.len(), 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn icon_size_change_resolves_known_role_at_exact_target_size() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut harness = FileIconResolverTestHarness::new();
        let root = std::env::temp_dir().join(format!(
            "tensor-files-icon-size-role-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let exact_path = root.join("exact.svg");
        fs::write(&exact_path, b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
        let role = FileIconRoleCacheKey {
            kind: FileIconKind::Named {
                icon_name: Arc::from(exact_path.to_string_lossy().as_ref()),
                fallback: NamedIconFallback::Application,
            },
        };
        let old_key = FileIconPathCacheKey {
            role: role.clone(),
            size_px: 48,
        };
        let target_key = FileIconPathCacheKey { role, size_px: 128 };
        let old_path = PathBuf::from("/theme/scalable/mimetypes/old.svg");

        assert_eq!(
            harness.resolver.resolve_path_cache_key(old_key.clone()),
            None
        );
        assert_eq!(harness.next_request_key(), Some(old_key.clone()));
        harness.complete(old_key, Some(old_path));
        assert_eq!(harness.resolver.drain_results(), 1);

        let resolved = harness
            .resolver
            .resolve_path_cache_key_for_icon_size_change(target_key.clone());
        assert_eq!(
            resolved.and_then(|icon| icon.path),
            Some(Arc::<Path>::from(exact_path.as_path()))
        );
        assert_eq!(harness.next_request_key(), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fast_exact_resolution_ignores_superseded_worker_result() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut harness = FileIconResolverTestHarness::new();
        let root = std::env::temp_dir().join(format!(
            "tensor-files-icon-size-superseded-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let exact_path = root.join("exact.svg");
        fs::write(&exact_path, b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
        let role = FileIconRoleCacheKey {
            kind: FileIconKind::Named {
                icon_name: Arc::from(exact_path.to_string_lossy().as_ref()),
                fallback: NamedIconFallback::Application,
            },
        };
        let old_key = FileIconPathCacheKey {
            role: role.clone(),
            size_px: 48,
        };
        let target_key = FileIconPathCacheKey { role, size_px: 128 };

        assert_eq!(
            harness.resolver.resolve_path_cache_key(old_key.clone()),
            None
        );
        assert_eq!(harness.next_request_key(), Some(old_key.clone()));
        harness.complete(old_key, Some(PathBuf::from("/theme/old.svg")));
        assert_eq!(harness.resolver.drain_results(), 1);

        assert_eq!(
            harness.resolver.resolve_path_cache_key(target_key.clone()),
            None
        );
        assert_eq!(harness.next_request_key(), Some(target_key.clone()));
        let exact = harness
            .resolver
            .resolve_path_cache_key_for_icon_size_change(target_key.clone())
            .expect("known role should resolve synchronously at the target size");
        assert_eq!(exact.path.as_deref(), Some(exact_path.as_path()));

        harness.complete(
            target_key.clone(),
            Some(PathBuf::from("/theme/stale-worker-result.svg")),
        );
        assert_eq!(harness.resolver.drain_results(), 0);
        assert_eq!(
            harness.resolver.cached_path_cache_key(&target_key),
            Some(exact)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn icon_size_change_keeps_an_unknown_role_preliminary_and_queues_exact_size() {
        let mut harness = FileIconResolverTestHarness::new();
        let target_key = FileIconPathCacheKey {
            role: FileIconRoleCacheKey {
                kind: FileIconKind::Mime {
                    mime: Arc::from("application/x-tensor-files-new-visible-role"),
                },
            },
            size_px: 128,
        };

        assert_eq!(
            harness
                .resolver
                .resolve_path_cache_key_for_icon_size_change(target_key.clone()),
            None
        );
        assert_eq!(harness.next_request_key(), Some(target_key));
    }
}
