use super::*;

fn launcher_test_executable(name: &str) -> (PathBuf, PathBuf) {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let dir = env::temp_dir().join(format!("fika-launcher-test-{unique}"));
    fs::create_dir_all(&dir).unwrap();
    let executable = dir.join(name);
    fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
    }
    (dir, executable)
}

fn desktop_app(id: &str, name: &str, mime_types: &[&str]) -> DesktopApplication {
    DesktopApplication {
        id: id.to_string(),
        desktop_file: PathBuf::from(format!("/apps/{id}")),
        name: name.to_string(),
        exec: format!("{name} %f"),
        icon: None,
        categories: Vec::new(),
        mime_types: mime_types.iter().map(|mime| mime.to_string()).collect(),
        actions: Vec::new(),
    }
}

fn service_menu(id: &str, name: &str, mime_types: &[&str]) -> DesktopServiceMenu {
    DesktopServiceMenu {
        id: id.to_string(),
        desktop_file: PathBuf::from(format!("/servicemenus/{id}")),
        name: name.to_string(),
        icon: None,
        mime_types: mime_types.iter().map(|mime| mime.to_string()).collect(),
        service_types: vec!["KonqPopupMenu/Plugin".to_string()],
        protocols: Vec::new(),
        submenu: None,
        priority: ServiceMenuPriority::Normal,
        required_url_count: None,
        min_url_count: None,
        max_url_count: None,
        show_if_executable: None,
        actions: vec![DesktopAction {
            id: "compress".to_string(),
            name: "Compress".to_string(),
            exec: "ark --add %F".to_string(),
            icon: None,
        }],
    }
}

#[test]
fn desktop_entry_parser_reads_application_mime_and_actions() {
    let entry = parse_desktop_application(
        "org.example.Viewer.desktop",
        "/apps/org.example.Viewer.desktop",
        "\
[Desktop Entry]\n\
Type=Application\n\
Name=Example Viewer\n\
Exec=viewer %f\n\
Icon=viewer\n\
Categories=Graphics;Viewer;\n\
MimeType=text/plain;image/png;\n\
Actions=print;\n\
\n\
[Desktop Action print]\n\
Name=Print\n\
Icon=document-print\n\
Exec=viewer --print %f\n",
    )
    .unwrap();

    assert_eq!(entry.name, "Example Viewer");
    assert_eq!(entry.categories, vec!["Graphics", "Viewer"]);
    assert_eq!(entry.mime_types, vec!["text/plain", "image/png"]);
    assert_eq!(entry.actions[0].name, "Print");
    assert_eq!(entry.actions[0].icon.as_deref(), Some("document-print"));
}

#[test]
fn desktop_service_menu_parser_reads_kde_popup_actions() {
    let entry = parse_desktop_service_menu(
        "compress.desktop",
        "/menus/compress.desktop",
        "\
[Desktop Entry]\n\
Type=Service\n\
Name=Archive Tools\n\
Icon=ark\n\
MimeType=all/allfiles;inode/directory;\n\
X-KDE-ServiceTypes=KonqPopupMenu/Plugin\n\
X-KDE-Protocols=file\n\
X-KDE-Priority=TopLevel\n\
X-KDE-Submenu=Archive\n\
X-KDE-MinNumberOfUrls=1\n\
X-KDE-MaxNumberOfUrls=8\n\
Actions=compress;\n\
\n\
[Desktop Action compress]\n\
Name=Compress\n\
Icon=archive-insert\n\
Exec=ark --add %F\n",
    )
    .unwrap();

    assert_eq!(entry.name, "Archive Tools");
    assert_eq!(entry.mime_types, vec!["all/allfiles", "inode/directory"]);
    assert_eq!(entry.service_types, vec!["KonqPopupMenu/Plugin"]);
    assert_eq!(entry.protocols, vec!["file"]);
    assert_eq!(entry.priority, ServiceMenuPriority::TopLevel);
    assert_eq!(entry.submenu.as_deref(), Some("Archive"));
    assert_eq!(entry.min_url_count, Some(1));
    assert_eq!(entry.max_url_count, Some(8));
    assert_eq!(entry.icon.as_deref(), Some("ark"));
    assert_eq!(entry.actions[0].name, "Compress");
    assert_eq!(entry.actions[0].icon.as_deref(), Some("archive-insert"));
}

