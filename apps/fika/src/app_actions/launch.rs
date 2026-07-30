use super::outcome::ShellActionOutcome;
use crate::ui::ark;
use crate::ui::open_file::{OpenFileRequest, default_open_file_launch_request};
use crate::ui::open_with::OpenWithLaunchRequest;
use crate::ui::operation_request::ShellOperationRequest;
use crate::ui::service_menu::ServiceMenuLaunchRequest;
use crate::ui::tasks::ShellTaskStatus;
use crate::{FikaApp, path_display_label};
use fika_core::service_menu_target_label;

impl FikaApp {
    pub(crate) fn launch_open_file_request(&mut self, request: &OpenFileRequest) {
        let launch = match default_open_file_launch_request(&self.mime_applications, request) {
            Ok(launch) => launch,
            Err(error) => {
                fika_log!("[fika] open-error {error}");
                self.scene
                    .record_task_status(ShellTaskStatus::failed("Open failed", error, false));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
        };
        self.scene.record_open_file_request(request);
        let path = launch.path.clone();
        let app_name = launch.app_name.clone();
        self.submit_operation_request(ShellOperationRequest::open_file_launch(
            launch.plan,
            path.clone(),
            app_name.clone(),
            format!("{} with {}", path_display_label(&path), app_name),
        ));
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
                fika_log!("[fika] service-menu-error action={action_id:?} {error}");
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
        self.submit_operation_request(ShellOperationRequest::service_menu_launch(
            request.plan,
            paths.first().cloned().unwrap_or_default(),
            app_name.clone(),
            target_label.clone(),
            format!("{target_label} with {app_name}"),
        ));
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }

    pub(crate) fn run_ark_extract_and_trash_action(&mut self, request: ServiceMenuLaunchRequest) {
        let paths = request.paths.clone();
        let app_name = request.app_name.clone();
        let target_label = service_menu_target_label(&paths);
        self.submit_operation_request(ShellOperationRequest::ark_extract_and_trash(
            request,
            format!("{target_label} with {app_name}"),
        ));
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }

    pub(crate) fn open_context_target_with_application(&mut self, desktop_id: String) {
        let request = match self
            .scene
            .open_with_launch_request_for_context_application(&self.mime_applications, &desktop_id)
        {
            Ok(request) => request,
            Err(error) => {
                fika_log!("[fika] open-with-error app={desktop_id:?} {error}");
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
        self.submit_operation_request(ShellOperationRequest::open_with_launch(
            request.plan,
            path.clone(),
            app_name.clone(),
            format!("{} using {}", path_display_label(&path), app_name),
        ));
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }
}
