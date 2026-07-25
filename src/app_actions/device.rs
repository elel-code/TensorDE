use crate::platform::ActiveEventLoop;

use super::outcome::{ShellActionEffect, ShellActionOutcome};
use crate::shell::context_menu::ShellContextMenuAction;
use crate::shell::metrics::WGPU_SHELL_PANE_ID;
use crate::shell::tasks::ShellTaskStatus;
use crate::shell::transfer::{ShellAsyncDeviceCompletion, ShellAsyncTaskResult};
use crate::{DeviceActionRequest, FikaWgpuApp};
use fika_core::{perform_device_place_operation, spawn_operation_task_with_completion};

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

        let tx = self.async_task_tx.clone();
        let proxy = self.event_loop_proxy.clone();
        let action_label = request.action.label();
        let action_name = request.action.as_str();
        let request_for_task = request.clone();
        if let Err(error) = spawn_operation_task_with_completion(
            move || async move {
                perform_device_place_operation(
                    WGPU_SHELL_PANE_ID,
                    request_for_task.id.clone(),
                    request_for_task.label.clone(),
                    request_for_task.operation,
                )
                .await
            },
            move |result| {
                if tx
                    .send(ShellAsyncTaskResult::Device(ShellAsyncDeviceCompletion {
                        request,
                        result,
                    }))
                    .is_ok()
                {
                    proxy.wake_up();
                }
            },
        ) {
            fika_log!("[fika-wgpu] device-action-runtime-error action={action_name} error={error}");
            self.scene.record_task_status(ShellTaskStatus::failed(
                format!("{action_label} failed"),
                error.to_string(),
                false,
            ));
            self.apply_action_outcome(event_loop, ShellActionOutcome::Redraw);
        }
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