#[test]
fn service_menu_roots_are_dedicated_service_directories() {
    let mut dirs = Vec::new();

    push_service_menu_roots(&mut dirs, PathBuf::from("/xdg/share"));

    assert_eq!(
        dirs,
        vec![
            PathBuf::from("/xdg/share/fika/servicemenus"),
            PathBuf::from("/xdg/share/kio/servicemenus"),
            PathBuf::from("/xdg/share/kservices5/ServiceMenus"),
            PathBuf::from("/xdg/share/konqueror/servicemenus"),
        ]
    );
    assert!(!dirs.contains(&PathBuf::from("/xdg/share/applications")));
}

#[test]
fn hidden_desktop_entries_are_not_applications() {
    assert!(
        parse_desktop_application(
            "hidden.desktop",
            "/apps/hidden.desktop",
            "[Desktop Entry]\nType=Application\nHidden=true\nName=Hidden\nExec=hidden %f\n",
        )
        .is_none()
    );
}

#[test]
fn service_actions_only_include_kde_service_menu_actions() {
    let mut app = desktop_app("viewer.desktop", "Viewer", &["text/plain"]);
    app.actions.push(DesktopAction {
        id: "print".to_string(),
        name: "Print".to_string(),
        exec: "viewer --print %f".to_string(),
        icon: None,
    });
    let mut added_app = desktop_app("sender.desktop", "Send To", &[]);
    added_app.actions.push(DesktopAction {
        id: "send".to_string(),
        name: "Send".to_string(),
        exec: "sender %f".to_string(),
        icon: None,
    });
    let list = parse_mimeapps_list(
        "\
[Added Associations]\n\
text/plain=sender.desktop;\n",
    );
    let cache = MimeApplicationCache::from_applications_service_menus_and_mimeapps(
        vec![app, added_app],
        vec![service_menu(
            "archive.desktop",
            "Archive Tools",
            &["all/allfiles"],
        )],
        &[list],
    );

    let actions = cache
        .service_actions_for_target(Some("text/plain"), false)
        .into_iter()
        .map(|action| (action.label, action.source_name))
        .collect::<Vec<_>>();

    assert_eq!(
        actions,
        vec![("Compress".to_string(), "Archive Tools".to_string())]
    );
    assert!(
        cache
            .service_actions_for_target(Some("inode/directory"), true)
            .is_empty()
    );
}

#[test]
fn service_actions_do_not_promote_application_desktop_actions() {
    let mut app = desktop_app("dev.zed.Zed.desktop", "Zed", &["inode/directory"]);
    app.actions.push(DesktopAction {
        id: "new-workspace".to_string(),
        name: "Open New Workspace".to_string(),
        exec: "zeditor --new %F".to_string(),
        icon: Some("zed".to_string()),
    });
    app.actions.push(DesktopAction {
        id: "new-window".to_string(),
        name: "New Window".to_string(),
        exec: "zeditor --new-window %F".to_string(),
        icon: Some("zed".to_string()),
    });
    let cache = MimeApplicationCache::from_applications_service_menus_and_mimeapps(
        vec![app],
        Vec::new(),
        &[],
    );

    assert!(
        cache
            .service_actions_for_target(Some("inode/directory"), true)
            .is_empty()
    );
    assert_eq!(cache.applications_for_mime("inode/directory").len(), 1);
}

