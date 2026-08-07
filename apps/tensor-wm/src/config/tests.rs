use super::*;
use crate::media::MediaAction;
use tensor_util::OutputScale;
use xkbcommon::xkb::keysyms;

fn parse(document: &str) -> Result<Config, ConfigError> {
    Config::from_kdl(Path::new("test.kdl"), document)
}

#[test]
fn parses_kdl_layout_and_ipc_socket() {
    let config = parse(
        r#"
        layout "spatial-2d"
        ipc-socket "/run/user/1000/tensor.sock"
        gpu "integrated"
        render-device "/dev/dri/renderD128"
        systemd "disabled"
        xwayland #true
        spawn-at-startup "waybar"
        spawn-at-startup "foot" "--server"
        "#,
    )
    .unwrap();

    assert_eq!(config.initial_layout, LayoutKind::Spatial2D);
    assert_eq!(config.layout_options, LayoutOptions::default());
    assert_eq!(config.overview_options, OverviewOptions::default());
    assert_eq!(
        config.ipc_socket,
        PathBuf::from("/run/user/1000/tensor.sock")
    );
    assert_eq!(config.gpu_preference, GpuPreference::Integrated);
    assert_eq!(
        config.render_device,
        Some(PathBuf::from("/dev/dri/renderD128"))
    );
    assert_eq!(config.systemd, SystemdMode::Disabled);
    assert!(config.xwayland.enabled());
    assert_eq!(
        config.startup_commands,
        vec![
            StartupCommand {
                argv: vec!["waybar".to_owned()]
            },
            StartupCommand {
                argv: vec!["foot".to_owned(), "--server".to_owned()]
            }
        ]
    );
}

#[test]
fn checked_in_kdl_example_matches_the_typed_schema() {
    parse(include_str!("../../examples/config.kdl")).unwrap();
}

