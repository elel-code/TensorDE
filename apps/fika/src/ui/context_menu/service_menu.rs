use super::{
    ShellContextMenuAction, ShellContextMenuCommand, ShellContextMenuIcon, ShellContextMenuItem,
    ShellContextSubmenu,
};
use fika_core::{ServiceMenuAction, ServiceMenuPriority};

pub(super) fn service_menu_root_items(actions: &[ServiceMenuAction]) -> Vec<ShellContextMenuItem> {
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

pub(super) fn service_menu_has_more_actions(actions: &[ServiceMenuAction]) -> bool {
    let (ungrouped, groups) = service_menu_partition_grouped_actions(actions.iter().collect());
    ungrouped
        .into_iter()
        .any(|action| !service_menu_action_promoted(action, actions.len()))
        || groups
            .iter()
            .any(|(_, group_actions)| !service_menu_group_promoted(group_actions))
}

pub(super) fn service_menu_more_items(actions: &[ServiceMenuAction]) -> Vec<ShellContextMenuItem> {
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

pub(super) fn service_menu_group_items(
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