#[test]
fn service_action_launch_plan_uses_action_exec() {
    let cache = MimeApplicationCache::from_applications_service_menus_and_mimeapps(
        Vec::new(),
        vec![service_menu(
            "archive.desktop",
            "Archive Tools",
            &["all/allfiles"],
        )],
        &[],
    );
    let action = cache
        .service_actions_for_target(Some("text/plain"), false)
        .remove(0);
    let plan = cache
        .service_action_launch_plan(&action.id, &[PathBuf::from("/tmp/a.txt")])
        .unwrap();

    assert_eq!(plan.app_name, "Archive Tools: Compress");
    assert_eq!(plan.commands[0].program, "ark");
    assert_eq!(plan.commands[0].args, vec!["--add", "/tmp/a.txt"]);
}

#[test]
fn service_actions_for_targets_intersect_targets_and_require_multi_exec() {
    let menu = DesktopServiceMenu {
        id: "archive.desktop".to_string(),
        desktop_file: PathBuf::from("/servicemenus/archive.desktop"),
        name: "Archive Tools".to_string(),
        icon: None,
        mime_types: vec!["all/allfiles".to_string()],
        service_types: vec!["KonqPopupMenu/Plugin".to_string()],
        protocols: Vec::new(),
        submenu: None,
        priority: ServiceMenuPriority::Normal,
        required_url_count: None,
        min_url_count: None,
        max_url_count: None,
        show_if_executable: None,
        actions: vec![
            DesktopAction {
                id: "compress".to_string(),
                name: "Compress".to_string(),
                exec: "ark --add %F".to_string(),
                icon: None,
            },
            DesktopAction {
                id: "inspect".to_string(),
                name: "Inspect One".to_string(),
                exec: "inspector %f".to_string(),
                icon: None,
            },
        ],
    };
    let cache = MimeApplicationCache::from_applications_service_menus_and_mimeapps(
        Vec::new(),
        vec![menu],
        &[],
    );
    let targets = vec![
        ServiceMenuTarget::new(Some("text/plain"), false),
        ServiceMenuTarget::new(Some("text/plain"), false),
    ];

    let actions = cache
        .service_actions_for_targets(&targets)
        .into_iter()
        .map(|action| action.label)
        .collect::<Vec<_>>();

    assert_eq!(actions, vec!["Compress".to_string()]);
}

#[test]
fn service_actions_deduplicate_labels_and_filter_builtin_window_actions() {
    let mut normal = service_menu("normal.desktop", "Normal Tools", &["inode/directory"]);
    normal.actions = vec![
        DesktopAction {
            id: "duplicate".to_string(),
            name: "Duplicate Action".to_string(),
            exec: "normal %f".to_string(),
            icon: None,
        },
        DesktopAction {
            id: "window".to_string(),
            name: "Open New Window".to_string(),
            exec: "not-fika %f".to_string(),
            icon: None,
        },
        DesktopAction {
            id: "open-window".to_string(),
            name: "Open New Window".to_string(),
            exec: "not-fika-open-window %f".to_string(),
            icon: None,
        },
    ];
    let mut top_level = service_menu("top.desktop", "Top Tools", &["inode/directory"]);
    top_level.priority = ServiceMenuPriority::TopLevel;
    top_level.actions = vec![DesktopAction {
        id: "duplicate".to_string(),
        name: "Duplicate   Action".to_string(),
        exec: "top %f".to_string(),
        icon: Some("top-icon".to_string()),
    }];

    let cache = MimeApplicationCache::from_applications_service_menus_and_mimeapps(
        Vec::new(),
        vec![normal, top_level],
        &[],
    );
    let actions = cache.service_actions_for_target(Some("inode/directory"), true);

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].label, "Duplicate   Action");
    assert_eq!(actions[0].source_name, "Top Tools");
    assert_eq!(actions[0].icon.as_deref(), Some("top-icon"));
}

