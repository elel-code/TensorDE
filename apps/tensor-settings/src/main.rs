use std::{env, io, process::ExitCode};

use tensor_settings::{
    ConfigDocumentState, ProductRegistry, SettingsConfig, SettingsSurface, SettingsWorkspace,
};

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
    let command = env::args().nth(1);
    if command.as_deref() == Some("--reload-land") {
        let socket = registry
            .endpoint(tensor_settings::ProductKind::Land)
            .socket_path
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Tensorland IPC socket"))?;
        let runtime = tensor_runtime::io_uring_runtime(8)?;
        runtime.block_on(async {
            let mut client = tensor_ipc::land::CompioClient::connect(&socket).await?;
            match client.call(tensor_ipc::land::Command::ReloadConfig).await? {
                tensor_ipc::land::ResultBody::Accepted => Ok::<_, Box<dyn std::error::Error>>(()),
                _ => Err(io::Error::other("Tensorland rejected the reload request").into()),
            }
        })?;
        println!("tensor-settings: Tensorland reload accepted");
        return Ok(());
    }
    if command.as_deref() == Some("--check") {
        let workspace = SettingsWorkspace::open(&registry, &config)?;
        for document in workspace.documents() {
            if let Err(diagnostic) = document.preview() {
                return Err(io::Error::other(format!(
                    "{}: {diagnostic}",
                    document.endpoint().product.title()
                ))
                .into());
            }
        }
        let editable = workspace
            .documents()
            .iter()
            .filter(|document| document.state() != ConfigDocumentState::Unsupported)
            .count();
        println!(
            "tensor-settings: {} products, {editable} KDL documents, max {} diagnostics, read-only={}",
            workspace.documents().len(),
            config.max_diagnostics,
            config.read_only
        );
        return Ok(());
    }
    if command.is_none() || command.as_deref() == Some("--surface") {
        let io = tensor_runtime::io_uring_runtime(16)?;
        SettingsSurface::open(SettingsWorkspace::open(&registry, &config)?)?.run(&io)?;
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("unknown tensor-settings command {}", command.unwrap()),
    )
    .into())
}
