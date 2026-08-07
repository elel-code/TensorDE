use std::{env, path::PathBuf};

use tensor_xdp::{SettingsService, SettingsSnapshot, TensorXdpConfig};

fn main() {
    if let Err(error) = run() {
        eprintln!("tensor-xdp: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse(env::args_os().skip(1))?;
    if cli.help {
        print_help();
        return Ok(());
    }
    let path = cli.config.unwrap_or_else(TensorXdpConfig::resolve_path);
    let config = TensorXdpConfig::load_or_default(&path)?;
    if cli.check {
        println!("tensor-xdp: configuration valid: {}", path.display());
        return Ok(());
    }
    let service = SettingsService::new(SettingsSnapshot::new(config.appearance));
    tensor_runtime::io_uring_runtime(4)?.block_on(service.run_with_reload(path))?;
    Ok(())
}

#[derive(Debug, Default)]
struct Cli {
    config: Option<PathBuf>,
    check: bool,
    help: bool,
}

impl Cli {
    fn parse(args: impl Iterator<Item = std::ffi::OsString>) -> Result<Self, String> {
        let mut cli = Self::default();
        let mut args = args;
        while let Some(argument) = args.next() {
            match argument.to_str() {
                Some("--config") => {
                    let path = args.next().ok_or("--config requires a path")?;
                    if cli.config.replace(path.into()).is_some() {
                        return Err("--config may be specified only once".to_owned());
                    }
                }
                Some("--check") => cli.check = true,
                Some("-h" | "--help") => cli.help = true,
                Some(value) => return Err(format!("unknown option {value:?}")),
                None => {
                    return Err(
                        "arguments must be valid UTF-8 except for --config paths".to_owned()
                    );
                }
            }
        }
        Ok(cli)
    }
}

fn print_help() {
    println!(
        "Usage: tensor-xdp [--config PATH] [--check]\n\n\
         Dedicated xdg-desktop-portal backend for TensorDE.\n\n\
         Options:\n\
           --config PATH  Load an explicit KDL configuration.\n\
           --check        Validate configuration without connecting to D-Bus.\n\
           -h, --help     Show this help."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_keeps_configuration_validation_independent_from_dbus() {
        let cli = Cli::parse(
            ["--config", "/tmp/xdp.kdl", "--check"]
                .into_iter()
                .map(std::ffi::OsString::from),
        )
        .unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("/tmp/xdp.kdl")));
        assert!(cli.check);
    }
}