#[test]
fn service_menu_conditions_filter_protocol_url_count_and_executable() {
    let (dir, executable) = launcher_test_executable("available-tool");
    let matching = DesktopServiceMenu {
        id: "matching.desktop".to_string(),
        desktop_file: PathBuf::from("/servicemenus/matching.desktop"),
        name: "Matching".to_string(),
        icon: None,
        mime_types: vec!["all/allfiles".to_string()],
        service_types: vec!["KonqPopupMenu/Plugin".to_string()],
        protocols: vec!["file".to_string()],
        submenu: Some("Tools".to_string()),
        priority: ServiceMenuPriority::TopLevel,
        required_url_count: None,
        min_url_count: Some(2),
        max_url_count: Some(3),
        show_if_executable: Some(executable.display().to_string()),
        actions: vec![DesktopAction {
            id: "run".to_string(),
            name: "Run Matching".to_string(),
            exec: "available-tool %F".to_string(),
            icon: None,
        }],
    };
    let remote_only = DesktopServiceMenu {
        id: "remote.desktop".to_string(),
        desktop_file: PathBuf::from("/servicemenus/remote.desktop"),
        name: "Remote".to_string(),
        icon: None,
        mime_types: vec!["all/allfiles".to_string()],
        service_types: vec!["KonqPopupMenu/Plugin".to_string()],
        protocols: vec!["smb".to_string()],
        submenu: None,
        priority: ServiceMenuPriority::Normal,
        required_url_count: None,
        min_url_count: None,
        max_url_count: None,
        show_if_executable: None,
        actions: vec![DesktopAction {
            id: "remote".to_string(),
            name: "Remote Only".to_string(),
            exec: "remote %F".to_string(),
            icon: None,
        }],
    };
    let missing_executable = DesktopServiceMenu {
        id: "missing.desktop".to_string(),
        desktop_file: PathBuf::from("/servicemenus/missing.desktop"),
        name: "Missing".to_string(),
        icon: None,
        mime_types: vec!["all/allfiles".to_string()],
        service_types: vec!["KonqPopupMenu/Plugin".to_string()],
        protocols: Vec::new(),
        submenu: None,
        priority: ServiceMenuPriority::Normal,
        required_url_count: None,
        min_url_count: None,
        max_url_count: None,
        show_if_executable: Some("/definitely/missing/fika-tool".to_string()),
        actions: vec![DesktopAction {
            id: "missing".to_string(),
            name: "Missing Tool".to_string(),
            exec: "missing %F".to_string(),
            icon: None,
        }],
    };
    let cache = MimeApplicationCache::from_applications_service_menus_and_mimeapps(
        Vec::new(),
        vec![remote_only, missing_executable, matching],
        &[],
    );
    let targets = vec![
        ServiceMenuTarget::new(Some("text/plain"), false),
        ServiceMenuTarget::new(Some("image/png"), false),
    ];

    let actions = cache.service_actions_for_targets(&targets);

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].label, "Run Matching");
    assert_eq!(actions[0].submenu.as_deref(), Some("Tools"));
    assert_eq!(actions[0].priority, ServiceMenuPriority::TopLevel);
    assert!(
        cache
            .service_actions_for_target(Some("text/plain"), false)
            .is_empty()
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn service_action_launch_plan_rejects_multi_paths_for_single_file_exec() {
    let mut menu = service_menu("sender.desktop", "Sender", &["all/allfiles"]);
    menu.actions = vec![DesktopAction {
        id: "send-one".to_string(),
        name: "Send One".to_string(),
        exec: "sender %f".to_string(),
        icon: None,
    }];
    let cache = MimeApplicationCache::from_applications_service_menus_and_mimeapps(
        Vec::new(),
        vec![menu],
        &[],
    );
    let action = cache
        .service_actions_for_target(Some("text/plain"), false)
        .remove(0);

    assert!(
        cache
            .service_action_launch_plan(
                &action.id,
                &[PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")]
            )
            .is_none()
    );
}

#[test]
fn mimeapps_parser_reads_default_added_and_removed_groups() {
    let list = parse_mimeapps_list(
        "\
[Default Applications]\n\
text/plain=default.desktop;\n\
[Added Associations]\n\
text/plain=added.desktop;other.desktop;\n\
[Removed Associations]\n\
text/plain=other.desktop;\n",
    );

    assert_eq!(
        list.default_apps["text/plain"],
        vec!["default.desktop".to_string()]
    );
    assert_eq!(list.added_associations["text/plain"].len(), 2);
    assert_eq!(
        list.removed_associations["text/plain"],
        vec!["other.desktop".to_string()]
    );
}

#[test]
fn set_default_mime_application_updates_mimeapps_contents() {
    let updated = set_default_mime_application_in_contents(
        "\
[Default Applications]\n\
text/plain=old.desktop;\n\
image/png=image.desktop;\n\
\n\
[Added Associations]\n\
text/plain=old.desktop;viewer.desktop;\n\
\n\
[Removed Associations]\n\
text/plain=viewer.desktop;blocked.desktop;\n",
        "text/plain",
        "viewer.desktop",
    )
    .unwrap();

    assert!(updated.contains("[Default Applications]\ntext/plain=viewer.desktop;\n"));
    assert!(updated.contains("image/png=image.desktop;"));
    assert!(updated.contains("[Added Associations]\ntext/plain=viewer.desktop;old.desktop;"));
    assert!(updated.contains("[Removed Associations]\ntext/plain=blocked.desktop;"));
    let parsed = parse_mimeapps_list(&updated);
    assert_eq!(
        parsed.default_apps["text/plain"],
        vec!["viewer.desktop".to_string()]
    );
    assert_eq!(
        parsed.added_associations["text/plain"],
        vec!["viewer.desktop".to_string(), "old.desktop".to_string()]
    );
    assert_eq!(
        parsed.removed_associations["text/plain"],
        vec!["blocked.desktop".to_string()]
    );
}

#[test]
fn set_default_mime_application_creates_missing_sections() {
    let updated =
        set_default_mime_application_in_contents("", "text/plain", "viewer.desktop").unwrap();

    assert_eq!(
        updated,
        "[Default Applications]\ntext/plain=viewer.desktop;\n\n[Added Associations]\ntext/plain=viewer.desktop;\n"
    );
}

#[test]
fn set_default_mime_application_rejects_invalid_values() {
    assert!(set_default_mime_application_in_contents("", "not-a-mime", "viewer.desktop").is_err());
    assert!(
        set_default_mime_application_in_contents("", "text/plain", "viewer;bad.desktop").is_err()
    );
    assert!(set_default_mime_application_in_contents("", "text/plain", "viewer").is_err());
}

#[test]
fn set_default_mime_application_at_writes_user_file() {
    let temp = env::temp_dir().join(format!(
        "fika-mimeapps-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = temp.join("config/mimeapps.list");

    set_default_mime_application_at(&path, "text/plain", "viewer.desktop").unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("text/plain=viewer.desktop;"));
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn mime_application_cache_orders_default_added_and_declared_apps() {
    let apps = vec![
        desktop_app("declared.desktop", "Declared", &["text/plain"]),
        desktop_app("default.desktop", "Default", &["text/plain"]),
        desktop_app("added.desktop", "Added", &[]),
        desktop_app("removed.desktop", "Removed", &["text/plain"]),
    ];
    let list = parse_mimeapps_list(
        "\
[Default Applications]\n\
text/plain=default.desktop;\n\
[Added Associations]\n\
text/plain=added.desktop;\n\
[Removed Associations]\n\
text/plain=removed.desktop;\n",
    );

    let cache = MimeApplicationCache::from_applications_and_mimeapps(apps, &[list]);
    let names = cache
        .applications_for_mime("text/plain")
        .into_iter()
        .map(|app| (app.name, app.is_default))
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            ("Default".to_string(), true),
            ("Added".to_string(), false),
            ("Declared".to_string(), false),
        ]
    );
}

#[test]
fn mime_application_cache_matches_wildcard_before_parent_fallback() {
    let apps = vec![
        desktop_app("generic-text.desktop", "Generic Text", &["text/plain"]),
        desktop_app("image-viewer.desktop", "Image Viewer", &["image/*"]),
    ];

    let cache = MimeApplicationCache::from_applications_and_mimeapps(apps, &[]);
    let names = cache
        .applications_for_mime("image/heic")
        .into_iter()
        .map(|app| app.name)
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["Image Viewer".to_string()]);
}

#[test]
fn mime_application_cache_respects_removed_associations_for_wildcard_apps() {
    let apps = vec![desktop_app(
        "image-viewer.desktop",
        "Image Viewer",
        &["image/*"],
    )];
    let list = parse_mimeapps_list(
        "\
[Removed Associations]\n\
image/heic=image-viewer.desktop;\n",
    );

    let cache = MimeApplicationCache::from_applications_and_mimeapps(apps, &[list]);
    let apps = cache.applications_for_mime("image/heic");

    assert!(apps.is_empty());
}

#[test]
fn mime_application_cache_falls_back_to_parent_text_plain_apps() {
    let apps = vec![desktop_app(
        "text-editor.desktop",
        "Text Editor",
        &["text/plain"],
    )];
    let list = parse_mimeapps_list(
        "\
[Default Applications]\n\
text/plain=text-editor.desktop;\n",
    );

    let cache = MimeApplicationCache::from_applications_and_mimeapps(apps, &[list]);
    let apps = cache.applications_for_mime("text/x-rust");

    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].name, "Text Editor");
    assert!(apps[0].is_default);
}

