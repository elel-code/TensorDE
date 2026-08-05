mod land;

use std::{env, process::ExitCode};

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(product) = arguments.next() else {
        print_help();
        return ExitCode::SUCCESS;
    };
    match product.as_str() {
        "land" => land::run(arguments.collect()),
        "wallpaper" => match tensor_ipc::wallpaper::client::run(arguments.collect()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        },
        "--help" | "-h" | "help" => {
            print_help();
            ExitCode::SUCCESS
        }
        product => {
            eprintln!("unknown Tensor IPC product `{product}`\n");
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        "usage:\n  tensor-msg land <command>\n  tensor-msg wallpaper <command>\n\nTensor Launcher, Greeter, Settings, and Idle do not expose IPC services."
    );
}
