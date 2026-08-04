use super::*;

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
