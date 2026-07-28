use std::fs;
use std::path::{Path, PathBuf};

use fika_core::{
    ArkCompressionMode, ServiceMenuAction, ServiceMenuPriority, ark_compress_launch_plan,
    ark_extract_and_trash_launch_plan, ark_extract_here_launch_plan, ark_extract_to_launch_plan,
    file_ops, is_archive_mime_or_path, is_network_path,
};

use crate::ui::service_menu::ServiceMenuLaunchRequest;

#[path = "ark/extract.rs"]
pub(crate) mod extract;

pub(crate) const BUILTIN_ARK_COMPRESS_TAR_GZ_ACTION_ID: &str = "fika.builtin.ark.compress-tar-gz";
pub(crate) const BUILTIN_ARK_COMPRESS_ZIP_ACTION_ID: &str = "fika.builtin.ark.compress-zip";
pub(crate) const BUILTIN_ARK_COMPRESS_ACTION_ID: &str = "fika.builtin.ark.compress";
pub(crate) const BUILTIN_ARK_EXTRACT_HERE_ACTION_ID: &str = "fika.builtin.ark.extract-here";
pub(crate) const BUILTIN_ARK_EXTRACT_AND_TRASH_ACTION_ID: &str =
    "fika.builtin.ark.extract-and-trash";
pub(crate) const BUILTIN_ARK_EXTRACT_TO_ACTION_ID: &str = "fika.builtin.ark.extract-to";
pub(crate) const BUILTIN_ARK_COMPRESS_SUBMENU: &str = "Compress";
pub(crate) const BUILTIN_ARK_EXTRACT_SUBMENU: &str = "Extract";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArkContextItem {
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) mime_type: Option<String>,
}

pub(crate) fn is_builtin_action(action_id: &str) -> bool {
    matches!(
        action_id,
        BUILTIN_ARK_COMPRESS_TAR_GZ_ACTION_ID
            | BUILTIN_ARK_COMPRESS_ZIP_ACTION_ID
            | BUILTIN_ARK_COMPRESS_ACTION_ID
            | BUILTIN_ARK_EXTRACT_HERE_ACTION_ID
            | BUILTIN_ARK_EXTRACT_AND_TRASH_ACTION_ID
            | BUILTIN_ARK_EXTRACT_TO_ACTION_ID
    )
}

pub(crate) fn is_extract_and_trash_action(action_id: &str) -> bool {
    action_id == BUILTIN_ARK_EXTRACT_AND_TRASH_ACTION_ID
}

pub(crate) fn append_builtin_service_actions(
    items: &[ArkContextItem],
    actions: &mut Vec<ServiceMenuAction>,
) {
    if items.is_empty() || !ark_context_items_are_local(items) {
        return;
    }

    if !ark_context_items_are_single_archive(items) {
        if ark_context_parent_writable(items) {
            push_builtin_ark_service_action(
                actions,
                BUILTIN_ARK_COMPRESS_TAR_GZ_ACTION_ID,
                &ark_compress_action_label(items, "tar.gz"),
                "archive-insert",
                Some(BUILTIN_ARK_COMPRESS_SUBMENU),
                ServiceMenuPriority::TopLevel,
            );
            push_builtin_ark_service_action(
                actions,
                BUILTIN_ARK_COMPRESS_ZIP_ACTION_ID,
                &ark_compress_action_label(items, "zip"),
                "archive-insert",
                Some(BUILTIN_ARK_COMPRESS_SUBMENU),
                ServiceMenuPriority::TopLevel,
            );
        }
        push_builtin_ark_service_action(
            actions,
            BUILTIN_ARK_COMPRESS_ACTION_ID,
            "Compress to…",
            "archive-insert",
            Some(BUILTIN_ARK_COMPRESS_SUBMENU),
            ServiceMenuPriority::TopLevel,
        );
    }

    let archive_items = ark_context_archive_items(items);
    if archive_items.is_empty() {
        return;
    }

    if ark_context_all_parents_writable(&archive_items) {
        push_builtin_ark_service_action(
            actions,
            BUILTIN_ARK_EXTRACT_HERE_ACTION_ID,
            "Extract here",
            "archive-extract",
            Some(BUILTIN_ARK_EXTRACT_SUBMENU),
            ServiceMenuPriority::TopLevel,
        );
        push_builtin_ark_service_action(
            actions,
            BUILTIN_ARK_EXTRACT_AND_TRASH_ACTION_ID,
            "Extract and trash archive",
            "archive-remove",
            Some(BUILTIN_ARK_EXTRACT_SUBMENU),
            ServiceMenuPriority::TopLevel,
        );
        push_builtin_ark_service_action(
            actions,
            BUILTIN_ARK_EXTRACT_TO_ACTION_ID,
            "Extract to…",
            "archive-extract",
            Some(BUILTIN_ARK_EXTRACT_SUBMENU),
            ServiceMenuPriority::TopLevel,
        );
    } else {
        push_builtin_ark_service_action(
            actions,
            BUILTIN_ARK_EXTRACT_TO_ACTION_ID,
            "Extract to…",
            "archive-extract",
            None,
            ServiceMenuPriority::TopLevel,
        );
    }
}

