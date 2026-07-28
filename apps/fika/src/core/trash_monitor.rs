use super::file_ops;
use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

#[derive(Debug)]
pub struct TrashEmptinessMonitor {
    files_dir: PathBuf,
    has_items: bool,
    watcher: Option<notify::RecommendedWatcher>,
    rx: Option<Receiver<notify::Result<Event>>>,
}

impl TrashEmptinessMonitor {
    pub fn new() -> Self {
        Self {
            files_dir: file_ops::trash_files_dir(),
            has_items: file_ops::trash_has_items(),
            watcher: None,
            rx: None,
        }
    }

    pub fn from_known_state(has_items: bool) -> Self {
        Self {
            files_dir: file_ops::trash_files_dir(),
            has_items,
            watcher: None,
            rx: None,
        }
    }

    pub fn has_items(&self) -> bool {
        self.has_items
    }

    pub fn set_known_state(&mut self, has_items: bool) -> bool {
        if self.has_items == has_items {
            return false;
        }
        self.has_items = has_items;
        true
    }

    pub fn refresh(&mut self) -> Option<bool> {
        let has_items = file_ops::sync_trash_status_empty()
            .map(|empty| !empty)
            .unwrap_or_else(|_| file_ops::trash_has_items());
        self.set_known_state(has_items).then_some(has_items)
    }

    pub fn start(&mut self) -> Result<(), String> {
        file_ops::ensure_trash_dirs()?;
        let files_dir = file_ops::trash_files_dir();
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = tx.send(event);
        })
        .map_err(|err| err.to_string())?;
        watcher
            .watch(&files_dir, RecursiveMode::NonRecursive)
            .map_err(|err| err.to_string())?;
        self.files_dir = files_dir;
        self.watcher = Some(watcher);
        self.rx = Some(rx);
        let _ = self.refresh();
        Ok(())
    }

    pub fn drain_changes(&mut self) -> Vec<bool> {
        let Some(rx) = self.rx.take() else {
            return Vec::new();
        };
        let mut should_refresh = false;
        while let Ok(event) = rx.try_recv() {
            should_refresh |= event
                .map(|event| trash_emptiness_event_may_change(&self.files_dir, &event))
                .unwrap_or(true);
        }
        self.rx = Some(rx);
        if should_refresh {
            self.refresh().into_iter().collect()
        } else {
            Vec::new()
        }
    }
}

impl Default for TrashEmptinessMonitor {
    fn default() -> Self {
        Self::new()
    }
}

fn trash_emptiness_event_may_change(root: &Path, event: &Event) -> bool {
    let relevant_path = event
        .paths
        .iter()
        .any(|path| path == root || path.parent() == Some(root));
    relevant_path && trash_emptiness_event_kind_may_change(&event.kind)
}

fn trash_emptiness_event_kind_may_change(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Any
            | EventKind::Create(
                CreateKind::Any | CreateKind::File | CreateKind::Folder | CreateKind::Other
            )
            | EventKind::Remove(
                RemoveKind::Any | RemoveKind::File | RemoveKind::Folder | RemoveKind::Other
            )
            | EventKind::Modify(
                ModifyKind::Name(
                    RenameMode::Any
                        | RenameMode::Both
                        | RenameMode::From
                        | RenameMode::To
                        | RenameMode::Other
                ) | ModifyKind::Any
                    | ModifyKind::Data(_)
                    | ModifyKind::Metadata(_)
                    | ModifyKind::Other
            )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::DataChange;

    #[test]
    fn known_state_changes_are_deduplicated() {
        let mut monitor = TrashEmptinessMonitor::from_known_state(false);

        assert!(!monitor.set_known_state(false));
        assert!(monitor.set_known_state(true));
        assert!(monitor.has_items());
        assert!(!monitor.set_known_state(true));
    }

    #[test]
    fn trash_emptiness_event_filter_tracks_child_create_remove_and_rename() {
        let root = PathBuf::from("/tmp/fika-trash-monitor/files");
        let child = root.join("deleted.txt");
        let unrelated = PathBuf::from("/tmp/fika-trash-monitor/other.txt");

        assert!(trash_emptiness_event_may_change(
            &root,
            &Event::new(EventKind::Create(CreateKind::File)).add_path(child.clone())
        ));
        assert!(trash_emptiness_event_may_change(
            &root,
            &Event::new(EventKind::Remove(RemoveKind::File)).add_path(child.clone())
        ));
        assert!(trash_emptiness_event_may_change(
            &root,
            &Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                .add_path(child.clone())
        ));
        assert!(!trash_emptiness_event_may_change(
            &root,
            &Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
                .add_path(unrelated)
        ));
    }
}
