use std::path::PathBuf;

use fika_core::{FileTransferMode, TrashViewOperation};

use crate::shell::context_menu::ShellContextMenuAction;
use crate::shell::pane::ShellPaneId;
use crate::shell::transfer::ShellAsyncTransferSource;

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
}