pub(crate) fn builtin_launch_request(
    action_id: &str,
    items: &[ArkContextItem],
) -> Result<Option<ServiceMenuLaunchRequest>, String> {
    match action_id {
        BUILTIN_ARK_COMPRESS_TAR_GZ_ACTION_ID
        | BUILTIN_ARK_COMPRESS_ZIP_ACTION_ID
        | BUILTIN_ARK_COMPRESS_ACTION_ID => {
            if !ark_context_items_are_local(items) {
                return Err("Ark actions require local file targets".to_string());
            }
            if ark_context_items_are_single_archive(items) {
                return Err("Compress is not available for a single archive".to_string());
            }
            let paths = items
                .iter()
                .map(|item| item.path.clone())
                .collect::<Vec<_>>();
            let mode = match action_id {
                BUILTIN_ARK_COMPRESS_TAR_GZ_ACTION_ID => ArkCompressionMode::TarGz,
                BUILTIN_ARK_COMPRESS_ZIP_ACTION_ID => ArkCompressionMode::Zip,
                _ => ArkCompressionMode::Dialog,
            };
            let plan = ark_compress_launch_plan(&paths, mode)?;
            Ok(Some(ServiceMenuLaunchRequest {
                paths,
                app_name: plan.app_name.clone(),
                plan,
            }))
        }
        BUILTIN_ARK_EXTRACT_HERE_ACTION_ID
        | BUILTIN_ARK_EXTRACT_AND_TRASH_ACTION_ID
        | BUILTIN_ARK_EXTRACT_TO_ACTION_ID => {
            if !ark_context_items_are_local(items) {
                return Err("Ark actions require local file targets".to_string());
            }
            let archive_items = ark_context_archive_items(items);
            if archive_items.is_empty() {
                return Err("Extract requires at least one archive".to_string());
            }
            let paths = archive_items
                .into_iter()
                .map(|item| item.path)
                .collect::<Vec<_>>();
            let plan = match action_id {
                BUILTIN_ARK_EXTRACT_HERE_ACTION_ID => ark_extract_here_launch_plan(&paths)?,
                BUILTIN_ARK_EXTRACT_AND_TRASH_ACTION_ID => {
                    ark_extract_and_trash_launch_plan(&paths)?
                }
                _ => ark_extract_to_launch_plan(&paths)?,
            };
            Ok(Some(ServiceMenuLaunchRequest {
                paths,
                app_name: plan.app_name.clone(),
                plan,
            }))
        }
        _ => Ok(None),
    }
}

fn ark_context_items_are_local(items: &[ArkContextItem]) -> bool {
    !items.is_empty()
        && items.iter().all(|item| {
            !file_ops::is_in_trash_files_dir(&item.path) && !is_network_path(&item.path)
        })
}

fn ark_context_item_is_archive(item: &ArkContextItem) -> bool {
    !item.is_dir && is_archive_mime_or_path(item.mime_type.as_deref(), &item.path)
}

fn ark_context_items_are_single_archive(items: &[ArkContextItem]) -> bool {
    matches!(items, [item] if ark_context_item_is_archive(item))
}

fn ark_context_archive_items(items: &[ArkContextItem]) -> Vec<ArkContextItem> {
    items
        .iter()
        .filter(|item| ark_context_item_is_archive(item))
        .cloned()
        .collect()
}

fn ark_context_parent_writable(items: &[ArkContextItem]) -> bool {
    items.first().is_none_or(ark_context_item_parent_writable)
}

fn ark_context_all_parents_writable(items: &[ArkContextItem]) -> bool {
    items.iter().all(ark_context_item_parent_writable)
}

fn ark_context_item_parent_writable(item: &ArkContextItem) -> bool {
    let parent = item
        .path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::metadata(parent)
        .map(|metadata| !metadata.permissions().readonly())
        .unwrap_or(true)
}

fn push_builtin_ark_service_action(
    actions: &mut Vec<ServiceMenuAction>,
    id: &str,
    label: &str,
    icon: &str,
    submenu: Option<&str>,
    priority: ServiceMenuPriority,
) {
    let label_key = service_action_label_key(label);
    let submenu_key = submenu.map(service_action_label_key);
    if actions.iter().any(|action| {
        service_action_label_key(&action.label) == label_key
            && action.submenu.as_deref().map(service_action_label_key) == submenu_key
    }) {
        return;
    }
    actions.push(ServiceMenuAction {
        id: id.to_string(),
        label: label.to_string(),
        source_name: "Ark".to_string(),
        icon: Some(icon.to_string()),
        submenu: submenu.map(str::to_string),
        priority,
    });
}

fn service_action_label_key(label: &str) -> String {
    label
        .bytes()
        .filter(|byte| byte.is_ascii_alphanumeric())
        .map(|byte| byte.to_ascii_lowercase() as char)
        .collect()
}

fn ark_compress_action_label(items: &[ArkContextItem], suffix: &str) -> String {
    format!("Compress to \"{}.{suffix}\"", ark_compress_base_name(items))
}

fn ark_compress_base_name(items: &[ArkContextItem]) -> String {
    match items {
        [item] => path_stem_or_name(&item.path).unwrap_or_else(|| "Archive".to_string()),
        [] => "Archive".to_string(),
        _ => ark_common_name_prefix(items).unwrap_or_else(|| "Archive".to_string()),
    }
}

fn ark_common_name_prefix(items: &[ArkContextItem]) -> Option<String> {
    let mut names = items
        .iter()
        .filter_map(|item| path_stem_or_name(&item.path))
        .collect::<Vec<_>>();
    let first = names.first()?.clone();
    let mut prefix_len = first.len();
    for name in names.drain(1..) {
        let common = first
            .chars()
            .zip(name.chars())
            .take_while(|(left, right)| left == right)
            .map(|(ch, _)| ch.len_utf8())
            .sum::<usize>();
        prefix_len = prefix_len.min(common);
    }
    let prefix = first[..prefix_len]
        .trim_matches(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '-' | '_' | '.'))
        .to_string();
    (prefix.len() >= 5).then_some(prefix)
}

fn path_stem_or_name(path: &Path) -> Option<String> {
    path.file_stem()
        .or_else(|| path.file_name())
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}
