#![allow(unexpected_cfgs)] // `tensor-kdl` derive emits optional downstream DOM impls.

use std::{collections::BTreeSet, path::Path};

use tensor_kdl::Decode;

use super::{ConfigDiagnostic, ConfigPreview};

const KNOWN_WIDGETS: [&str; 8] = [
    "launcher",
    "workspaces",
    "active-window",
    "media",
    "system-status",
    "clock",
    "notifications",
    "control-center",
];
const MAX_LAUNCHER_COMMAND_ARGS: usize = 64;
const MAX_LAUNCHER_COMMAND_BYTES: usize = 16 * 1024;
const MIN_MEDIA_OSD_TIMEOUT_MS: u64 = 250;
const MAX_MEDIA_OSD_TIMEOUT_MS: u64 = 60_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShellLayoutPreview {
    pub panel_height: u32,
    pub popover_width: u32,
    pub popover_height: u32,
    pub osd_width: u32,
    pub osd_height: u32,
    pub edge_gap: i32,
}

impl Default for ShellLayoutPreview {
    fn default() -> Self {
        Self {
            panel_height: 40,
            popover_width: 420,
            popover_height: 560,
            osd_width: 320,
            osd_height: 96,
            edge_gap: 12,
        }
    }
}

#[derive(Debug, Default, Decode)]
struct ShellFile {
    #[kdl(child)]
    layout: Option<LayoutFile>,
    #[kdl(child)]
    panel: Option<PanelFile>,
    #[kdl(child)]
    media: Option<MediaFile>,
    #[kdl(child)]
    launcher: Option<LauncherFile>,
    #[kdl(child)]
    tensorland: Option<TensorlandFile>,
}

#[derive(Debug, Default, Decode)]
struct LayoutFile {
    #[kdl(child(name = "panel-height"), unwrap(argument))]
    panel_height: Option<u32>,
    #[kdl(child(name = "popover-width"), unwrap(argument))]
    popover_width: Option<u32>,
    #[kdl(child(name = "popover-height"), unwrap(argument))]
    popover_height: Option<u32>,
    #[kdl(child(name = "osd-width"), unwrap(argument))]
    osd_width: Option<u32>,
    #[kdl(child(name = "osd-height"), unwrap(argument))]
    osd_height: Option<u32>,
    #[kdl(child(name = "edge-gap"), unwrap(argument))]
    edge_gap: Option<i32>,
}

#[derive(Debug, Default, Decode)]
struct PanelFile {
    #[kdl(child)]
    left: Option<WidgetList>,
    #[kdl(child)]
    center: Option<WidgetList>,
    #[kdl(child)]
    right: Option<WidgetList>,
}

#[derive(Debug, Default, Decode)]
struct MediaFile {
    #[kdl(child(name = "playback-osd"), unwrap(argument))]
    _playback_osd: Option<bool>,
    #[kdl(child(name = "playback-osd-timeout-ms"), unwrap(argument))]
    playback_osd_timeout_ms: Option<u64>,
}

#[derive(Debug, Decode)]
struct WidgetList {
    #[kdl(arguments)]
    widgets: Vec<String>,
}

#[derive(Debug, Default, Decode)]
struct LauncherFile {
    #[kdl(child)]
    command: Option<CommandList>,
}

#[derive(Debug, Decode)]
struct CommandList {
    #[kdl(arguments)]
    arguments: Vec<String>,
}

#[derive(Debug, Default, Decode)]
struct TensorlandFile {
    #[kdl(child(name = "config-path"), unwrap(argument))]
    _config_path: Option<String>,
    #[kdl(child(name = "ipc-socket"), unwrap(argument))]
    _ipc_socket: Option<String>,
}

pub(super) fn validate(path: &Path, source: &str) -> Result<ConfigPreview, ConfigDiagnostic> {
    let file: ShellFile = tensor_kdl::read(source).map_err(|error| ConfigDiagnostic {
        message: tensor_kdl::format_error_named(&error, source, &path.display().to_string()),
    })?;
    let ShellFile {
        layout,
        panel,
        media,
        launcher,
        tensorland,
    } = file;
    let _ = tensorland;
    let layout = resolve_layout(layout)?;
    validate_panel(panel)?;
    validate_media(media)?;
    validate_launcher(launcher)?;
    Ok(ConfigPreview::Shell { layout })
}

fn validate_media(media: Option<MediaFile>) -> Result<(), ConfigDiagnostic> {
    let Some(timeout) = media.and_then(|media| media.playback_osd_timeout_ms) else {
        return Ok(());
    };
    if (MIN_MEDIA_OSD_TIMEOUT_MS..=MAX_MEDIA_OSD_TIMEOUT_MS).contains(&timeout) {
        Ok(())
    } else {
        Err(ConfigDiagnostic {
            message: format!(
                "media playback OSD timeout is {timeout} ms; expected \
                 {MIN_MEDIA_OSD_TIMEOUT_MS}..={MAX_MEDIA_OSD_TIMEOUT_MS} ms"
            ),
        })
    }
}

