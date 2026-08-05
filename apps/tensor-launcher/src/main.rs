use std::{env, process::ExitCode};

use tensor_launcher::{LauncherCatalog, LauncherConfig};

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
    let config = LauncherConfig::load_default_path()?;
    let catalog = LauncherCatalog::discover(&config)?;
    let mut arguments = env::args().skip(1);
    let first = arguments.next();
    if first.as_deref() == Some("--check") {
        println!(
            "tensor-launcher: {} applications, {} bounded diagnostics",
            catalog.entries().len(),
            catalog.diagnostics().len()
        );
        return Ok(());
    }
    let query = first
        .into_iter()
        .chain(arguments)
        .collect::<Vec<_>>()
        .join(" ");
    for result in catalog.query(&query, config.max_results) {
        let entry = catalog.entry(result);
        println!("{}\t{}\t{}", entry.id, entry.name, entry.exec);
    }
    Ok(())
}
