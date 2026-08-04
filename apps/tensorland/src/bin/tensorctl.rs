use std::{
    collections::VecDeque,
    env,
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use tensorland::{
    ipc::{
        Command, EventMessage, EventTopic, FrameDecoder, IPC_PROTOCOL_VERSION, Request, Response,
        ResultBody, ServerMessage, encode,
    },
    layout::LayoutKind,
};
use thiserror::Error;

const REQUEST_ID: u64 = 1;

#[derive(Debug, Parser)]
#[command(author, version, about = "Control a running Tensorland compositor")]
struct Cli {
    /// Connect to this Tensorland IPC socket.
    #[arg(long)]
    socket: Option<PathBuf>,
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    Ping,
    GetState,
    GetOutputs,
    GetWorkspaces,
    GetOverview,
    GetConfigStatus,
    ReloadConfig,
    /// Stream versioned configuration reload results until disconnected.
    WatchConfig,
    SetLayout {
        layout: LayoutKind,
    },
    /// Queue a direct argv launch (no shell) on the compositor.
    Spawn {
        /// Program and arguments, for example: `tensorctl spawn -- foot --server`
        #[arg(required = true, num_args = 1.., trailing_var_arg = true, allow_hyphen_values = true)]
        argv: Vec<String>,
    },
    /// Activate virtual desktop by zero-based index.
    SetWorkspace {
        index: u32,
    },
    /// Move the focused window to another virtual desktop.
    MoveFocusedToWorkspace {
        index: u32,
        /// Also switch to that desktop after the move.
        #[arg(long, default_value_t = false)]
        follow: bool,
    },
    /// Activate a window by stable Tensorland view ID.
    ActivateView {
        view: u64,
    },
    /// Move a window family by stable view ID to a regular workspace.
    MoveViewToWorkspace {
        view: u64,
        index: u32,
        /// Also switch to the destination and focus the requested view.
        #[arg(long, default_value_t = false)]
        follow: bool,
    },
    /// Request that one exact stable view close.
    CloseView {
        view: u64,
    },
    /// Move the focused window into the configured hidden minimize workspace.
    MinimizeFocused,
    /// Restore a minimized window by stable Tensorland view ID.
    RestoreMinimized {
        view: u64,
        /// Restore without activating the retained origin workspace.
        #[arg(long, default_value_t = false)]
        stay: bool,
    },
    /// Set connector logical origin, e.g. `set-output-position HDMI-A-1 1920 0`.
    SetOutputPosition {
        name: String,
        x: i32,
        y: i32,
    },
    /// Enable or disable a connector (`set-output-enabled eDP-1 false`).
    SetOutputEnabled {
        name: String,
        enabled: bool,
    },
    /// Set scale percent (`set-output-scale HDMI-A-1 125` → 1.25).
    SetOutputScale {
        name: String,
        scale_percent: u32,
    },
    Quit,
}

impl From<CliCommand> for Command {
    fn from(command: CliCommand) -> Self {
        match command {
            CliCommand::Ping => Self::Ping,
            CliCommand::GetState => Self::GetState,
            CliCommand::GetOutputs => Self::GetOutputs,
            CliCommand::GetWorkspaces => Self::GetWorkspaces,
            CliCommand::GetOverview => Self::GetOverview,
            CliCommand::GetConfigStatus => Self::GetConfigStatus,
            CliCommand::ReloadConfig => Self::ReloadConfig,
            CliCommand::WatchConfig => Self::Subscribe {
                events: vec![EventTopic::ConfigReload],
            },
            CliCommand::SetLayout { layout } => Self::SetLayout { layout },
            CliCommand::Spawn { argv } => Self::Spawn { argv },
            CliCommand::SetWorkspace { index } => Self::SetWorkspace { index },
            CliCommand::MoveFocusedToWorkspace { index, follow } => {
                Self::MoveFocusedToWorkspace { index, follow }
            }
            CliCommand::ActivateView { view } => Self::ActivateView { view },
            CliCommand::MoveViewToWorkspace {
                view,
                index,
                follow,
            } => Self::MoveViewToWorkspace {
                view,
                index,
                follow,
            },
            CliCommand::CloseView { view } => Self::CloseView { view },
            CliCommand::MinimizeFocused => Self::MinimizeFocused,
            CliCommand::RestoreMinimized { view, stay } => Self::RestoreMinimized {
                view,
                follow: !stay,
            },
            CliCommand::SetOutputPosition { name, x, y } => Self::SetOutputPosition { name, x, y },
            CliCommand::SetOutputEnabled { name, enabled } => {
                Self::SetOutputEnabled { name, enabled }
            }
            CliCommand::SetOutputScale {
                name,
                scale_percent,
            } => Self::SetOutputScale {
                name,
                scale_percent,
            },
            CliCommand::Quit => Self::Quit,
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), ClientError> {
    let socket = resolve_socket(cli.socket);
    let watch_config = matches!(&cli.command, CliCommand::WatchConfig);
    let mut stream = UnixStream::connect(&socket).map_err(|source| ClientError::Connect {
        path: socket,
        source,
    })?;
    let request = Request::new(REQUEST_ID, cli.command.into());
    stream.write_all(&encode(&request)?)?;
    let mut reader = ServerReader::new();
    let response = reader.read_response(&mut stream)?;

    if response.version != IPC_PROTOCOL_VERSION {
        return Err(ClientError::ResponseVersion(response.version));
    }
    if response.request_id != REQUEST_ID {
        return Err(ClientError::RequestId(response.request_id));
    }
    if let ResultBody::Error(error) = &response.result {
        return Err(ClientError::Server {
            code: error.code.clone(),
            message: error.message.clone(),
        });
    }

    if watch_config {
        let mut stdout = io::stdout().lock();
        loop {
            let event = reader.read_event(&mut stream)?;
            if event.version != IPC_PROTOCOL_VERSION {
                return Err(ClientError::EventVersion(event.version));
            }
            serde_json::to_writer(&mut stdout, &event)?;
            writeln!(stdout)?;
        }
    }

    serde_json::to_writer(io::stdout().lock(), &response.result)?;
    println!();
    Ok(())
}

fn resolve_socket(explicit: Option<PathBuf>) -> PathBuf {
    explicit
        .or_else(|| env::var_os("TENSOR_IPC_SOCKET").map(PathBuf::from))
        .or_else(|| {
            env::var_os("XDG_RUNTIME_DIR")
                .map(|directory| PathBuf::from(directory).join("tensor.sock"))
        })
        .unwrap_or_else(|| PathBuf::from("/tmp/tensor.sock"))
}

struct ServerReader {
    decoder: FrameDecoder,
    pending: VecDeque<ServerMessage>,
}

impl ServerReader {
    fn new() -> Self {
        Self {
            decoder: FrameDecoder::new(),
            pending: VecDeque::new(),
        }
    }

    fn read_response(&mut self, stream: &mut UnixStream) -> Result<Response, ClientError> {
        match self.read_message(stream)? {
            ServerMessage::Response(response) => Ok(response),
            ServerMessage::Event(_) => Err(ClientError::UnexpectedMessage("event before response")),
        }
    }

    fn read_event(&mut self, stream: &mut UnixStream) -> Result<EventMessage, ClientError> {
        match self.read_message(stream)? {
            ServerMessage::Event(event) => Ok(event),
            ServerMessage::Response(_) => {
                Err(ClientError::UnexpectedMessage("response on event stream"))
            }
        }
    }

    fn read_message(&mut self, stream: &mut UnixStream) -> Result<ServerMessage, ClientError> {
        let mut buffer = [0; 16 * 1024];
        loop {
            if let Some(message) = self.pending.pop_front() {
                return Ok(message);
            }
            let read = stream.read(&mut buffer)?;
            if read == 0 {
                return Err(ClientError::UnexpectedEof);
            }
            self.pending
                .extend(self.decoder.push::<ServerMessage>(&buffer[..read])?);
        }
    }
}

#[derive(Debug, Error)]
enum ClientError {
    #[error("failed to connect to Tensor IPC socket {path}: {source}")]
    Connect { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Codec(#[from] tensorland::ipc::CodecError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("Tensor IPC closed before returning a response")]
    UnexpectedEof,
    #[error("Tensor returned protocol version {0}; expected {IPC_PROTOCOL_VERSION}")]
    ResponseVersion(u16),
    #[error("Tensor event used protocol version {0}; expected {IPC_PROTOCOL_VERSION}")]
    EventVersion(u16),
    #[error("Tensor returned request ID {0}; expected {REQUEST_ID}")]
    RequestId(u64),
    #[error("Tensor IPC error {code}: {message}")]
    Server { code: String, message: String },
    #[error("Tensor IPC sent an unexpected message: {0}")]
    UnexpectedMessage(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tensorland::ipc::{ConfigReloadEvent, ConfigReloadEventResult, ServerEvent};

    #[test]
    fn cli_commands_map_to_protocol_commands() {
        assert!(matches!(Command::from(CliCommand::Ping), Command::Ping));
        assert!(matches!(
            Command::from(CliCommand::GetOverview),
            Command::GetOverview
        ));
        assert!(matches!(
            Command::from(CliCommand::GetConfigStatus),
            Command::GetConfigStatus
        ));
        assert!(matches!(
            Command::from(CliCommand::ReloadConfig),
            Command::ReloadConfig
        ));
        assert!(matches!(
            Command::from(CliCommand::WatchConfig),
            Command::Subscribe { events }
                if events == [EventTopic::ConfigReload]
        ));
        assert!(matches!(
            Command::from(CliCommand::SetLayout {
                layout: LayoutKind::MasterStack
            }),
            Command::SetLayout {
                layout: LayoutKind::MasterStack
            }
        ));
        assert!(matches!(
            Command::from(CliCommand::Spawn {
                argv: vec!["foot".to_owned(), "--server".to_owned()]
            }),
            Command::Spawn { argv } if argv == ["foot", "--server"]
        ));
        assert!(matches!(
            Command::from(CliCommand::MinimizeFocused),
            Command::MinimizeFocused
        ));
        assert!(matches!(
            Command::from(CliCommand::ActivateView { view: 11 }),
            Command::ActivateView { view: 11 }
        ));
        assert!(matches!(
            Command::from(CliCommand::MoveViewToWorkspace {
                view: 11,
                index: 2,
                follow: true,
            }),
            Command::MoveViewToWorkspace {
                view: 11,
                index: 2,
                follow: true,
            }
        ));
        assert!(matches!(
            Command::from(CliCommand::CloseView { view: 12 }),
            Command::CloseView { view: 12 }
        ));
        assert!(matches!(
            Command::from(CliCommand::RestoreMinimized {
                view: 9,
                stay: false,
            }),
            Command::RestoreMinimized {
                view: 9,
                follow: true,
            }
        ));
    }

    #[test]
    fn reader_retains_an_event_coalesced_with_the_subscription_response() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let event = EventMessage {
            version: IPC_PROTOCOL_VERSION,
            sequence: 1,
            event: ServerEvent::ConfigReload(ConfigReloadEvent {
                request_id: 2,
                generation: 3,
                result: ConfigReloadEventResult::Applied,
            }),
        };
        let mut frames = encode(&ServerMessage::Response(Response::new(
            REQUEST_ID,
            ResultBody::Accepted,
        )))
        .unwrap();
        frames.extend(encode(&ServerMessage::Event(event.clone())).unwrap());
        server.write_all(&frames).unwrap();

        let mut reader = ServerReader::new();
        assert!(matches!(
            reader.read_response(&mut client).unwrap().result,
            ResultBody::Accepted
        ));
        assert_eq!(reader.read_event(&mut client).unwrap().sequence, 1);
        assert_eq!(reader.pending.len(), 0);
    }
}
