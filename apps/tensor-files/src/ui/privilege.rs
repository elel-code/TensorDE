use tensor_files_core::{PrivilegedCommand, run_via_dbus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellPrivilegeOutcome {
    pub(crate) privileged: bool,
    pub(crate) message: Option<String>,
}

impl ShellPrivilegeOutcome {
    pub(crate) fn normal() -> Self {
        Self {
            privileged: false,
            message: None,
        }
    }

    fn privileged(message: String) -> Self {
        Self {
            privileged: true,
            message: Some(message),
        }
    }
}

pub(crate) async fn run_privileged_command(
    command: PrivilegedCommand,
) -> Result<ShellPrivilegeOutcome, String> {
    let result = run_via_dbus(command).await;
    result
        .result
        .map(ShellPrivilegeOutcome::privileged)
        .map_err(|error| format!("administrator operation failed: {error}"))
}

#[cfg(test)]
pub(crate) fn run_privileged_command_sync(
    command: PrivilegedCommand,
) -> Result<ShellPrivilegeOutcome, String> {
    futures_lite::future::block_on(tensor_files_core::run_operation_task(move || async move {
        run_privileged_command(command).await
    }))
    .map_err(|error| error.to_string())?
}

pub(crate) fn should_attempt_privileged_operation(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("permission denied")
        || error.contains("os error 13")
        || error.contains("operation not permitted")
        || error.contains("os error 1")
}
