use std::{env, io, process::ExitCode};

use tensor_settings::{ProductRegistry, SettingsConfig};

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
    let config = SettingsConfig::load_default_path()?;
    let registry = ProductRegistry::from_environment();
    if env::args().nth(1).as_deref() == Some("--check") {
        println!(
            "tensor-settings: {} products, max {} diagnostics, read-only={}",
            registry.endpoints().len(),
            config.max_diagnostics,
            config.read_only
        );
        return Ok(());
    }
    Err(io::Error::other(format!(
        "native Wayland surface is not implemented yet; use `tensor-settings --check` to validate {}",
        SettingsConfig::resolve_path().display()
    ))
    .into())
}
