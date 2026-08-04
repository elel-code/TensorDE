use super::*;

#[test]
fn falls_back_to_generic_snapshot_when_compositor_adapters_are_disabled() {
    let config = AdapterConfig {
        generic_wayland: true,
        hyprland: false,
        niri: false,
    };

    let snapshot = read_desktop_snapshot(&config);
    assert_eq!(snapshot.compositor, Some(CompositorKind::GenericWayland));
    assert!(snapshot.outputs.is_empty());
}

#[test]
fn allows_disabling_all_adapters() {
    let config = AdapterConfig {
        generic_wayland: false,
        hyprland: false,
        niri: false,
    };

    let snapshot = read_desktop_snapshot(&config);
    assert_eq!(snapshot.compositor, None);
    assert!(snapshot.outputs.is_empty());
}

#[test]
fn parses_output_state_override_values() {
    assert_eq!(
        parse_output_state_override("unfocused"),
        Some(OutputStateOverride::Unfocused)
    );
    assert_eq!(
        parse_output_state_override("fullscreen"),
        Some(OutputStateOverride::Fullscreen)
    );
    assert_eq!(
        parse_output_state_override("hidden"),
        Some(OutputStateOverride::Hidden)
    );
    assert_eq!(parse_output_state_override("auto"), None);
    assert_eq!(parse_output_state_override("invalid"), None);
}

#[test]
fn reads_output_state_override_from_file() {
    let path = std::env::temp_dir().join(format!(
        "tensor-wallpaper-output-state-override-{}",
        std::process::id()
    ));
    fs::write(&path, "fullscreen\n").unwrap();

    assert_eq!(
        read_output_state_override_file(&path),
        Some(OutputStateOverride::Fullscreen)
    );

    fs::write(&path, "active").unwrap();
    assert_eq!(
        read_output_state_override_file(&path),
        Some(OutputStateOverride::Active)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn parses_desktop_outputs_override_values() {
    let outputs = parse_desktop_outputs_override("eDP-1:1920x1080@1.5, HDMI-A-1").unwrap();

    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0].name, "eDP-1");
    assert_eq!(outputs[0].width, Some(1920));
    assert_eq!(outputs[0].height, Some(1080));
    assert_eq!(outputs[0].scale, 1.5);
    assert_eq!(outputs[1].name, "HDMI-A-1");
    assert_eq!(outputs[1].width, None);
    assert_eq!(outputs[1].height, None);

    assert_eq!(parse_desktop_outputs_override("auto"), None);
    assert_eq!(parse_desktop_outputs_override("compositor"), None);
    assert_eq!(parse_desktop_outputs_override("eDP-1:bad"), None);
    assert_eq!(parse_desktop_outputs_override("eDP-1:1920x1080@0"), None);
}

#[test]
fn parses_cursor_parallax_override_values() {
    assert_eq!(
        DesktopCursorParallax::parse_override("HDMI-A-1:0.25,-0.5"),
        Some((
            Some("HDMI-A-1".to_owned()),
            DesktopCursorParallax { x: 0.25, y: -0.5 }
        ))
    );
    assert_eq!(
        DesktopCursorParallax::parse_override("2,-2"),
        Some((None, DesktopCursorParallax { x: 1.0, y: -1.0 }))
    );
    assert_eq!(DesktopCursorParallax::parse_override("auto"), None);
    assert_eq!(DesktopCursorParallax::parse_override("bad"), None);
}

#[test]
fn applies_cursor_parallax_override_to_named_or_focused_output() {
    let mut snapshot = DesktopSnapshot {
        outputs: vec![
            DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            },
            DesktopOutput {
                focused: true,
                ..DesktopOutput::virtual_output("HDMI-A-1")
            },
        ],
        ..DesktopSnapshot::default()
    };

    apply_cursor_parallax_override(
        &mut snapshot,
        Some("eDP-1"),
        DesktopCursorParallax { x: -0.25, y: 0.5 },
    );
    assert_eq!(
        snapshot.outputs[0].cursor_parallax,
        Some(DesktopCursorParallax { x: -0.25, y: 0.5 })
    );

    apply_cursor_parallax_override(
        &mut snapshot,
        None,
        DesktopCursorParallax { x: 0.75, y: -0.5 },
    );
    assert_eq!(
        snapshot.outputs[1].cursor_parallax,
        Some(DesktopCursorParallax { x: 0.75, y: -0.5 })
    );
}

#[test]
fn applies_output_state_override_to_snapshot_outputs() {
    let mut snapshot = DesktopSnapshot {
        outputs: vec![
            DesktopOutput {
                focused: true,
                visible: true,
                has_fullscreen: false,
                ..DesktopOutput::virtual_output("eDP-1")
            },
            DesktopOutput {
                focused: true,
                visible: false,
                has_fullscreen: true,
                ..DesktopOutput::virtual_output("HDMI-A-1")
            },
        ],
        ..DesktopSnapshot::default()
    };

    apply_output_state_override(&mut snapshot, OutputStateOverride::Unfocused);

    assert!(snapshot.outputs.iter().all(|output| !output.focused));
    assert!(snapshot.outputs.iter().all(|output| output.visible));
    assert!(snapshot.outputs.iter().all(|output| !output.has_fullscreen));

    apply_output_state_override(&mut snapshot, OutputStateOverride::Fullscreen);

    assert!(snapshot.outputs.iter().all(|output| output.focused));
    assert!(snapshot.outputs.iter().all(|output| output.visible));
    assert!(snapshot.outputs.iter().all(|output| output.has_fullscreen));
}
