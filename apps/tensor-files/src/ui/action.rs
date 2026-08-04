use tensor_files_core::FileClipboardRole;

use crate::ui::context_menu::{ShellContextMenuAction, ShellContextMenuCommand};
use crate::ui::create_rename::CreateEntryKind;
use crate::ui::options::ShellViewMode;
use crate::ui::shortcuts::FileKeyboardCommand;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ContextMenuCommandDispatch {
    Action {
        action: ShellContextMenuAction,
        dispatch: ContextMenuActionDispatch,
    },
    SetViewMode(ShellViewMode),
    CreateEntry {
        kind: CreateEntryKind,
        privileged: bool,
    },
    RunServiceMenuAction {
        action_id: String,
    },
    OpenWithApplication {
        desktop_id: String,
    },
    RedrawOnly,
}

pub(crate) fn context_menu_command_dispatch(
    command: ShellContextMenuCommand,
) -> ContextMenuCommandDispatch {
    match command {
        ShellContextMenuCommand::Builtin(action) => ContextMenuCommandDispatch::Action {
            action,
            dispatch: context_menu_action_dispatch(action),
        },
        ShellContextMenuCommand::SetViewMode(view_mode) => {
            ContextMenuCommandDispatch::SetViewMode(view_mode)
        }
        ShellContextMenuCommand::CreateEntry { kind, privileged } => {
            ContextMenuCommandDispatch::CreateEntry { kind, privileged }
        }
        ShellContextMenuCommand::RunServiceMenuAction { action_id } => {
            ContextMenuCommandDispatch::RunServiceMenuAction { action_id }
        }
        ShellContextMenuCommand::OpenWithApplication { desktop_id } => {
            ContextMenuCommandDispatch::OpenWithApplication { desktop_id }
        }
        ShellContextMenuCommand::OpenSubmenu(_) => ContextMenuCommandDispatch::RedrawOnly,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContextMenuActionDispatch {
    OpenWith,
    Refresh,
    ToggleHiddenFiles,
    OpenContextTargetInSplitPane,
    SelectAll,
    Properties,
    CreateNew,
    Rename { privileged: bool },
    AddToPlaces,
    AddNetworkFolder,
    RemovePlace,
    TrashView,
    MoveToTrash { privileged: bool },
    FileClipboard(FileClipboardRole),
    CopyLocation,
    Device,
    Paste { privileged: bool },
    Noop,
}

pub(crate) fn context_menu_action_dispatch(
    action: ShellContextMenuAction,
) -> ContextMenuActionDispatch {
    match action {
        ShellContextMenuAction::OpenWith => ContextMenuActionDispatch::OpenWith,
        ShellContextMenuAction::Refresh => ContextMenuActionDispatch::Refresh,
        ShellContextMenuAction::ToggleHiddenFiles => ContextMenuActionDispatch::ToggleHiddenFiles,
        ShellContextMenuAction::SplitPane | ShellContextMenuAction::OpenInNewPane => {
            ContextMenuActionDispatch::OpenContextTargetInSplitPane
        }
        ShellContextMenuAction::SelectAll => ContextMenuActionDispatch::SelectAll,
        ShellContextMenuAction::Properties => ContextMenuActionDispatch::Properties,
        ShellContextMenuAction::CreateNew => ContextMenuActionDispatch::CreateNew,
        ShellContextMenuAction::Rename => ContextMenuActionDispatch::Rename { privileged: false },
        ShellContextMenuAction::RenameAsAdministrator => {
            ContextMenuActionDispatch::Rename { privileged: true }
        }
        ShellContextMenuAction::AddToPlaces => ContextMenuActionDispatch::AddToPlaces,
        ShellContextMenuAction::AddNetworkFolder => ContextMenuActionDispatch::AddNetworkFolder,
        ShellContextMenuAction::RemovePlace => ContextMenuActionDispatch::RemovePlace,
        ShellContextMenuAction::RestoreFromTrash
        | ShellContextMenuAction::DeletePermanently
        | ShellContextMenuAction::EmptyTrash => ContextMenuActionDispatch::TrashView,
        ShellContextMenuAction::MoveToTrash => {
            ContextMenuActionDispatch::MoveToTrash { privileged: false }
        }
        ShellContextMenuAction::MoveToTrashAsAdministrator => {
            ContextMenuActionDispatch::MoveToTrash { privileged: true }
        }
        ShellContextMenuAction::Copy => {
            ContextMenuActionDispatch::FileClipboard(FileClipboardRole::Copy)
        }
        ShellContextMenuAction::Cut => {
            ContextMenuActionDispatch::FileClipboard(FileClipboardRole::Cut)
        }
        ShellContextMenuAction::CopyLocation => ContextMenuActionDispatch::CopyLocation,
        ShellContextMenuAction::MountDevice
        | ShellContextMenuAction::UnmountDevice
        | ShellContextMenuAction::EjectDevice
        | ShellContextMenuAction::SafelyRemoveDevice => ContextMenuActionDispatch::Device,
        ShellContextMenuAction::Paste => ContextMenuActionDispatch::Paste { privileged: false },
        ShellContextMenuAction::PasteAsAdministrator => {
            ContextMenuActionDispatch::Paste { privileged: true }
        }
        ShellContextMenuAction::ViewMode => ContextMenuActionDispatch::Noop,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileKeyboardCommandDispatch {
    Clipboard(FileClipboardRole),
    Paste,
    Rename,
    Delete,
}

pub(crate) fn file_keyboard_command_dispatch(
    command: FileKeyboardCommand,
) -> FileKeyboardCommandDispatch {
    match command {
        FileKeyboardCommand::Copy => {
            FileKeyboardCommandDispatch::Clipboard(FileClipboardRole::Copy)
        }
        FileKeyboardCommand::Cut => FileKeyboardCommandDispatch::Clipboard(FileClipboardRole::Cut),
        FileKeyboardCommand::Paste => FileKeyboardCommandDispatch::Paste,
        FileKeyboardCommand::Rename => FileKeyboardCommandDispatch::Rename,
        FileKeyboardCommand::Delete => FileKeyboardCommandDispatch::Delete,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menu_dispatch_groups_related_actions() {
        assert_eq!(
            context_menu_action_dispatch(ShellContextMenuAction::RenameAsAdministrator),
            ContextMenuActionDispatch::Rename { privileged: true }
        );
        assert_eq!(
            context_menu_action_dispatch(ShellContextMenuAction::MoveToTrash),
            ContextMenuActionDispatch::MoveToTrash { privileged: false }
        );
        assert_eq!(
            context_menu_action_dispatch(ShellContextMenuAction::EmptyTrash),
            ContextMenuActionDispatch::TrashView
        );
        assert_eq!(
            context_menu_action_dispatch(ShellContextMenuAction::Cut),
            ContextMenuActionDispatch::FileClipboard(FileClipboardRole::Cut)
        );
    }

    #[test]
    fn context_menu_command_dispatch_separates_plans_from_menu_shape() {
        assert_eq!(
            context_menu_command_dispatch(ShellContextMenuCommand::SetViewMode(
                ShellViewMode::Details
            )),
            ContextMenuCommandDispatch::SetViewMode(ShellViewMode::Details)
        );
        assert_eq!(
            context_menu_command_dispatch(ShellContextMenuCommand::CreateEntry {
                kind: CreateEntryKind::Folder,
                privileged: true,
            }),
            ContextMenuCommandDispatch::CreateEntry {
                kind: CreateEntryKind::Folder,
                privileged: true,
            }
        );
        assert_eq!(
            context_menu_command_dispatch(ShellContextMenuCommand::Builtin(
                ShellContextMenuAction::Copy
            )),
            ContextMenuCommandDispatch::Action {
                action: ShellContextMenuAction::Copy,
                dispatch: ContextMenuActionDispatch::FileClipboard(FileClipboardRole::Copy),
            }
        );
    }

    #[test]
    fn file_keyboard_dispatch_maps_clipboard_roles() {
        assert_eq!(
            file_keyboard_command_dispatch(FileKeyboardCommand::Copy),
            FileKeyboardCommandDispatch::Clipboard(FileClipboardRole::Copy)
        );
        assert_eq!(
            file_keyboard_command_dispatch(FileKeyboardCommand::Delete),
            FileKeyboardCommandDispatch::Delete
        );
    }
}
