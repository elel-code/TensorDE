use std::path::PathBuf;

use fika_core::{
    DevicePlaceOperation, MimeApplication, ServiceMenuAction, ServiceMenuPriority, ViewPoint,
    file_ops, is_network_path, is_network_root_path,
};

use crate::ui::create_rename::CreateEntryKind;
use crate::ui::options::ShellViewMode;
use crate::ui::pane::ShellPaneId;

#[path = "context_menu/paint.rs"]
pub(crate) mod paint;
#[path = "context_menu/safe_triangle.rs"]
pub(crate) mod safe_triangle;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellDevicePlace {
    pub(crate) id: String,
    pub(crate) mounted: bool,
    pub(crate) ejectable: bool,
    pub(crate) can_power_off: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShellContextTarget {
    Item {
        pane: ShellPaneId,
        index: usize,
        path: PathBuf,
        is_dir: bool,
        selection_count: usize,
    },
    Blank {
        pane: ShellPaneId,
        path: PathBuf,
    },
    Place {
        index: usize,
        label: String,
        path: PathBuf,
        group: &'static str,
        device: Option<ShellDevicePlace>,
        network: bool,
        trash: bool,
        root: bool,
        editable: bool,
    },
}

impl ShellContextTarget {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Item { .. } => "item",
            Self::Blank { .. } => "blank",
            Self::Place { .. } => "place",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShellContextMenuCommand {
    Builtin(ShellContextMenuAction),
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
    OpenSubmenu(ShellContextSubmenu),
}

impl ShellContextMenuCommand {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Builtin(action) => action.as_str(),
            Self::SetViewMode(view_mode) => view_mode.as_str(),
            Self::CreateEntry { kind, privileged } => {
                if *privileged {
                    kind.admin_as_str()
                } else {
                    kind.as_str()
                }
            }
            Self::RunServiceMenuAction { .. } => "run-service-menu-action",
            Self::OpenWithApplication { .. } => "open-with-application",
            Self::OpenSubmenu(submenu) => submenu.as_str(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellContextSubmenu {
    CreateNew,
    OpenWith,
    ServiceMenu,
    ServiceMenuGroup(usize),
    ViewMode,
}

impl ShellContextSubmenu {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::CreateNew => "submenu-create-new",
            Self::OpenWith => "submenu-open-with",
            Self::ServiceMenu => "submenu-service-menu",
            Self::ServiceMenuGroup(_) => "submenu-service-menu-group",
            Self::ViewMode => "submenu-view-mode",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellContextMenuItem {
    pub(crate) command: ShellContextMenuCommand,
    pub(crate) label: String,
    pub(crate) separator_before: bool,
    pub(crate) submenu: Option<ShellContextSubmenu>,
    pub(crate) icon: ShellContextMenuIcon,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShellContextMenuIcon {
    Builtin(ShellContextMenuAction),
    Service(Option<String>),
    Application(Option<String>),
}

impl ShellContextMenuItem {
    fn builtin(action: ShellContextMenuAction) -> Self {
        Self {
            command: ShellContextMenuCommand::Builtin(action),
            label: action.label().to_string(),
            separator_before: false,
            submenu: None,
            icon: ShellContextMenuIcon::Builtin(action),
        }
    }

    fn builtin_submenu(
        action: ShellContextMenuAction,
        label: impl Into<String>,
        submenu: ShellContextSubmenu,
    ) -> Self {
        Self {
            command: ShellContextMenuCommand::OpenSubmenu(submenu),
            label: label.into(),
            separator_before: false,
            submenu: Some(submenu),
            icon: ShellContextMenuIcon::Builtin(action),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellContextMenuAction {
    OpenWith,
    OpenInNewPane,
    SplitPane,
    Copy,
    Cut,
    CopyLocation,
    Rename,
    RenameAsAdministrator,
    MoveToTrash,
    MoveToTrashAsAdministrator,
    RestoreFromTrash,
    DeletePermanently,
    EmptyTrash,
    AddToPlaces,
    AddNetworkFolder,
    CreateNew,
    Paste,
    PasteAsAdministrator,
    SelectAll,
    ViewMode,
    ToggleHiddenFiles,
    Refresh,
    Properties,
    RemovePlace,
    MountDevice,
    UnmountDevice,
    EjectDevice,
    SafelyRemoveDevice,
}

impl ShellContextMenuAction {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::OpenWith => "Open With",
            Self::OpenInNewPane => "Open in New Pane",
            Self::SplitPane => "Split View",
            Self::Copy => "Copy",
            Self::Cut => "Cut",
            Self::CopyLocation => "Copy Location",
            Self::Rename => "Rename",
            Self::RenameAsAdministrator => "Rename as Administrator",
            Self::MoveToTrash => "Move to Trash",
            Self::MoveToTrashAsAdministrator => "Move to Trash as Administrator",
            Self::RestoreFromTrash => "Restore to Former Location",
            Self::DeletePermanently => "Delete Permanently",
            Self::EmptyTrash => "Empty Trash",
            Self::AddToPlaces => "Add to Places",
            Self::AddNetworkFolder => "Add Network Folder...",
            Self::CreateNew => "Create New",
            Self::Paste => "Paste",
            Self::PasteAsAdministrator => "Paste as Administrator",
            Self::SelectAll => "Select All",
            Self::ViewMode => "View Mode",
            Self::ToggleHiddenFiles => "Show Hidden Files",
            Self::Refresh => "Refresh",
            Self::Properties => "Properties",
            Self::RemovePlace => "Remove",
            Self::MountDevice => "Mount",
            Self::UnmountDevice => "Unmount",
            Self::EjectDevice => "Eject",
            Self::SafelyRemoveDevice => "Safely Remove",
        }
    }

    pub(crate) fn label_for_hidden_state(self, show_hidden: bool) -> &'static str {
        match (self, show_hidden) {
            (Self::ToggleHiddenFiles, true) => "Hide Hidden Files",
            (Self::ToggleHiddenFiles, false) => "Show Hidden Files",
            _ => self.label(),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::OpenWith => "open-with",
            Self::OpenInNewPane => "open-in-new-pane",
            Self::SplitPane => "split-pane",
            Self::Copy => "copy",
            Self::Cut => "cut",
            Self::CopyLocation => "copy-location",
            Self::Rename => "rename",
            Self::RenameAsAdministrator => "rename-as-administrator",
            Self::MoveToTrash => "move-to-trash",
            Self::MoveToTrashAsAdministrator => "move-to-trash-as-administrator",
            Self::RestoreFromTrash => "restore-from-trash",
            Self::DeletePermanently => "delete-permanently",
            Self::EmptyTrash => "empty-trash",
            Self::AddToPlaces => "add-to-places",
            Self::AddNetworkFolder => "add-network-folder",
            Self::CreateNew => "create-new",
            Self::Paste => "paste",
            Self::PasteAsAdministrator => "paste-as-administrator",
            Self::SelectAll => "select-all",
            Self::ViewMode => "view-mode",
            Self::ToggleHiddenFiles => "toggle-hidden-files",
            Self::Refresh => "refresh",
            Self::Properties => "properties",
            Self::RemovePlace => "remove-place",
            Self::MountDevice => "mount-device",
            Self::UnmountDevice => "unmount-device",
            Self::EjectDevice => "eject-device",
            Self::SafelyRemoveDevice => "safely-remove-device",
        }
    }
}

pub(crate) fn device_place_operation_for_context_action(
    action: ShellContextMenuAction,
) -> Option<DevicePlaceOperation> {
    match action {
        ShellContextMenuAction::MountDevice => Some(DevicePlaceOperation::Mount),
        ShellContextMenuAction::UnmountDevice => Some(DevicePlaceOperation::Unmount),
        ShellContextMenuAction::EjectDevice => Some(DevicePlaceOperation::Eject),
        ShellContextMenuAction::SafelyRemoveDevice => Some(DevicePlaceOperation::SafelyRemove),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ShellContextMenu {
    pub(crate) target: ShellContextTarget,
    pub(crate) position: ViewPoint,
    pub(crate) open_with_apps: Vec<MimeApplication>,
    pub(crate) service_actions: Vec<ServiceMenuAction>,
    pub(crate) hovered_row: Option<usize>,
    pub(crate) active_submenu: Option<ShellContextSubmenu>,
    pub(crate) active_submenu_row: Option<usize>,
    pub(crate) hovered_submenu_row: Option<usize>,
}

impl ShellContextMenu {
    #[cfg(test)]
    pub(crate) fn new(target: ShellContextTarget, position: ViewPoint) -> Self {
        Self {
            target,
            position,
            open_with_apps: Vec::new(),
            service_actions: Vec::new(),
            hovered_row: None,
            active_submenu: None,
            active_submenu_row: None,
            hovered_submenu_row: None,
        }
    }

    pub(crate) fn with_dynamic(
        target: ShellContextTarget,
        position: ViewPoint,
        open_with_apps: Vec<MimeApplication>,
        service_actions: Vec<ServiceMenuAction>,
    ) -> Self {
        Self {
            target,
            position,
            open_with_apps,
            service_actions,
            hovered_row: None,
            active_submenu: None,
            active_submenu_row: None,
            hovered_submenu_row: None,
        }
    }
}

fn context_menu_builtin_actions(target: &ShellContextTarget) -> Vec<ShellContextMenuAction> {
    const ITEM_FILE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::OpenWith,
        ShellContextMenuAction::OpenInNewPane,
        ShellContextMenuAction::Copy,
        ShellContextMenuAction::Cut,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Rename,
        ShellContextMenuAction::RenameAsAdministrator,
        ShellContextMenuAction::MoveToTrash,
        ShellContextMenuAction::MoveToTrashAsAdministrator,
        ShellContextMenuAction::Properties,
    ];
    const ITEM_DIR_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::OpenInNewPane,
        ShellContextMenuAction::OpenWith,
        ShellContextMenuAction::AddToPlaces,
        ShellContextMenuAction::Copy,
        ShellContextMenuAction::Cut,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Rename,
        ShellContextMenuAction::RenameAsAdministrator,
        ShellContextMenuAction::MoveToTrash,
        ShellContextMenuAction::MoveToTrashAsAdministrator,
        ShellContextMenuAction::Properties,
    ];
    const NETWORK_ITEM_FILE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::OpenWith,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Properties,
    ];
    const NETWORK_ITEM_DIR_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::OpenInNewPane,
        ShellContextMenuAction::AddToPlaces,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Properties,
    ];
    const TRASH_ITEM_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::RestoreFromTrash,
        ShellContextMenuAction::Copy,
        ShellContextMenuAction::DeletePermanently,
        ShellContextMenuAction::Properties,
    ];
    const BLANK_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::CreateNew,
        ShellContextMenuAction::OpenWith,
        ShellContextMenuAction::AddToPlaces,
        ShellContextMenuAction::Paste,
        ShellContextMenuAction::PasteAsAdministrator,
        ShellContextMenuAction::SelectAll,
        ShellContextMenuAction::ViewMode,
        ShellContextMenuAction::ToggleHiddenFiles,
        ShellContextMenuAction::SplitPane,
        ShellContextMenuAction::Refresh,
        ShellContextMenuAction::Properties,
    ];
    const NETWORK_BLANK_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::AddToPlaces,
        ShellContextMenuAction::SelectAll,
        ShellContextMenuAction::ViewMode,
        ShellContextMenuAction::ToggleHiddenFiles,
        ShellContextMenuAction::SplitPane,
        ShellContextMenuAction::Refresh,
        ShellContextMenuAction::Properties,
    ];
    const NETWORK_ROOT_BLANK_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::AddNetworkFolder,
        ShellContextMenuAction::SelectAll,
        ShellContextMenuAction::ViewMode,
        ShellContextMenuAction::ToggleHiddenFiles,
        ShellContextMenuAction::SplitPane,
        ShellContextMenuAction::Refresh,
        ShellContextMenuAction::Properties,
    ];
    const TRASH_BLANK_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::EmptyTrash,
        ShellContextMenuAction::SelectAll,
        ShellContextMenuAction::Refresh,
        ShellContextMenuAction::Properties,
    ];
    const PLACE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::OpenInNewPane,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Properties,
    ];
    const NETWORK_ROOT_PLACE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::OpenInNewPane,
        ShellContextMenuAction::AddNetworkFolder,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Properties,
    ];
    const TRASH_PLACE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::OpenInNewPane,
        ShellContextMenuAction::EmptyTrash,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::Properties,
    ];
    const EDITABLE_PLACE_ACTIONS: &[ShellContextMenuAction] = &[
        ShellContextMenuAction::OpenInNewPane,
        ShellContextMenuAction::CopyLocation,
        ShellContextMenuAction::RemovePlace,
        ShellContextMenuAction::Properties,
    ];
    match target {
        ShellContextTarget::Item { path, .. } if file_ops::is_in_trash_files_dir(path) => {
            TRASH_ITEM_ACTIONS.to_vec()
        }
        ShellContextTarget::Item {
            path, is_dir: true, ..
        } if is_network_path(path) => NETWORK_ITEM_DIR_ACTIONS.to_vec(),
        ShellContextTarget::Item { path, .. } if is_network_path(path) => {
            NETWORK_ITEM_FILE_ACTIONS.to_vec()
        }
        ShellContextTarget::Item { is_dir: true, .. } => ITEM_DIR_ACTIONS.to_vec(),
        ShellContextTarget::Item { .. } => ITEM_FILE_ACTIONS.to_vec(),
        ShellContextTarget::Blank { path, .. } if file_ops::is_trash_files_dir(path) => {
            TRASH_BLANK_ACTIONS.to_vec()
        }
        ShellContextTarget::Blank { path, .. } if is_network_root_path(path) => {
            NETWORK_ROOT_BLANK_ACTIONS.to_vec()
        }
        ShellContextTarget::Blank { path, .. } if is_network_path(path) => {
            NETWORK_BLANK_ACTIONS.to_vec()
        }
        ShellContextTarget::Blank { .. } => BLANK_ACTIONS.to_vec(),
        ShellContextTarget::Place { trash: true, .. } => TRASH_PLACE_ACTIONS.to_vec(),
        ShellContextTarget::Place {
            network: true,
            path,
            ..
        } if is_network_root_path(path) => NETWORK_ROOT_PLACE_ACTIONS.to_vec(),
        ShellContextTarget::Place {
            device: Some(device),
            ..
        } => {
            let mut actions = Vec::new();
            if device.mounted {
                actions.extend([
                    ShellContextMenuAction::OpenInNewPane,
                    ShellContextMenuAction::CopyLocation,
                    ShellContextMenuAction::UnmountDevice,
                ]);
            } else {
                actions.push(ShellContextMenuAction::MountDevice);
            }
            if device.ejectable {
                actions.push(ShellContextMenuAction::EjectDevice);
            }
            if device.can_power_off {
                actions.push(ShellContextMenuAction::SafelyRemoveDevice);
            }
            actions.push(ShellContextMenuAction::Properties);
            actions
        }
        ShellContextTarget::Place { editable: true, .. } => EDITABLE_PLACE_ACTIONS.to_vec(),
        ShellContextTarget::Place { .. } => PLACE_ACTIONS.to_vec(),
    }
}

