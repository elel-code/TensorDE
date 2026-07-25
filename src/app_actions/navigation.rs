use std::path::PathBuf;

use crate::platform::ActiveEventLoop;
use crate::platform::PhysicalSize;

use super::outcome::ShellActionOutcome;
use crate::FikaWgpuApp;
use crate::shell::location::LocationDraftPurpose;
use crate::shell::operation_request::ShellOperationRequest;
use crate::shell::pane::ShellPaneId;
use crate::shell::shortcuts::PathNavigationAction;
use crate::shell::tasks::ShellTaskStatus;
use crate::shell::transfer::{ShellAsyncNavigationCompletion, ShellNavigationHistoryUpdate};
use fika_core::default_user_places_path;

impl FikaWgpuApp {
    pub(crate) fn perform_path_navigation(
        &mut self,
        _event_loop: &ActiveEventLoop,
        action: PathNavigationAction,
    ) {
        let pane = self.scene.active_pane();
        let target = match action {
            PathNavigationAction::Back => self
                .scene
                .pane_history(pane)
                .back
                .last()
                .cloned()
                .map(|path| (path, ShellNavigationHistoryUpdate::Back)),
            PathNavigationAction::Forward => self
                .scene
                .pane_history(pane)
                .forward
                .last()
                .cloned()
                .map(|path| (path, ShellNavigationHistoryUpdate::Forward)),
            PathNavigationAction::Parent => self
                .scene
                .parent_directory_path_for_pane(pane)
                .map(|path| (path, ShellNavigationHistoryUpdate::Push)),
        };
        if let Some((path, history)) = target {
            self.queue_path_navigation(pane, path, history, action.reason());
        }
    }

    pub(crate) fn reload_scene_path(&mut self, event_loop: &ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.reload_current_path(size) {
            Ok(true) => {
                self.scene.record_task_status(ShellTaskStatus::completed(
                    "Refreshed",
                    self.scene.active_pane_path_label(),
                    false,
                ));
                self.apply_action_outcome(
                    event_loop,
                    ShellActionOutcome::Present("reload-directory"),
                );
            }
            Ok(false) => {
                self.scene.record_task_status(ShellTaskStatus::completed(
                    "Refresh skipped",
                    self.scene.active_pane_path_label(),
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
            Err(error) => {
                fika_log!("[fika-wgpu] reload-error {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Refresh failed",
                    error,
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }

    pub(crate) fn commit_location_draft(&mut self, event_loop: &ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        if self.scene.location_draft_purpose() == Some(LocationDraftPurpose::AddNetworkFolder) {
            self.commit_add_network_folder_draft(event_loop, size);
            return;
        }
        let input = self.scene.location_draft_value().unwrap_or("").to_string();
        let Some((pane, path)) = self.scene.resolved_location_draft() else {
            fika_log!("[fika-wgpu] location-error input={input:?} error=empty");
            return;
        };
        let closed = self.scene.close_location_draft(size);
        if !self.queue_path_navigation(
            pane,
            path,
            ShellNavigationHistoryUpdate::Push,
            "location-commit",
        ) {
            fika_log!("[fika-wgpu] location-unchanged input={input:?}");
            self.apply_window_action_outcome(ShellActionOutcome::redraw_if(closed));
        }
    }

    fn commit_add_network_folder_draft(
        &mut self,
        _event_loop: &ActiveEventLoop,
        size: PhysicalSize<u32>,
    ) {
        let input = self.scene.location_draft_value().unwrap_or("").to_string();
        let request = match self.scene.add_network_folder_request_from_draft() {
            Ok(request) => request,
            Err(error) => {
                fika_log!("[fika-wgpu] add-network-folder-error input={input:?} error={error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Add Network Folder failed",
                    error,
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
        };

        let _ = self.scene.close_location_draft(size);
        match self.scene.add_network_folder_place(
            &default_user_places_path(),
            &request.path,
            request.label,
            size,
        ) {
            Ok(_) => {
                self.queue_path_navigation(
                    request.pane,
                    request.path,
                    ShellNavigationHistoryUpdate::Push,
                    "add-network-folder",
                );
            }
            Err(error) => {
                fika_log!("[fika-wgpu] add-network-folder-error input={input:?} error={error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Add Network Folder failed",
                    error,
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }

    pub(crate) fn load_path_into_pane(
        &mut self,
        _event_loop: &ActiveEventLoop,
        pane: ShellPaneId,
        path: PathBuf,
        reason: &'static str,
    ) {
        self.queue_path_navigation(pane, path, ShellNavigationHistoryUpdate::Push, reason);
    }

    fn queue_path_navigation(
        &mut self,
        pane: ShellPaneId,
        target_path: PathBuf,
        history: ShellNavigationHistoryUpdate,
        reason: &'static str,
    ) -> bool {
        if self.renderer.is_none() {
            return false;
        }
        let pane = self.scene.normalized_pane_id(pane);
        let Some(source_path) = self.scene.pane_state(pane).map(|state| state.path.clone()) else {
            return false;
        };
        if source_path == target_path {
            return false;
        }

        let generation = self.navigation_generations[pane.index()].wrapping_add(1);
        self.navigation_generations[pane.index()] = generation;
        self.submit_operation_request(ShellOperationRequest::navigation(
            generation,
            pane,
            source_path,
            target_path,
            history,
            reason,
        ));

        true
    }

    pub(crate) fn apply_async_navigation_completion(
        &mut self,
        completion: ShellAsyncNavigationCompletion,
        size: PhysicalSize<u32>,
    ) -> bool {
        let pane = completion.pane;
        if self.navigation_generations[pane.index()] != completion.generation {
            return false;
        }
        if !self
            .scene
            .pane_state(pane)
            .is_some_and(|state| state.path == completion.source_path)
        {
            return false;
        }

        let entries = match completion.result {
            Ok(entries) => entries,
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] async-navigation-error reason={} path={} error={error}",
                    completion.reason,
                    completion.target_path.display()
                );
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Open folder failed",
                    error,
                    false,
                ));
                return true;
            }
        };

        let history = self.scene.pane_history_mut(pane);
        match completion.history {
            ShellNavigationHistoryUpdate::Push => {
                history.push_back(completion.source_path);
                history.clear_forward();
            }
            ShellNavigationHistoryUpdate::Back => {
                if history.back.last() == Some(&completion.target_path) {
                    history.back.pop();
                }
                history.push_forward(completion.source_path);
            }
            ShellNavigationHistoryUpdate::Forward => {
                if history.forward.last() == Some(&completion.target_path) {
                    history.forward.pop();
                }
                history.push_back(completion.source_path);
            }
        }
        self.scene
            .apply_loaded_path_to_pane(pane, completion.target_path, entries, size);
        true
    }
}
