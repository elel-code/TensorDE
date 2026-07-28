use super::{LauncherError, SystemdLaunchResult};
use crate::core::pane::PaneId;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct OpenWithLaunchResult {
    pub pane_id: PaneId,
    pub path: PathBuf,
    pub app_name: String,
    pub result: Result<SystemdLaunchResult, LauncherError>,
}

impl OpenWithLaunchResult {
    pub fn status_message(&self) -> String {
        match &self.result {
            Ok(launch) => format!(
                "Opened {} with {} via {} systemd unit(s)",
                self.path.display(),
                self.app_name,
                launch.units.len()
            ),
            Err(err) => format!(
                "Cannot open {} with {}: {err}",
                self.path.display(),
                self.app_name
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NewWindowLaunchResult {
    pub pane_id: PaneId,
    pub path: PathBuf,
    pub result: Result<SystemdLaunchResult, LauncherError>,
}

impl NewWindowLaunchResult {
    pub fn status_message(&self) -> String {
        match &self.result {
            Ok(launch) => format!(
                "Opened {} in new window via {} systemd unit(s)",
                self.path.display(),
                launch.units.len()
            ),
            Err(err) => format!("Cannot open {} in new window: {err}", self.path.display()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ServiceMenuLaunchResult {
    pub pane_id: PaneId,
    pub target_label: String,
    pub app_name: String,
    pub result: Result<SystemdLaunchResult, LauncherError>,
}

impl ServiceMenuLaunchResult {
    pub fn status_message(&self) -> String {
        match &self.result {
            Ok(launch) => format!(
                "Ran {} for {} via {} systemd unit(s)",
                self.app_name,
                self.target_label,
                launch.units.len()
            ),
            Err(err) => format!(
                "Cannot run {} for {}: {err}",
                self.app_name, self.target_label
            ),
        }
    }
}

pub fn service_menu_target_label(paths: &[PathBuf]) -> String {
    match paths {
        [path] => path.display().to_string(),
        paths => format!("{} items", paths.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_results_format_success_and_error_status_messages() {
        let open = OpenWithLaunchResult {
            pane_id: PaneId(1),
            path: PathBuf::from("/tmp/readme.txt"),
            app_name: "Editor".to_string(),
            result: Ok(SystemdLaunchResult {
                units: vec!["app.service".to_string()],
            }),
        };
        assert_eq!(
            open.status_message(),
            "Opened /tmp/readme.txt with Editor via 1 systemd unit(s)"
        );

        let new_window = NewWindowLaunchResult {
            pane_id: PaneId(1),
            path: PathBuf::from("/tmp"),
            result: Err(LauncherError::TerminalNotFound),
        };
        assert_eq!(
            new_window.status_message(),
            "Cannot open /tmp in new window: cannot find a supported terminal emulator"
        );

        let service = ServiceMenuLaunchResult {
            pane_id: PaneId(1),
            target_label: "2 items".to_string(),
            app_name: "Ark".to_string(),
            result: Ok(SystemdLaunchResult {
                units: vec!["one.service".to_string(), "two.service".to_string()],
            }),
        };
        assert_eq!(
            service.status_message(),
            "Ran Ark for 2 items via 2 systemd unit(s)"
        );
    }

    #[test]
    fn service_menu_target_label_reports_single_path_or_count() {
        assert_eq!(
            service_menu_target_label(&[PathBuf::from("/tmp/a.txt")]),
            "/tmp/a.txt"
        );
        assert_eq!(
            service_menu_target_label(&[PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]),
            "2 items"
        );
    }
}
