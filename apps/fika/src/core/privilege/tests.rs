use super::*;

#[test]
fn external_edit_commits_scratch_back_and_cleans_token_dir() {
    let temp = test_dir("commit");
    let original_dir = temp.join("original");
    let scratch_root = temp.join("fika-edit");
    fs::create_dir_all(&original_dir).unwrap();
    let original = original_dir.join("config.txt");
    fs::write(&original, "old").unwrap();

    let service = PrivilegedService::new_for_tests(scratch_root);
    let (scratch_path, token) = service
        .prepare_external_edit_inner(original.clone(), 0)
        .unwrap();
    let scratch_path = PathBuf::from(scratch_path);
    fs::write(&scratch_path, "new").unwrap();

    let committed = service
        .commit_external_edit_inner(&token, scratch_path.clone())
        .unwrap();

    assert_eq!(committed, original.display().to_string());
    assert_eq!(fs::read_to_string(&original).unwrap(), "new");
    assert!(!scratch_path.parent().unwrap().exists());
    assert!(service.external_edits.lock().unwrap().is_empty());

    let _ = fs::remove_dir_all(temp);
}

#[test]
fn external_edit_rejects_changed_original() {
    let temp = test_dir("changed-original");
    let original_dir = temp.join("original");
    let scratch_root = temp.join("fika-edit");
    fs::create_dir_all(&original_dir).unwrap();
    let original = original_dir.join("config.txt");
    fs::write(&original, "old").unwrap();

    let service = PrivilegedService::new_for_tests(scratch_root);
    let (scratch_path, token) = service
        .prepare_external_edit_inner(original.clone(), 0)
        .unwrap();
    fs::write(&original, "changed elsewhere").unwrap();

    let error = service
        .commit_external_edit_inner(&token, PathBuf::from(scratch_path))
        .unwrap_err();

    assert!(error.contains("outside this edit session"));
    assert_eq!(fs::read_to_string(&original).unwrap(), "changed elsewhere");

    let _ = fs::remove_dir_all(temp);
}

#[test]
fn external_edit_discard_removes_scratch() {
    let temp = test_dir("discard");
    let original_dir = temp.join("original");
    let scratch_root = temp.join("fika-edit");
    fs::create_dir_all(&original_dir).unwrap();
    let original = original_dir.join("config.txt");
    fs::write(&original, "old").unwrap();

    let service = PrivilegedService::new_for_tests(scratch_root);
    let (scratch_path, token) = service
        .prepare_external_edit_inner(original.clone(), 0)
        .unwrap();
    let scratch_path = PathBuf::from(scratch_path);

    service.discard_external_edit_inner(&token).unwrap();

    assert_eq!(fs::read_to_string(&original).unwrap(), "old");
    assert!(!scratch_path.parent().unwrap().exists());
    assert!(service.external_edits.lock().unwrap().is_empty());

    let _ = fs::remove_dir_all(temp);
}

