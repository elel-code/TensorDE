use super::*;

#[test]
fn copy_reports_progress() {
    let temp = test_dir("progress");
    fs::create_dir_all(&temp).unwrap();
    let source = temp.join("source.bin");
    let target = temp.join("target");
    fs::create_dir(&target).unwrap();
    fs::write(&source, vec![7_u8; 128 * 1024]).unwrap();

    let mut progress_events = Vec::new();
    let destination =
        perform_transfer_with_progress("copy", &source, &target, "keep-both", None, |progress| {
            progress_events.push(progress);
        })
        .unwrap();

    assert!(destination.exists());
    assert_eq!(fs::metadata(destination).unwrap().len(), 128 * 1024);
    assert!(progress_events.last().is_some_and(
        |progress| progress.bytes_done == 128 * 1024 && progress.bytes_total == 128 * 1024
    ));
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn async_copy_reports_progress_via_compio() {
    let temp = test_dir("async-progress");
    fs::create_dir_all(&temp).unwrap();
    let source = temp.join("source.bin");
    let target = temp.join("target");
    fs::create_dir(&target).unwrap();
    let payload = vec![11_u8; 96 * 1024];
    fs::write(&source, &payload).unwrap();
    let controller = OperationController::new();

    let outcome =
        futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
            let source = source.clone();
            let target = target.clone();
            let controller = controller.clone();
            move || async move {
                let progress_controller = controller.clone();
                perform_transfer_with_progress_outcome_async(
                    "copy",
                    &source,
                    &target,
                    "keep-both",
                    Some(controller),
                    move |progress| {
                        progress_controller.set_progress(progress);
                    },
                )
                .await
            }
        }))
        .unwrap()
        .unwrap();

    assert_eq!(fs::read(outcome.destination).unwrap(), payload);
    let progress = controller.progress();
    assert_eq!(progress.bytes_done, 96 * 1024);
    assert_eq!(progress.bytes_total, 96 * 1024);
    assert_eq!(controller.progress(), progress);
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn create_file_async_uses_compio_unique_destination() {
    let temp = test_dir("create-file-async");
    fs::create_dir_all(&temp).unwrap();
    fs::write(temp.join("New File.txt"), b"occupied").unwrap();

    let created =
        futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
            let temp = temp.clone();
            move || async move { create_file_async(&temp, "New File.txt").await }
        }))
        .unwrap()
        .unwrap();

    assert_eq!(created.file_name().unwrap(), "New File copy.txt");
    assert!(created.is_file());
    assert_eq!(fs::read(temp.join("New File.txt")).unwrap(), b"occupied");
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn create_exact_async_helpers_use_requested_path() {
    let temp = test_dir("create-exact-async");
    fs::create_dir_all(&temp).unwrap();
    let folder = temp.join("made");
    let file = temp.join("note.txt");

    futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
        let folder = folder.clone();
        let file = file.clone();
        move || async move {
            create_folder_at_async(&folder).await?;
            create_file_at_async(&file).await
        }
    }))
    .unwrap()
    .unwrap();

    assert!(folder.is_dir());
    assert!(file.is_file());
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn rename_path_async_uses_compio_and_rejects_conflicts() {
    let temp = test_dir("rename-async");
    fs::create_dir_all(&temp).unwrap();
    let original = temp.join("old.txt");
    let occupied = temp.join("taken.txt");
    fs::write(&original, b"old").unwrap();
    fs::write(&occupied, b"taken").unwrap();

    let conflict =
        futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
            let original = original.clone();
            move || async move { rename_path_async(&original, "taken.txt").await }
        }))
        .unwrap()
        .unwrap_err();
    assert_eq!(conflict, "an item with that name already exists");

    let renamed =
        futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
            let original = original.clone();
            move || async move { rename_path_async(&original, "new.txt").await }
        }))
        .unwrap()
        .unwrap();

    assert_eq!(renamed, temp.join("new.txt"));
    assert!(!original.exists());
    assert!(renamed.is_file());
    assert_eq!(fs::read(occupied).unwrap(), b"taken");
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn copy_can_be_cancelled() {
    let temp = test_dir("cancel");
    fs::create_dir_all(&temp).unwrap();
    let source = temp.join("source.bin");
    let target = temp.join("target");
    fs::create_dir(&target).unwrap();
    fs::write(&source, vec![11_u8; 256 * 1024]).unwrap();

    let controller = OperationController::new();
    let cancel_from_progress = controller.clone();
    let result = perform_transfer_with_progress(
        "copy",
        &source,
        &target,
        "keep-both",
        Some(controller),
        move |_| cancel_from_progress.cancel(),
    );

    assert!(result.is_err());
    assert!(!target.join("source.bin").exists());
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn cancelled_directory_copy_removes_partial_destination_root() {
    let temp = test_dir("cancel-directory");
    fs::create_dir_all(&temp).unwrap();
    let source = temp.join("source");
    let nested = source.join("nested");
    let target = temp.join("target");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(nested.join("first.bin"), vec![13_u8; 128 * 1024]).unwrap();
    fs::write(source.join("second.bin"), vec![17_u8; 128 * 1024]).unwrap();

    let controller = OperationController::new();
    let cancel_from_progress = controller.clone();
    let result = perform_transfer_with_progress(
        "copy",
        &source,
        &target,
        "keep-both",
        Some(controller),
        move |_| cancel_from_progress.cancel(),
    );

    assert!(result.is_err());
    assert!(!target.join("source").exists());
    assert!(source.join("nested/first.bin").exists());
    assert!(source.join("second.bin").exists());
    let _ = fs::remove_dir_all(temp);
}

#[cfg(unix)]
#[test]
fn copy_preserves_symlinks_instead_of_dereferencing() {
    let temp = test_dir("copy-symlink");
    let source_dir = temp.join("source");
    let target = temp.join("target");
    let nested_target = temp.join("nested-target");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir(&target).unwrap();
    fs::create_dir(&nested_target).unwrap();
    fs::write(source_dir.join("file.txt"), "linked file").unwrap();
    fs::create_dir(source_dir.join("folder")).unwrap();
    std::os::unix::fs::symlink("file.txt", source_dir.join("file-link")).unwrap();
    std::os::unix::fs::symlink("folder", source_dir.join("folder-link")).unwrap();

    let copied_file_link = perform_transfer_with_progress(
        "copy",
        &source_dir.join("file-link"),
        &target,
        "keep-both",
        None,
        |_| {},
    )
    .unwrap();
    let copied_folder_link = perform_transfer_with_progress(
        "copy",
        &source_dir.join("folder-link"),
        &target,
        "keep-both",
        None,
        |_| {},
    )
    .unwrap();

    assert!(
        fs::symlink_metadata(&copied_file_link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&copied_file_link).unwrap(),
        PathBuf::from("file.txt")
    );
    assert!(
        fs::symlink_metadata(&copied_folder_link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&copied_folder_link).unwrap(),
        PathBuf::from("folder")
    );

    let mut directory_progress = Vec::new();
    let copied_dir = perform_transfer_with_progress(
        "copy",
        &source_dir,
        &nested_target,
        "keep-both",
        None,
        |progress| directory_progress.push(progress),
    )
    .unwrap();
    assert!(!directory_progress.is_empty());
    assert!(
        directory_progress
            .iter()
            .all(|progress| progress.bytes_done <= progress.bytes_total)
    );
    assert!(
        directory_progress
            .last()
            .is_some_and(|progress| progress.bytes_done == progress.bytes_total)
    );

    let copied_nested_file_link = copied_dir.join("file-link");
    let copied_nested_folder_link = copied_dir.join("folder-link");
    assert!(
        fs::symlink_metadata(&copied_nested_file_link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&copied_nested_file_link).unwrap(),
        PathBuf::from("file.txt")
    );
    assert!(
        fs::symlink_metadata(&copied_nested_folder_link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&copied_nested_folder_link).unwrap(),
        PathBuf::from("folder")
    );

    let _ = fs::remove_dir_all(temp);
}

#[cfg(unix)]
#[test]
fn copy_preserves_broken_symlink_without_dereferencing() {
    let temp = test_dir("copy-broken-symlink");
    let source_dir = temp.join("source");
    let target = temp.join("target");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir(&target).unwrap();
    let source = source_dir.join("missing-link");
    std::os::unix::fs::symlink("missing-target.txt", &source).unwrap();

    assert!(!source.exists());
    assert!(path_exists(&source));

    let copied =
        perform_transfer_with_progress("copy", &source, &target, "keep-both", None, |_| {})
            .unwrap();

    assert!(!copied.exists());
    assert!(
        fs::symlink_metadata(&copied)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&copied).unwrap(),
        PathBuf::from("missing-target.txt")
    );

    let _ = fs::remove_dir_all(temp);
}