pub(crate) fn context_menu_items(menu: &ShellContextMenu) -> Vec<ShellContextMenuItem> {
    context_menu_items_for_target(&menu.target, &menu.open_with_apps, &menu.service_actions)
}

fn context_menu_items_for_target(
    target: &ShellContextTarget,
    open_with_apps: &[MimeApplication],
    service_actions: &[ServiceMenuAction],
) -> Vec<ShellContextMenuItem> {
    let mut items = context_menu_builtin_actions(target)
        .iter()
        .copied()
        .map(|action| {
            let mut item = match action {
                ShellContextMenuAction::OpenWith => ShellContextMenuItem::builtin_submenu(
                    action,
                    action.label(),
                    ShellContextSubmenu::OpenWith,
                ),
                ShellContextMenuAction::CreateNew => ShellContextMenuItem::builtin_submenu(
                    action,
                    action.label(),
                    ShellContextSubmenu::CreateNew,
                ),
                ShellContextMenuAction::ViewMode => ShellContextMenuItem::builtin_submenu(
                    action,
                    action.label(),
                    ShellContextSubmenu::ViewMode,
                ),
                _ => ShellContextMenuItem::builtin(action),
            };
            item.separator_before = context_menu_separator_before_builtin(target, action);
            item
        })
        .collect::<Vec<_>>();

    if let Some(insert_at) = items
        .iter()
        .position(|item| item.submenu == Some(ShellContextSubmenu::OpenWith))
    {
        let app_items = root_open_with_application_items(open_with_apps);
        items.splice(insert_at..insert_at, app_items);
    }

    if !service_actions.is_empty() {
        let insert_at = items
            .iter()
            .position(|item| {
                matches!(
                    item.command,
                    ShellContextMenuCommand::Builtin(ShellContextMenuAction::Copy)
                        | ShellContextMenuCommand::Builtin(ShellContextMenuAction::SelectAll)
                )
            })
            .unwrap_or(items.len());
        let mut service_items = service_menu_root_items(service_actions);
        if service_menu_has_more_actions(service_actions) {
            let mut more = ShellContextMenuItem::builtin_submenu(
                ShellContextMenuAction::Properties,
                "More Actions",
                ShellContextSubmenu::ServiceMenu,
            );
            more.icon = ShellContextMenuIcon::Service(None);
            more.separator_before = service_items.is_empty();
            service_items.push(more);
        }
        if !service_items.is_empty() {
            if let Some(first) = service_items.first_mut() {
                first.separator_before = true;
            }
            items.splice(insert_at..insert_at, service_items);
        }
    }

    items
}

