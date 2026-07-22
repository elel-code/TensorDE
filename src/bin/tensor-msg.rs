use std::{
    env,
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use tensor_compositor::{
    ipc::{Command, FrameDecoder, IPC_PROTOCOL_VERSION, Request, Response, ResultBody, encode},
    layout::LayoutKind,
};
use thiserror::Error;

const REQUEST_ID: u64 = 1;

#[derive(Debug, Parser)]
#[command(author, version, about = "Control a running Tensor compositor")]
struct Cli {
    /// Connect to this Tensor IPC socket.
    #[arg(long)]
    socket: Option<PathBuf>,
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    Ping,
    GetState,
    SetLayout { layout: LayoutKind },
    Quit,
}

impl From<CliCommand> for Command {
    fn from(command: CliCommand) -> Self {
        match command {
            CliCommand::Ping => Self::Ping,
            CliCommand::GetState => Self::GetState,
            CliCommand::SetLayout { layout } => Self::SetLayout { layout },
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
    let mut stream = UnixStream::connect(&socket).map_err(|source| ClientError::Connect {
        path: socket,
        source,
    })?;
    let request = Request::new(REQUEST_ID, cli.command.into());
    stream.write_all(&encode(&request)?)?;
    let response = read_response(&mut stream)?;

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

fn read_response(stream: &mut UnixStream) -> Result<Response, ClientError> {
    let mut decoder = FrameDecoder::new();
    let mut buffer = [0; 16 * 1024];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Err(ClientError::UnexpectedEof);
        }
        if let Some(response) = decoder
            .push::<Response>(&buffer[..read])?
            .into_iter()
            .next()
        {
            return Ok(response);
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
    Codec(#[from] tensor_compositor::ipc::CodecError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("Tensor IPC closed before returning a response")]
    UnexpectedEof,
    #[error("Tensor returned protocol version {0}; expected {IPC_PROTOCOL_VERSION}")]
    ResponseVersion(u16),
    #[error("Tensor returned request ID {0}; expected {REQUEST_ID}")]
    RequestId(u64),
    #[error("Tensor IPC error {code}: {message}")]
    Server { code: String, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_commands_map_to_protocol_commands() {
        assert!(matches!(Command::from(CliCommand::Ping), Command::Ping));
        assert!(matches!(
            Command::from(CliCommand::SetLayout {
                layout: LayoutKind::MasterStack
            }),
            Command::SetLayout {
                layout: LayoutKind::MasterStack
            }
        ));
    }
}