#[test]
fn external_edit_can_sync_multiple_saves_before_cleanup() {
    let temp = test_dir("multi-sync");
    let original_dir = temp.join("original");
    let scratch_root = temp.join("fika-edit");
    fs::create_dir_all(&original_dir).unwrap();
    let original = original_dir.join("config.txt");
    fs::write(&original, "old").unwrap();

    let service = PrivilegedService::new_for_tests(scratch_root);
    let (scratch_path, token) = service
        .prepare_external_edit_inner(original.clone(), 0)
        .unwrap();
    let scratch_path = PathBuf::from(scratch_path);

    {
        let mut edits = service.external_edits.lock().unwrap();
        let edit = edits.get_mut(&token).unwrap();
        fs::write(&scratch_path, "new").unwrap();
        sync_external_edit(edit).unwrap();
        fs::write(&scratch_path, "newer").unwrap();
        sync_external_edit(edit).unwrap();
    }

    assert_eq!(fs::read_to_string(&original).unwrap(), "newer");
    assert!(scratch_path.exists());

    service.discard_external_edit_inner(&token).unwrap();
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn stale_external_edit_expires_with_final_writeback() {
    let temp = test_dir("expire");
    let original_dir = temp.join("original");
    let scratch_root = temp.join("fika-edit");
    fs::create_dir_all(&original_dir).unwrap();
    let original = original_dir.join("config.txt");
    fs::write(&original, "old").unwrap();

    let service = PrivilegedService::new_for_tests(scratch_root);
    let (scratch_path, token) = service
        .prepare_external_edit_inner(original.clone(), 0)
        .unwrap();
    let scratch_path = PathBuf::from(scratch_path);
    fs::write(&scratch_path, "expired edit").unwrap();
    {
        let mut edits = service.external_edits.lock().unwrap();
        edits.get_mut(&token).unwrap().created_secs = 0;
    }

    service.expire_stale_external_edits();

    assert_eq!(fs::read_to_string(&original).unwrap(), "expired edit");
    assert!(!scratch_path.parent().unwrap().exists());
    assert!(service.external_edits.lock().unwrap().is_empty());
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn cleanup_refuses_paths_outside_fika_edit() {
    let temp = test_dir("cleanup");
    fs::create_dir_all(&temp).unwrap();
    let path = temp.join("token").join("file.txt");

    let error = cleanup_scratch_token_dir(&path).unwrap_err();

    assert!(error.contains("outside fika-edit"));
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn unit_finished_state_detection_is_conservative() {
    assert!(is_finished_unit_state("inactive"));
    assert!(is_finished_unit_state("failed"));
    assert!(!is_finished_unit_state("activating"));
    assert!(!is_finished_unit_state("active"));
    assert!(!is_finished_unit_state("reloading"));
}

#[test]
fn service_mode_selects_expected_bus() {
    let address = Some("unix:path=/run/user/1000/bus".to_string());
    assert!(matches!(
        bus_connection_address(&address, &ServiceMode::System),
        BusConnection::System
    ));
    assert!(matches!(
        bus_connection_address(&address, &ServiceMode::SessionPkexec { allowed_uid: 1000 }),
        BusConnection::SessionAddress("unix:path=/run/user/1000/bus")
    ));
    assert!(matches!(
        bus_connection_address(&None, &ServiceMode::SessionPkexec { allowed_uid: 1000 }),
        BusConnection::Session
    ));
}

#[test]
fn helper_lifecycle_summary_reports_mode_bus_and_activity() {
    assert_eq!(
        helper_lifecycle_summary("starting", &ServiceMode::System, &None, 0, 0),
        "phase=starting mode=system-bus bus_connection=system authorized_subject=polkit session_address=false idle_for=0 active_external_edits=0"
    );

    let address = Some("unix:path=/run/user/1000/bus".to_string());
    assert_eq!(
        helper_lifecycle_summary(
            "exiting",
            &ServiceMode::SessionPkexec { allowed_uid: 1000 },
            &address,
            180,
            2,
        ),
        "phase=exiting mode=session-bus-pkexec bus_connection=provided-session authorized_subject=uid:1000 session_address=true idle_for=180 active_external_edits=2"
    );

    assert_eq!(
        helper_lifecycle_summary(
            "starting",
            &ServiceMode::SessionPkexec { allowed_uid: 1000 },
            &None,
            0,
            0,
        ),
        "phase=starting mode=session-bus-pkexec bus_connection=session authorized_subject=uid:1000 session_address=false idle_for=0 active_external_edits=0"
    );
}

#[test]
fn privilege_env_flag_truthy_values_are_explicit() {
    assert!(env_flag_is_truthy("1"));
    assert!(env_flag_is_truthy(" true "));
    assert!(env_flag_is_truthy("YES"));
    assert!(env_flag_is_truthy("on"));
    assert!(!env_flag_is_truthy(""));
    assert!(!env_flag_is_truthy("0"));
    assert!(!env_flag_is_truthy("false"));
    assert!(!env_flag_is_truthy("disabled"));
}

#[test]
fn privileged_commands_reject_network_paths_before_dbus() {
    let commands = [
        PrivilegedCommand::CreateFolder {
            parent: PathBuf::from("smb://server/share/"),
            name: "folder".to_string(),
        },
        PrivilegedCommand::CreateFile {
            parent: PathBuf::from("smb://server/share/"),
            name: "file.txt".to_string(),
        },
        PrivilegedCommand::Rename {
            path: PathBuf::from("smb://server/share/file.txt"),
            new_name: "renamed.txt".to_string(),
        },
        PrivilegedCommand::Trash {
            paths: vec![PathBuf::from("smb://server/share/file.txt")],
        },
        PrivilegedCommand::Transfer {
            operation: "copy".to_string(),
            source: PathBuf::from("smb://server/share/file.txt"),
            target_dir: PathBuf::from("/tmp"),
        },
        PrivilegedCommand::Transfer {
            operation: "copy".to_string(),
            source: PathBuf::from("/tmp/file.txt"),
            target_dir: PathBuf::from("smb://server/share/"),
        },
    ];

    for command in commands {
        let error = command.validate_local_paths().unwrap_err();
        assert!(error.contains("network locations are not supported"));
        assert!(error.contains("smb://server/share"));
    }
}

#[test]
fn polkit_diagnostics_include_action_and_install_hint() {
    let failed = polkit_check_failed_message("missing action");
    assert!(failed.contains(ACTION_ID));
    assert!(failed.contains(POLICY_FILE));
    assert!(failed.contains("missing action"));

    let denied = polkit_denied_message();
    assert!(denied.contains(ACTION_ID));

    let unavailable = polkit_authority_unavailable_message("no service");
    assert!(unavailable.contains("polkit"));
    assert!(unavailable.contains(ACTION_ID));
    assert!(unavailable.contains(POLICY_FILE));
    assert!(unavailable.contains("no service"));
    assert!(unavailable.contains("desktop polkit agent"));
}

#[test]
fn privileged_helper_start_diagnostic_separates_activation_paths() {
    let message = privileged_helper_start_failed_message(
        "system service missing",
        "session service missing",
        "pkexec not installed",
    );

    assert!(message.contains("System bus activation failed: system service missing"));
    assert!(message.contains("Development session-bus helper failed: session service missing"));
    assert!(message.contains("pkexec fallback failed: pkexec not installed"));
    assert!(message.contains(POLICY_FILE));
    assert!(message.contains("desktop polkit agent"));
}

fn test_dir(name: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "fika-privilege-{name}-{}-{}",
        std::process::id(),
        TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}