#[cfg(test)]
pub(crate) fn context_menu_actions(target: &ShellContextTarget) -> Vec<ShellContextMenuAction> {
    context_menu_items_for_target(target, &[], &[])
        .into_iter()
        .filter_map(|item| match (&item.command, &item.icon) {
            (ShellContextMenuCommand::Builtin(action), _) => Some(*action),
            (_, ShellContextMenuIcon::Builtin(action)) => Some(*action),
            _ => None,
        })
        .collect()
}

const ROOT_OPEN_WITH_APP_LIMIT: usize = 2;

fn root_open_with_application_items(
    open_with_apps: &[MimeApplication],
) -> Vec<ShellContextMenuItem> {
    let has_default = open_with_apps.iter().any(|app| app.is_default);

    open_with_apps
        .iter()
        .filter(|app| !has_default || !app.is_default)
        .take(ROOT_OPEN_WITH_APP_LIMIT)
        .map(|app| {
            let name = app.name.trim();
            let name = if name.is_empty() {
                app.id.as_str()
            } else {
                name
            };
            ShellContextMenuItem {
                command: ShellContextMenuCommand::OpenWithApplication {
                    desktop_id: app.id.clone(),
                },
                label: format!("Open With {name}"),
                separator_before: false,
                submenu: None,
                icon: ShellContextMenuIcon::Application(app.icon.clone()),
            }
        })
        .collect()
}

