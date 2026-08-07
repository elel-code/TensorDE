use std::{env, io, process::ExitCode, time::Duration};

use tensor_launcher::{
    LaunchPlan, LauncherCatalog, LauncherCatalogWatcher, LauncherClient, LauncherConfig,
    LauncherSession, LauncherSurface,
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
    let config = LauncherConfig::load_default_path()?;
    let mut arguments = env::args().skip(1);
    let first = arguments.next();
    if first.as_deref() == Some("--watch") {
        let mut watcher = LauncherCatalogWatcher::start(config)?;
        println!(
            "tensor-launcher: watching {} applications",
            watcher.catalog().entries().len()
        );
        tensor_runtime::io_uring_runtime(4)?.block_on(async {
            loop {
                compio::runtime::time::sleep(Duration::from_secs(1)).await;
                match watcher.refresh_if_changed() {
                    Ok(true) => println!(
                        "tensor-launcher: catalog refreshed, {} applications",
                        watcher.catalog().entries().len()
                    ),
                    Ok(false) => {}
                    Err(error) => eprintln!("tensor-launcher: catalog refresh failed: {error}"),
                }
            }
        });
        return Ok(());
    }
    if first.is_none() || first.as_deref() == Some("--surface") {
        let watcher = LauncherCatalogWatcher::start(config.clone())?;
        let mut session = LauncherSession::new(watcher.catalog().clone(), config.max_results);
        if first.as_deref() == Some("--surface") {
            let query = arguments.collect::<Vec<_>>().join(" ");
            if !query.is_empty() {
                session.replace_query(query)?;
            }
        }
        let io = tensor_runtime::io_uring_runtime(16)?;
        LauncherSurface::open_with_watcher(session, Some(watcher))?.run(&io)?;
        return Ok(());
    }
    let catalog = LauncherCatalog::discover(&config)?;
    if first.as_deref() == Some("--check") {
        println!(
            "tensor-launcher: {} applications, {} bounded diagnostics",
            catalog.entries().len(),
            catalog.diagnostics().len()
        );
        return Ok(());
    }
    if first.as_deref() == Some("--launch") {
        let query = arguments.collect::<Vec<_>>().join(" ");
        if query.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--launch requires a non-empty application query",
            )
            .into());
        }
        let result = catalog.query(&query, 1).into_iter().next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no application matches `{query}`"),
            )
        })?;
        let entry = catalog.entry(result);
        let plan = LaunchPlan::for_entry(entry)?;
        tensor_runtime::io_uring_runtime(16)?.block_on(async {
            let mut client = LauncherClient::connect().await?;
            client.submit(plan).await
        })?;
        println!("tensor-launcher: queued {} ({})", entry.name, entry.id);
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