#[test]
fn mime_application_cache_keeps_exact_mime_over_parent_apps() {
    let apps = vec![
        desktop_app("rust-ide.desktop", "Rust IDE", &["text/x-rust"]),
        desktop_app("text-editor.desktop", "Text Editor", &["text/plain"]),
    ];

    let cache = MimeApplicationCache::from_applications_and_mimeapps(apps, &[]);
    let names = cache
        .applications_for_mime("text/x-rust")
        .into_iter()
        .map(|app| app.name)
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["Rust IDE".to_string()]);
}

#[test]
fn mime_application_cache_lists_all_applications_for_other_application_picker() {
    let apps = vec![
        desktop_app("writer.desktop", "Writer", &[]),
        desktop_app("viewer.desktop", "Viewer", &["text/plain"]),
    ];

    let cache = MimeApplicationCache::from_applications_and_mimeapps(apps, &[]);
    let names = cache
        .all_applications()
        .into_iter()
        .map(|app| app.name)
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["Viewer".to_string(), "Writer".to_string()]);
}

#[test]
fn parse_mimeinfo_cache_reads_mime_cache_section() {
    let cache = parse_mimeinfo_cache(
        "\
[Other]\n\
text/plain=ignored.desktop;\n\
[MIME Cache]\n\
text/plain=viewer.desktop;writer.desktop;\n\
inode/directory=file-manager.desktop;\n",
    );

    assert_eq!(
        cache.associations.get("text/plain"),
        Some(&vec![
            "viewer.desktop".to_string(),
            "writer.desktop".to_string()
        ])
    );
    assert_eq!(
        cache.associations.get("inode/directory"),
        Some(&vec!["file-manager.desktop".to_string()])
    );
    assert!(!cache.associations.contains_key("Other"));
}

