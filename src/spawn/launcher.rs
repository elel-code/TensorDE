use std::{
    ffi::{OsStr, OsString},
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
    os::unix::process::CommandExt,
    process::{Child, Command, ExitStatus, Stdio},
};

#[cfg(feature = "systemd")]
use std::thread;

use thiserror::Error;

#[cfg(feature = "systemd")]
use tracing::warn;

use crate::service::{SESSION_ENVIRONMENT_NAMES, SystemdMode};

#[cfg(feature = "systemd")]
use super::scope;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpawnStrategy {
    Direct,
    SystemdScope,
}

impl SpawnStrategy {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::SystemdScope => "systemd-scope",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpawnedProcess {
    pid: u32,
    strategy: SpawnStrategy,
}

impl SpawnedProcess {
    pub const fn pid(self) -> u32 {
        self.pid
    }

    pub const fn strategy(self) -> SpawnStrategy {
        self.strategy
    }
}

#[derive(Clone, Debug)]
pub struct ProcessLauncher {
    mode: SystemdMode,
    systemd_detected: bool,
    environment: Vec<(OsString, OsString)>,
    environment_managed: bool,
}

impl ProcessLauncher {
    pub fn new(mode: SystemdMode) -> Self {
        Self::with_systemd_detection(mode, SystemdMode::detected())
    }

    pub const fn with_systemd_detection(mode: SystemdMode, detected: bool) -> Self {
        Self {
            mode,
            systemd_detected: detected,
            environment: Vec::new(),
            environment_managed: false,
        }
    }

    pub fn with_environment<I, K, V>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        self.set_environment(values);
        self
    }

    pub fn set_environment<I, K, V>(&mut self, values: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        self.environment = values
            .into_iter()
            .map(|(name, value)| (name.into(), value.into()))
            .collect();
        self.environment_managed = true;
    }

    pub const fn strategy(&self) -> SpawnStrategy {
        let systemd_scope = match self.mode {
            SystemdMode::Enabled => true,
            SystemdMode::Auto => cfg!(feature = "systemd") && self.systemd_detected,
            SystemdMode::Disabled => false,
        };
        if systemd_scope {
            SpawnStrategy::SystemdScope
        } else {
            SpawnStrategy::Direct
        }
    }

    pub fn spawn<I, S>(
        &self,
        program: impl AsRef<OsStr>,
        args: I,
    ) -> Result<SpawnedProcess, SpawnError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        self.spawn_command(command)
    }

    pub fn spawn_command(&self, mut command: Command) -> Result<SpawnedProcess, SpawnError> {
        if self.environment_managed {
            for name in SESSION_ENVIRONMENT_NAMES {
                command.env_remove(name);
            }
        }
        command
            .env_remove("NOTIFY_SOCKET")
            .envs(self.environment.iter().map(|(name, value)| (name, value)));
        match self.strategy() {
            SpawnStrategy::Direct => launch_direct(command),
            SpawnStrategy::SystemdScope => self.launch_scoped(command),
        }
    }

    #[cfg(feature = "systemd")]
    fn launch_scoped(&self, command: Command) -> Result<SpawnedProcess, SpawnError> {
        let strict = self.mode == SystemdMode::Enabled;
        launch_scoped(command, strict)
    }

    #[cfg(not(feature = "systemd"))]
    fn launch_scoped(&self, command: Command) -> Result<SpawnedProcess, SpawnError> {
        if self.mode == SystemdMode::Enabled {
            Err(SpawnError::SystemdUnavailable)
        } else {
            launch_direct(command)
        }
    }
}

fn launch_direct(mut command: Command) -> Result<SpawnedProcess, SpawnError> {
    let command_name = command.get_program().to_owned();
    let pipes = ForkPipes::new(false)?;
    prepare_double_fork(&mut command, &pipes);

    let mut intermediate = command.spawn().map_err(|source| SpawnError::Command {
        command: command_name,
        source,
    })?;
    drop(pipes.pid_write);
    let pids = read_pids(&pipes.pid_read)?;
    wait_for_intermediate(&mut intermediate)?;

    Ok(SpawnedProcess {
        pid: pids.client,
        strategy: SpawnStrategy::Direct,
    })
}

