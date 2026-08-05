use std::process::ExitCode;

use tensor_greeter::GreeterConfig;

fn main() -> ExitCode {
    match GreeterConfig::load_default_path() {
        Ok(config) => {
            println!(
                "tensor-greeter: {} sessions, greetd socket {}",
                config.sessions.len(),
                config.greetd_socket.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