#[test]
fn mime_application_cache_uses_mimeinfo_cache_associations() {
    let apps = vec![
        desktop_app("writer.desktop", "Writer", &[]),
        desktop_app("viewer.desktop", "Viewer", &[]),
    ];
    let cache_file = parse_mimeinfo_cache(
        "\
[MIME Cache]\n\
text/plain=viewer.desktop;writer.desktop;\n",
    );

    let cache =
        MimeApplicationCache::from_applications_mimeinfo_and_mimeapps(apps, &[cache_file], &[]);
    let names = cache
        .applications_for_mime("text/plain")
        .into_iter()
        .map(|app| app.name)
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["Viewer".to_string(), "Writer".to_string()]);
}

#[test]
fn mimeapps_list_overrides_mimeinfo_cache_associations() {
    let apps = vec![
        desktop_app("writer.desktop", "Writer", &[]),
        desktop_app("viewer.desktop", "Viewer", &[]),
    ];
    let cache_file = parse_mimeinfo_cache(
        "\
[MIME Cache]\n\
text/plain=viewer.desktop;writer.desktop;\n",
    );
    let list = parse_mimeapps_list(
        "\
[Default Applications]\n\
text/plain=writer.desktop;\n\
[Removed Associations]\n\
text/plain=viewer.desktop;\n",
    );

    let cache =
        MimeApplicationCache::from_applications_mimeinfo_and_mimeapps(apps, &[cache_file], &[list]);
    let apps = cache.applications_for_mime("text/plain");

    assert_eq!(
        apps.iter()
            .map(|app| (app.name.as_str(), app.is_default))
            .collect::<Vec<_>>(),
        vec![("Writer", true)]
    );
}