#[cfg(feature = "systemd")]
fn launch_scoped(mut command: Command, strict: bool) -> Result<SpawnedProcess, SpawnError> {
    let command_name = command.get_program().to_owned();
    let pipes = ForkPipes::new(true)?;
    prepare_double_fork(&mut command, &pipes);

    let spawn_thread = thread::Builder::new()
        .name("tensor-client-spawn".to_owned())
        .spawn(move || command.spawn())
        .map_err(SpawnError::SpawnThread)?;

    let pids = read_pids(&pipes.pid_read);
    drop(pipes.pid_write);
    drop(pipes.gate_read);

    let scope_result = pids.as_ref().map_err(clone_io_error).and_then(|pids| {
        scope::start(command_name.as_os_str(), pids.intermediate, pids.client)
            .map_err(SpawnError::SystemdScope)
    });

    if strict && let Err(error) = &scope_result {
        if let Ok(pids) = &pids {
            terminate_blocked_client(pids.client);
        }
        tracing::debug!(%error, "terminating client whose required scope failed");
    }

    // Closing the last write end releases both the intermediate process and the client.
    drop(pipes.gate_write);
    let mut intermediate = spawn_thread
        .join()
        .map_err(|_| SpawnError::SpawnThreadPanicked)?
        .map_err(|source| SpawnError::Command {
            command: command_name,
            source,
        })?;
    wait_for_intermediate(&mut intermediate)?;
    let pids = pids?;

    match scope_result {
        Ok(()) => Ok(SpawnedProcess {
            pid: pids.client,
            strategy: SpawnStrategy::SystemdScope,
        }),
        Err(error) if strict => Err(error),
        Err(error) => {
            warn!(%error, pid = pids.client, "systemd scope failed; client is running directly");
            Ok(SpawnedProcess {
                pid: pids.client,
                strategy: SpawnStrategy::Direct,
            })
        }
    }
}

fn wait_for_intermediate(child: &mut Child) -> Result<(), SpawnError> {
    let status = child.wait().map_err(SpawnError::Wait)?;
    if status.success() {
        Ok(())
    } else {
        Err(SpawnError::IntermediateExited(status))
    }
}

struct ForkPipes {
    pid_read: OwnedFd,
    pid_write: OwnedFd,
    gate_read: Option<OwnedFd>,
    gate_write: Option<OwnedFd>,
}

