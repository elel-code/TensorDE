use std::path::PathBuf;

use fika_core::{DesktopLaunchPlan, FileTransferMode, TrashViewOperation};

use crate::DeviceActionRequest;
use crate::shell::context_menu::ShellContextMenuAction;
use crate::shell::create_rename::{CreateEntryRequest, RenameEntryRequest};
use crate::shell::pane::ShellPaneId;
use crate::shell::transfer::{ShellAsyncLaunchKind, ShellAsyncTransferSource};

/// Typed async work submitted by UI actions into the operation dispatcher.
///
/// Call sites should build a request and call `FikaWgpuApp::submit_operation_request`
/// rather than invoking individual `start_async_*` helpers.
#[derive(Clone, Debug)]
pub(crate) enum ShellOperationRequest {
    Transfer {
        source: ShellAsyncTransferSource,
        target_dir: PathBuf,
        mode: FileTransferMode,
        paths: Vec<PathBuf>,
        label: &'static str,
        clear_clipboard: bool,
        privileged: bool,
    },
    PasteText {
        target_dir: PathBuf,
        text: String,
    },
    MoveToTrash {
        paths: Vec<PathBuf>,
        pane_to_reload: ShellPaneId,
        privileged: bool,
        clear_selection_pane: Option<ShellPaneId>,
    },
    TrashView {
        action: ShellContextMenuAction,
        operation: TrashViewOperation,
        paths: Vec<PathBuf>,
        pane_to_reload: ShellPaneId,
    },
    Create {
        request: CreateEntryRequest,
    },
    Rename {
        request: RenameEntryRequest,
    },
    Device {
        request: DeviceActionRequest,
    },
    Launch {
        kind: ShellAsyncLaunchKind,
        running_label: String,
        running_detail: String,
        work: ShellLaunchWork,
    },
}

/// Launch-side work payload owned by the dispatcher after submit.
#[derive(Clone, Debug)]
pub(crate) enum ShellLaunchWork {
    Systemd {
        plan: DesktopLaunchPlan,
        path: PathBuf,
        app_name: String,
        target_label: Option<String>,
    },
    ArkExtractAndTrash {
        request: crate::shell::service_menu::ServiceMenuLaunchRequest,
    },
}

impl ShellOperationRequest {
    pub(crate) fn transfer(
        source: ShellAsyncTransferSource,
        target_dir: PathBuf,
        mode: FileTransferMode,
        paths: Vec<PathBuf>,
        label: &'static str,
        clear_clipboard: bool,
        privileged: bool,
    ) -> Self {
        Self::Transfer {
            source,
            target_dir,
            mode,
            paths,
            label,
            clear_clipboard,
            privileged,
        }
    }

    pub(crate) fn paste_text(target_dir: PathBuf, text: String) -> Self {
        Self::PasteText { target_dir, text }
    }

    pub(crate) fn move_to_trash(
        paths: Vec<PathBuf>,
        pane_to_reload: ShellPaneId,
        privileged: bool,
        clear_selection_pane: Option<ShellPaneId>,
    ) -> Self {
        Self::MoveToTrash {
            paths,
            pane_to_reload,
            privileged,
            clear_selection_pane,
        }
    }

    pub(crate) fn trash_view(
        action: ShellContextMenuAction,
        operation: TrashViewOperation,
        paths: Vec<PathBuf>,
        pane_to_reload: ShellPaneId,
    ) -> Self {
        Self::TrashView {
            action,
            operation,
            paths,
            pane_to_reload,
        }
    }

    pub(crate) fn create(request: CreateEntryRequest) -> Self {
        Self::Create { request }
    }

    pub(crate) fn rename(request: RenameEntryRequest) -> Self {
        Self::Rename { request }
    }

    pub(crate) fn device(request: DeviceActionRequest) -> Self {
        Self::Device { request }
    }

    pub(crate) fn launch(
        kind: ShellAsyncLaunchKind,
        running_label: impl Into<String>,
        running_detail: impl Into<String>,
        work: ShellLaunchWork,
    ) -> Self {
        Self::Launch {
            kind,
            running_label: running_label.into(),
            running_detail: running_detail.into(),
            work,
        }
    }

    pub(crate) fn open_file_launch(
        plan: DesktopLaunchPlan,
        path: PathBuf,
        app_name: String,
        running_detail: String,
    ) -> Self {
        Self::launch(
            ShellAsyncLaunchKind::OpenFile,
            "Opening",
            running_detail,
            ShellLaunchWork::Systemd {
                plan,
                path,
                app_name,
                target_label: None,
            },
        )
    }

    pub(crate) fn open_with_launch(
        plan: DesktopLaunchPlan,
        path: PathBuf,
        app_name: String,
        running_detail: String,
    ) -> Self {
        Self::launch(
            ShellAsyncLaunchKind::OpenWith,
            "Opening With",
            running_detail,
            ShellLaunchWork::Systemd {
                plan,
                path,
                app_name,
                target_label: None,
            },
        )
    }

    pub(crate) fn service_menu_launch(
        plan: DesktopLaunchPlan,
        path: PathBuf,
        app_name: String,
        target_label: String,
        running_detail: String,
    ) -> Self {
        Self::launch(
            ShellAsyncLaunchKind::ServiceMenu,
            "Running Action",
            running_detail,
            ShellLaunchWork::Systemd {
                plan,
                path,
                app_name,
                target_label: Some(target_label),
            },
        )
    }

    pub(crate) fn ark_extract_and_trash(
        request: crate::shell::service_menu::ServiceMenuLaunchRequest,
        running_detail: String,
    ) -> Self {
        Self::launch(
            ShellAsyncLaunchKind::ArkExtractAndTrash,
            "Extracting",
            running_detail,
            ShellLaunchWork::ArkExtractAndTrash { request },
        )
    }
}