#[test]
fn exec_field_codes_expand_to_launch_command() {
    let command = exec_to_launch_commands(
        "viewer --name %c --desktop %k %f",
        "Viewer",
        Path::new("/apps/viewer.desktop"),
        &[PathBuf::from("/tmp/file.txt")],
    )
    .unwrap()
    .remove(0);

    assert_eq!(command.program, "viewer");
    assert_eq!(
        command.args,
        vec![
            "--name",
            "Viewer",
            "--desktop",
            "/apps/viewer.desktop",
            "/tmp/file.txt"
        ]
    );
}

#[test]
fn exec_embedded_multi_file_code_expands_single_path() {
    let command = exec_to_launch_commands(
        "ghostty +new-window --working-directory=%F",
        "Ghostty",
        Path::new("/apps/com.mitchellh.ghostty.desktop"),
        &[PathBuf::from("/tmp/fika service target")],
    )
    .unwrap()
    .remove(0);

    assert_eq!(command.program, "ghostty");
    assert_eq!(
        command.args,
        vec![
            "+new-window",
            "--working-directory=/tmp/fika service target"
        ]
    );
}

#[test]
fn embedded_multi_file_code_does_not_advertise_multi_path_support() {
    assert!(exec_supports_multiple_paths("ark --add %F"));
    assert!(!exec_supports_multiple_paths(
        "ghostty +new-window --working-directory=%F"
    ));
}

#[test]
fn systemd_launch_unit_name_sanitizes_desktop_id() {
    assert_eq!(
        systemd_launch_unit_name("org.example.Viewer.desktop", 2, 0x2a),
        "fika-open-with-org.example.Viewer.desktop-2-2a.service"
    );
    assert_eq!(
        systemd_launch_unit_name("///", 0, 0x2a),
        "fika-open-with-application-0-2a.service"
    );
}