pub(crate) fn context_submenu_actions(
    submenu: ShellContextSubmenu,
    menu: &ShellContextMenu,
) -> Vec<ShellContextMenuItem> {
    match submenu {
        ShellContextSubmenu::CreateNew => vec![
            ShellContextMenuItem {
                command: ShellContextMenuCommand::CreateEntry {
                    kind: CreateEntryKind::Folder,
                    privileged: false,
                },
                label: "Folder".to_string(),
                separator_before: false,
                submenu: None,
                icon: ShellContextMenuIcon::Builtin(ShellContextMenuAction::CreateNew),
            },
            ShellContextMenuItem {
                command: ShellContextMenuCommand::CreateEntry {
                    kind: CreateEntryKind::File,
                    privileged: false,
                },
                label: "Text File".to_string(),
                separator_before: false,
                submenu: None,
                icon: ShellContextMenuIcon::Builtin(ShellContextMenuAction::CreateNew),
            },
            ShellContextMenuItem {
                command: ShellContextMenuCommand::CreateEntry {
                    kind: CreateEntryKind::Folder,
                    privileged: true,
                },
                label: "Folder as Administrator".to_string(),
                separator_before: true,
                submenu: None,
                icon: ShellContextMenuIcon::Builtin(ShellContextMenuAction::CreateNew),
            },
            ShellContextMenuItem {
                command: ShellContextMenuCommand::CreateEntry {
                    kind: CreateEntryKind::File,
                    privileged: true,
                },
                label: "Text File as Administrator".to_string(),
                separator_before: false,
                submenu: None,
                icon: ShellContextMenuIcon::Builtin(ShellContextMenuAction::CreateNew),
            },
        ],
        ShellContextSubmenu::OpenWith => {
            let apps = menu.open_with_apps.as_slice();
            if apps.is_empty() {
                return vec![ShellContextMenuItem {
                    command: ShellContextMenuCommand::Builtin(ShellContextMenuAction::OpenWith),
                    label: "Other Application...".to_string(),
                    separator_before: false,
                    submenu: None,
                    icon: ShellContextMenuIcon::Builtin(ShellContextMenuAction::OpenWith),
                }];
            }
            let mut items = apps
                .iter()
                .take(12)
                .map(|app| ShellContextMenuItem {
                    command: ShellContextMenuCommand::OpenWithApplication {
                        desktop_id: app.id.clone(),
                    },
                    label: if app.is_default {
                        format!("{} (default)", app.name)
                    } else {
                        app.name.clone()
                    },
                    separator_before: false,
                    submenu: None,
                    icon: ShellContextMenuIcon::Application(app.icon.clone()),
                })
                .collect::<Vec<_>>();
            items.push(ShellContextMenuItem {
                command: ShellContextMenuCommand::Builtin(ShellContextMenuAction::OpenWith),
                label: "Other Application...".to_string(),
                separator_before: !items.is_empty(),
                submenu: None,
                icon: ShellContextMenuIcon::Builtin(ShellContextMenuAction::OpenWith),
            });
            items
        }
        ShellContextSubmenu::ServiceMenu => {
            let mut items = service_menu_more_items(&menu.service_actions);
            if items.is_empty() {
                items.push(ShellContextMenuItem {
                    command: ShellContextMenuCommand::OpenSubmenu(ShellContextSubmenu::ServiceMenu),
                    label: "No Actions".to_string(),
                    separator_before: false,
                    submenu: None,
                    icon: ShellContextMenuIcon::Service(None),
                });
            }
            items
        }
        ShellContextSubmenu::ServiceMenuGroup(group_index) => {
            let mut items = service_menu_group_items(&menu.service_actions, group_index);
            if items.is_empty() {
                items.push(ShellContextMenuItem {
                    command: ShellContextMenuCommand::OpenSubmenu(
                        ShellContextSubmenu::ServiceMenuGroup(group_index),
                    ),
                    label: "No Actions".to_string(),
                    separator_before: false,
                    submenu: None,
                    icon: ShellContextMenuIcon::Service(None),
                });
            }
            items
        }
        ShellContextSubmenu::ViewMode => [
            (ShellViewMode::Icons, "Icons"),
            (ShellViewMode::Compact, "Compact"),
            (ShellViewMode::Details, "Details"),
        ]
        .into_iter()
        .map(|(view_mode, label)| ShellContextMenuItem {
            command: ShellContextMenuCommand::SetViewMode(view_mode),
            label: label.to_string(),
            separator_before: false,
            submenu: None,
            icon: ShellContextMenuIcon::Builtin(ShellContextMenuAction::ViewMode),
        })
        .collect(),
    }
}

