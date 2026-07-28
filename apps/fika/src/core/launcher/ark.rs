use super::{DesktopLaunchCommand, DesktopLaunchPlan};
use std::path::{Path, PathBuf};

const ARK_DESKTOP_FILE: &str = "/usr/share/applications/org.kde.ark.desktop";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArkCompressionMode {
    Dialog,
    TarGz,
    Zip,
}

impl ArkCompressionMode {
    fn suffix(self) -> Option<&'static str> {
        match self {
            Self::Dialog => None,
            Self::TarGz => Some("tar.gz"),
            Self::Zip => Some("zip"),
        }
    }

    fn desktop_id(self) -> &'static str {
        match self {
            Self::Dialog => "fika-ark-compress-dialog",
            Self::TarGz => "fika-ark-compress-tar-gz",
            Self::Zip => "fika-ark-compress-zip",
        }
    }

    fn app_name(self) -> &'static str {
        match self {
            Self::Dialog => "Ark: Compress To",
            Self::TarGz => "Ark: Compress to TAR.GZ",
            Self::Zip => "Ark: Compress to ZIP",
        }
    }
}

pub fn ark_compress_launch_plan(
    paths: &[PathBuf],
    mode: ArkCompressionMode,
) -> Result<DesktopLaunchPlan, String> {
    if paths.is_empty() {
        return Err("No item selected".to_string());
    }
    let mut args = vec!["--add".to_string(), "--changetofirstpath".to_string()];
    if let Some(suffix) = mode.suffix() {
        args.extend(["--autofilename".to_string(), suffix.to_string()]);
    } else {
        args.push("--dialog".to_string());
    }
    args.extend(paths.iter().map(|path| path.display().to_string()));
    Ok(DesktopLaunchPlan {
        desktop_id: mode.desktop_id().to_string(),
        desktop_file: PathBuf::from(ARK_DESKTOP_FILE),
        app_name: mode.app_name().to_string(),
        commands: vec![DesktopLaunchCommand {
            program: "ark".to_string(),
            args,
        }],
    })
}

pub fn ark_extract_here_launch_plan(archives: &[PathBuf]) -> Result<DesktopLaunchPlan, String> {
    ark_extract_launch_plan(
        "fika-ark-extract-here",
        "Ark: Extract Here",
        archives,
        ArkExtractionMode::Here,
    )
}

pub fn ark_extract_to_launch_plan(archives: &[PathBuf]) -> Result<DesktopLaunchPlan, String> {
    ark_extract_launch_plan(
        "fika-ark-extract-to",
        "Ark: Extract To",
        archives,
        ArkExtractionMode::Dialog,
    )
}

