use super::directory::DirectoryListerEvent;
use super::pane::{PaneController, PaneId};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

mod tasks;

pub use tasks::{
    CreateItemResult, FileTransferMode, RenameItemResult, TransferTaskResult, TrashSelectionResult,
    TrashViewOperation, TrashViewOperationResult, UndoTaskResult, action_status,
    create_item_result, create_item_result_async, created_item_label, default_created_item_name,
    parent_dirs, paste_text_result, paste_text_result_async, push_unique_path, rename_item_result,
    rename_item_result_async, transfer_paths_result, transfer_paths_result_async,
    trash_selection_result, trash_selection_result_async, trash_view_operation_result,
    trash_view_operation_result_async, undo_record_result, undo_record_result_async,
};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UndoSerial(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UndoRecord {
    pub serial: UndoSerial,
    pub label: String,
    pub affected_dirs: Vec<PathBuf>,
    pub payload: UndoPayload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UndoPayload {
    None,
    Create { items: Vec<CreateUndoItem> },
    Rename { items: Vec<RenameUndoItem> },
    Trash { items: Vec<TrashUndoItem> },
    Transfer { items: Vec<TransferUndoItem> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    Transfer {
        pane_id: PaneId,
        mode: FileTransferMode,
        item_count: usize,
        label: String,
        clear_clipboard: bool,
    },
    PasteText {
        pane_id: PaneId,
    },
    TrashSelection {
        pane_id: PaneId,
        item_count: usize,
    },
    TrashView {
        pane_id: PaneId,
        operation: TrashViewOperation,
        item_count: usize,
    },
    Rename {
        pane_id: PaneId,
        path: PathBuf,
    },
    Create {
        pane_id: PaneId,
        kind: CreatedItemKind,
    },
    Undo {
        pane_id: PaneId,
        label: String,
    },
    External {
        pane_id: PaneId,
        title: String,
        detail: Option<String>,
        cancellable: bool,
    },
}

impl Operation {
    pub fn pane_id(&self) -> PaneId {
        match self {
            Self::Transfer { pane_id, .. }
            | Self::PasteText { pane_id }
            | Self::TrashSelection { pane_id, .. }
            | Self::TrashView { pane_id, .. }
            | Self::Rename { pane_id, .. }
            | Self::Create { pane_id, .. }
            | Self::Undo { pane_id, .. }
            | Self::External { pane_id, .. } => *pane_id,
        }
    }

    pub fn title(&self) -> String {
        match self {
            Self::Transfer {
                mode, item_count, ..
            } => mode.progress_label(*item_count),
            Self::PasteText { .. } => "Pasting text".to_string(),
            Self::TrashSelection { item_count, .. } => {
                format!("Moving {item_count} item(s) to trash")
            }
            Self::TrashView {
                operation,
                item_count,
                ..
            } => operation.progress_label(*item_count),
            Self::Rename { path, .. } => format!("Renaming {}", path.display()),
            Self::Create { kind, .. } => {
                format!(
                    "Creating {}",
                    created_item_label(*kind).to_ascii_lowercase()
                )
            }
            Self::Undo { label, .. } => format!("Undoing {label}"),
            Self::External { title, .. } => title.clone(),
        }
    }

    pub fn progress_label(&self) -> String {
        match self {
            Self::Transfer { label, .. } => label.clone(),
            Self::PasteText { .. } => "Paste".to_string(),
            _ => self.title(),
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::External { detail, .. } => detail.as_deref(),
            _ => None,
        }
    }

    pub fn cancellable(&self) -> bool {
        match self {
            Self::Transfer { .. } | Self::PasteText { .. } => true,
            Self::External { cancellable, .. } => *cancellable,
            _ => false,
        }
    }

    pub fn clear_clipboard(&self) -> bool {
        matches!(
            self,
            Self::Transfer {
                clear_clipboard: true,
                ..
            }
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreatedItemKind {
    File,
    Folder,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateUndoItem {
    pub path: PathBuf,
    pub kind: CreatedItemKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenameUndoItem {
    pub original_path: PathBuf,
    pub renamed_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrashUndoItem {
    pub original_path: PathBuf,
    pub trash_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferUndoItem {
    pub operation: String,
    pub original_source: PathBuf,
    pub destination: PathBuf,
    pub overwritten_backup: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AffectedDirectoryRefresh {
    pub pane_id: PaneId,
    pub event: DirectoryListerEvent,
}

#[derive(Debug, Default)]
pub struct OperationQueue {
    undo_serial: u64,
    undo_records: VecDeque<UndoRecord>,
}

impl OperationQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_undo(&mut self, label: String, affected_dirs: Vec<PathBuf>) -> UndoRecord {
        self.register_undo_with_payload(label, affected_dirs, UndoPayload::None)
    }

    pub fn register_undo_with_payload(
        &mut self,
        label: String,
        affected_dirs: Vec<PathBuf>,
        payload: UndoPayload,
    ) -> UndoRecord {
        self.undo_serial += 1;
        let record = UndoRecord {
            serial: UndoSerial(self.undo_serial),
            label,
            affected_dirs,
            payload,
        };
        self.undo_records.push_back(record.clone());
        record
    }

    pub fn latest_undo(&self) -> Option<&UndoRecord> {
        self.undo_records.back()
    }

    pub fn take_latest_undo(&mut self, serial: UndoSerial) -> Option<UndoRecord> {
        let latest = self.undo_records.back()?;
        if latest.serial != serial {
            return None;
        }
        self.undo_records.pop_back()
    }

    pub fn refresh_affected_panes(
        controller: &mut PaneController,
        affected_dirs: &[PathBuf],
    ) -> Vec<AffectedDirectoryRefresh> {
        let mut refreshes = Vec::new();
        let pane_ids = controller.pane_ids().to_vec();
        for pane_id in pane_ids {
            let Some(current_dir) = controller
                .pane(pane_id)
                .map(|pane| pane.current_dir.clone())
            else {
                continue;
            };
            if affected_dirs
                .iter()
                .any(|affected| same_directory(affected, &current_dir))
                && let Some(event) = controller.reload(pane_id)
            {
                refreshes.push(AffectedDirectoryRefresh { pane_id, event });
            }
        }
        refreshes
    }
}

fn same_directory(left: &Path, right: &Path) -> bool {
    left == right
        || left
            .canonicalize()
            .ok()
            .zip(right.canonicalize().ok())
            .is_some_and(|(left, right)| left == right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_undo_serial_is_rejected() {
        let mut queue = OperationQueue::new();
        let first = queue.register_undo("first".to_string(), vec![PathBuf::from("/tmp/a")]);
        let second = queue.register_undo("second".to_string(), vec![PathBuf::from("/tmp/b")]);

        assert!(queue.take_latest_undo(first.serial).is_none());
        assert_eq!(queue.take_latest_undo(second.serial), Some(second));
    }

    #[test]
    fn undo_payload_is_recorded_with_serial() {
        let mut queue = OperationQueue::new();
        let record = queue.register_undo_with_payload(
            "trash".to_string(),
            vec![PathBuf::from("/tmp/a")],
            UndoPayload::Trash {
                items: vec![TrashUndoItem {
                    original_path: PathBuf::from("/tmp/a/file.txt"),
                    trash_path: PathBuf::from("/tmp/trash/file.txt"),
                }],
            },
        );

        assert_eq!(record.serial, UndoSerial(1));
        assert_eq!(queue.latest_undo(), Some(&record));
    }

    #[test]
    fn transfer_undo_payload_records_original_and_destination() {
        let mut queue = OperationQueue::new();
        let record = queue.register_undo_with_payload(
            "copy".to_string(),
            vec![PathBuf::from("/tmp/target")],
            UndoPayload::Transfer {
                items: vec![TransferUndoItem {
                    operation: "copy".to_string(),
                    original_source: PathBuf::from("/tmp/source/file.txt"),
                    destination: PathBuf::from("/tmp/target/file.txt"),
                    overwritten_backup: None,
                }],
            },
        );

        assert_eq!(record.serial, UndoSerial(1));
        assert_eq!(queue.latest_undo(), Some(&record));
    }

    #[test]
    fn create_undo_payload_records_created_item_kind() {
        let mut queue = OperationQueue::new();
        let record = queue.register_undo_with_payload(
            "create folder".to_string(),
            vec![PathBuf::from("/tmp/target")],
            UndoPayload::Create {
                items: vec![CreateUndoItem {
                    path: PathBuf::from("/tmp/target/New Folder"),
                    kind: CreatedItemKind::Folder,
                }],
            },
        );

        assert_eq!(record.serial, UndoSerial(1));
        assert_eq!(queue.latest_undo(), Some(&record));
    }

    #[test]
    fn rename_undo_payload_records_original_and_renamed_path() {
        let mut queue = OperationQueue::new();
        let record = queue.register_undo_with_payload(
            "rename".to_string(),
            vec![PathBuf::from("/tmp/target")],
            UndoPayload::Rename {
                items: vec![RenameUndoItem {
                    original_path: PathBuf::from("/tmp/target/old.txt"),
                    renamed_path: PathBuf::from("/tmp/target/new.txt"),
                }],
            },
        );

        assert_eq!(record.serial, UndoSerial(1));
        assert_eq!(queue.latest_undo(), Some(&record));
    }

    #[test]
    fn affected_directory_refresh_targets_inactive_original_pane() {
        let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
        let first = controller.focused().unwrap();
        let second = controller.split(first).unwrap();
        controller.load(second, PathBuf::from("/tmp/b"));
        controller.focus(second);

        let refreshes =
            OperationQueue::refresh_affected_panes(&mut controller, &[PathBuf::from("/tmp/a")]);

        assert_eq!(refreshes.len(), 1);
        assert_eq!(refreshes[0].pane_id, first);
        assert_eq!(controller.focused(), Some(second));
    }
}
