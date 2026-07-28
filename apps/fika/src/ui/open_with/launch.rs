use std::sync::Arc;

use fika_core::{MimeApplicationCache, file_ops};

use crate::ui::context_menu::ShellContextTarget;
use crate::ui::open_with::{
    OpenWithDefaultUpdate, OpenWithLaunchRequest, ShellOpenWithChooser,
    open_with_application_categories_for_applications, open_with_applications_for_mime,
};

pub(crate) fn chooser_for_context_target(
    target: &ShellContextTarget,
    item_mime_type: Option<Arc<str>>,
    cache: &MimeApplicationCache,
) -> Result<ShellOpenWithChooser, String> {
    let (path, mime_type) = match target {
        ShellContextTarget::Item { path, is_dir, .. } => {
            if file_ops::is_in_trash_files_dir(path) {
                return Err("Open With is not available inside Trash".to_string());
            }
            let mime_type = if *is_dir {
                item_mime_type.or_else(|| Some(Arc::from("inode/directory")))
            } else {
                item_mime_type
            };
            (path.clone(), mime_type)
        }
        ShellContextTarget::Blank { path, .. } => {
            if file_ops::is_trash_files_dir(path) {
                return Err("Open With is not available inside Trash".to_string());
            }
            (path.clone(), Some(Arc::from("inode/directory")))
        }
        ShellContextTarget::Place { .. } => {
            return Err(format!(
                "target={} is not a file or folder target",
                target.kind()
            ));
        }
    };
    let applications = open_with_applications_for_mime(cache, mime_type.as_deref());
    if applications.is_empty() {
        return Err("no desktop applications found".to_string());
    }
    let application_categories =
        open_with_application_categories_for_applications(cache, &applications);
    Ok(ShellOpenWithChooser::new(
        path,
        mime_type,
        applications,
        application_categories,
    ))
}

pub(crate) fn launch_request_for_chooser(
    chooser: &ShellOpenWithChooser,
    cache: &MimeApplicationCache,
) -> Result<OpenWithLaunchRequest, String> {
    let selected = chooser
        .selected_application()
        .ok_or_else(|| "no application is selected".to_string())?;
    let app = cache
        .application(&selected.id)
        .ok_or_else(|| format!("application not found: {}", selected.id))?;
    let plan = app
        .launch_plan(std::slice::from_ref(&chooser.path))
        .ok_or_else(|| format!("{} did not produce a launch command", app.name))?;
    let default_update = if chooser.set_as_default && !selected.is_default {
        let mime_type = chooser
            .mime_type
            .as_deref()
            .ok_or("cannot set a default application for an unknown MIME type")?
            .to_string();
        Some(OpenWithDefaultUpdate {
            mime_type,
            desktop_id: selected.id.clone(),
        })
    } else {
        None
    };
    Ok(OpenWithLaunchRequest {
        path: chooser.path.clone(),
        app_name: plan.app_name.clone(),
        default_update,
        plan,
    })
}

pub(crate) fn launch_request_for_context_application(
    target: &ShellContextTarget,
    cache: &MimeApplicationCache,
    desktop_id: &str,
) -> Result<OpenWithLaunchRequest, String> {
    let path = match target {
        ShellContextTarget::Item { path, .. } if !file_ops::is_in_trash_files_dir(path) => {
            path.clone()
        }
        ShellContextTarget::Blank { path, .. } => path.clone(),
        _ => return Err("Open With application requires a file or folder target".to_string()),
    };
    let app = cache
        .application(desktop_id)
        .ok_or_else(|| format!("application not found: {desktop_id}"))?;
    let plan = app
        .launch_plan(std::slice::from_ref(&path))
        .ok_or_else(|| format!("{} did not produce a launch command", app.name))?;
    Ok(OpenWithLaunchRequest {
        path,
        app_name: plan.app_name.clone(),
        default_update: None,
        plan,
    })
}