fn context_menu_separator_before_builtin(
    target: &ShellContextTarget,
    action: ShellContextMenuAction,
) -> bool {
    let Some(row) = context_menu_builtin_actions(target)
        .iter()
        .position(|candidate| *candidate == action)
    else {
        return false;
    };
    context_menu_separator_before(target, row)
}

pub(crate) fn context_menu_separator_before(target: &ShellContextTarget, row: usize) -> bool {
    let Some(action) = context_menu_builtin_actions(target).get(row).copied() else {
        return false;
    };
    match target {
        ShellContextTarget::Item { path, .. } if file_ops::is_in_trash_files_dir(path) => {
            action == ShellContextMenuAction::Properties
        }
        ShellContextTarget::Item { .. } => {
            action == ShellContextMenuAction::Copy
                || action == ShellContextMenuAction::Rename
                || action == ShellContextMenuAction::Properties
        }
        ShellContextTarget::Blank { path, .. } if file_ops::is_trash_files_dir(path) => {
            action == ShellContextMenuAction::SelectAll
                || action == ShellContextMenuAction::Properties
        }
        ShellContextTarget::Blank { .. } => {
            action == ShellContextMenuAction::Paste
                || action == ShellContextMenuAction::SelectAll
                || action == ShellContextMenuAction::ViewMode
                || action == ShellContextMenuAction::Properties
        }
        ShellContextTarget::Place {
            device: Some(_), ..
        } => matches!(
            action,
            ShellContextMenuAction::MountDevice
                | ShellContextMenuAction::UnmountDevice
                | ShellContextMenuAction::Properties
        ),
        ShellContextTarget::Place {
            network: true,
            path,
            ..
        } if is_network_root_path(path) => {
            action == ShellContextMenuAction::AddNetworkFolder
                || action == ShellContextMenuAction::Properties
        }
        ShellContextTarget::Place { .. } => action == ShellContextMenuAction::Properties,
    }
}

