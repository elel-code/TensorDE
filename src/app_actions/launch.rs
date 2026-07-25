use super::outcome::ShellActionOutcome;
use crate::shell::ark::{self, extract::execute_ark_extract_and_trash};
use crate::shell::metrics::WGPU_SHELL_PANE_ID;
use crate::shell::open_file::{OpenFileRequest, default_open_file_launch_request};
use crate::shell::open_with::OpenWithLaunchRequest;
use crate::shell::service_menu::ServiceMenuLaunchRequest;
use crate::shell::tasks::ShellTaskStatus;
use crate::shell::transfer::ShellAsyncLaunchKind;
use crate::{FikaWgpuApp, path_display_label};
use fika_core::{
    OpenWithLaunchResult, ServiceMenuLaunchResult, launch_with_systemd_user,
    service_menu_target_label,
};

impl FikaWgpuApp {
    pub(crate) fn launch_open_file_request(&mut self, request: &OpenFileRequest) {
        let launch = match default_open_file_launch_request(&self.mime_applications, request) {
            Ok(launch) => launch,
            Err(error) => {
                fika_log!("[fika-wgpu] open-error {error}");
                self.scene
                    .record_task_status(ShellTaskStatus::failed("Open failed", error, false));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
        };
        self.scene.record_open_file_request(request);
        let path = launch.path.clone();
        let app_name = launch.app_name.clone();
        self.start_async_launch_task(
            ShellAsyncLaunchKind::OpenFile,
            "Opening",
            format!("{} with {}", path_display_label(&path), app_name),
            move || async move {
                match launch_with_systemd_user(launch.plan).await {
                    Ok(result) => {
                        let message = format!(
                            "Opened {} with {} via {} systemd unit(s)",
                            path.display(),
                            app_name,
                            result.units.len()
                        );
                        fika_log!(
                            "[fika-wgpu] open-finished path={} app={:?} units={}",
                            path.display(),
                            app_name,
                            result.units.join(",")
                        );
                        (true, message)
                    }
                    Err(error) => {
                        let message =
                            format!("Cannot open {} with {}: {error}", path.display(), app_name);
                        fika_log!(
                            "[fika-wgpu] open-finished path={} app={:?} error={error}",
                            path.display(),
                            app_name
                        );
                        (false, message)
                    }
                }
            },
        );
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }

    pub(crate) fn run_context_service_menu_action(&mut self, action_id: String) {
        let extract_and_trash = ark::is_extract_and_trash_action(&action_id);
        let request = match self
            .scene
            .service_menu_launch_request(&self.mime_applications, &action_id)
        {
            Ok(request) => request,
            Err(error) => {
                fika_log!("[fika-wgpu] service-menu-error action={action_id:?} {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Action failed",
                    error,
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
        };
        if extract_and_trash {
            self.run_ark_extract_and_trash_action(request);
            return;
        }
        let paths = request.paths.clone();
        let app_name = request.app_name.clone();
        let target_label = service_menu_target_label(&paths);
        self.start_async_launch_task(
            ShellAsyncLaunchKind::ServiceMenu,
            "Running Action",
            format!("{target_label} with {app_name}"),
            move || async move {
                let result = launch_with_systemd_user(request.plan).await;
                let success = result.is_ok();
                let status = ServiceMenuLaunchResult {
                    pane_id: WGPU_SHELL_PANE_ID,
                    target_label,
                    app_name,
                    result,
                }
                .status_message();
                fika_log!("[fika-wgpu] service-menu-finished {status}");
                (success, status)
            },
        );
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }

    pub(crate) fn run_ark_extract_and_trash_action(&mut self, request: ServiceMenuLaunchRequest) {
        let paths = request.paths.clone();
        let app_name = request.app_name.clone();
        let target_label = service_menu_target_label(&paths);
        self.start_async_launch_task(
            ShellAsyncLaunchKind::ArkExtractAndTrash,
            "Extracting",
            format!("{target_label} with {app_name}"),
            move || async move {
                match execute_ark_extract_and_trash(request).await {
                    Ok(message) => {
                        let status = format!("Ran {app_name} for {target_label}: {message}");
                        fika_log!("[fika-wgpu] service-menu-finished {status}");
                        (true, status)
                    }
                    Err(err) => {
                        let status = format!("Cannot run {app_name} for {target_label}: {err}");
                        fika_log!("[fika-wgpu] service-menu-finished {status}");
                        (false, status)
                    }
                }
            },
        );
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }

    pub(crate) fn open_context_target_with_application(&mut self, desktop_id: String) {
        let request = match self
            .scene
            .open_with_launch_request_for_context_application(&self.mime_applications, &desktop_id)
        {
            Ok(request) => request,
            Err(error) => {
                fika_log!("[fika-wgpu] open-with-error app={desktop_id:?} {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Open With failed",
                    error,
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
        };
        self.launch_open_with_request(request);
    }

    pub(crate) fn launch_open_with_request(&mut self, request: OpenWithLaunchRequest) {
        let path = request.path.clone();
        let app_name = request.app_name.clone();
        self.start_async_launch_task(
            ShellAsyncLaunchKind::OpenWith,
            "Opening With",
            format!("{} using {}", path_display_label(&path), app_name),
            move || async move {
                let result = launch_with_systemd_user(request.plan).await;
                let success = result.is_ok();
                let status = OpenWithLaunchResult {
                    pane_id: WGPU_SHELL_PANE_ID,
                    path,
                    app_name,
                    result,
                }
                .status_message();
                fika_log!("[fika-wgpu] open-with-finished {status}");
                (success, status)
            },
        );
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }
}
