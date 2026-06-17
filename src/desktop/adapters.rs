//! Compositor adapter helpers for building desktop snapshots.

use super::{CompositorKind, DesktopOutput, DesktopSnapshot, PowerState};
use crate::config::AdapterConfig;
use serde_json::Value;
use std::fmt;
use std::process::Command;

pub fn read_desktop_snapshot(config: &AdapterConfig) -> DesktopSnapshot {
    if config.hyprland && std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        if let Ok(snapshot) = hyprland::read_snapshot() {
            return with_power_state(snapshot);
        }
    }

    if config.niri && std::env::var_os("NIRI_SOCKET").is_some() {
        if let Ok(snapshot) = niri::read_snapshot() {
            return with_power_state(snapshot);
        }
    }

    let mut snapshot = DesktopSnapshot::placeholder();
    if config.generic_wayland {
        #[cfg(feature = "gtk-renderer")]
        {
            snapshot.outputs = crate::renderer::gtk::gdk_desktop_outputs();
        }
        snapshot.compositor = Some(CompositorKind::GenericWayland);
    }
    with_power_state(snapshot)
}

fn with_power_state(mut snapshot: DesktopSnapshot) -> DesktopSnapshot {
    if snapshot.power == PowerState::Unknown {
        snapshot.power = super::power::read_power_state();
    }
    snapshot
}

#[derive(Debug)]
pub enum AdapterError {
    CommandFailed {
        program: String,
        args: Vec<String>,
        message: String,
    },
    Parse(serde_json::Error),
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandFailed {
                program,
                args,
                message,
            } => write!(f, "{} {} failed: {message}", program, args.join(" ")),
            Self::Parse(source) => write!(f, "failed to parse compositor JSON: {source}"),
        }
    }
}

impl std::error::Error for AdapterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(source) => Some(source),
            Self::CommandFailed { .. } => None,
        }
    }
}

mod hyprland {
    use super::*;

    pub fn read_snapshot() -> Result<DesktopSnapshot, AdapterError> {
        let monitors = run_json_command("hyprctl", &["-j", "monitors"])?;
        let clients = run_json_command("hyprctl", &["-j", "clients"])?;
        Ok(snapshot_from_json(&monitors, &clients))
    }

    fn snapshot_from_json(monitors: &Value, clients: &Value) -> DesktopSnapshot {
        let mut outputs = Vec::new();
        for monitor in value_array(monitors) {
            let Some(name) = string_field(monitor, "name") else {
                continue;
            };
            let workspace =
                nested_string_field(monitor, &["activeWorkspace", "name"]).or_else(|| {
                    nested_i64_field(monitor, &["activeWorkspace", "id"]).map(|id| id.to_string())
                });
            let workspace_id = nested_i64_field(monitor, &["activeWorkspace", "id"]);
            let monitor_id = i64_field(monitor, "id");
            outputs.push(DesktopOutput {
                name,
                make: string_field(monitor, "make"),
                model: string_field(monitor, "model"),
                width: u32_field(monitor, "width"),
                height: u32_field(monitor, "height"),
                scale: f32_field(monitor, "scale").unwrap_or(1.0),
                focused: bool_field(monitor, "focused").unwrap_or(false),
                visible: bool_field(monitor, "disabled")
                    .map(|disabled| !disabled)
                    .unwrap_or(true),
                has_fullscreen: has_fullscreen_client(
                    clients,
                    monitor_id,
                    workspace_id,
                    workspace.as_deref(),
                ),
                active_workspace: workspace,
            });
        }

        DesktopSnapshot {
            compositor: Some(CompositorKind::Hyprland),
            outputs,
            power: PowerState::Unknown,
            session_active: true,
        }
    }

