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
    /// Parse and validate only the KDL configuration, then exit.
    #[arg(long, conflicts_with_all = ["check", "session"])]
    pub validate_config: bool,
    /// Start as the primary Wayland session and publish its environment.
    #[arg(long)]
    pub session: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_config_is_a_config_only_mode() {
        let cli = Cli::try_parse_from([
            "tensorland",
            "--validate-config",
            "--config",
            "/tmp/config.kdl",
        ])
        .unwrap();
        assert!(cli.validate_config);
        assert_eq!(cli.config, Some(PathBuf::from("/tmp/config.kdl")));
    }

    #[test]
    fn validate_config_rejects_runtime_modes() {
        assert!(Cli::try_parse_from(["tensorland", "--validate-config", "--check"]).is_err());
        assert!(Cli::try_parse_from(["tensorland", "--validate-config", "--session"]).is_err());
    }
}