#[cfg(unix)]
#[test]
fn keep_both_treats_broken_symlink_destination_as_conflict() {
    let temp = test_dir("broken-symlink-conflict");
    let source_dir = temp.join("source");
    let target = temp.join("target");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir(&target).unwrap();
    let source = source_dir.join("note.txt");
    let occupied = target.join("note.txt");
    fs::write(&source, "new").unwrap();
    std::os::unix::fs::symlink("missing-target.txt", &occupied).unwrap();

    assert!(!occupied.exists());
    assert!(path_exists(&occupied));

    let copied =
        perform_transfer_with_progress("copy", &source, &target, "keep-both", None, |_| {})
            .unwrap();

    assert_eq!(copied, target.join("note copy.txt"));
    assert_eq!(fs::read_to_string(copied).unwrap(), "new");
    assert!(
        fs::symlink_metadata(&occupied)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&occupied).unwrap(),
        PathBuf::from("missing-target.txt")
    );

    let _ = fs::remove_dir_all(temp);
}

#[test]
fn transfer_core_rejects_self_and_descendant_targets() {
    let temp = test_dir("target-relation");
    let source = temp.join("source");
    let child = source.join("child");
    let sibling = temp.join("sibling");
    fs::create_dir_all(&child).unwrap();
    fs::create_dir_all(&sibling).unwrap();

    assert_eq!(
        transfer_target_relation(&source, &source),
        Some(TransferTargetRelation::Same)
    );
    assert_eq!(
        transfer_target_relation(&source, &child),
        Some(TransferTargetRelation::Descendant)
    );
    assert_eq!(transfer_target_relation(&source, &sibling), None);
    assert_eq!(
        perform_transfer_with_progress("copy", &source, &source, "keep-both", None, |_| {})
            .unwrap_err(),
        "cannot transfer an item onto itself"
    );
    assert_eq!(
        perform_transfer_with_progress("copy", &source, &child, "keep-both", None, |_| {})
            .unwrap_err(),
        "cannot transfer a folder into itself"
    );

    let _ = fs::remove_dir_all(temp);
}

