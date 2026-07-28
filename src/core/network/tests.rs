use super::*;

#[test]
fn network_root_normalizes_file_manager_and_cosmic_roots() {
    assert_eq!(
        normalize_network_uri("remote:/"),
        Ok(NETWORK_ROOT_URI.to_string())
    );
    assert_eq!(
        normalize_network_uri("network:///"),
        Ok(NETWORK_ROOT_URI.to_string())
    );
    assert_eq!(
        normalize_network_uri("NETWORK:/"),
        Ok(NETWORK_ROOT_URI.to_string())
    );
    assert!(is_network_root_path(&PathBuf::from(LEGACY_REMOTE_ROOT_URI)));
    assert_eq!(network_root_path(), PathBuf::from(NETWORK_ROOT_URI));
}

#[test]
fn network_share_uri_parses_supported_schemes() {
    assert_eq!(
        normalize_network_uri("SMB://server/Share%20Name"),
        Ok("smb://server/Share%20Name".to_string())
    );
    assert_eq!(
        parse_network_location("smb://server/Share%20Name").unwrap(),
        NetworkLocation {
            uri: "smb://server/Share%20Name".to_string(),
            display_name: "Share Name on server".to_string(),
            local_path: None,
            scheme: "smb".to_string(),
            icon_name: "folder-remote".to_string(),
        }
    );
    assert_eq!(
        parse_network_location("sftp://user@example.test/home/yk")
            .unwrap()
            .display_name,
        "yk on example.test"
    );
}

#[test]
fn network_paths_support_parent_and_child_navigation() {
    assert_eq!(
        network_parent_path(Path::new("smb://server/share/folder/child/")),
        Some(PathBuf::from("smb://server/share/folder/"))
    );
    assert_eq!(
        network_parent_path(Path::new("smb://server/share/")),
        Some(network_root_path())
    );
    assert_eq!(
        network_child_path(Path::new("smb://server/share/"), "Reports 2026"),
        Some(PathBuf::from("smb://server/share/Reports%202026"))
    );
    assert_eq!(network_uri_from_path(Path::new("/tmp/local")), None);
    assert!(is_network_path(Path::new("sftp://example.test/home/yk")));
}

#[test]
fn network_uri_rejects_unsupported_or_incomplete_values() {
    assert_eq!(normalize_network_uri(""), Err(NetworkUrlError::Empty));
    assert_eq!(
        normalize_network_uri("/tmp/share"),
        Err(NetworkUrlError::MissingScheme("/tmp/share".to_string()))
    );
    assert_eq!(
        normalize_network_uri("http://example.test/"),
        Err(NetworkUrlError::UnsupportedScheme("http".to_string()))
    );
    assert_eq!(
        normalize_network_uri("smb:/server/share"),
        Err(NetworkUrlError::MissingAuthority("smb".to_string()))
    );
    assert_eq!(
        normalize_network_uri("network:///server"),
        Err(NetworkUrlError::RootOnlyScheme("network".to_string()))
    );
}

#[test]
fn network_auth_debug_redacts_password() {
    let auth = NetworkAuth {
        username: Some("yk".to_string()),
        domain: Some("WORKGROUP".to_string()),
        password: Some("secret".to_string()),
        anonymous: false,
        remember: true,
    };
    let debug = format!("{auth:?}");
    assert!(debug.contains("yk"));
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("secret"));
}

#[test]
fn filesystem_type_classifies_remote_and_gvfs_mounts() {
    assert_eq!(
        classify_network_filesystem("ext4"),
        NetworkFilesystemKind::Local
    );
    assert_eq!(
        classify_network_filesystem("cifs"),
        NetworkFilesystemKind::Remote
    );
    assert_eq!(
        classify_network_filesystem("fuse.sshfs"),
        NetworkFilesystemKind::Remote
    );
    assert_eq!(
        classify_network_filesystem("fuse.gvfsd-fuse"),
        NetworkFilesystemKind::Gvfs
    );
}

#[test]
fn gio_cancelled_errors_map_to_network_cancelled() {
    let error = gio::glib::Error::new(gio::IOErrorEnum::Cancelled, "operation cancelled");
    assert_eq!(
        network_gio_error("smb://server/share/", "enumerate", error),
        NetworkScanError::Cancelled
    );
}

#[test]
fn auth_required_message_includes_prompt_defaults_without_passwords() {
    assert_eq!(
        network_auth_required_prompt("Password required", "yk", "WORKGROUP"),
        NetworkAuthPrompt {
            message: "Password required; user: yk; domain: WORKGROUP".to_string(),
            default_username: Some("yk".to_string()),
            default_domain: Some("WORKGROUP".to_string()),
        }
    );
    assert_eq!(
        network_auth_required_prompt("", "", ""),
        NetworkAuthPrompt {
            message: "authentication required".to_string(),
            default_username: None,
            default_domain: None,
        }
    );
}

#[test]
fn network_auth_store_keys_credentials_by_mount_root() {
    let uri = "smb://server/share/folder/report.txt";
    let auth = NetworkAuth {
        username: Some("yk".to_string()),
        domain: Some("WORKGROUP".to_string()),
        password: Some("secret".to_string()),
        anonymous: false,
        remember: false,
    };

    remember_network_auth(uri, auth.clone()).unwrap();

    assert_eq!(
        network_auth_key("smb://server/share/other"),
        Ok("smb://server/share/".to_string())
    );
    assert_eq!(
        stored_network_auth_for_uri("smb://server/share/other"),
        Some(auth)
    );
    forget_network_auth("smb://server/share/").unwrap();
    assert_eq!(stored_network_auth_for_uri(uri), None);
}

#[test]
fn network_auth_store_keys_host_scoped_protocols_by_authority() {
    assert_eq!(
        network_auth_key("sftp://user@example.test/home/yk"),
        Ok("sftp://user@example.test/".to_string())
    );
}
