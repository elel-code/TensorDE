use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    mpsc::{self, Receiver, Sender},
};
use std::thread;

use crate::ui::icon_roles::{
    FileIconKind, FileIconPathCacheKey, FileIconProfile, FileIconRoleCacheKey, NamedIconFallback,
    file_icon_path_cache_key_with_stamp, file_icon_profile, icon_cache_size,
};
use crate::ui::role_worker_queue::{
    PriorityWorkerQueue, PriorityWorkerRequest, WorkerRequestPriority,
};
use crate::{Entry, IconThemeResolver, file_icon_snapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedFileIcon {
    pub(crate) path: Option<PathBuf>,
}

pub(crate) struct FileIconResolver {
    cached: HashMap<FileIconPathCacheKey, ResolvedFileIcon>,
    pending: HashMap<FileIconPathCacheKey, IconResolvePriority>,
    fast_theme: IconThemeResolver,
    fast_profiles: HashMap<FileIconRoleCacheKey, FileIconProfile>,
    request_tx: Option<Sender<IconResolveRequest>>,
    result_rx: Receiver<IconResolveResult>,
}

const FILE_MANAGER_VISIBLE_ICON_PREWARM_SIZES: &[u16] = &[
    16, 22, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208, 224, 240, 256,
];

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
            .name("fika-icon-resolver".to_string())
            .spawn(move || icon_resolve_worker(request_rx, result_tx))
            .ok()
            .map(|_| request_tx);
        let mut resolver = Self {
            cached: HashMap::new(),
            pending: HashMap::new(),
            fast_theme: IconThemeResolver::default(),
            fast_profiles: HashMap::new(),
            request_tx,
            result_rx,
        };
        resolver.prewarm_common_visible_roles();
        resolver
    }

    fn prewarm_common_visible_roles(&mut self) {
        let roles = [
            FileIconKind::Directory,
            FileIconKind::File { extension: None },
            FileIconKind::PreliminaryFile { extension: None },
            FileIconKind::Mime {
                mime: Arc::from(fika_core::GENERIC_BINARY_MIME),
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
        self.drain_results();
        (self.resolve_key_fast(key), false)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn resolve_entry_visible_fast(
        &mut self,
        directory: &Path,
        entry: &Entry,
        icon_size: f32,
    ) -> ResolvedFileIcon {
        self.drain_results();
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
        self.drain_results();
        let icon_name = icon_name.trim();
        if icon_name.is_empty() {
            return None;
        }
        let key = FileIconPathCacheKey {
            role: FileIconRoleCacheKey {
                kind: FileIconKind::Named {
                    icon_name: icon_name.to_string(),
                    fallback,
                },
            },
            size_px: icon_cache_size(icon_size),
        };
        self.resolve_key(key, IconResolvePriority::Deferred)
    }

    pub(crate) fn resolve_named_fast(
        &mut self,
        icon_name: &str,
        fallback: NamedIconFallback,
        icon_size: f32,
    ) -> Option<ResolvedFileIcon> {
        self.drain_results();
        let icon_name = icon_name.trim();
        if icon_name.is_empty() {
            return None;
        }
        let key = FileIconPathCacheKey {
            role: FileIconRoleCacheKey {
                kind: FileIconKind::Named {
                    icon_name: icon_name.to_string(),
                    fallback,
                },
            },
            size_px: icon_cache_size(icon_size),
        };
        Some(self.resolve_key_fast(key))
    }

    pub(crate) fn resolve_named_exact_fast(
        &mut self,
        icon_name: &str,
        icon_size: f32,
    ) -> Option<PathBuf> {
        self.drain_results();
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
        self.drain_results();
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
        self.drain_results();
        self.resolve_key_fast(key)
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
                file_icon_profile(&key.role.kind, fika_core::MimeDatabase::shared())
            });
        let icon = file_icon_snapshot(profile, key.size_px, &mut self.fast_theme);
        self.pending.remove(&key);
        self.cached.insert(key, icon.clone());
        icon
    }

    pub(crate) fn drain_results(&mut self) -> usize {
        let (visible, deferred) = self.drain_results_by_priority();
        visible + deferred
    }

    pub(crate) fn drain_results_by_priority(&mut self) -> (usize, usize) {
        let visible = 0usize;
        let mut deferred = 0usize;
        while let Ok(result) = self.result_rx.try_recv() {
            let _ = self.pending.remove(&result.key);
            deferred += 1;
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
    let mime = fika_core::MimeDatabase::shared();
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
    mime: &fika_core::MimeDatabase,
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
                pending: HashMap::new(),
                fast_theme: IconThemeResolver::default(),
                fast_profiles: HashMap::new(),
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
            icon: ResolvedFileIcon { path },
        });
    }
}

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
            path: Some(PathBuf::from("/theme/text-plain.svg")),
        };
        let mut resolver = FileIconResolver {
            cached: HashMap::new(),
            pending: HashMap::from([(key.clone(), IconResolvePriority::Deferred)]),
            fast_theme: IconThemeResolver::default(),
            fast_profiles: HashMap::new(),
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
            "fika-named-visible-{}-{}",
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
            pending: HashMap::new(),
            fast_theme: IconThemeResolver::default(),
            fast_profiles: HashMap::new(),
            request_tx: Some(request_tx),
            result_rx,
        };

        let entry = Entry::new(fika_core::EntryData {
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
}