impl ForkPipes {
    fn new(with_gate: bool) -> Result<Self, SpawnError> {
        let (pid_read, pid_write) = pipe_cloexec().map_err(SpawnError::Pipe)?;
        let (gate_read, gate_write) = if with_gate {
            let (read, write) = pipe_cloexec().map_err(SpawnError::Pipe)?;
            (Some(read), Some(write))
        } else {
            (None, None)
        };
        Ok(Self {
            pid_read,
            pid_write,
            gate_read,
            gate_write,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct ForkPids {
    #[cfg(feature = "systemd")]
    intermediate: u32,
    client: u32,
}

#[allow(unsafe_code)]
fn prepare_double_fork(command: &mut Command, pipes: &ForkPipes) {
    let pid_read = pipes.pid_read.as_raw_fd();
    let pid_write = pipes.pid_write.as_raw_fd();
    let gate_read = pipes.gate_read.as_ref().map(AsRawFd::as_raw_fd);
    let gate_write = pipes.gate_write.as_ref().map(AsRawFd::as_raw_fd);

    unsafe {
        command.pre_exec(move || {
            close_fd(pid_read);
            if let Some(fd) = gate_write {
                close_fd(fd);
            }

            let intermediate_pid = libc::getpid();
            match libc::fork() {
                -1 => Err(io::Error::last_os_error()),
                0 => {
                    close_fd(pid_write);
                    if let Some(fd) = gate_read {
                        wait_for_release(fd)?;
                    }
                    Ok(())
                }
                client_pid => {
                    let message = encode_pids(intermediate_pid, client_pid)?;
                    write_all(pid_write, &message)?;
                    close_fd(pid_write);
                    if let Some(fd) = gate_read {
                        wait_for_release(fd)?;
                    }
                    libc::_exit(0)
                }
            }
        });
    }
}

fn encode_pids(intermediate: libc::pid_t, client: libc::pid_t) -> io::Result<[u8; 8]> {
    let intermediate = u32::try_from(intermediate)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid intermediate PID"))?;
    let client = u32::try_from(client)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid client PID"))?;
    let mut message = [0; 8];
    message[..4].copy_from_slice(&intermediate.to_ne_bytes());
    message[4..].copy_from_slice(&client.to_ne_bytes());
    Ok(message)
}

fn read_pids(fd: &OwnedFd) -> Result<ForkPids, SpawnError> {
    let mut message = [0; 8];
    read_all(fd.as_raw_fd(), &mut message).map_err(SpawnError::PidTransfer)?;
    Ok(ForkPids {
        #[cfg(feature = "systemd")]
        intermediate: u32::from_ne_bytes(message[..4].try_into().unwrap()),
        client: u32::from_ne_bytes(message[4..].try_into().unwrap()),
    })
}

#[cfg(feature = "systemd")]
fn clone_io_error(error: &SpawnError) -> SpawnError {
    match error {
        SpawnError::PidTransfer(error) => {
            SpawnError::PidTransfer(io::Error::new(error.kind(), error.to_string()))
        }
        _ => SpawnError::PidTransfer(io::Error::other(error.to_string())),
    }
}

#[allow(unsafe_code)]
fn pipe_cloexec() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [-1; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } == -1 {
        return Err(io::Error::last_os_error());
    }
    unsafe { Ok((OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1]))) }
}

#[allow(unsafe_code)]
fn close_fd(fd: RawFd) {
    unsafe {
        libc::close(fd);
    }
}

#[cfg(feature = "systemd")]
#[allow(unsafe_code)]
fn terminate_blocked_client(pid: u32) {
    unsafe {
        // The client is still blocked before exec while its transient scope is created.
        let _ = libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
}

#[allow(unsafe_code)]
fn write_all(fd: RawFd, mut buffer: &[u8]) -> io::Result<()> {
    while !buffer.is_empty() {
        let written = unsafe { libc::write(fd, buffer.as_ptr().cast(), buffer.len()) };
        if written == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if written == 0 {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "PID pipe closed"));
        }
        buffer = &buffer[written as usize..];
    }
    Ok(())
}

#[allow(unsafe_code)]
fn read_all(fd: RawFd, mut buffer: &mut [u8]) -> io::Result<()> {
    while !buffer.is_empty() {
        let read = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
        if read == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "PID pipe closed before sending both PIDs",
            ));
        }
        let (_, remaining) = std::mem::take(&mut buffer).split_at_mut(read as usize);
        buffer = remaining;
    }
    Ok(())
}

fn wait_for_release(fd: RawFd) -> io::Result<()> {
    let mut byte = [0];
    loop {
        match read_all_or_eof(fd, &mut byte) {
            Ok(0) => return Ok(()),
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

#[allow(unsafe_code)]
fn read_all_or_eof(fd: RawFd, buffer: &mut [u8]) -> io::Result<usize> {
    let read = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
    if read == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(read as usize)
    }
}

#[derive(Debug, Error)]
pub enum SpawnError {
    #[error("failed to create client-launch pipe: {0}")]
    Pipe(io::Error),
    #[error("failed to start client process {command:?}: {source}")]
    Command {
        command: std::ffi::OsString,
        source: io::Error,
    },
    #[error("failed to transfer client process IDs: {0}")]
    PidTransfer(io::Error),
    #[error("failed to create client-spawn worker: {0}")]
    SpawnThread(io::Error),
    #[error("client-spawn worker panicked")]
    SpawnThreadPanicked,
    #[error("failed to reap intermediate client process: {0}")]
    Wait(io::Error),
    #[error("intermediate client process exited unsuccessfully: {0}")]
    IntermediateExited(ExitStatus),
    #[cfg(feature = "systemd")]
    #[error(transparent)]
    SystemdScope(#[from] scope::ScopeError),
    #[cfg(not(feature = "systemd"))]
    #[error("systemd client scopes were required but support is not compiled in")]
    SystemdUnavailable,
}

#[cfg(test)]
mod tests {
    use std::{fs, fs::File, path::PathBuf, thread};

    use super::*;

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
}
