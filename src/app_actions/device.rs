use crate::platform::ActiveEventLoop;

use super::outcome::{ShellActionEffect, ShellActionOutcome};
use crate::shell::context_menu::ShellContextMenuAction;
use crate::shell::operation_request::ShellOperationRequest;
use crate::shell::tasks::ShellTaskStatus;
use crate::shell::transfer::ShellAsyncDeviceCompletion;
use crate::{DeviceActionRequest, FikaWgpuApp};

impl FikaWgpuApp {
    pub(crate) fn perform_device_context_action(
        &mut self,
        event_loop: &ActiveEventLoop,
        action: ShellContextMenuAction,
    ) {
        let Some(request) = self.scene.context_target_device_action(action) else {
            fika_log!(
                "[fika-wgpu] device-action-error action={} target=none",
                action.as_str()
            );
            self.scene.record_task_status(ShellTaskStatus::failed(
                format!("{} failed", action.label()),
                "No device target",
                false,
            ));
            self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            return;
        };
        self.perform_device_action_request(event_loop, request);
    }

    pub(crate) fn perform_device_action_request(
        &mut self,
        event_loop: &ActiveEventLoop,
        request: DeviceActionRequest,
    ) {
        fika_log!(
            "[fika-wgpu] device-action-start action={} id={:?} label={:?}",
            request.action.as_str(),
            request.id,
            request.label
        );
        self.scene.record_task_status(ShellTaskStatus::completed(
            request.action.label(),
            request.operation.in_progress_message(&request.label),
            false,
        ));
        // Clear transient overlays so the UI can redraw while mount/unmount runs.
        self.scene.context_target = None;
        self.scene.context_menu = None;
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
        self.submit_operation_request(ShellOperationRequest::device(request));
        let _ = event_loop;
    }

    pub(crate) fn apply_async_device_completion(
        &mut self,
        event_loop: &ActiveEventLoop,
        completion: ShellAsyncDeviceCompletion,
    ) -> bool {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return false;
        };
        let request = completion.request;
        let result = completion.result;
        let mount_point = match &result.result {
            Ok(Some(path)) => Some(path.clone()),
            Ok(None) => None,
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] device-action-error action={} id={:?} label={:?} error={error}",
                    request.action.as_str(),
                    request.id,
                    request.label
                );
                None
            }
        };

        match self
            .scene
            .apply_device_place_operation_result(&request, &result, size)
        {
            Ok(()) => {
                if let Some(path) = mount_point {
                    self.apply_action_effect(
                        event_loop,
                        ShellActionEffect::load_path(request.pane, path, "device-mount"),
                    );
                }
                true
            }
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] device-action-refresh-error action={} id={:?} error={error}",
                    request.action.as_str(),
                    request.id
                );
                true
            }
        }
    }
}