    fn has_fullscreen_client(
        clients: &Value,
        monitor_id: Option<i64>,
        workspace_id: Option<i64>,
        workspace_name: Option<&str>,
    ) -> bool {
        value_array(clients).into_iter().any(|client| {
            if !bool_field(client, "fullscreen").unwrap_or(false) {
                return false;
            }
            if bool_field(client, "hidden").unwrap_or(false) {
                return false;
            }
            let client_monitor = i64_field(client, "monitor");
            let client_workspace_id = nested_i64_field(client, &["workspace", "id"]);
            let client_workspace_name = nested_string_field(client, &["workspace", "name"]);
            monitor_id.is_some() && client_monitor == monitor_id
                || workspace_id.is_some() && client_workspace_id == workspace_id
                || workspace_name.is_some() && client_workspace_name.as_deref() == workspace_name
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::json;

        #[test]
        fn maps_hyprland_monitors_and_fullscreen_clients() {
            let monitors = json!([
                {
                    "id": 0,
                    "name": "eDP-1",
                    "make": "Framework",
                    "model": "Laptop",
                    "width": 2256,
                    "height": 1504,
                    "scale": 1.5,
                    "focused": true,
                    "activeWorkspace": { "id": 3, "name": "3" }
                },
                {
                    "id": 1,
                    "name": "DP-1",
                    "disabled": true,
                    "activeWorkspace": { "id": 4, "name": "web" }
                }
            ]);
            let clients = json!([
                {
                    "monitor": 0,
                    "fullscreen": true,
                    "hidden": false,
                    "workspace": { "id": 3, "name": "3" }
                }
            ]);

            let snapshot = snapshot_from_json(&monitors, &clients);
            assert_eq!(snapshot.compositor, Some(CompositorKind::Hyprland));
            assert_eq!(snapshot.outputs.len(), 2);
            assert!(snapshot.outputs[0].focused);
            assert!(snapshot.outputs[0].has_fullscreen);
            assert_eq!(snapshot.outputs[0].scale, 1.5);
            assert!(!snapshot.outputs[1].visible);
        }
    }
}

mod niri {
    use super::*;

    pub fn read_snapshot() -> Result<DesktopSnapshot, AdapterError> {
        let outputs = run_json_command("niri", &["msg", "--json", "outputs"])?;
        let workspaces = run_json_command("niri", &["msg", "--json", "workspaces"])?;
        let windows = run_json_command("niri", &["msg", "--json", "windows"])?;
        Ok(snapshot_from_json(&outputs, &workspaces, &windows))
    }

    fn snapshot_from_json(outputs: &Value, workspaces: &Value, windows: &Value) -> DesktopSnapshot {
        let workspaces = value_array(workspaces);
        let windows = value_array(windows);
        let mut snapshot_outputs = Vec::new();

        for (fallback_name, output) in output_objects(outputs) {
            let Some(name) = string_field(output, "name").or(fallback_name) else {
                continue;
            };
            let active_workspace = active_workspace_for_output(&workspaces, &name);
            let focused = active_workspace
                .as_ref()
                .and_then(|workspace| bool_field(workspace.value, "is_focused"))
                .or_else(|| bool_field(output, "focused"))
                .unwrap_or(false);
            let active_workspace_id =
                active_workspace.and_then(|workspace| i64_field(workspace.value, "id"));
            let active_workspace_name = active_workspace.and_then(|workspace| {
                string_field(workspace.value, "name")
                    .or_else(|| i64_field(workspace.value, "idx").map(|idx| idx.to_string()))
            });
            let has_fullscreen = has_fullscreen_window(&windows, active_workspace_id, &name);

            snapshot_outputs.push(DesktopOutput {
                name,
                make: string_field(output, "make"),
                model: string_field(output, "model"),
                width: nested_u32_field(output, &["logical", "width"])
                    .or_else(|| nested_u32_field(output, &["current_mode", "width"]))
                    .or_else(|| u32_field(output, "width")),
                height: nested_u32_field(output, &["logical", "height"])
                    .or_else(|| nested_u32_field(output, &["current_mode", "height"]))
                    .or_else(|| u32_field(output, "height")),
                scale: f32_field(output, "scale").unwrap_or(1.0),
                focused,
                visible: bool_field(output, "power").unwrap_or(true),
                has_fullscreen,
                active_workspace: active_workspace_name,
            });
        }

        DesktopSnapshot {
            compositor: Some(CompositorKind::Niri),
            outputs: snapshot_outputs,
            power: PowerState::Unknown,
            session_active: true,
        }
    }

    #[derive(Clone, Copy)]
    struct WorkspaceRef<'a> {
        value: &'a Value,
    }

    fn active_workspace_for_output<'a>(
        workspaces: &'a [&'a Value],
        output_name: &str,
    ) -> Option<WorkspaceRef<'a>> {
        workspaces
            .iter()
            .copied()
            .find(|workspace| {
                workspace_output_name(workspace).as_deref() == Some(output_name)
                    && bool_field(workspace, "is_active").unwrap_or(false)
            })
            .or_else(|| {
                workspaces.iter().copied().find(|workspace| {
                    workspace_output_name(workspace).as_deref() == Some(output_name)
                })
            })
            .map(|value| WorkspaceRef { value })
    }

    fn workspace_output_name(workspace: &Value) -> Option<String> {
        string_field(workspace, "output")
            .or_else(|| nested_string_field(workspace, &["output", "name"]))
            .or_else(|| string_field(workspace, "output_name"))
    }

    fn has_fullscreen_window(
        windows: &[&Value],
        active_workspace_id: Option<i64>,
        output_name: &str,
    ) -> bool {
        windows.iter().copied().any(|window| {
            if !bool_field(window, "is_fullscreen")
                .or_else(|| bool_field(window, "fullscreen"))
                .unwrap_or(false)
            {
                return false;
            }

            let workspace_match = active_workspace_id.is_some()
                && (i64_field(window, "workspace_id") == active_workspace_id
                    || nested_i64_field(window, &["workspace", "id"]) == active_workspace_id);
            let output_match = string_field(window, "output").as_deref() == Some(output_name)
                || nested_string_field(window, &["output", "name"]).as_deref() == Some(output_name);
            workspace_match || output_match
        })
    }

    fn output_objects(value: &Value) -> Vec<(Option<String>, &Value)> {
        if let Some(array) = value.as_array() {
            return array.iter().map(|output| (None, output)).collect();
        }
        value
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .map(|(name, output)| (Some(name.clone()), output))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::json;

        #[test]
        fn maps_niri_outputs_workspaces_and_fullscreen_windows() {
            let outputs = json!({
                "eDP-1": {
                    "make": "Framework",
                    "model": "Laptop",
                    "logical": { "width": 1504, "height": 1002 },
                    "scale": 1.5,
                    "power": true
                },
                "DP-1": {
                    "name": "DP-1",
                    "current_mode": { "width": 2560, "height": 1440 },
                    "power": false
                }
            });
            let workspaces = json!([
                { "id": 10, "idx": 1, "output": "eDP-1", "is_active": true, "is_focused": true },
                { "id": 11, "name": "web", "output": "DP-1", "is_active": true, "is_focused": false }
            ]);
            let windows = json!([
                { "workspace_id": 10, "is_fullscreen": true }
            ]);

            let snapshot = snapshot_from_json(&outputs, &workspaces, &windows);
            assert_eq!(snapshot.compositor, Some(CompositorKind::Niri));
            assert_eq!(snapshot.outputs.len(), 2);
            assert_eq!(snapshot.outputs[0].name, "DP-1");
            assert!(!snapshot.outputs[0].visible);
            assert_eq!(snapshot.outputs[1].name, "eDP-1");
            assert!(snapshot.outputs[1].focused);
            assert!(snapshot.outputs[1].has_fullscreen);
            assert_eq!(snapshot.outputs[1].active_workspace.as_deref(), Some("1"));
        }
    }
}

