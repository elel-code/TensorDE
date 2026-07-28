use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

pub(crate) use fika_core::ViewMode as ShellViewMode;

#[derive(Debug)]
pub(crate) struct StartupOptions {
    pub(crate) path: PathBuf,
    pub(crate) view_mode: ShellViewMode,
    pub(crate) view_mode_explicit: bool,
    pub(crate) auto_cycle_views: bool,
}

pub(crate) fn parse_start_options() -> Result<Option<StartupOptions>, String> {
    let mut args = env::args_os();
    let program = args
        .next()
        .and_then(|value| value.into_string().ok())
        .unwrap_or_else(|| "fika".to_string());

    parse_start_options_from(program, args)
}

fn parse_start_options_from(
    program: String,
    args: impl IntoIterator<Item = OsString>,
) -> Result<Option<StartupOptions>, String> {
    let mut args = args.into_iter();
    let mut view_mode = ShellViewMode::Icons;
    let mut view_mode_explicit = false;
    let mut auto_cycle_views = false;
    let mut path = None;
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!("Usage: {program} [--view icons|compact|details] [--auto-cycle-views] [PATH]");
            return Ok(None);
        }
        if arg == "--auto-cycle-views" {
            auto_cycle_views = true;
            continue;
        }
        if arg == "--view" {
            let Some(value) = args.next() else {
                return Err(format!(
                    "usage: {program} [--view icons|compact|details] [--auto-cycle-views] [PATH]"
                ));
            };
            let value = value
                .to_str()
                .ok_or_else(|| "--view value must be valid UTF-8".to_string())?;
            view_mode = ShellViewMode::parse(value)?;
            view_mode_explicit = true;
            continue;
        }
        if let Some(value) = arg.to_str().and_then(|arg| arg.strip_prefix("--view=")) {
            view_mode = ShellViewMode::parse(value)?;
            view_mode_explicit = true;
            continue;
        }
        if arg.to_str().is_some_and(|arg| arg.starts_with("--")) {
            return Err(format!("unknown option: {}", arg.to_string_lossy()));
        }
        if path.replace(PathBuf::from(arg)).is_some() {
            return Err(format!(
                "usage: {program} [--view icons|compact|details] [--auto-cycle-views] [PATH]"
            ));
        }
    }

    let path = match path {
        Some(path) => path,
        None => env::current_dir().map_err(|error| format!("current directory: {error}"))?,
    };
    Ok(Some(StartupOptions {
        path,
        view_mode,
        view_mode_explicit,
        auto_cycle_views,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_view_mode_cycle_and_path() {
        let options = parse_start_options_from(
            "fika".to_string(),
            [
                OsString::from("--view=compact"),
                OsString::from("--auto-cycle-views"),
                OsString::from("/etc"),
            ],
        )
        .unwrap()
        .unwrap();

        assert_eq!(options.path, PathBuf::from("/etc"));
        assert_eq!(options.view_mode, ShellViewMode::Compact);
        assert!(options.view_mode_explicit);
        assert!(options.auto_cycle_views);
    }

    #[test]
    fn parses_split_view_flag_value() {
        let options = parse_start_options_from(
            "fika".to_string(),
            [
                OsString::from("--view"),
                OsString::from("details"),
                OsString::from("/tmp"),
            ],
        )
        .unwrap()
        .unwrap();

        assert_eq!(options.path, PathBuf::from("/tmp"));
        assert_eq!(options.view_mode, ShellViewMode::Details);
        assert!(options.view_mode_explicit);
    }

    #[test]
    fn default_view_mode_is_not_explicit() {
        let options = parse_start_options_from("fika".to_string(), [OsString::from("/tmp")])
            .unwrap()
            .unwrap();

        assert_eq!(options.path, PathBuf::from("/tmp"));
        assert_eq!(options.view_mode, ShellViewMode::Icons);
        assert!(!options.view_mode_explicit);
    }

    #[test]
    fn rejects_duplicate_path_and_unknown_option() {
        assert!(
            parse_start_options_from(
                "fika".to_string(),
                [OsString::from("/etc"), OsString::from("/tmp")]
            )
            .unwrap_err()
            .contains("usage:")
        );
        assert_eq!(
            parse_start_options_from("fika".to_string(), [OsString::from("--bad")]).unwrap_err(),
            "unknown option: --bad"
        );
    }
}
