use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Cli {
    /// Read configuration from this KDL file.
    #[arg(short, long)]
    pub config: Option<PathBuf>,
    /// Validate startup dependencies, then exit before the event loop.
    #[arg(long)]
    pub check: bool,
}