#[test]
fn systemd_units_for_launch_plan_resolves_executable_path() {
    let (dir, executable) = launcher_test_executable("viewer");
    let plan = DesktopLaunchPlan {
        desktop_id: "viewer.desktop".to_string(),
        desktop_file: PathBuf::from("/apps/viewer.desktop"),
        app_name: "Viewer".to_string(),
        commands: vec![DesktopLaunchCommand {
            program: executable.display().to_string(),
            args: vec!["/tmp/file.txt".to_string()],
        }],
    };

    let units = systemd_units_for_launch_plan_with_nonce(&plan, 0x2a).unwrap();

    assert_eq!(units.len(), 1);
    assert_eq!(
        units[0].unit_name,
        "fika-open-with-viewer.desktop-0-2a.service"
    );
    assert_eq!(units[0].command.program, executable.display().to_string());
    assert_eq!(units[0].command.args, vec!["/tmp/file.txt"]);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn current_executable_launch_plan_targets_running_binary() {
    let plan = current_executable_launch_plan(
        "fika-new-window",
        "Fika",
        vec!["/tmp/fika-window".to_string()],
    )
    .unwrap();

    assert_eq!(plan.desktop_id, "fika-new-window");
    assert_eq!(plan.app_name, "Fika");
    assert_eq!(plan.commands.len(), 1);
    assert!(Path::new(&plan.commands[0].program).is_absolute());
    assert_eq!(plan.commands[0].args, vec!["/tmp/fika-window"]);
}

#[test]
fn terminal_launch_plan_selects_first_supported_terminal_command() {
    let (dir, executable) = launcher_test_executable("terminal");
    let plan = terminal_launch_plan_for_commands(vec![
        DesktopLaunchCommand {
            program: "/definitely/missing/fika-terminal".to_string(),
            args: Vec::new(),
        },
        DesktopLaunchCommand {
            program: executable.display().to_string(),
            args: vec!["--workdir".to_string(), "/tmp/fika-terminal".to_string()],
        },
    ])
    .unwrap();

    assert_eq!(plan.desktop_id, "fika-terminal");
    assert_eq!(plan.app_name, "Terminal");
    assert_eq!(plan.commands.len(), 1);
    assert_eq!(plan.commands[0].program, executable.display().to_string());
    assert_eq!(
        plan.commands[0].args,
        vec!["--workdir".to_string(), "/tmp/fika-terminal".to_string()]
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn terminal_launch_plan_reports_missing_terminal() {
    assert_eq!(
        terminal_launch_plan_for_commands(vec![DesktopLaunchCommand {
            program: "/definitely/missing/fika-terminal".to_string(),
            args: Vec::new(),
        }]),
        Err(LauncherError::TerminalNotFound)
    );
}

#[test]
fn systemd_properties_include_execstart_tuple() {
    let (dir, executable) = launcher_test_executable("viewer");
    let unit = SystemdLaunchUnit {
        unit_name: "fika-open-with-viewer-0.service".to_string(),
        description: "Fika Open With Viewer".to_string(),
        command: DesktopLaunchCommand {
            program: executable.display().to_string(),
            args: vec!["--flag".to_string(), "/tmp/file.txt".to_string()],
        },
    };

    let names = systemd_properties_for_launch_unit(&unit)
        .unwrap()
        .into_iter()
        .map(|(name, _)| name)
        .collect::<Vec<_>>();

    assert!(names.contains(&"Description".to_string()));
    assert!(names.contains(&"Type".to_string()));
    assert!(names.contains(&"ExecStart".to_string()));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn systemd_units_report_empty_plan_and_missing_program() {
    let empty = DesktopLaunchPlan {
        desktop_id: "empty.desktop".to_string(),
        desktop_file: PathBuf::from("/apps/empty.desktop"),
        app_name: "Empty".to_string(),
        commands: Vec::new(),
    };
    assert_eq!(
        systemd_units_for_launch_plan_with_nonce(&empty, 0x2a),
        Err(LauncherError::EmptyLaunchPlan {
            app_name: "Empty".to_string()
        })
    );

    let missing = DesktopLaunchPlan {
        desktop_id: "missing.desktop".to_string(),
        desktop_file: PathBuf::from("/apps/missing.desktop"),
        app_name: "Missing".to_string(),
        commands: vec![DesktopLaunchCommand {
            program: "/definitely/missing/fika-viewer".to_string(),
            args: Vec::new(),
        }],
    };
    assert_eq!(
        systemd_units_for_launch_plan_with_nonce(&missing, 0x2a),
        Err(LauncherError::ProgramNotFound {
            program: "/definitely/missing/fika-viewer".to_string()
        })
    );
}
