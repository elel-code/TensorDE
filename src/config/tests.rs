use super::*;

fn parse(document: &str) -> Result<Config, ConfigError> {
    Config::from_toml(Path::new("test.toml"), document)
}

#[test]
fn parses_toml_layout_and_ipc_socket() {
    let config = parse(
        r#"
        layout = { kind = "spatial-2d" }
        ipc_socket = "/run/user/1000/tensor.sock"
        gpu = "integrated"
        render_device = "/dev/dri/renderD128"
        systemd = "disabled"
        xwayland = true
        spawn_at_startup = [["waybar"], ["foot", "--server"]]
        "#,
    )
    .unwrap();

    assert_eq!(config.initial_layout, LayoutKind::Spatial2D);
    assert_eq!(config.layout_options, LayoutOptions::default());
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
fn parses_nested_layout_policy() {
    let config = parse(
        r#"
        [layout]
        kind = "scrolling-1d"
        gaps = 12
        default_column_width = { proportion = 0.625 }
        master_width = { fixed = 900 }
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
fn parses_per_output_rules() {
    let config = parse(
        r#"
        [[outputs]]
        name = "eDP-1"
        scale = 1.31
        mode = "2560x1600@239.760"
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
        [[outputs]]
        name = "HDMI-A-1"
        position = { x = 0, y = 0 }
        max_refresh_millihertz = 120000
        enabled = true
        [[outputs]]
        name = "eDP-1"
        enabled = false
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
        [appearance.focus_ring]
        enabled = true
        width = 6
        color = "#2e70ffff"
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
        }
    );
}

#[test]
fn accepts_integer_output_scale_literals() {
    let config = parse(
        r#"
        [[outputs]]
        name = "eDP-1"
        scale = 2
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
        [[outputs]]
        name = "eDP-1"
        mode = "1920x1200"
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
        parse(
            r#"
            [[outputs]]
            name = "DP-1"
            scale = 0
            "#
        ),
        Err(ConfigError::InvalidOutputScale { .. })
    ));
    assert!(matches!(
        parse(
            r#"
            [[outputs]]
            name = "DP-1"
            scale = 1.25
            [[outputs]]
            name = "DP-1"
            scale = 1.5
            "#
        ),
        Err(ConfigError::DuplicateOutput { .. })
    ));
}

#[test]
fn rejects_malformed_output_modes() {
    for mode in ["2560", "0x1600", "2560x1600@0", "2560x1600@239.7601"] {
        let config = format!(
            r#"
            [[outputs]]
            name = "DP-1"
            mode = "{mode}"
            "#
        );
        assert!(matches!(
            parse(&config),
            Err(ConfigError::InvalidOutputMode { .. })
        ));
    }
}

#[test]
fn layout_length_requires_one_valid_mode() {
    let both = parse(
        r#"
        [layout]
        kind = "scrolling-1d"
        default_column_width = { proportion = 0.5, fixed = 800 }
        "#,
    )
    .unwrap_err();
    assert!(
        matches!(
            &both,
            ConfigError::InvalidLayoutOption {
                option: "default_column_width",
                ..
            }
        ),
        "unexpected error: {both:?}"
    );

    let zero = parse(
        r#"
        [layout]
        kind = "scrolling-1d"
        master_width = { fixed = 0 }
        "#,
    )
    .unwrap_err();
    assert!(
        matches!(
            &zero,
            ConfigError::InvalidLayoutOption {
                option: "master_width",
                ..
            }
        ),
        "unexpected error: {zero:?}"
    );
}

#[test]
fn rejects_unknown_systemd_mode() {
    assert!(matches!(
        parse(r#"systemd = "launchd""#),
        Err(ConfigError::UnknownSystemd(_))
    ));
}

#[test]
fn rejects_unknown_toml_keys() {
    assert!(matches!(
        parse(r#"compatibility = true"#),
        Err(ConfigError::Parse { .. })
    ));
}

#[test]
fn rejects_empty_startup_commands() {
    assert!(matches!(
        parse(r#"spawn_at_startup = [[]]"#),
        Err(ConfigError::EmptyStartupCommand { index: 0 })
    ));
}

#[test]
fn rejects_legacy_kdl_paths() {
    assert!(matches!(
        Config::from_toml(Path::new("test.kdl"), "layout \"scrolling-1d\""),
        Err(ConfigError::LegacyKdl { .. })
    ));
}
