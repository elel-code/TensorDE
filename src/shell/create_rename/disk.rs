use fika_core::{PrivilegedCommand, file_ops, run_operation_task};

use super::{CreateEntryKind, CreateEntryRequest, RenameEntryRequest};
use crate::shell::privilege::{ShellPrivilegeOutcome, run_privileged_command_sync};

pub(crate) fn create_entry_on_disk(request: &CreateEntryRequest) -> Result<(), String> {
    let path = request.path.clone();
    let kind = request.kind;
    // Dialog commit is currently synchronous; wait on the Compio worker instead of
    // inventing a second OS thread around pollster.
    futures_lite::future::block_on(run_operation_task(move || async move {
        match kind {
            CreateEntryKind::Folder => file_ops::create_folder_at_async(&path)
                .await
                .map_err(|error| format!("create folder {}: {error}", path.display())),
            CreateEntryKind::File => file_ops::create_file_at_async(&path)
                .await
                .map_err(|error| format!("create file {}: {error}", path.display())),
        }
    }))
    .map_err(|error| error.to_string())?
}

pub(crate) fn create_entry_on_disk_explicit(
    request: &CreateEntryRequest,
) -> Result<ShellPrivilegeOutcome, String> {
    if request.privileged {
        let command = match request.kind {
            CreateEntryKind::Folder => PrivilegedCommand::CreateFolder {
                parent: request.parent.clone(),
                name: request.name.clone(),
            },
            CreateEntryKind::File => PrivilegedCommand::CreateFile {
                parent: request.parent.clone(),
                name: request.name.clone(),
            },
        };
        run_privileged_command_sync(command)
    } else {
        create_entry_on_disk(request).map(|()| ShellPrivilegeOutcome::normal())
    }
}

pub(crate) fn rename_entry_on_disk(request: &RenameEntryRequest) -> Result<(), String> {
    let source = request.source.clone();
    let target = request.target.clone();
    futures_lite::future::block_on(run_operation_task(move || async move {
        file_ops::rename_path_to_async(&source, &target)
            .await
            .map_err(|error| {
                format!(
                    "rename {} to {}: {error}",
                    source.display(),
                    target.display()
                )
            })
    }))
    .map_err(|error| error.to_string())?
}

pub(crate) fn rename_entry_on_disk_explicit(
    request: &RenameEntryRequest,
) -> Result<ShellPrivilegeOutcome, String> {
    if request.privileged {
        run_privileged_command_sync(PrivilegedCommand::Rename {
            path: request.source.clone(),
            new_name: request.name.clone(),
        })
    } else {
        rename_entry_on_disk(request).map(|()| ShellPrivilegeOutcome::normal())
    }
}
