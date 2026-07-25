use fika_core::{PrivilegedCommand, file_ops};

use super::{CreateEntryKind, CreateEntryRequest, RenameEntryRequest};
use crate::shell::privilege::{ShellPrivilegeOutcome, run_privileged_command};

pub(crate) async fn create_entry_on_disk_async(request: CreateEntryRequest) -> Result<(), String> {
    let path = request.path;
    match request.kind {
        CreateEntryKind::Folder => file_ops::create_folder_at_async(&path)
            .await
            .map_err(|error| format!("create folder {}: {error}", path.display())),
        CreateEntryKind::File => file_ops::create_file_at_async(&path)
            .await
            .map_err(|error| format!("create file {}: {error}", path.display())),
    }
}

pub(crate) async fn create_entry_on_disk_explicit_async(
    request: CreateEntryRequest,
) -> Result<ShellPrivilegeOutcome, String> {
    if request.privileged {
        let command = match request.kind {
            CreateEntryKind::Folder => PrivilegedCommand::CreateFolder {
                parent: request.parent,
                name: request.name,
            },
            CreateEntryKind::File => PrivilegedCommand::CreateFile {
                parent: request.parent,
                name: request.name,
            },
        };
        run_privileged_command(command).await
    } else {
        create_entry_on_disk_async(request)
            .await
            .map(|()| ShellPrivilegeOutcome::normal())
    }
}

#[cfg(test)]
pub(crate) fn create_entry_on_disk(request: &CreateEntryRequest) -> Result<(), String> {
    use fika_core::run_operation_task;
    let request = request.clone();
    // Tests wait on the Compio worker; production UI uses the async completion path.
    futures_lite::future::block_on(run_operation_task(move || async move {
        create_entry_on_disk_async(request).await
    }))
    .map_err(|error| error.to_string())?
}

pub(crate) async fn rename_entry_on_disk_async(request: RenameEntryRequest) -> Result<(), String> {
    let source = request.source;
    let target = request.target;
    file_ops::rename_path_to_async(&source, &target)
        .await
        .map_err(|error| {
            format!(
                "rename {} to {}: {error}",
                source.display(),
                target.display()
            )
        })
}

pub(crate) async fn rename_entry_on_disk_explicit_async(
    request: RenameEntryRequest,
) -> Result<ShellPrivilegeOutcome, String> {
    if request.privileged {
        run_privileged_command(PrivilegedCommand::Rename {
            path: request.source,
            new_name: request.name,
        })
        .await
    } else {
        rename_entry_on_disk_async(request)
            .await
            .map(|()| ShellPrivilegeOutcome::normal())
    }
}

#[cfg(test)]
pub(crate) fn rename_entry_on_disk(request: &RenameEntryRequest) -> Result<(), String> {
    use fika_core::run_operation_task;
    let request = request.clone();
    futures_lite::future::block_on(run_operation_task(move || async move {
        rename_entry_on_disk_async(request).await
    }))
    .map_err(|error| error.to_string())?
}
