use std::{env, process::ExitCode};

use tensor_greeter::{GreetdClient, GreeterConfig, GreeterModel, GreeterSurface, discover_users};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = GreeterConfig::load_default_path()?;
    let command = env::args().nth(1);
    if command.as_deref() == Some("--check") {
        println!(
            "tensor-greeter: configuration valid, {} sessions, max {} users",
            config.sessions.len(),
            config.max_users
        );
        return Ok(());
    }
    let io = tensor_runtime::io_uring_runtime(16)?;
    if command.as_deref() == Some("--check-greetd") {
        io.block_on(GreetdClient::connect(&config.greetd_socket))?;
        println!(
            "tensor-greeter: connected to {}",
            config.greetd_socket.display()
        );
    } else {
        let users = io.block_on(discover_users(config.max_users))?;
        let model = GreeterModel::new(users, config.sessions, config.max_auth_message_bytes)?;
        GreeterSurface::open(model, config.greetd_socket)?.run(&io)?;
    }
    Ok(())
}