fn service_menu_root_items(actions: &[ServiceMenuAction]) -> Vec<ShellContextMenuItem> {
    let (ungrouped, groups) = service_menu_partition_grouped_actions(actions.iter().collect());
    let mut items = ungrouped
        .into_iter()
        .filter(|action| service_menu_action_promoted(action, actions.len()))
        .map(service_menu_action_item)
        .collect::<Vec<_>>();
    for (group_index, (label, group_actions)) in groups.iter().enumerate() {
        if service_menu_group_promoted(group_actions) {
            items.push(service_menu_group_submenu_item(label, group_index));
        }
    }
    items
}

fn service_menu_has_more_actions(actions: &[ServiceMenuAction]) -> bool {
    let (ungrouped, groups) = service_menu_partition_grouped_actions(actions.iter().collect());
    ungrouped
        .into_iter()
        .any(|action| !service_menu_action_promoted(action, actions.len()))
        || groups
            .iter()
            .any(|(_, group_actions)| !service_menu_group_promoted(group_actions))
}

fn service_menu_more_items(actions: &[ServiceMenuAction]) -> Vec<ShellContextMenuItem> {
    let (ungrouped, groups) = service_menu_partition_grouped_actions(actions.iter().collect());
    let mut items = ungrouped
        .into_iter()
        .filter(|action| !service_menu_action_promoted(action, actions.len()))
        .map(service_menu_action_item)
        .collect::<Vec<_>>();
    let mut appended_group = false;
    for (group_index, (label, group_actions)) in groups.iter().enumerate() {
        if service_menu_group_promoted(group_actions) {
            continue;
        }
        let mut item = service_menu_group_submenu_item(label, group_index);
        item.separator_before = !items.is_empty() && !appended_group;
        appended_group = true;
        items.push(item);
    }
    items
}