pub fn ark_extract_and_trash_launch_plan(
    archives: &[PathBuf],
) -> Result<DesktopLaunchPlan, String> {
    ark_extract_launch_plan(
        "fika-ark-extract-and-trash",
        "Ark: Extract and Trash Archive",
        archives,
        ArkExtractionMode::Here,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArkExtractionMode {
    Here,
    Dialog,
}

fn ark_extract_launch_plan(
    desktop_id: &str,
    app_name: &str,
    archives: &[PathBuf],
    mode: ArkExtractionMode,
) -> Result<DesktopLaunchPlan, String> {
    if archives.is_empty() {
        return Err("No archive selected".to_string());
    }
    let destination = archive_destination(&archives[0]);
    let mut args = vec!["--batch".to_string()];
    match mode {
        ArkExtractionMode::Here => args.push("--autosubfolder".to_string()),
        ArkExtractionMode::Dialog => args.push("--dialog".to_string()),
    }
    args.extend([
        "--destination".to_string(),
        destination.display().to_string(),
    ]);
    args.extend(archives.iter().map(|archive| archive.display().to_string()));
    Ok(DesktopLaunchPlan {
        desktop_id: desktop_id.to_string(),
        desktop_file: PathBuf::from(ARK_DESKTOP_FILE),
        app_name: app_name.to_string(),
        commands: vec![DesktopLaunchCommand {
            program: "ark".to_string(),
            args,
        }],
    })
}

fn archive_destination(archive: &Path) -> &Path {
    archive
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ark_compress_fallback_builds_direct_suffix_launch_plans() {
        let paths = vec![
            PathBuf::from("/tmp/fika-ark-compress/note.txt"),
            PathBuf::from("/tmp/fika-ark-compress/todo.txt"),
        ];

        let tar_gz = ark_compress_launch_plan(&paths, ArkCompressionMode::TarGz).unwrap();
        assert_eq!(tar_gz.desktop_id, "fika-ark-compress-tar-gz");
        assert_eq!(tar_gz.app_name, "Ark: Compress to TAR.GZ");
        assert_eq!(tar_gz.desktop_file, PathBuf::from(ARK_DESKTOP_FILE));
        assert_eq!(tar_gz.commands.len(), 1);
        assert_eq!(tar_gz.commands[0].program, "ark");
        assert_eq!(
            tar_gz.commands[0].args,
            vec![
                "--add",
                "--changetofirstpath",
                "--autofilename",
                "tar.gz",
                "/tmp/fika-ark-compress/note.txt",
                "/tmp/fika-ark-compress/todo.txt"
            ]
        );

        let zip = ark_compress_launch_plan(&paths, ArkCompressionMode::Zip).unwrap();
        assert_eq!(zip.desktop_id, "fika-ark-compress-zip");
        assert_eq!(zip.app_name, "Ark: Compress to ZIP");
        assert_eq!(
            zip.commands[0].args,
            vec![
                "--add",
                "--changetofirstpath",
                "--autofilename",
                "zip",
                "/tmp/fika-ark-compress/note.txt",
                "/tmp/fika-ark-compress/todo.txt"
            ]
        );
    }

    #[test]
    fn ark_compress_dialog_fallback_builds_systemd_launch_plan() {
        let plan = ark_compress_launch_plan(
            &[PathBuf::from("/tmp/fika-ark-compress/note.txt")],
            ArkCompressionMode::Dialog,
        )
        .unwrap();

        assert_eq!(plan.desktop_id, "fika-ark-compress-dialog");
        assert_eq!(plan.app_name, "Ark: Compress To");
        assert_eq!(plan.desktop_file, PathBuf::from(ARK_DESKTOP_FILE));
        assert_eq!(plan.commands.len(), 1);
        assert_eq!(plan.commands[0].program, "ark");
        assert_eq!(
            plan.commands[0].args,
            vec![
                "--add",
                "--changetofirstpath",
                "--dialog",
                "/tmp/fika-ark-compress/note.txt"
            ]
        );
    }

    #[test]
    fn ark_extract_fallback_builds_systemd_launch_plans() {
        let archives = vec![PathBuf::from("/tmp/fika-ark-extract/archive.zip")];

        let here = ark_extract_here_launch_plan(&archives).unwrap();
        assert_eq!(here.desktop_id, "fika-ark-extract-here");
        assert_eq!(here.app_name, "Ark: Extract Here");
        assert_eq!(here.desktop_file, PathBuf::from(ARK_DESKTOP_FILE));
        assert_eq!(here.commands.len(), 1);
        assert_eq!(here.commands[0].program, "ark");
        assert_eq!(
            here.commands[0].args,
            vec![
                "--batch",
                "--autosubfolder",
                "--destination",
                "/tmp/fika-ark-extract",
                "/tmp/fika-ark-extract/archive.zip"
            ]
        );

        let extract_to = ark_extract_to_launch_plan(&archives).unwrap();
        assert_eq!(extract_to.desktop_id, "fika-ark-extract-to");
        assert_eq!(extract_to.app_name, "Ark: Extract To");
        assert_eq!(extract_to.desktop_file, PathBuf::from(ARK_DESKTOP_FILE));
        assert_eq!(extract_to.commands.len(), 1);
        assert_eq!(extract_to.commands[0].program, "ark");
        assert_eq!(
            extract_to.commands[0].args,
            vec![
                "--batch",
                "--dialog",
                "--destination",
                "/tmp/fika-ark-extract",
                "/tmp/fika-ark-extract/archive.zip"
            ]
        );
    }

    #[test]
    fn ark_extract_fallback_accepts_multiple_archives() {
        let archives = vec![
            PathBuf::from("/tmp/fika-ark-extract/one.zip"),
            PathBuf::from("/tmp/fika-ark-extract/two.tar.gz"),
        ];

        let here = ark_extract_here_launch_plan(&archives).unwrap();

        assert_eq!(
            here.commands[0].args,
            vec![
                "--batch",
                "--autosubfolder",
                "--destination",
                "/tmp/fika-ark-extract",
                "/tmp/fika-ark-extract/one.zip",
                "/tmp/fika-ark-extract/two.tar.gz"
            ]
        );
    }

    #[test]
    fn ark_extract_and_trash_uses_batch_extract_plan() {
        let archives = vec![PathBuf::from("/tmp/fika-ark-extract/archive.zip")];

        let plan = ark_extract_and_trash_launch_plan(&archives).unwrap();

        assert_eq!(plan.desktop_id, "fika-ark-extract-and-trash");
        assert_eq!(plan.app_name, "Ark: Extract and Trash Archive");
        assert_eq!(plan.commands[0].program, "ark");
        assert_eq!(
            plan.commands[0].args,
            vec![
                "--batch",
                "--autosubfolder",
                "--destination",
                "/tmp/fika-ark-extract",
                "/tmp/fika-ark-extract/archive.zip"
            ]
        );
    }
}
