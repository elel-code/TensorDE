use super::*;
use crate::core::operations::{CreateUndoItem, RenameUndoItem, UndoSerial};

fn test_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "fika-operation-tasks-{name}-{}-{nanos}",
        std::process::id()
    ))
}

#[test]
fn rename_item_result_renames_item_and_records_affected_dir() {
    let temp = test_dir("rename-item");
    std::fs::create_dir_all(&temp).unwrap();
    let original = temp.join("old.txt");
    let renamed = temp.join("new.txt");
    std::fs::write(&original, "rename").unwrap();

    let result = rename_item_result(PaneId(11), original.clone(), "new.txt".to_string());
    let renamed_path = result.result.unwrap();

    assert_eq!(result.pane_id, PaneId(11));
    assert_eq!(result.original_path, original.clone());
    assert_eq!(result.affected_dirs, vec![temp.clone()]);
    assert_eq!(renamed_path, renamed);
    assert!(!original.exists());
    assert_eq!(std::fs::read_to_string(&renamed_path).unwrap(), "rename");
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn create_item_result_creates_default_folder_and_records_affected_dir() {
    let temp = test_dir("create-folder");
    std::fs::create_dir_all(&temp).unwrap();

    let result = create_item_result(PaneId(5), temp.clone(), CreatedItemKind::Folder);
    let created = result.result.unwrap();

    assert_eq!(result.pane_id, PaneId(5));
    assert_eq!(result.kind, CreatedItemKind::Folder);
    assert_eq!(result.affected_dirs, vec![temp.clone()]);
    assert_eq!(created.file_name().unwrap(), "New Folder");
    assert!(created.is_dir());
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn create_item_result_uses_keep_both_name_for_default_file() {
    let temp = test_dir("create-file");
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::write(temp.join("New File.txt"), "occupied").unwrap();

    let result = create_item_result(PaneId(6), temp.clone(), CreatedItemKind::File);
    let created = result.result.unwrap();

    assert_eq!(result.kind, CreatedItemKind::File);
    assert_eq!(result.affected_dirs, vec![temp.clone()]);
    assert_eq!(created.file_name().unwrap(), "New File copy.txt");
    assert!(created.is_file());
    assert!(temp.join("New File.txt").exists());
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn create_item_result_uses_requested_parent_directory() {
    let temp = test_dir("create-item-parent");
    let current_dir = temp.join("current");
    let directory_target = temp.join("directory-target");
    std::fs::create_dir_all(&current_dir).unwrap();
    std::fs::create_dir_all(&directory_target).unwrap();

    let result = create_item_result(PaneId(7), directory_target.clone(), CreatedItemKind::File);

    assert_eq!(result.result, Ok(directory_target.join("New File.txt")));
    assert!(directory_target.join("New File.txt").exists());
    assert!(!current_dir.join("New File.txt").exists());
    assert_eq!(result.affected_dirs, vec![directory_target]);
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn paste_text_result_writes_plain_text_file_and_records_create_undo() {
    let temp = test_dir("paste-text");
    std::fs::create_dir_all(&temp).unwrap();

    let result = paste_text_result(PaneId(15), temp.clone(), "plain text");

    let destination = temp.join("Pasted Text.txt");
    assert_eq!(result.pane_id, PaneId(15));
    assert_eq!(result.mode, FileTransferMode::Copy);
    assert!(!result.clear_clipboard);
    assert_eq!(result.label, "Paste");
    assert_eq!(result.success_count, 1);
    assert_eq!(result.failure_count, 0);
    assert_eq!(result.affected_dirs, vec![temp.clone()]);
    assert!(result.undo_items.is_empty());
    assert_eq!(
        result.created_items,
        vec![CreateUndoItem {
            path: destination.clone(),
            kind: CreatedItemKind::File,
        }]
    );
    assert_eq!(std::fs::read_to_string(destination).unwrap(), "plain text");
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn transfer_paths_result_copies_item_and_records_transfer_undo() {
    let temp = test_dir("transfer-copy");
    let source_dir = temp.join("source");
    let target_dir = temp.join("target");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();
    let source = source_dir.join("note.txt");
    std::fs::write(&source, "copy").unwrap();

    let result = transfer_paths_result(
        PaneId(7),
        target_dir.clone(),
        FileTransferMode::Copy,
        vec![source.clone()],
        "Copy",
        false,
        None,
    );

    let destination = target_dir.join("note.txt");
    assert_eq!(result.pane_id, PaneId(7));
    assert_eq!(result.mode, FileTransferMode::Copy);
    assert!(!result.clear_clipboard);
    assert_eq!(result.success_count, 1);
    assert_eq!(result.failure_count, 0);
    assert_eq!(result.affected_dirs, vec![target_dir.clone()]);
    assert_eq!(result.refresh_dirs, vec![target_dir.clone()]);
    assert_eq!(
        result.undo_items,
        vec![TransferUndoItem {
            operation: "copy".to_string(),
            original_source: source.clone(),
            destination: destination.clone(),
            overwritten_backup: None,
        }]
    );
    assert_eq!(std::fs::read_to_string(destination).unwrap(), "copy");
    assert!(source.exists());
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn transfer_paths_result_moves_item_and_marks_both_directories() {
    let temp = test_dir("transfer-move");
    let source_dir = temp.join("source");
    let target_dir = temp.join("target");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();
    let source = source_dir.join("note.txt");
    std::fs::write(&source, "move").unwrap();

    let result = transfer_paths_result(
        PaneId(8),
        target_dir.clone(),
        FileTransferMode::Move,
        vec![source.clone()],
        "Move",
        true,
        None,
    );

    let destination = target_dir.join("note.txt");
    assert_eq!(result.mode, FileTransferMode::Move);
    assert!(result.clear_clipboard);
    assert_eq!(result.success_count, 1);
    assert_eq!(result.failure_count, 0);
    assert_eq!(
        result.affected_dirs,
        vec![target_dir.clone(), source_dir.clone()]
    );
    assert_eq!(
        result.refresh_dirs,
        vec![target_dir.clone(), source_dir.clone()]
    );
    assert_eq!(result.undo_items[0].operation, "move");
    assert_eq!(result.undo_items[0].original_source, source);
    assert_eq!(result.undo_items[0].destination, destination.clone());
    assert_eq!(std::fs::read_to_string(destination).unwrap(), "move");
    assert!(!source_dir.join("note.txt").exists());
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn transfer_paths_result_updates_shared_transfer_progress() {
    let temp = test_dir("transfer-progress");
    let source_dir = temp.join("source");
    let target_dir = temp.join("target");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();
    let source = source_dir.join("note.bin");
    std::fs::write(&source, vec![42_u8; 32 * 1024]).unwrap();
    let controller = OperationController::new();

    let result = transfer_paths_result(
        PaneId(13),
        target_dir,
        FileTransferMode::Copy,
        vec![source],
        "Copy",
        false,
        Some(controller.clone()),
    );

    assert_eq!(result.success_count, 1);
    let progress = controller.progress();
    assert_eq!(progress.bytes_total, 32 * 1024);
    assert_eq!(progress.bytes_done, 32 * 1024);
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn transfer_paths_result_async_copies_item_with_compio_progress() {
    let temp = test_dir("transfer-async-progress");
    let source_dir = temp.join("source");
    let target_dir = temp.join("target");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();
    let source = source_dir.join("note.bin");
    std::fs::write(&source, vec![7_u8; 96 * 1024]).unwrap();
    let controller = OperationController::new();

    let result =
        futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
            let target_dir = target_dir.clone();
            let source = source.clone();
            let controller = controller.clone();
            move || async move {
                transfer_paths_result_async(
                    PaneId(21),
                    target_dir,
                    FileTransferMode::Copy,
                    vec![source],
                    "Copy",
                    false,
                    Some(controller),
                )
                .await
            }
        }))
        .unwrap();

    assert_eq!(result.success_count, 1);
    assert_eq!(
        std::fs::read(target_dir.join("note.bin")).unwrap(),
        vec![7_u8; 96 * 1024]
    );
    let progress = controller.progress();
    assert_eq!(progress.bytes_total, 96 * 1024);
    assert_eq!(progress.bytes_done, 96 * 1024);
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn transfer_paths_result_async_copies_directory_recursively() {
    let temp = test_dir("transfer-async-recursive");
    let source_dir = temp.join("source");
    let target_dir = temp.join("target");
    std::fs::create_dir_all(source_dir.join("nested")).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(source_dir.join("root.txt"), b"root").unwrap();
    std::fs::write(source_dir.join("nested").join("child.txt"), b"child").unwrap();
    let controller = OperationController::new();

    let result =
        futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
            let target_dir = target_dir.clone();
            let source_dir = source_dir.clone();
            let controller = controller.clone();
            move || async move {
                transfer_paths_result_async(
                    PaneId(22),
                    target_dir,
                    FileTransferMode::Copy,
                    vec![source_dir],
                    "Copy",
                    false,
                    Some(controller),
                )
                .await
            }
        }))
        .unwrap();

    assert_eq!(result.success_count, 1);
    assert_eq!(
        std::fs::read_to_string(target_dir.join("source").join("root.txt")).unwrap(),
        "root"
    );
    assert_eq!(
        std::fs::read_to_string(target_dir.join("source").join("nested").join("child.txt"))
            .unwrap(),
        "child"
    );
    let progress = controller.progress();
    assert_eq!(progress.bytes_total, 9);
    assert_eq!(progress.bytes_done, 9);
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn transfer_paths_result_honors_cancel_flag_before_transfer() {
    let temp = test_dir("transfer-cancel");
    let source_dir = temp.join("source");
    let target_dir = temp.join("target");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();
    let source = source_dir.join("note.bin");
    std::fs::write(&source, "cancel").unwrap();
    let controller = OperationController::new();
    controller.cancel();

    let result = transfer_paths_result(
        PaneId(14),
        target_dir.clone(),
        FileTransferMode::Copy,
        vec![source],
        "Copy",
        false,
        Some(controller),
    );

    assert_eq!(result.success_count, 0);
    assert_eq!(result.failure_count, 1);
    assert!(result.affected_dirs.is_empty());
    assert!(result.refresh_dirs.is_empty());
    assert!(std::fs::read_dir(&target_dir).unwrap().next().is_none());
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn transfer_paths_result_refreshes_target_after_failed_attempt() {
    let temp = test_dir("transfer-failed-refresh");
    let target_dir = temp.join("target");
    std::fs::create_dir_all(&target_dir).unwrap();

    let result = transfer_paths_result(
        PaneId(17),
        target_dir.clone(),
        FileTransferMode::Copy,
        vec![temp.join("missing.txt")],
        "Copy",
        false,
        None,
    );

    assert_eq!(result.success_count, 0);
    assert_eq!(result.failure_count, 1);
    assert!(result.affected_dirs.is_empty());
    assert_eq!(result.refresh_dirs, vec![target_dir]);
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn trash_view_operation_result_restores_items_and_marks_original_dir() {
    let temp = test_dir("trash-restore");
    std::fs::create_dir_all(&temp).unwrap();
    let unique_name = format!(
        "restore-{}.txt",
        temp.file_name().unwrap().to_string_lossy()
    );
    let original = temp.join(unique_name);
    std::fs::write(&original, "restore").unwrap();
    let trashed = file_ops::trash_paths(std::slice::from_ref(&original));
    assert!(trashed.failures.is_empty());
    let trash_path = trashed.successes[0].trash_path.clone();
    assert!(!original.exists());

    let result = trash_view_operation_result(
        PaneId(16),
        TrashViewOperation::Restore {
            conflict_policy: file_ops::TrashRestoreConflictPolicy::Skip,
        },
        vec![trash_path],
    );

    assert_eq!(result.pane_id, PaneId(16));
    assert_eq!(
        result.operation,
        TrashViewOperation::Restore {
            conflict_policy: file_ops::TrashRestoreConflictPolicy::Skip
        }
    );
    assert_eq!(result.success_count, 1);
    assert_eq!(result.failure_count, 0);
    assert_eq!(
        result.affected_dirs,
        vec![file_ops::trash_files_dir(), temp.clone()]
    );
    assert_eq!(std::fs::read_to_string(&original).unwrap(), "restore");
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn trash_view_operation_result_reports_restore_conflict_without_overwrite() {
    let temp = test_dir("trash-restore-conflict");
    std::fs::create_dir_all(&temp).unwrap();
    let unique_name = format!(
        "restore-conflict-{}.txt",
        temp.file_name().unwrap().to_string_lossy()
    );
    let original = temp.join(unique_name);
    std::fs::write(&original, "trashed").unwrap();
    let trashed = file_ops::trash_paths(std::slice::from_ref(&original));
    assert!(trashed.failures.is_empty());
    let trash_path = trashed.successes[0].trash_path.clone();
    std::fs::write(&original, "replacement").unwrap();

    let result = trash_view_operation_result(
        PaneId(16),
        TrashViewOperation::Restore {
            conflict_policy: file_ops::TrashRestoreConflictPolicy::Skip,
        },
        vec![trash_path.clone()],
    );

    assert_eq!(result.success_count, 0);
    assert_eq!(result.failure_count, 0);
    assert_eq!(result.restore_conflicts.len(), 1);
    assert_eq!(result.restore_conflicts[0].original_path, original);
    assert_eq!(result.restore_conflicts[0].trash_path, trash_path);
    assert!(result.affected_dirs.is_empty());
    assert_eq!(
        std::fs::read_to_string(&result.restore_conflicts[0].original_path).unwrap(),
        "replacement"
    );
    assert!(result.restore_conflicts[0].trash_path.exists());
    let _ =
        file_ops::permanently_delete_trash_paths(&[result.restore_conflicts[0].trash_path.clone()]);
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn trash_view_operation_result_replaces_restore_conflict_when_confirmed() {
    let temp = test_dir("trash-restore-replace-conflict");
    std::fs::create_dir_all(&temp).unwrap();
    let unique_name = format!(
        "restore-replace-conflict-{}.txt",
        temp.file_name().unwrap().to_string_lossy()
    );
    let original = temp.join(unique_name);
    std::fs::write(&original, "trashed").unwrap();
    let trashed = file_ops::trash_paths(std::slice::from_ref(&original));
    assert!(trashed.failures.is_empty());
    let trash_path = trashed.successes[0].trash_path.clone();
    std::fs::write(&original, "replacement").unwrap();

    let result = trash_view_operation_result(
        PaneId(16),
        TrashViewOperation::Restore {
            conflict_policy: file_ops::TrashRestoreConflictPolicy::Replace,
        },
        vec![trash_path.clone()],
    );

    assert_eq!(result.success_count, 1);
    assert_eq!(result.failure_count, 0);
    assert!(result.restore_conflicts.is_empty());
    assert_eq!(
        result.affected_dirs,
        vec![file_ops::trash_files_dir(), temp.clone()]
    );
    assert_eq!(std::fs::read_to_string(&original).unwrap(), "trashed");
    assert!(!trash_path.exists());
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn trash_view_operation_result_deletes_items_permanently() {
    let temp = test_dir("trash-delete-permanently");
    std::fs::create_dir_all(&temp).unwrap();
    let original = temp.join("delete.txt");
    std::fs::write(&original, "delete").unwrap();
    let trashed = file_ops::trash_paths(std::slice::from_ref(&original));
    assert!(trashed.failures.is_empty());
    let trash_path = trashed.successes[0].trash_path.clone();
    assert!(!original.exists());

    let result = trash_view_operation_result(
        PaneId(17),
        TrashViewOperation::DeletePermanently,
        vec![trash_path.clone()],
    );

    assert_eq!(result.pane_id, PaneId(17));
    assert_eq!(result.operation, TrashViewOperation::DeletePermanently);
    assert_eq!(result.success_count, 1);
    assert_eq!(result.failure_count, 0);
    assert_eq!(result.affected_dirs, vec![file_ops::trash_files_dir()]);
    assert!(!trash_path.exists());
    assert!(!original.exists());
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn undo_record_result_restores_transfer_payload() {
    let temp = test_dir("undo-transfer");
    let source_dir = temp.join("source");
    let target_dir = temp.join("target");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();
    let source = source_dir.join("note.txt");
    let destination = target_dir.join("note.txt");
    std::fs::write(&source, "undo").unwrap();

    let transfer = transfer_paths_result(
        PaneId(9),
        target_dir,
        FileTransferMode::Move,
        vec![source.clone()],
        "Move",
        true,
        None,
    );
    assert_eq!(transfer.success_count, 1);
    assert!(destination.exists());
    assert!(!source.exists());

    let undo = undo_record_result(UndoRecord {
        serial: UndoSerial(1),
        label: "Move".to_string(),
        affected_dirs: transfer.affected_dirs,
        payload: UndoPayload::Transfer {
            items: transfer.undo_items,
        },
    });

    assert_eq!(undo.result, Ok("restored 1 item(s)".to_string()));
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "undo");
    assert!(!destination.exists());
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn undo_record_result_restores_rename_payload() {
    let temp = test_dir("undo-rename");
    std::fs::create_dir_all(&temp).unwrap();
    let original = temp.join("old.txt");
    std::fs::write(&original, "undo rename").unwrap();
    let rename = rename_item_result(PaneId(12), original.clone(), "new.txt".to_string());
    let renamed = rename.result.unwrap();
    assert!(renamed.exists());
    assert!(!original.exists());

    let undo = undo_record_result(UndoRecord {
        serial: UndoSerial(1),
        label: "Rename".to_string(),
        affected_dirs: rename.affected_dirs,
        payload: UndoPayload::Rename {
            items: vec![RenameUndoItem {
                original_path: original.clone(),
                renamed_path: renamed.clone(),
            }],
        },
    });

    assert_eq!(undo.result, Ok("restored 1 item(s)".to_string()));
    assert_eq!(std::fs::read_to_string(&original).unwrap(), "undo rename");
    assert!(!renamed.exists());
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn undo_record_result_removes_created_payload() {
    let temp = test_dir("undo-create");
    std::fs::create_dir_all(&temp).unwrap();
    let create = create_item_result(PaneId(10), temp.clone(), CreatedItemKind::File);
    let created = create.result.unwrap();
    assert!(created.exists());

    let undo = undo_record_result(UndoRecord {
        serial: UndoSerial(1),
        label: "Create File".to_string(),
        affected_dirs: create.affected_dirs,
        payload: UndoPayload::Create {
            items: vec![CreateUndoItem {
                path: created.clone(),
                kind: CreatedItemKind::File,
            }],
        },
    });

    assert_eq!(undo.result, Ok("removed 1 item(s)".to_string()));
    assert!(!created.exists());
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn affected_parent_dirs_are_stable_and_deduplicated() {
    let dirs = parent_dirs([
        PathBuf::from("/tmp/a/one.txt"),
        PathBuf::from("/tmp/a/two.txt"),
        PathBuf::from("/tmp/b/three.txt"),
    ]);

    assert_eq!(dirs, vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]);
}

#[test]
fn action_status_reports_mixed_file_operation_results() {
    assert_eq!(action_status("Moved", 2, 0), "Moved: 2 item(s)");
    assert_eq!(action_status("Moved", 0, 1), "Moved failed for 1 item(s)");
    assert_eq!(action_status("Moved", 2, 1), "Moved: 2 item(s), 1 failed");
}