fn service_menu_group_items(
    actions: &[ServiceMenuAction],
    group_index: usize,
) -> Vec<ShellContextMenuItem> {
    let (_, groups) = service_menu_partition_grouped_actions(actions.iter().collect());
    groups
        .into_iter()
        .nth(group_index)
        .map(|(_, group_actions)| {
            group_actions
                .into_iter()
                .map(service_menu_action_item)
                .collect()
        })
        .unwrap_or_default()
}

fn service_menu_partition_grouped_actions(
    actions: Vec<&ServiceMenuAction>,
) -> (
    Vec<&ServiceMenuAction>,
    Vec<(String, Vec<&ServiceMenuAction>)>,
) {
    let mut grouped: Vec<(String, Vec<&ServiceMenuAction>)> = Vec::new();
    let ungrouped = actions
        .iter()
        .copied()
        .filter(|action| action.submenu.is_none())
        .collect::<Vec<_>>();
    for action in actions
        .into_iter()
        .filter(|action| action.submenu.is_some())
    {
        let group = action.submenu.as_deref().unwrap_or_default().to_string();
        if let Some((_, group_actions)) = grouped
            .iter_mut()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(&group))
        {
            group_actions.push(action);
        } else {
            grouped.push((group, vec![action]));
        }
    }
    (ungrouped, grouped)
}

fn service_menu_group_promoted(actions: &[&ServiceMenuAction]) -> bool {
    actions
        .iter()
        .any(|action| action.priority == ServiceMenuPriority::TopLevel)
}

fn service_menu_group_submenu_item(label: &str, group_index: usize) -> ShellContextMenuItem {
    let mut item = ShellContextMenuItem::builtin_submenu(
        ShellContextMenuAction::Properties,
        label.to_string(),
        ShellContextSubmenu::ServiceMenuGroup(group_index),
    );
    item.command =
        ShellContextMenuCommand::OpenSubmenu(ShellContextSubmenu::ServiceMenuGroup(group_index));
    item.icon = ShellContextMenuIcon::Service(None);
    item
}

fn service_menu_action_promoted(action: &ServiceMenuAction, action_count: usize) -> bool {
    if action.priority == ServiceMenuPriority::TopLevel {
        return true;
    }
    if action.submenu.is_some() {
        return false;
    }
    if action_count <= 4 {
        return true;
    }
    let label = action.label.to_ascii_lowercase();
    [
        "compress", "extract", "archive", "terminal", "send to", "copy to", "move to",
    ]
    .iter()
    .any(|keyword| label.contains(keyword))
}

pub(crate) fn service_menu_action_item(action: &ServiceMenuAction) -> ShellContextMenuItem {
    ShellContextMenuItem {
        command: ShellContextMenuCommand::RunServiceMenuAction {
            action_id: action.id.clone(),
        },
        label: action.label.clone(),
        separator_before: false,
        submenu: None,
        icon: ShellContextMenuIcon::Service(action.icon.clone()),
    }
}