fn resolve_layout(file: Option<LayoutFile>) -> Result<ShellLayoutPreview, ConfigDiagnostic> {
    let defaults = ShellLayoutPreview::default();
    let Some(file) = file else {
        return Ok(defaults);
    };
    let layout = ShellLayoutPreview {
        panel_height: file.panel_height.unwrap_or(defaults.panel_height),
        popover_width: file.popover_width.unwrap_or(defaults.popover_width),
        popover_height: file.popover_height.unwrap_or(defaults.popover_height),
        osd_width: file.osd_width.unwrap_or(defaults.osd_width),
        osd_height: file.osd_height.unwrap_or(defaults.osd_height),
        edge_gap: file.edge_gap.unwrap_or(defaults.edge_gap),
    };
    let message = if layout.panel_height == 0 {
        Some("panel height must be non-zero")
    } else if layout.popover_width == 0 || layout.popover_height == 0 {
        Some("popover dimensions must be non-zero")
    } else if layout.osd_width == 0 || layout.osd_height == 0 {
        Some("OSD dimensions must be non-zero")
    } else if layout.edge_gap < 0 {
        Some("edge gap must not be negative")
    } else {
        None
    };
    match message {
        Some(message) => Err(ConfigDiagnostic {
            message: message.into(),
        }),
        None => Ok(layout),
    }
}

fn validate_panel(panel: Option<PanelFile>) -> Result<(), ConfigDiagnostic> {
    let Some(panel) = panel else {
        return Ok(());
    };
    let mut unique = BTreeSet::new();
    for (section, list) in [
        ("left", panel.left),
        ("center", panel.center),
        ("right", panel.right),
    ] {
        let Some(list) = list else {
            continue;
        };
        for widget in list.widgets {
            if !KNOWN_WIDGETS.contains(&widget.as_str()) {
                return Err(ConfigDiagnostic {
                    message: format!("unknown panel widget `{widget}` in `{section}` section"),
                });
            }
            if !unique.insert(widget.clone()) {
                return Err(ConfigDiagnostic {
                    message: format!("panel widget `{widget}` is configured more than once"),
                });
            }
        }
    }
    Ok(())
}

fn validate_launcher(launcher: Option<LauncherFile>) -> Result<(), ConfigDiagnostic> {
    let Some(command) = launcher.and_then(|launcher| launcher.command) else {
        return Ok(());
    };
    if command.arguments.is_empty() || command.arguments[0].is_empty() {
        return Err(ConfigDiagnostic {
            message: "launcher command must contain a non-empty program".into(),
        });
    }
    if command.arguments.len() > MAX_LAUNCHER_COMMAND_ARGS {
        return Err(ConfigDiagnostic {
            message: format!(
                "launcher command contains {} arguments; maximum is {MAX_LAUNCHER_COMMAND_ARGS}",
                command.arguments.len()
            ),
        });
    }
    let bytes = command
        .arguments
        .iter()
        .try_fold(0_usize, |total, argument| {
            if argument.contains('\0') {
                None
            } else {
                total.checked_add(argument.len())
            }
        });
    let Some(bytes) = bytes else {
        return Err(ConfigDiagnostic {
            message: "launcher command contains a NUL byte or overflows its size accounting".into(),
        });
    };
    if bytes > MAX_LAUNCHER_COMMAND_BYTES {
        return Err(ConfigDiagnostic {
            message: format!(
                "launcher command contains {bytes} bytes; maximum is {MAX_LAUNCHER_COMMAND_BYTES}"
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_matches_shell_defaults_and_overrides() {
        let ConfigPreview::Shell { layout } = validate(
            Path::new("shell.kdl"),
            "layout { panel-height 52; edge-gap 18; }",
        )
        .unwrap() else {
            panic!("expected shell preview")
        };
        assert_eq!(layout.panel_height, 52);
        assert_eq!(layout.edge_gap, 18);
        assert_eq!(layout.popover_width, 420);
    }

    #[test]
    fn semantic_errors_match_shell_runtime_contract() {
        assert!(validate(Path::new("shell.kdl"), "layout { panel-height 0 }").is_err());
        assert!(
            validate(
                Path::new("shell.kdl"),
                "panel { left \"clock\"; right \"clock\"; }",
            )
            .is_err()
        );
        assert!(validate(Path::new("shell.kdl"), "launcher { command }").is_err());
        assert!(
            validate(
                Path::new("shell.kdl"),
                "media { playback-osd-timeout-ms 0 }",
            )
            .is_err()
        );
    }

    #[test]
    fn complete_runtime_endpoints_are_accepted() {
        validate(
            Path::new("shell.kdl"),
            r#"
                launcher { command "tensor-launcher" "--surface" }
                media { playback-osd #true; playback-osd-timeout-ms 3000 }
                tensorland {
                    config-path "/etc/tensor/config.kdl"
                    ipc-socket "/run/user/1000/tensor.sock"
                }
            "#,
        )
        .unwrap();
    }
}