#[cfg(unix)]
#[test]
fn transfer_core_rejects_symlinked_descendant_target() {
    let temp = test_dir("symlink-target-relation");
    let source = temp.join("source");
    let child = source.join("child");
    let link = temp.join("link-to-child");
    fs::create_dir_all(&child).unwrap();
    std::os::unix::fs::symlink(&child, &link).unwrap();

    assert_eq!(
        transfer_target_relation(&source, &link),
        Some(TransferTargetRelation::Descendant)
    );
    assert_eq!(
        perform_transfer_with_progress("copy", &source, &link, "keep-both", None, |_| {})
            .unwrap_err(),
        "cannot transfer a folder into itself"
    );

    let _ = fs::remove_dir_all(temp);
}

#[test]
fn transfer_conflict_policy_can_keep_both_overwrite_or_rename() {
    let temp = test_dir("conflict");
    fs::create_dir_all(&temp).unwrap();
    let source_dir = temp.join("source");
    let target = temp.join("target");
    fs::create_dir(&source_dir).unwrap();
    fs::create_dir(&target).unwrap();
    let source = source_dir.join("note.txt");
    fs::write(&source, b"new").unwrap();
    fs::write(target.join("note.txt"), b"old").unwrap();

    let kept = perform_transfer_with_progress("copy", &source, &target, "keep-both", None, |_| {})
        .unwrap();
    assert_eq!(
        kept.file_name().and_then(|name| name.to_str()),
        Some("note copy.txt")
    );
    assert_eq!(fs::read_to_string(target.join("note.txt")).unwrap(), "old");

    let overwritten =
        perform_transfer_with_progress("copy", &source, &target, "overwrite", None, |_| {})
            .unwrap();
    assert_eq!(overwritten, target.join("note.txt"));
    assert_eq!(fs::read_to_string(target.join("note.txt")).unwrap(), "new");

    let renamed = perform_transfer_with_progress(
        "copy",
        &source,
        &target,
        "rename:custom-note.txt",
        None,
        |_| {},
    )
    .unwrap();
    assert_eq!(renamed, target.join("custom-note.txt"));
    assert_eq!(
        fs::read_to_string(target.join("custom-note.txt")).unwrap(),
        "new"
    );

    let rejected_existing =
        perform_transfer_with_progress("copy", &source, &target, "rename:note.txt", None, |_| {});
    assert!(rejected_existing.is_err());

    let _ = fs::remove_dir_all(temp);
}