#[test]
fn parses_bounded_overview_geometry_policy() {
    let config = parse(r#"overview outer-gap=32 workspace-gap=20"#).unwrap();

    assert_eq!(config.overview_options, OverviewOptions::new(32, 20));
    assert!(matches!(
        parse(r#"overview outer-gap=100001"#),
        Err(ConfigError::Parse(ref diagnostic))
            if diagnostic.error_context().code == tensor_kdl::ErrorCode::ExceededLimit
    ));
}

#[test]
fn media_keys_are_typed_bounded_and_configurable() {
    let config = parse(
        r#"media-keys enabled=#true previous="XF86AudioRewind" play-pause="XF86AudioPause" next="XF86AudioForward""#,
    )
    .unwrap();

    assert_eq!(
        config.media_keys.action_for(keysyms::KEY_XF86AudioRewind),
        Some(MediaAction::Previous)
    );
    assert_eq!(
        config.media_keys.action_for(keysyms::KEY_XF86AudioPause),
        Some(MediaAction::PlayPause)
    );
    assert_eq!(
        config.media_keys.action_for(keysyms::KEY_XF86AudioForward),
        Some(MediaAction::Next)
    );
    assert_eq!(Config::default().restart_required_change(&config), None);

    let disabled = parse(r#"media-keys enabled=#false"#).unwrap();
    assert_eq!(
        disabled.media_keys.action_for(keysyms::KEY_XF86AudioPlay),
        None
    );
    assert!(matches!(
        parse(r#"media-keys previous="TensorNotAKeysym""#),
        Err(ConfigError::MediaKeys(
            MediaKeyConfigError::UnknownKeysym { .. }
        ))
    ));
    let oversized = format!(
        "media-keys previous=\"{}\"",
        "x".repeat(super::media::MAX_MEDIA_KEYSYM_NAME_BYTES + 1)
    );
    assert!(matches!(
        parse(&oversized),
        Err(ConfigError::MediaKeys(
            MediaKeyConfigError::InvalidKeysymName { .. }
        ))
    ));
    assert!(matches!(
        parse(r#"media-keys previous="XF86AudioPlay" play-pause="XF86AudioPlay""#),
        Err(ConfigError::MediaKeys(MediaKeyConfigError::DuplicateKeysym))
    ));
}

#[test]
fn parses_nested_layout_policy() {
    let config = parse(
        r#"
        layout "scrolling-1d" gaps=12 {
            default-column-width proportion=0.625
            master-width fixed=900
        }
        "#,
    )
    .unwrap();

    assert_eq!(config.layout_options.gap, 12);
    assert_eq!(
        config.layout_options.scrolling_default_width,
        LayoutLength::proportion(6250, 10_000)
    );
    assert_eq!(config.layout_options.master_width, LayoutLength::fixed(900));
}

#[test]
fn parses_regular_and_hidden_workspace_policy() {
    let config = parse(
        r#"
        workspaces default-count=4 {
            hidden "minimized" minimize-target=#true
            hidden "communication" show-in-overview=#false
        }
        "#,
    )
    .unwrap();

    assert_eq!(config.workspaces.regular_count, 4);
    assert_eq!(config.workspaces.hidden.len(), 2);
    assert_eq!(config.workspaces.hidden[0].name, "minimized");
    assert!(config.workspaces.hidden[0].minimize_target);
    assert!(!config.workspaces.hidden[1].show_in_overview);
}

#[test]
fn workspace_policy_requires_one_bounded_minimize_target() {
    assert!(matches!(
        parse("workspaces default-count=0"),
        Err(ConfigError::Workspaces(
            WorkspaceConfigError::RegularCount { .. }
        ))
    ));
    assert!(matches!(
        parse(
            r#"workspaces {
                hidden "scratch"
            }"#
        ),
        Err(ConfigError::Workspaces(
            WorkspaceConfigError::MissingMinimizeTarget
        ))
    ));
    assert!(matches!(
        parse(
            r#"workspaces {
                hidden "one" minimize-target=#true
                hidden "two" minimize-target=#true
            }"#
        ),
        Err(ConfigError::Workspaces(
            WorkspaceConfigError::MultipleMinimizeTargets
        ))
    ));
}

#[test]
fn parses_per_output_rules() {
    let config = parse(
        r#"
        output "eDP-1" scale=1.31 mode="2560x1600@239.760"
        "#,
    )
    .unwrap();

    assert_eq!(
        config.output_rules["eDP-1"],
        OutputRule {
            scale: Some(OutputScale::from_units(157).unwrap()),
            mode: Some(OutputMode::new(2560, 1600, Some(239_760))),
            position: None,
            enabled: true,
            max_refresh_millihertz: None,
        }
    );
}

#[test]
fn parses_output_placement_and_refresh_cap() {
    let config = parse(
        r#"
        output "HDMI-A-1" max-refresh-millihertz=120000 enabled=#true {
            position x=0 y=0
        }
        output "eDP-1" enabled=#false
        "#,
    )
    .unwrap();
    assert_eq!(config.output_rules["HDMI-A-1"].position, Some((0, 0)));
    assert_eq!(
        config.output_rules["HDMI-A-1"].max_refresh_millihertz,
        Some(120_000)
    );
    assert!(!config.output_rules["eDP-1"].enabled);
}

#[test]
fn parses_scene_appearance_policy() {
    let config = parse(
        r##"
        appearance {
            focus-ring enabled=#true width=6 color="#2e70ffff"
            window-shadow enabled=#true offset-x=-3 offset-y=8 blur-radius=20 spread=2 color="#10203080"
            window-corners radius=12
        }
        "##,
    )
    .unwrap();

    assert_eq!(
        config.appearance,
        SceneAppearance {
            focus_ring: crate::scene::FocusRingStyle {
                enabled: true,
                width: 6,
                color: crate::scene::LinearRgba16::new(0x2e2e, 0x7070, u16::MAX, u16::MAX,),
            },
            window_shadow: crate::scene::WindowShadowStyle {
                enabled: true,
                offset_x: -3,
                offset_y: 8,
                blur_radius: 20,
                spread: 2,
                color: crate::scene::LinearRgba16::new(0x1010, 0x2020, 0x3030, 0x8080),
            },
            window_corners: crate::scene::WindowCornerStyle { radius: 12 },
        }
    );
}

#[test]
fn accepts_integer_output_scale_literals() {
    let config = parse(
        r#"
        output "eDP-1" scale=2
        "#,
    )
    .unwrap();
    assert_eq!(
        config.output_rules["eDP-1"].scale,
        Some(OutputScale::from_units(240).unwrap())
    );
}

#[test]
fn accepts_resolution_only_output_mode() {
    let config = parse(
        r#"
        output "eDP-1" mode="1920x1200"
        "#,
    )
    .unwrap();

    assert_eq!(
        config.output_rules["eDP-1"].mode,
        Some(OutputMode::new(1920, 1200, None))
    );
}

#[test]
fn rejects_invalid_and_duplicate_output_scales() {
    assert!(matches!(
        parse(r#"output "DP-1" scale=0"#),
        Err(ConfigError::Parse(_))
    ));
    assert!(matches!(
        parse(
            r#"
            output "DP-1" scale=1.25
            output "DP-1" scale=1.5
            "#
        ),
        Err(ConfigError::DuplicateOutput { .. })
    ));
}

#[test]
fn parses_environment_cursor_and_debug_nodes() {
    let config = parse(
        r#"
        environment {
            clear "GTK_IM_MODULE"
            set "EDITOR" "hx"
            set "BROWSER" "firefox"
        }
        cursor {
            xcursor-theme "Adwaita"
            xcursor-size 32
            hide-when-typing
            hide-after-inactive-ms 3000
        }
        debug frame-stats=#true force-full-redraw=#true
        "#,
    )
    .unwrap();

    assert_eq!(config.environment.clear, vec!["GTK_IM_MODULE".to_owned()]);
    assert_eq!(
        config.environment.set.get("EDITOR").map(String::as_str),
        Some("hx")
    );
    assert_eq!(
        config.environment.set.get("BROWSER").map(String::as_str),
        Some("firefox")
    );
    assert_eq!(
        config.cursor,
        CursorConfig {
            theme: "Adwaita".to_owned(),
            size: 32,
            hide_when_typing: true,
            hide_after_inactive_ms: Some(3000),
        }
    );
    assert_eq!(
        config.debug,
        DebugConfig {
            frame_stats: true,
            force_full_redraw: true,
        }
    );
}

#[test]
fn cursor_uses_niri_child_nodes_and_rejects_the_old_property_shape() {
    assert!(matches!(
        parse(r#"cursor theme="Adwaita" size=32 hide-when-typing=#true"#),
        Err(ConfigError::Parse(_))
    ));
    assert!(matches!(
        parse(
            r#"
            cursor {
                hide-when-typing #true
            }
            "#
        ),
        Err(ConfigError::Parse(_))
    ));
    assert!(matches!(
        parse(
            r#"
            cursor {
                xcursor-size 256
            }
            "#
        ),
        Err(ConfigError::Parse(_))
    ));
}

#[test]
fn rejects_duplicate_environment_set_rules() {
    assert!(matches!(
        parse(
            r#"
            environment {
                set "EDITOR" "hx"
                set "EDITOR" "vim"
            }
            "#
        ),
        Err(ConfigError::DuplicateEnvironmentVariable { .. })
    ));
}

#[test]
fn rejects_malformed_output_modes() {
    for mode in ["2560", "0x1600", "2560x1600@0", "2560x1600@239.7601"] {
        let config = format!(r#"output "DP-1" mode="{mode}""#);
        assert!(matches!(parse(&config), Err(ConfigError::Parse(_))));
    }
}

#[test]
fn scalar_policy_errors_point_to_the_output_property() {
    for (document, property, code) in [
        (
            r#"output "DP-1" scale=0"#,
            "scale",
            tensor_kdl::ErrorCode::ExceededLimit,
        ),
        (
            r#"output "DP-1" mode="2560x1600@0""#,
            "mode",
            tensor_kdl::ErrorCode::InvalidNumber,
        ),
        (
            r#"output "DP-1" max-refresh-millihertz=0"#,
            "max-refresh-millihertz",
            tensor_kdl::ErrorCode::ExceededLimit,
        ),
    ] {
        let ConfigError::Parse(diagnostic) = parse(document).unwrap_err() else {
            panic!("expected source diagnostic for {property}")
        };
        assert_eq!(diagnostic.error_context().code, code);
        assert_eq!(
            diagnostic.error_context().consumed,
            document.find(property).unwrap()
        );
        assert_eq!(
            diagnostic.line_column(),
            (1, document.find(property).unwrap() + 1)
        );
    }
}

#[test]
fn layout_length_requires_one_valid_mode() {
    let both_document = r#"
        layout "scrolling-1d" {
            default-column-width proportion=0.5 fixed=800
        }
        "#;
    let ConfigError::Parse(both) = parse(both_document).unwrap_err() else {
        panic!("expected a source-aware cross-field error")
    };
    assert_eq!(
        both.error_context().code,
        tensor_kdl::ErrorCode::DuplicateProperty
    );
    assert_eq!(
        both.error_context().consumed,
        both_document.find("fixed").unwrap()
    );

    let zero = parse(
        r#"
        layout "scrolling-1d" {
            master-width fixed=0
        }
        "#,
    )
    .unwrap_err();
    assert!(
        matches!(&zero, ConfigError::Parse(_)),
        "unexpected error: {zero:?}"
    );

    let missing_document = r#"
        layout "scrolling-1d" {
            master-width
        }
        "#;
    let ConfigError::Parse(missing) = parse(missing_document).unwrap_err() else {
        panic!("expected a source-aware missing-width error")
    };
    assert_eq!(
        missing.error_context().code,
        tensor_kdl::ErrorCode::MissingProperty
    );
    assert_eq!(
        missing.error_context().consumed,
        missing_document.find("master-width").unwrap()
    );
}

#[test]
fn scalar_policy_errors_point_to_layout_properties() {
    for (document, property) in [
        (r#"layout "scrolling-1d" gaps=100001"#, "gaps"),
        (
            r#"layout "scrolling-1d" { master-width proportion=0 }"#,
            "proportion",
        ),
        (r#"layout "scrolling-1d" { master-width fixed=0 }"#, "fixed"),
    ] {
        let ConfigError::Parse(diagnostic) = parse(document).unwrap_err() else {
            panic!("expected source diagnostic for {property}")
        };
        assert_eq!(
            diagnostic.error_context().code,
            tensor_kdl::ErrorCode::ExceededLimit
        );
        assert_eq!(
            diagnostic.error_context().consumed,
            document.find(property).unwrap()
        );
    }
}

#[test]
fn rejects_unknown_systemd_mode() {
    assert!(matches!(
        parse(r#"systemd "launchd""#),
        Err(ConfigError::UnknownSystemd(_))
    ));
}

#[test]
fn rejects_unknown_kdl_nodes_and_malformed_syntax() {
    assert!(matches!(
        parse("compatibility #true"),
        Err(ConfigError::Parse(_))
    ));
    assert!(matches!(
        parse(r#"layout "scrolling-1d" compatibility=#true"#),
        Err(ConfigError::Parse(_))
    ));
    assert!(matches!(parse("layout {"), Err(ConfigError::Parse(_))));
}

#[test]
fn parse_errors_retain_named_source_and_structured_context() {
    let error = parse("compatibility #true").unwrap_err();
    let ConfigError::Parse(diagnostic) = error else {
        panic!("expected a parser diagnostic")
    };

    assert_eq!(diagnostic.path(), Path::new("test.kdl"));
    assert_eq!(diagnostic.source_text(), "compatibility #true");
    assert_eq!(diagnostic.line_column(), (1, 1));
    assert_eq!(
        diagnostic.error_context().code,
        tensor_kdl::ErrorCode::UnknownChild
    );
    assert!(diagnostic.compact().contains("test.kdl:1:1"));
    assert!(!format!("{:?}", diagnostic.report()).is_empty());
}

#[test]
fn rejects_empty_startup_commands() {
    assert!(matches!(
        parse("spawn-at-startup"),
        Err(ConfigError::EmptyStartupCommand { index: 0 })
    ));
}

#[test]
fn rejects_legacy_toml_paths() {
    assert!(matches!(
        Config::from_kdl(Path::new("test.toml"), "layout \"scrolling-1d\""),
        Err(ConfigError::LegacyToml { .. })
    ));
    assert!(matches!(
        Config::load_or_default(Path::new("missing-tensor-config.toml")),
        Err(ConfigError::LegacyToml { .. })
    ));
}

#[test]
fn reload_transaction_preserves_the_last_valid_configuration() {
    let initial = parse(r#"layout "scrolling-1d""#).unwrap();
    let mut transaction = ConfigTransaction::new("test.kdl", initial.clone());

    let rejected = transaction.apply_candidate(parse(
        r#"layout "scrolling-1d" { master-width proportion=0.5 fixed=900 }"#,
    ));
    assert!(!rejected.applied());
    assert_eq!(rejected.generation(), 0);
    assert_eq!(transaction.active(), &initial);
    assert_eq!(transaction.generation(), 0);
    assert_eq!(
        transaction
            .last_failure()
            .map(|value| value.error_code.as_str()),
        Some("duplicate property")
    );

    let replacement = parse(r#"layout "spatial-2d""#).unwrap();
    let applied = transaction.apply_candidate(Ok(replacement.clone()));
    assert!(applied.applied());
    assert_eq!(applied.generation(), 1);
    assert_eq!(transaction.active(), &replacement);
    assert!(transaction.last_failure().is_none());
}

#[test]
fn reload_rejects_a_missing_file_without_committing_defaults() {
    let initial = parse(r#"layout "spatial-2d""#).unwrap();
    let path = std::env::temp_dir().join(format!(
        "tensor-missing-reload-{}-{}.kdl",
        std::process::id(),
        line!()
    ));
    assert!(!path.exists());
    let mut transaction = ConfigTransaction::new(&path, initial.clone());

    let ConfigReloadResult::Rejected(failure) = transaction.reload() else {
        panic!("a missing reload file must be rejected")
    };

    assert_eq!(failure.generation, 0);
    assert_eq!(failure.diagnostic.category, ConfigDiagnosticCategory::Io);
    assert_eq!(failure.diagnostic.error_code, "read");
    assert!(matches!(
        failure.error,
        ConfigError::Read { source, .. }
            if source.kind() == std::io::ErrorKind::NotFound
    ));
    assert_eq!(transaction.generation(), 0);
    assert_eq!(transaction.active(), &initial);
    assert_eq!(transaction.last_failure(), Some(&failure.diagnostic));
}

#[test]
fn reload_diagnostic_metadata_is_bounded_and_source_free() {
    let secret = "secret-source-value";
    let source = format!("layout {secret}");
    let path = PathBuf::from(format!("/tmp/config with spaces/{}", "界".repeat(2_000)));
    let diagnostic = ConfigDiagnostic::new(
        &path,
        &source,
        tensor_kdl::ErrorCtx::new(tensor_kdl::ErrorCode::ExceededLimit, 0)
            .with_message("x".repeat(MAX_DIAGNOSTIC_SUMMARY_BYTES * 2)),
    );
    let metadata = diagnostic.metadata();

    assert_eq!(metadata.category, ConfigDiagnosticCategory::Policy);
    assert!(metadata.path.len() <= MAX_DIAGNOSTIC_PATH_BYTES);
    assert!(metadata.summary.len() <= MAX_DIAGNOSTIC_SUMMARY_BYTES);
    assert!(metadata.validation_command.len() <= MAX_VALIDATION_COMMAND_BYTES);
    assert!(metadata.validation_command.contains("--validate-config"));
    let encoded = serde_json::to_string(&metadata).unwrap();
    assert!(!encoded.contains(secret));
}