fn run_json_command(program: &str, args: &[&str]) -> Result<Value, AdapterError> {
    let output =
        Command::new(program)
            .args(args)
            .output()
            .map_err(|err| AdapterError::CommandFailed {
                program: program.to_owned(),
                args: args.iter().map(|arg| (*arg).to_owned()).collect(),
                message: err.to_string(),
            })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let message = if stderr.is_empty() {
            format!("exited with status {}", output.status)
        } else {
            stderr
        };
        return Err(AdapterError::CommandFailed {
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            message,
        });
    }
    serde_json::from_slice(&output.stdout).map_err(AdapterError::Parse)
}

fn value_array(value: &Value) -> Vec<&Value> {
    value
        .as_array()
        .map(|array| array.iter().collect())
        .unwrap_or_default()
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn bool_field(value: &Value, field: &str) -> Option<bool> {
    value.get(field).and_then(value_as_bool)
}

fn i64_field(value: &Value, field: &str) -> Option<i64> {
    value.get(field).and_then(value_as_i64)
}

fn u32_field(value: &Value, field: &str) -> Option<u32> {
    value.get(field).and_then(value_as_u32)
}

fn f32_field(value: &Value, field: &str) -> Option<f32> {
    value.get(field).and_then(value_as_f32)
}

fn nested_string_field(value: &Value, path: &[&str]) -> Option<String> {
    nested_value(value, path)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn nested_i64_field(value: &Value, path: &[&str]) -> Option<i64> {
    nested_value(value, path).and_then(value_as_i64)
}

fn nested_u32_field(value: &Value, path: &[&str]) -> Option<u32> {
    nested_value(value, path).and_then(value_as_u32)
}

fn nested_value<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for field in path {
        current = current.get(field)?;
    }
    Some(current)
}

fn value_as_bool(value: &Value) -> Option<bool> {
    if let Some(value) = value.as_bool() {
        return Some(value);
    }
    if let Some(value) = value.as_i64() {
        return Some(value != 0);
    }
    value.as_str().and_then(|value| match value {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    })
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn value_as_u32(value: &Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .or_else(|| value.as_i64().and_then(|value| u32::try_from(value).ok()))
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn value_as_f32(value: &Value) -> Option<f32> {
    value
        .as_f64()
        .map(|value| value as f32)
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

#[cfg(test)]
mod tests {
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
}
