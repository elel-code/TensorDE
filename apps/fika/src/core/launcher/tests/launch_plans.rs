use super::*;

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
