use std::{
    ffi::{OsStr, OsString},
    io,
    os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd, RawFd},
    os::unix::process::CommandExt,
    process::{Child, Command, ExitStatus, Stdio},
};

#[cfg(feature = "systemd")]
use std::thread;

use rustix::{
    io::{close, read, retry_on_intr, write},
    pipe::{PipeFlags, pipe_with},
    process::{Pid, getpid},
    runtime::{Fork, exit_group, kernel_fork},
};
use thiserror::Error;

#[cfg(feature = "systemd")]
use rustix::process::{Signal, kill_process};
#[cfg(feature = "systemd")]
use tensor_dbus::Connection;
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
    /// Names stripped from the child environment before the managed set is applied.
    environment_clear: Vec<OsString>,
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
            environment_clear: Vec::new(),
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

    /// Names to remove from the inherited process environment for launched children.
    pub fn set_environment_clear<I, S>(&mut self, names: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.environment_clear = names.into_iter().map(Into::into).collect();
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
        self.spawn_in(program, args, None::<&OsStr>)
    }

    pub fn spawn_in<I, S, P>(
        &self,
        program: impl AsRef<OsStr>,
        args: I,
        working_directory: Option<P>,
    ) -> Result<SpawnedProcess, SpawnError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        P: AsRef<OsStr>,
    {
        let command = launch_command(program, args, working_directory);
        self.spawn_command(command)
    }

    /// Spawn with a compositor-minted `XDG_ACTIVATION_TOKEN`.
    pub fn spawn_with_activation<I, S, T>(
        &self,
        program: impl AsRef<OsStr>,
        args: I,
        activation_token: T,
    ) -> Result<SpawnedProcess, SpawnError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        T: AsRef<OsStr>,
    {
        self.spawn_with_activation_in(program, args, None::<&OsStr>, activation_token)
    }

    pub fn spawn_with_activation_in<I, S, P, T>(
        &self,
        program: impl AsRef<OsStr>,
        args: I,
        working_directory: Option<P>,
        activation_token: T,
    ) -> Result<SpawnedProcess, SpawnError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        P: AsRef<OsStr>,
        T: AsRef<OsStr>,
    {
        let command = launch_command(program, args, working_directory);
        self.spawn_command_with_activation(command, Some(activation_token.as_ref()))
    }

    pub fn spawn_command(&self, command: Command) -> Result<SpawnedProcess, SpawnError> {
        self.spawn_command_with_activation(command, None::<&OsStr>)
    }

    #[cfg(feature = "systemd")]
    pub(super) async fn spawn_in_async<I, S, P>(
        &self,
        program: impl AsRef<OsStr>,
        args: I,
        working_directory: Option<P>,
        connection: &mut Option<Connection>,
    ) -> Result<SpawnedProcess, SpawnError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        P: AsRef<OsStr>,
    {
        let command = launch_command(program, args, working_directory);
        self.spawn_command_with_activation_async(command, None, connection)
            .await
    }

    #[cfg(feature = "systemd")]
    pub(super) async fn spawn_with_activation_in_async<I, S, P, T>(
        &self,
        program: impl AsRef<OsStr>,
        args: I,
        working_directory: Option<P>,
        activation_token: T,
        connection: &mut Option<Connection>,
    ) -> Result<SpawnedProcess, SpawnError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        P: AsRef<OsStr>,
        T: AsRef<OsStr>,
    {
        let command = launch_command(program, args, working_directory);
        self.spawn_command_with_activation_async(
            command,
            Some(activation_token.as_ref()),
            connection,
        )
        .await
    }

    fn spawn_command_with_activation(
        &self,
        mut command: Command,
        activation_token: Option<&OsStr>,
    ) -> Result<SpawnedProcess, SpawnError> {
        self.prepare_command(&mut command, activation_token);
        match self.strategy() {
            SpawnStrategy::Direct => launch_direct(command),
            SpawnStrategy::SystemdScope => self.launch_scoped(command),
        }
    }

    fn prepare_command(&self, command: &mut Command, activation_token: Option<&OsStr>) {
        if self.environment_managed {
            for name in SESSION_ENVIRONMENT_NAMES {
                command.env_remove(name);
            }
            for name in &self.environment_clear {
                command.env_remove(name);
            }
        }
        command
            .env_remove("NOTIFY_SOCKET")
            .envs(self.environment.iter().map(|(name, value)| (name, value)));
        // Apply after the managed session set so launch tokens always win.
        if let Some(token) = activation_token {
            command
                .env("XDG_ACTIVATION_TOKEN", token)
                .env("DESKTOP_STARTUP_ID", token);
        }
    }

    #[cfg(feature = "systemd")]
    async fn spawn_command_with_activation_async(
        &self,
        mut command: Command,
        activation_token: Option<&OsStr>,
        connection: &mut Option<Connection>,
    ) -> Result<SpawnedProcess, SpawnError> {
        self.prepare_command(&mut command, activation_token);
        match self.strategy() {
            SpawnStrategy::Direct => launch_direct(command),
            SpawnStrategy::SystemdScope => {
                launch_scoped(command, self.mode == SystemdMode::Enabled, connection).await
            }
        }
    }

    #[cfg(feature = "systemd")]
    fn launch_scoped(&self, _command: Command) -> Result<SpawnedProcess, SpawnError> {
        Err(SpawnError::SystemdRequiresAsyncWorker)
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

fn launch_command<I, S, P>(
    program: impl AsRef<OsStr>,
    args: I,
    working_directory: Option<P>,
) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
    P: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(path) = working_directory {
        command.current_dir(path.as_ref());
    }
    command
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
async fn launch_scoped(
    mut command: Command,
    strict: bool,
    connection: &mut Option<Connection>,
) -> Result<SpawnedProcess, SpawnError> {
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

    let scope_result = match pids.as_ref().map_err(clone_io_error) {
        Ok(pids) => {
            let connect_result = if connection.is_none() {
                Connection::session_bus()
                    .await
                    .map(|new_connection| *connection = Some(new_connection))
                    .map_err(scope::ScopeError::from)
                    .map_err(SpawnError::SystemdScope)
            } else {
                Ok(())
            };
            let result = match connect_result {
                Ok(()) => scope::start(
                    connection.as_mut().expect("connection was installed"),
                    command_name.as_os_str(),
                    pids.intermediate,
                    pids.client,
                )
                .await
                .map_err(SpawnError::SystemdScope),
                Err(error) => Err(error),
            };
            if result.is_err() {
                *connection = None;
            }
            result
        }
        Err(error) => Err(error),
    };

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
    // The descriptors are duplicated by Command's initial fork. Turn the
    // child-side descriptors back into OwnedFd values inside pre_exec so
    // rustix can perform the PID and gate transfers without more raw reads or
    // writes. Taking each raw value prevents an accidental second owner if
    // Command ever retries the closure after a setup error.
    let mut pid_read = Some(pipes.pid_read.as_raw_fd());
    let mut pid_write = Some(pipes.pid_write.as_raw_fd());
    let mut gate_read = pipes.gate_read.as_ref().map(AsRawFd::as_raw_fd);
    let mut gate_write = pipes.gate_write.as_ref().map(AsRawFd::as_raw_fd);

    unsafe {
        command.pre_exec(move || {
            // Tensor consumes termination signals through signalfd; applications
            // must retain their normal signal delivery after exec.
            crate::signals::unblock_all_for_child()?;
            if let Some(fd) = pid_read.take() {
                close_fd(fd);
            }
            if let Some(fd) = gate_write.take() {
                close_fd(fd);
            }
            let pid_write = pid_write.take().map(|fd| OwnedFd::from_raw_fd(fd));
            let gate_read = gate_read.take().map(|fd| OwnedFd::from_raw_fd(fd));

            let intermediate_pid = getpid();
            match kernel_fork().map_err(io::Error::from)? {
                Fork::Child(_) => {
                    drop(pid_write);
                    if let Some(fd) = gate_read.as_ref() {
                        wait_for_release(fd)?;
                    }
                    Ok(())
                }
                Fork::ParentOf(client_pid) => {
                    let message = encode_pids(intermediate_pid, client_pid)?;
                    if let Some(fd) = pid_write.as_ref() {
                        write_all(fd, &message)?;
                    }
                    drop(pid_write);
                    if let Some(fd) = gate_read.as_ref() {
                        wait_for_release(fd)?;
                    }
                    exit_group(0)
                }
            }
        });
    }
}

fn encode_pids(intermediate: Pid, client: Pid) -> io::Result<[u8; 8]> {
    let intermediate = u32::try_from(intermediate.as_raw_pid())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid intermediate PID"))?;
    let client = u32::try_from(client.as_raw_pid())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid client PID"))?;
    let mut message = [0; 8];
    message[..4].copy_from_slice(&intermediate.to_ne_bytes());
    message[4..].copy_from_slice(&client.to_ne_bytes());
    Ok(message)
}

fn read_pids(fd: &OwnedFd) -> Result<ForkPids, SpawnError> {
    let mut message = [0; 8];
    read_all(fd, &mut message).map_err(SpawnError::PidTransfer)?;
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

fn pipe_cloexec() -> io::Result<(OwnedFd, OwnedFd)> {
    pipe_with(PipeFlags::CLOEXEC).map_err(io::Error::from)
}

#[allow(unsafe_code)]
fn close_fd(fd: RawFd) {
    unsafe {
        close(fd);
    }
}

#[cfg(feature = "systemd")]
fn terminate_blocked_client(pid: u32) {
    if let Some(pid) = Pid::from_raw(pid as i32) {
        // The client is still blocked before exec while its transient scope is created.
        let _ = kill_process(pid, Signal::KILL);
    }
}

fn write_all(fd: impl AsFd, buffer: &[u8]) -> io::Result<()> {
    let mut written = 0;
    while written != buffer.len() {
        let count = retry_on_intr(|| write(&fd, &buffer[written..])).map_err(io::Error::from)?;
        if count == 0 {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "PID pipe closed"));
        }
        written += count;
    }
    Ok(())
}

fn read_all(fd: impl AsFd, buffer: &mut [u8]) -> io::Result<()> {
    let mut start = 0;
    while start != buffer.len() {
        let count = retry_on_intr(|| read(&fd, &mut buffer[start..])).map_err(io::Error::from)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "PID pipe closed before sending both PIDs",
            ));
        }
        start += count;
    }
    Ok(())
}

fn wait_for_release(fd: impl AsFd) -> io::Result<()> {
    let mut byte = [0];
    loop {
        let count = retry_on_intr(|| read(&fd, &mut byte)).map_err(io::Error::from)?;
        if count == 0 {
            return Ok(());
        }
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
    #[cfg(feature = "systemd")]
    #[error("systemd-scoped launches require the asynchronous launch worker")]
    SystemdRequiresAsyncWorker,
    #[cfg(not(feature = "systemd"))]
    #[error("systemd client scopes were required but support is not compiled in")]
    SystemdUnavailable,
}
