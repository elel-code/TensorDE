use fika_core::{expand_user_path, home_dir, normalize_start_dir};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    Manager,
    Chooser,
}

#[derive(Clone, Debug)]
pub(crate) struct Args {
    pub(crate) mode: Mode,
    pub(crate) start_dir: PathBuf,
    pub(crate) chooser_directories: bool,
    pub(crate) chooser_multiple: bool,
    pub(crate) chooser_title: Option<String>,
    pub(crate) chooser_accept_label: Option<String>,
    pub(crate) chooser_filter_index: usize,
    pub(crate) chooser_return_filter: bool,
    pub(crate) chooser_choices: Vec<String>,
    pub(crate) chooser_return_choices: bool,
}

impl Args {
    pub(crate) fn parse(args: impl Iterator<Item = String>) -> Self {
        let mut mode = Mode::Manager;
        let mut start_dir = None;
        let mut chooser_directories = false;
        let mut chooser_multiple = false;
        let mut chooser_title = None;
        let mut chooser_accept_label = None;
        let mut chooser_filter_index = 0usize;
        let mut chooser_return_filter = false;
        let mut chooser_choices = Vec::new();
        let mut chooser_return_choices = false;
        let mut pending_title = false;
        let mut pending_accept_label = false;
        let mut pending_filter_index = false;
        let mut pending_choices = false;
        let mut skip_next = false;

        for arg in args {
            if skip_next {
                skip_next = false;
                continue;
            }
            if pending_title {
                chooser_title = (!arg.is_empty()).then_some(arg);
                pending_title = false;
                continue;
            }
            if pending_accept_label {
                chooser_accept_label = (!arg.is_empty()).then_some(arg);
                pending_accept_label = false;
                continue;
            }
            if pending_filter_index {
                chooser_filter_index = arg.parse().unwrap_or_default();
                pending_filter_index = false;
                continue;
            }
            if pending_choices {
                chooser_choices = arg
                    .split('\n')
                    .filter(|choice| !choice.is_empty())
                    .map(str::to_string)
                    .collect();
                pending_choices = false;
                continue;
            }

            match arg.as_str() {
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                "--chooser" => mode = Mode::Chooser,
                "--chooser-directory" => {
                    mode = Mode::Chooser;
                    chooser_directories = true;
                }
                "--chooser-multiple" => {
                    mode = Mode::Chooser;
                    chooser_multiple = true;
                }
                "--chooser-save"
                | "--chooser-save-files"
                | "--chooser-filters"
                | "--chooser-parent-window" => {
                    mode = Mode::Chooser;
                    skip_next = true;
                }
                "--chooser-title" => {
                    mode = Mode::Chooser;
                    pending_title = true;
                }
                "--chooser-accept-label" => {
                    mode = Mode::Chooser;
                    pending_accept_label = true;
                }
                "--chooser-filter-index" => {
                    mode = Mode::Chooser;
                    pending_filter_index = true;
                }
                "--chooser-return-filter" => {
                    mode = Mode::Chooser;
                    chooser_return_filter = true;
                }
                "--chooser-choices" => {
                    mode = Mode::Chooser;
                    pending_choices = true;
                }
                "--chooser-return-choices" => {
                    mode = Mode::Chooser;
                    chooser_return_choices = true;
                }
                _ if start_dir.is_none() => start_dir = Some(expand_user_path(&arg)),
                _ => {}
            }
        }

        let start_dir = normalize_start_dir(start_dir.unwrap_or_else(home_dir));
        Self {
            mode,
            start_dir,
            chooser_directories,
            chooser_multiple,
            chooser_title,
            chooser_accept_label,
            chooser_filter_index,
            chooser_return_filter,
            chooser_choices,
            chooser_return_choices,
        }
    }
}

fn print_help() {
    println!(
        "Usage: fika [--chooser] [START_DIR]\n\n\
         Options:\n\
           --chooser                 Start the file chooser shell.\n\
           --chooser-directory       Select folders instead of files.\n\
           --chooser-multiple        Select more than one path before confirmation.\n\
           --chooser-title TITLE     Use TITLE as the chooser window title.\n\
           --chooser-accept-label L  Use L in the chooser chrome.\n\
           --chooser-filter-index N  Return N as selected filter metadata.\n\
           --chooser-return-filter   Print selected filter metadata before paths.\n\
           --chooser-choices LIST    Preserve portal choice metadata.\n\
           --chooser-return-choices  Print selected choice metadata before paths.\n\
           -h, --help                Show this help."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chooser_mode_without_versioned_dependencies() {
        let args = Args::parse(
            ["--chooser", "--chooser-directory", "/tmp"]
                .into_iter()
                .map(str::to_string),
        );

        assert_eq!(args.mode, Mode::Chooser);
        assert!(args.chooser_directories);
        assert_eq!(args.start_dir, PathBuf::from("/tmp"));
    }

    #[test]
    fn parses_network_start_dir_without_normalizing_to_local_parent() {
        let args = Args::parse(
            ["--chooser", "smb://server/share/Reports"]
                .into_iter()
                .map(str::to_string),
        );

        assert_eq!(args.mode, Mode::Chooser);
        assert_eq!(args.start_dir, PathBuf::from("smb://server/share/Reports"));
    }
}
