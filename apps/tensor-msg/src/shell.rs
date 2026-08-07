use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use tensor_dbus::{
    Connection, freedesktop::mpris::MprisAction, tensor::shell::perform_media_action,
};
use tensor_runtime::io_uring_runtime;

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    bin_name = "tensor-msg shell",
    about = "Send a bounded command to the running Tensor Shell"
)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Control the active MPRIS player selected by Tensor Shell.
    Media {
        #[arg(value_enum)]
        action: MediaAction,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum MediaAction {
    Previous,
    PlayPause,
    Next,
}

impl From<MediaAction> for MprisAction {
    fn from(action: MediaAction) -> Self {
        match action {
            MediaAction::Previous => Self::Previous,
            MediaAction::PlayPause => Self::PlayPause,
            MediaAction::Next => Self::Next,
        }
    }
}

pub fn run(args: Vec<String>) -> ExitCode {
    let arguments = std::iter::once("tensor-msg shell".to_owned()).chain(args);
    match run_client(Cli::parse_from(arguments)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run_client(cli: Cli) -> Result<(), ClientError> {
    let runtime = io_uring_runtime(4)?;
    runtime.block_on(async move {
        let mut connection = Connection::session_bus().await?;
        match cli.command {
            CliCommand::Media { action } => {
                perform_media_action(&mut connection, action.into()).await?
            }
        }
        Ok(())
    })
}

#[derive(Debug, thiserror::Error)]
enum ClientError {
    #[error("could not create the Compio io_uring runtime: {0}")]
    Runtime(#[from] std::io::Error),
    #[error("Tensor Shell command failed: {0}")]
    Dbus(#[from] tensor_dbus::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versioned_media_control_actions() {
        let cli = Cli::try_parse_from(["tensor-msg shell", "media", "play-pause"]).unwrap();
        assert!(matches!(
            cli.command,
            CliCommand::Media {
                action: MediaAction::PlayPause
            }
        ));
    }
}
