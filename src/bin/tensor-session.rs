use std::{
    env,
    ffi::{OsStr, OsString},
    path::PathBuf,
    process::{Command, ExitCode},
};

use tensor_compositor::service::{SystemdMode, configured_mode};

const SERVICE: &str = "tensor.service";
const SHUTDOWN_TARGET: &str = "tensor-shutdown.target";
const SESSION_ENVIRONMENT: &[&str] = &[
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "TENSOR_IPC_SOCKET",
];

fn main() -> ExitCode {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if running_as_user_service() {
        return exec_compositor(&args);
    }

    let mode = match configured_mode(config_path(&args)) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("failed to load Tensor configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    let manager_available = cfg!(feature = "systemd") && systemctl_available();
    if !mode.resolve(manager_available) {
        return exec_compositor(&args);
    }
    if mode == SystemdMode::Enabled && !manager_available {
        eprintln!("systemd integration is enabled, but no user manager is available");
        return ExitCode::FAILURE;
    }

    if is_active() {
        eprintln!("a Tensor session is already running");
        return ExitCode::FAILURE;
    }

    let _ = run_systemctl(["--user", "reset-failed"]);
    let _ = run_systemctl(["--user", "import-environment"]);
    let _ = Command::new("dbus-update-activation-environment")
        .arg("--all")
        .status();

    let status = run_systemctl(["--user", "--wait", "start", SERVICE]);
    let _ = run_systemctl([
        "--user",
        "start",
        "--job-mode=replace-irreversibly",
        SHUTDOWN_TARGET,
    ]);
    let _ = run_systemctl(
        ["--user", "unset-environment"]
            .into_iter()
            .chain(SESSION_ENVIRONMENT.iter().copied()),
    );

    match status {
        Some(status) if status.success() => ExitCode::SUCCESS,
        Some(_) | None => ExitCode::FAILURE,
    }
}

fn running_as_user_service() -> bool {
    let Some(manager_pid) = env::var_os("MANAGERPID") else {
        return false;
    };
    let Some(exec_pid) = env::var_os("SYSTEMD_EXEC_PID") else {
        return false;
    };
    manager_pid != exec_pid && exec_pid == OsString::from(std::process::id().to_string())
}

fn systemctl_available() -> bool {
    Command::new("systemctl")
        .args(["--user", "show-environment"])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn config_path(args: &[OsString]) -> Option<PathBuf> {
    let mut args = args.iter();
    while let Some(argument) = args.next() {
        if argument == "--config" || argument == "-c" {
            return args.next().map(PathBuf::from);
        }
        if let Some(path) = argument
            .to_str()
            .and_then(|argument| argument.strip_prefix("--config="))
        {
            return Some(PathBuf::from(path));
        }
    }
    None
}

fn is_active() -> bool {
    Command::new("systemctl")
        .args(["--user", "--quiet", "is-active", SERVICE])
        .status()
        .is_ok_and(|status| status.success())
}

fn run_systemctl<I, S>(args: I) -> Option<std::process::ExitStatus>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("systemctl").args(args).status().ok()
}

fn exec_compositor(args: &[OsString]) -> ExitCode {
    let mut command = Command::new("tensor-compositor");
    command.arg("--session").args(args);
    command.env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY");

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        eprintln!("failed to exec tensor-compositor: {error}");
        ExitCode::FAILURE
    }

    #[cfg(not(unix))]
    {
        match command.status() {
            Ok(status) if status.success() => ExitCode::SUCCESS,
            Ok(_) | Err(_) => ExitCode::FAILURE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn extracts_split_config_argument() {
        let args = arguments(&["--config", "/tmp/tensor.toml", "--check"]);
        assert_eq!(config_path(&args), Some(PathBuf::from("/tmp/tensor.toml")));
    }

    #[test]
    fn extracts_joined_config_argument() {
        let args = arguments(&["--config=/tmp/tensor.toml"]);
        assert_eq!(config_path(&args), Some(PathBuf::from("/tmp/tensor.toml")));
    }

    #[test]
    fn missing_config_argument_uses_normal_resolution() {
        assert_eq!(config_path(&arguments(&["--check"])), None);
    }
}