#[test]
fn overwrite_replaces_existing_directory_atomically() {
    let temp = test_dir("overwrite-dir");
    fs::create_dir_all(&temp).unwrap();
    let source_parent = temp.join("source-parent");
    let target = temp.join("target");
    let source = source_parent.join("project");
    let existing = target.join("project");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&existing).unwrap();
    fs::write(source.join("new.txt"), "new").unwrap();
    fs::write(existing.join("old.txt"), "old").unwrap();

    let overwritten =
        perform_transfer_with_progress("copy", &source, &target, "overwrite", None, |_| {})
            .unwrap();

    assert_eq!(overwritten, existing);
    assert_eq!(fs::read_to_string(existing.join("new.txt")).unwrap(), "new");
    assert!(!existing.join("old.txt").exists());
    assert!(!fs::read_dir(&target).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("fika-overwrite-backup")
    }));

    let _ = fs::remove_dir_all(temp);
}

#[test]
fn overwrite_outcome_can_restore_replaced_item_on_undo() {
    let temp = test_dir("overwrite-undo");
    fs::create_dir_all(&temp).unwrap();
    let source_dir = temp.join("source");
    let target = temp.join("target");
    fs::create_dir(&source_dir).unwrap();
    fs::create_dir(&target).unwrap();
    let source = source_dir.join("note.txt");
    let destination = target.join("note.txt");
    fs::write(&source, "new").unwrap();
    fs::write(&destination, "old").unwrap();

    let outcome =
        perform_transfer_with_progress_outcome("copy", &source, &target, "overwrite", None, |_| {})
            .unwrap();
    let backup = outcome.overwritten_backup.clone().unwrap();

    assert_eq!(outcome.destination, destination);
    assert_eq!(fs::read_to_string(&destination).unwrap(), "new");
    assert!(backup.exists());

    undo_transfer_with_backup("copy", &source, &destination, Some(&backup)).unwrap();

    assert_eq!(fs::read_to_string(&destination).unwrap(), "old");
    assert!(!backup.exists());
    assert!(source.exists());
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn undo_transfer_removes_copy_and_restores_move() {
    let temp = test_dir("undo");
    fs::create_dir_all(&temp).unwrap();
    let source_dir = temp.join("source");
    let target = temp.join("target");
    fs::create_dir(&source_dir).unwrap();
    fs::create_dir(&target).unwrap();

    let copy_source = source_dir.join("copy.txt");
    fs::write(&copy_source, b"copy").unwrap();
    let copied =
        perform_transfer_with_progress("copy", &copy_source, &target, "keep-both", None, |_| {})
            .unwrap();
    undo_transfer("copy", &copy_source, &copied).unwrap();
    assert!(!copied.exists());
    assert!(copy_source.exists());

    let move_source = source_dir.join("move.txt");
    fs::write(&move_source, b"move").unwrap();
    let moved =
        perform_transfer_with_progress("move", &move_source, &target, "keep-both", None, |_| {})
            .unwrap();
    assert!(!move_source.exists());
    undo_transfer("move", &move_source, &moved).unwrap();
    assert!(move_source.exists());
    assert!(!moved.exists());
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn undo_create_folder_removes_empty_created_folder() {
    let temp = test_dir("undo-create-folder");
    fs::create_dir_all(&temp).unwrap();
    let created = create_folder(&temp, "New Folder").unwrap();

    undo_create_folder(&created).unwrap();

    assert!(!created.exists());
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn create_file_uses_unique_destination_and_can_be_undone() {
    let temp = test_dir("create-file");
    fs::create_dir_all(&temp).unwrap();
    fs::write(temp.join("New File.txt"), b"occupied").unwrap();

    let created = create_file(&temp, "New File.txt").unwrap();

    assert_eq!(created.file_name().unwrap(), "New File copy.txt");
    assert!(created.is_file());
    undo_create_file(&created).unwrap();
    assert!(!created.exists());
    assert!(temp.join("New File.txt").exists());
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn write_unique_file_uses_keep_both_names_and_writes_bytes() {
    let temp = test_dir("write-unique-file");
    fs::create_dir_all(&temp).unwrap();
    fs::write(temp.join("Pasted Text.txt"), b"old").unwrap();

    let created = write_unique_file(&temp, "Pasted Text", "txt", b"new").unwrap();

    assert_eq!(created.file_name().unwrap(), "Pasted Text copy.txt");
    assert_eq!(fs::read(&created).unwrap(), b"new");
    assert_eq!(fs::read(temp.join("Pasted Text.txt")).unwrap(), b"old");
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn undo_rename_restores_original_name() {
    let temp = test_dir("undo-rename");
    fs::create_dir_all(&temp).unwrap();
    let original = temp.join("old.txt");
    fs::write(&original, "contents").unwrap();

    let renamed = rename_path(&original, "new.txt").unwrap();
    undo_rename(&original, &renamed).unwrap();

    assert!(original.exists());
    assert!(!renamed.exists());
    let _ = fs::remove_dir_all(temp);
}

#[cfg(unix)]
#[test]
fn rename_treats_broken_symlink_destination_as_occupied() {
    let temp = test_dir("rename-broken-symlink-conflict");
    fs::create_dir_all(&temp).unwrap();
    let source = temp.join("source.txt");
    let occupied = temp.join("taken.txt");
    fs::write(&source, "contents").unwrap();
    std::os::unix::fs::symlink("missing-target.txt", &occupied).unwrap();

    assert_eq!(
        rename_path(&source, "taken.txt").unwrap_err(),
        "an item with that name already exists"
    );
    assert!(source.exists());
    assert!(!occupied.exists());
    assert!(path_exists(&occupied));

    let _ = fs::remove_dir_all(temp);
}

#[test]
fn undo_trash_restores_original_paths() {
    let temp = test_dir("undo-trash");
    let original_dir = temp.join("originals");
    let trash_dir = temp.join("trash");
    fs::create_dir_all(&original_dir).unwrap();
    fs::create_dir_all(&trash_dir).unwrap();
    let first = original_dir.join("first.txt");
    let second = original_dir.join("second.txt");
    let trashed_first = trash_dir.join("first.txt");
    let trashed_second = trash_dir.join("second.txt");
    fs::write(&trashed_first, "first").unwrap();
    fs::write(&trashed_second, "second").unwrap();

    let items = vec![
        (first.clone(), trashed_first.clone()),
        (second.clone(), trashed_second.clone()),
    ];

    undo_trash(&items).unwrap();

    assert_eq!(fs::read_to_string(&first).unwrap(), "first");
    assert_eq!(fs::read_to_string(&second).unwrap(), "second");
    assert!(!trashed_first.exists());
    assert!(!trashed_second.exists());
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn trash_paths_records_original_and_trash_destinations() {
    let temp = test_dir("trash-records");
    fs::create_dir_all(&temp).unwrap();
    let first = temp.join("first.txt");
    fs::write(&first, "first").unwrap();

    let summary = trash_paths(std::slice::from_ref(&first));

    if summary.failures.is_empty() {
        assert_eq!(summary.successes.len(), 1);
        assert_eq!(summary.successes[0].original_path, first);
        assert!(summary.successes[0].trash_path.exists());
        let _ = undo_trash(&[(
            summary.successes[0].original_path.clone(),
            summary.successes[0].trash_path.clone(),
        )]);
    }
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn trash_paths_async_records_original_and_trash_destinations() {
    let temp = test_dir("trash-records-async");
    fs::create_dir_all(&temp).unwrap();
    let first = temp.join("first.txt");
    fs::write(&first, "first").unwrap();

    let summary =
        futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
            let first = first.clone();
            move || async move { trash_paths_async(vec![first]).await }
        }))
        .unwrap();

    if summary.failures.is_empty() {
        assert_eq!(summary.successes.len(), 1);
        assert_eq!(summary.successes[0].original_path, first);
        assert!(!summary.successes[0].original_path.exists());
        assert!(summary.successes[0].trash_path.exists());
        assert_eq!(
            trash_metadata(&summary.successes[0].trash_path)
                .unwrap()
                .original_path,
            summary.successes[0].original_path
        );
        let _ = undo_trash(&[(
            summary.successes[0].original_path.clone(),
            summary.successes[0].trash_path.clone(),
        )]);
    }
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn trash_path_helpers_identify_xdg_trash_files_location() {
    let trash_files = trash_files_dir();

    assert!(is_trash_files_dir(&trash_files));
    assert!(is_in_trash_files_dir(&trash_files.join("trashed.txt")));
    assert!(!is_in_trash_files_dir(
        &trash_files.with_file_name("outside-trash")
    ));
}

#[test]
fn empty_trash_async_removes_files_metadata_and_updates_status() {
    let temp = test_dir("empty-trash-async");
    let files_dir = temp.join("Trash").join("files");
    let info_dir = temp.join("Trash").join("info");
    let trashrc = temp.join("config").join("trashrc");
    fs::create_dir_all(&files_dir).unwrap();
    fs::create_dir_all(&info_dir).unwrap();

    let original = temp.join("original.txt");
    let trash_path = files_dir.join("trashed.txt");
    fs::write(&trash_path, b"trashed").unwrap();
    fs::write(info_dir.join("trashed.txt.trashinfo"), trashinfo(&original)).unwrap();
    fs::write(
        info_dir.join("orphan.trashinfo"),
        trashinfo(&temp.join("orphan.txt")),
    )
    .unwrap();
    write_trash_status_empty_at(&trashrc, false).unwrap();

    let summary =
        futures_lite::future::block_on(crate::core::operation_runtime::run_operation_task({
            let files_dir = files_dir.clone();
            let info_dir = info_dir.clone();
            let trashrc = trashrc.clone();
            move || async move { empty_trash_in_dirs_async(files_dir, info_dir, trashrc).await }
        }))
        .unwrap();

    assert_eq!(summary.successes.len(), 1);
    assert_eq!(summary.successes[0].original_path, trash_path);
    assert!(summary.failures.is_empty());
    assert!(fs::read_dir(&files_dir).unwrap().next().is_none());
    assert!(fs::read_dir(&info_dir).unwrap().next().is_none());
    assert!(trash_status_empty_at(&trashrc));

    let _ = fs::remove_dir_all(temp);
}

#[test]
fn empty_trash_sync_uses_swap_emptying_path() {
    let temp = test_dir("empty-trash-sync");
    let files_dir = temp.join("Trash").join("files");
    let info_dir = temp.join("Trash").join("info");
    let trashrc = temp.join("config").join("trashrc");
    fs::create_dir_all(files_dir.join("nested")).unwrap();
    fs::create_dir_all(&info_dir).unwrap();

    let original = temp.join("original.txt");
    let trash_path = files_dir.join("nested");
    fs::write(trash_path.join("child.txt"), b"trashed").unwrap();
    fs::write(info_dir.join("nested.trashinfo"), trashinfo(&original)).unwrap();
    fs::write(
        info_dir.join("orphan.trashinfo"),
        trashinfo(&temp.join("orphan.txt")),
    )
    .unwrap();
    write_trash_status_empty_at(&trashrc, false).unwrap();

    let summary = empty_trash_in_dirs(files_dir.clone(), info_dir.clone(), trashrc.clone());

    assert_eq!(summary.successes.len(), 1);
    assert_eq!(summary.successes[0].original_path, trash_path);
    assert!(summary.failures.is_empty());
    assert!(fs::read_dir(&files_dir).unwrap().next().is_none());
    assert!(fs::read_dir(&info_dir).unwrap().next().is_none());
    assert!(trash_status_empty_at(&trashrc));

    let _ = fs::remove_dir_all(temp);
}

#[test]
fn trashinfo_path_decodes_original_location() {
    let info = "[Trash Info]\nPath=/tmp/a%20b%5Bc%5D.txt\nDeletionDate=2026-06-02T10:11:12\n";

    assert_eq!(
        trash_original_path_from_info(info).unwrap(),
        PathBuf::from("/tmp/a b[c].txt")
    );
}

#[test]
fn trashinfo_path_rejects_missing_relative_or_invalid_values() {
    assert_eq!(
        trash_original_path_from_info("[Trash Info]\nDeletionDate=now\n").unwrap_err(),
        "trash metadata is missing Path"
    );
    assert_eq!(
        trash_original_path_from_info("[Trash Info]\nPath=relative/file.txt\n").unwrap_err(),
        "trash metadata Path is not absolute: relative/file.txt"
    );
    assert_eq!(
        trash_original_path_from_info("[Trash Info]\nPath=/tmp/%XX.txt\n").unwrap_err(),
        "trash metadata Path contains invalid percent escape"
    );
}

#[test]
fn trashrc_status_empty_defaults_and_parses_status_group() {
    assert_eq!(trash_status_empty_from_contents(""), None);
    assert_eq!(
        trash_status_empty_from_contents("[Other]\nEmpty=false\n"),
        None
    );
    assert_eq!(
        trash_status_empty_from_contents("[Status]\nEmpty=false\n"),
        Some(false)
    );
    assert_eq!(
        trash_status_empty_from_contents("[Status]\nEmpty=true\n"),
        Some(true)
    );
    assert_eq!(
        trash_status_empty_from_contents("[Status]\nEmpty=1\n"),
        Some(true)
    );
    assert_eq!(
        trash_status_empty_from_contents("[Status]\nEmpty=no\n"),
        Some(false)
    );
}

#[test]
fn trashrc_status_write_round_trips() {
    let temp = test_dir("trashrc-status");
    let path = temp.join("config").join("trashrc");

    assert!(trash_status_empty_at(&path));

    write_trash_status_empty_at(&path, false).unwrap();
    assert!(!trash_status_empty_at(&path));
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "[Status]\nEmpty=false\n"
    );

    write_trash_status_empty_at(&path, true).unwrap();
    assert!(trash_status_empty_at(&path));
    assert_eq!(fs::read_to_string(&path).unwrap(), "[Status]\nEmpty=true\n");

    let _ = fs::remove_dir_all(temp);
}

fn test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "fika-file-ops-{name}-{}-{}",
        std::process::id(),
        current_trash_time()
    ))
}
