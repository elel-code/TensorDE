use std::{
    fs,
    fs::File,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
};

use super::*;
use crate::service::SystemdMode;

#[test]
fn strategy_follows_mode_and_detection() {
    #[cfg(feature = "systemd")]
    assert_eq!(
        ProcessLauncher::with_systemd_detection(SystemdMode::Auto, true).strategy(),
        SpawnStrategy::SystemdScope
    );
    #[cfg(not(feature = "systemd"))]
    assert_eq!(
        ProcessLauncher::with_systemd_detection(SystemdMode::Auto, true).strategy(),
        SpawnStrategy::Direct
    );
    assert_eq!(
        ProcessLauncher::with_systemd_detection(SystemdMode::Auto, false).strategy(),
        SpawnStrategy::Direct
    );
    assert_eq!(
        ProcessLauncher::with_systemd_detection(SystemdMode::Enabled, false).strategy(),
        SpawnStrategy::SystemdScope
    );
    assert_eq!(
        ProcessLauncher::with_systemd_detection(SystemdMode::Disabled, true).strategy(),
        SpawnStrategy::Direct
    );
}

#[test]
fn direct_spawn_executes_without_a_shell() {
    let path = PathBuf::from(format!("target/tensor-spawn-{}", std::process::id()));
    let _ = fs::remove_file(&path);

    let process = ProcessLauncher::with_systemd_detection(SystemdMode::Disabled, true)
        .spawn("touch", [path.as_os_str()])
        .unwrap();

    assert_eq!(process.strategy(), SpawnStrategy::Direct);
    for _ in 0..100 {
        if path.exists() {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(path.exists());
    fs::remove_file(path).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn direct_spawn_restores_child_signal_delivery() {
    struct ResetSignalMask;

    impl Drop for ResetSignalMask {
        fn drop(&mut self) {
            crate::signals::unblock_all_for_child().unwrap();
        }
    }

    crate::signals::block_early().unwrap();
    let _reset_signal_mask = ResetSignalMask;
    let path = PathBuf::from(format!(
        "target/tensor-spawn-signals-{}",
        std::process::id()
    ));
    let _ = fs::remove_file(&path);
    let mut command = Command::new("cat");
    command
        .arg("/proc/self/status")
        .stdout(Stdio::from(File::create(&path).unwrap()));

    ProcessLauncher::with_systemd_detection(SystemdMode::Disabled, false)
        .spawn_command(command)
        .unwrap();

    let mut status = String::new();
    for _ in 0..100 {
        status = fs::read_to_string(&path).unwrap();
        if status.contains("SigBlk:\t0000000000000000") {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(status.contains("SigBlk:\t0000000000000000"));
    fs::remove_file(path).unwrap();
}

#[test]
fn direct_spawn_reports_a_missing_executable() {
    let error = ProcessLauncher::with_systemd_detection(SystemdMode::Disabled, false)
        .spawn(
            "tensor-command-that-does-not-exist",
            std::iter::empty::<&str>(),
        )
        .unwrap_err();
    assert!(matches!(error, SpawnError::Command { .. }));
}

#[test]
fn child_environment_is_injected_and_notify_socket_is_removed() {
    let path = PathBuf::from(format!("target/tensor-spawn-env-{}", std::process::id()));
    let _ = fs::remove_file(&path);
    let mut command = Command::new("env");
    command
        .env("NOTIFY_SOCKET", "/tmp/parent-notify.sock")
        .stdout(Stdio::from(File::create(&path).unwrap()));

    ProcessLauncher::with_systemd_detection(SystemdMode::Disabled, false)
        .with_environment([("TENSOR_TEST_ENV", "available")])
        .spawn_command(command)
        .unwrap();

    let mut environment = String::new();
    for _ in 0..100 {
        environment = fs::read_to_string(&path).unwrap();
        if environment.contains("TENSOR_TEST_ENV=available") {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(environment.contains("TENSOR_TEST_ENV=available"));
    assert!(!environment.contains("NOTIFY_SOCKET="));
    fs::remove_file(path).unwrap();
}

#[test]
fn managed_environment_removes_stale_session_values() {
    let path = PathBuf::from(format!(
        "target/tensor-spawn-clean-env-{}",
        std::process::id()
    ));
    let _ = fs::remove_file(&path);
    let mut command = Command::new("env");
    command
        .env("DISPLAY", ":99")
        .stdout(Stdio::from(File::create(&path).unwrap()));

    ProcessLauncher::with_systemd_detection(SystemdMode::Disabled, false)
        .with_environment([("WAYLAND_DISPLAY", "tensor-0")])
        .spawn_command(command)
        .unwrap();

    let mut environment = String::new();
    for _ in 0..100 {
        environment = fs::read_to_string(&path).unwrap();
        if environment.contains("WAYLAND_DISPLAY=tensor-0") {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(environment.contains("WAYLAND_DISPLAY=tensor-0"));
    assert!(!environment.contains("DISPLAY=:99"));
    fs::remove_file(path).unwrap();
}

#[cfg(not(feature = "systemd"))]
#[test]
fn enabled_mode_requires_compiled_systemd_support() {
    let error = ProcessLauncher::with_systemd_detection(SystemdMode::Enabled, true)
        .spawn("true", std::iter::empty::<&str>())
        .unwrap_err();
    assert!(matches!(error, SpawnError::SystemdUnavailable));
}
