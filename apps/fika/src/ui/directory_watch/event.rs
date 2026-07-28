use std::path::Path;

use notify::event::{MetadataKind as NotifyMetadataKind, ModifyKind as NotifyModifyKind};
use notify::{Event as NotifyEvent, EventKind as NotifyEventKind};

pub(super) fn shell_directory_watch_event_mutates(kind: &NotifyEventKind) -> bool {
    match kind {
        NotifyEventKind::Access(_) | NotifyEventKind::Other => false,
        NotifyEventKind::Modify(NotifyModifyKind::Metadata(kind)) => {
            matches!(
                kind,
                NotifyMetadataKind::Any | NotifyMetadataKind::WriteTime
            )
        }
        _ => true,
    }
}

pub(super) fn shell_directory_watch_event_touches_path(
    event: &NotifyEvent,
    directory: &Path,
) -> bool {
    event.paths.is_empty()
        || event
            .paths
            .iter()
            .any(|path| shell_directory_watch_path_touches_directory(path, directory))
}

fn shell_directory_watch_path_touches_directory(path: &Path, directory: &Path) -> bool {
    if path.is_relative() {
        return true;
    }
    if path == directory || path.parent() == Some(directory) || path.starts_with(directory) {
        return true;
    }

    let Some(canonical_directory) = directory.canonicalize().ok() else {
        return false;
    };
    if path == canonical_directory
        || path.parent() == Some(canonical_directory.as_path())
        || path.starts_with(&canonical_directory)
    {
        return true;
    }
    path.parent()
        .and_then(|parent| parent.canonicalize().ok())
        .is_some_and(|parent| {
            parent == canonical_directory || parent.starts_with(&canonical_directory)
        })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn directory_watch_event_filter_ignores_non_mutating_events() {
        assert!(!shell_directory_watch_event_mutates(
            &NotifyEventKind::Access(notify::event::AccessKind::Read)
        ));
        assert!(!shell_directory_watch_event_mutates(
            &NotifyEventKind::Modify(NotifyModifyKind::Metadata(NotifyMetadataKind::Permissions))
        ));
        assert!(shell_directory_watch_event_mutates(
            &NotifyEventKind::Modify(NotifyModifyKind::Metadata(NotifyMetadataKind::WriteTime))
        ));
        assert!(shell_directory_watch_event_mutates(
            &NotifyEventKind::Create(notify::event::CreateKind::File)
        ));
    }

    #[test]
    fn directory_watch_event_path_matching_stays_inside_directory() {
        let event = NotifyEvent {
            kind: NotifyEventKind::Any,
            paths: vec![PathBuf::from("/tmp/fika-watch-root/child.txt")],
            attrs: Default::default(),
        };

        assert!(shell_directory_watch_event_touches_path(
            &event,
            Path::new("/tmp/fika-watch-root")
        ));
        assert!(!shell_directory_watch_event_touches_path(
            &event,
            Path::new("/tmp/fika-watch")
        ));
    }

    #[test]
    fn directory_watch_empty_path_event_touches_any_directory() {
        let event = NotifyEvent {
            kind: NotifyEventKind::Any,
            paths: Vec::new(),
            attrs: Default::default(),
        };

        assert!(shell_directory_watch_event_touches_path(
            &event,
            Path::new("/tmp/fika-watch-root")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn directory_watch_event_path_matching_accepts_canonical_symlink_children() {
        let root = test_dir("watch-symlink");
        let real = root.join("real");
        let link = root.join("link");
        fs::create_dir_all(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let event = NotifyEvent {
            kind: NotifyEventKind::Create(notify::event::CreateKind::File),
            paths: vec![real.join("child.txt")],
            attrs: Default::default(),
        };

        assert!(shell_directory_watch_event_touches_path(&event, &link));

        fs::remove_file(&link).unwrap();
        fs::remove_dir_all(&root).unwrap();
    }

    fn test_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("fika-watch-{name}-{unique}"))
    }
}
