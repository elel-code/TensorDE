use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventKind};
use std::path::{Path, PathBuf};

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

pub(super) fn coalesce_watcher_deltas(
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
