#![allow(unexpected_cfgs)] // `tensor-kdl` derive emits optional downstream DOM impls.

use std::{collections::BTreeSet, path::Path};

use tensor_kdl::{Decode, DecodeDocument};

use super::{ConfigDiagnostic, ConfigPreview};
use crate::ProductKind;

const MAX_QUERY_RESULTS: u64 = 64;
const MAX_CATALOG_ENTRIES: u64 = 131_072;
const MAX_CATALOG_DIAGNOSTICS: u64 = 128;
const MAX_USERS: u64 = 256;
const MAX_AUTH_MESSAGE_BYTES: u64 = 16 * 1024;
const MAX_TIMEOUT_SECONDS: u64 = u32::MAX as u64 / 1_000;
const MIN_FILES_PREVIEW_SIZE: u16 = 16;
const MAX_FILES_PREVIEW_SIZE: u16 = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LauncherConfigPreview {
    pub max_results: u32,
    pub max_catalog_entries: u32,
    pub max_diagnostics: u32,
    pub application_directories: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GreeterConfigPreview {
    pub max_users: u32,
    pub max_auth_message_bytes: u32,
    pub session_count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FilesConfigPreview {
    pub places_sidebar_width: Option<f32>,
    pub places_sidebar_visible: Option<bool>,
    pub view_mode: Option<String>,
    pub show_hidden: Option<bool>,
    pub icons_preview_size: Option<u16>,
    pub compact_preview_size: Option<u16>,
    pub details_preview_size: Option<u16>,
    pub dark_mode: Option<bool>,
    pub background_blur: Option<bool>,
    pub background_opacity: Option<f32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PowerPolicyPreview {
    pub monitor_off_after_seconds: Option<u32>,
    pub lock_after_seconds: Option<u32>,
    pub suspend_after_seconds: Option<u32>,
    pub post_lock_monitor_off_after_seconds: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdleConfigPreview {
    pub enabled: bool,
    pub respect_inhibitors: bool,
    pub ac: PowerPolicyPreview,
    pub battery: PowerPolicyPreview,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XdpColorScheme {
    NoPreference,
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XdpContrast {
    Normal,
    High,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XdpConfigPreview {
    pub color_scheme: XdpColorScheme,
    pub contrast: XdpContrast,
    pub reduced_motion: bool,
}

pub(super) fn validate(
    product: ProductKind,
    path: &Path,
    source: &str,
) -> Result<Option<ConfigPreview>, ConfigDiagnostic> {
    let preview = match product {
        ProductKind::Launcher => ConfigPreview::Launcher(validate_launcher(path, source)?),
        ProductKind::Greeter => ConfigPreview::Greeter(validate_greeter(path, source)?),
        ProductKind::Files => ConfigPreview::Files(validate_files(path, source)?),
        ProductKind::Idle => ConfigPreview::Idle(validate_idle(path, source)?),
        ProductKind::Xdp => ConfigPreview::Xdp(validate_xdp(path, source)?),
        _ => return Ok(None),
    };
    Ok(Some(preview))
}

#[derive(Debug, Default, Decode)]
struct FilesFile {
    #[kdl(child)]
    places: Option<FilesPlaces>,
    #[kdl(child)]
    view: Option<FilesView>,
    #[kdl(child)]
    appearance: Option<FilesAppearance>,
}

#[derive(Debug, Default, Decode)]
struct FilesPlaces {
    #[kdl(child)]
    sidebar: Option<FilesSidebar>,
}

#[derive(Debug, Default, Decode)]
struct FilesSidebar {
    #[kdl(child, unwrap(argument))]
    width: Option<f32>,
    #[kdl(child, unwrap(argument))]
    visible: Option<bool>,
}

#[derive(Debug, Default, Decode)]
struct FilesView {
    #[kdl(child, unwrap(argument))]
    mode: Option<String>,
    #[kdl(child(name = "show-hidden"), unwrap(argument))]
    show_hidden: Option<bool>,
    #[kdl(child(name = "icons-preview-size"), unwrap(argument))]
    icons_preview_size: Option<u16>,
    #[kdl(child(name = "compact-preview-size"), unwrap(argument))]
    compact_preview_size: Option<u16>,
    #[kdl(child(name = "details-preview-size"), unwrap(argument))]
    details_preview_size: Option<u16>,
}

#[derive(Debug, Default, Decode)]
struct FilesAppearance {
    #[kdl(child(name = "dark-mode"), unwrap(argument))]
    dark_mode: Option<bool>,
    #[kdl(child(name = "background-blur"), unwrap(argument))]
    background_blur: Option<bool>,
    #[kdl(child(name = "background-opacity"), unwrap(argument))]
    background_opacity: Option<f32>,
}

fn validate_files(path: &Path, source: &str) -> Result<FilesConfigPreview, ConfigDiagnostic> {
    let file: FilesFile = parse(path, source)?;
    let sidebar = file
        .places
        .and_then(|places| places.sidebar)
        .unwrap_or_default();
    validate_positive_finite("places.sidebar.width", sidebar.width)?;
    let view = file.view.unwrap_or_default();
    if let Some(mode) = view.mode.as_deref()
        && !matches!(mode, "icons" | "compact" | "details")
    {
        return Err(diagnostic(format!("view.mode is invalid: {mode:?}")));
    }
    validate_preview_size("view.icons-preview-size", view.icons_preview_size)?;
    validate_preview_size("view.compact-preview-size", view.compact_preview_size)?;
    validate_preview_size("view.details-preview-size", view.details_preview_size)?;
    let appearance = file.appearance.unwrap_or_default();
    validate_unit_interval(
        "appearance.background-opacity",
        appearance.background_opacity,
    )?;
    Ok(FilesConfigPreview {
        places_sidebar_width: sidebar.width,
        places_sidebar_visible: sidebar.visible,
        view_mode: view.mode,
        show_hidden: view.show_hidden,
        icons_preview_size: view.icons_preview_size,
        compact_preview_size: view.compact_preview_size,
        details_preview_size: view.details_preview_size,
        dark_mode: appearance.dark_mode,
        background_blur: appearance.background_blur,
        background_opacity: appearance.background_opacity,
    })
}

fn validate_positive_finite(field: &str, value: Option<f32>) -> Result<(), ConfigDiagnostic> {
    if value.is_some_and(|value| !value.is_finite() || value <= 0.0) {
        return Err(diagnostic(format!(
            "{field} must be finite and greater than zero"
        )));
    }
    Ok(())
}

fn validate_unit_interval(field: &str, value: Option<f32>) -> Result<(), ConfigDiagnostic> {
    if value.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err(diagnostic(format!(
            "{field} must be finite and in 0.0..=1.0"
        )));
    }
    Ok(())
}

fn validate_preview_size(field: &str, value: Option<u16>) -> Result<(), ConfigDiagnostic> {
    if value
        .is_some_and(|value| !(MIN_FILES_PREVIEW_SIZE..=MAX_FILES_PREVIEW_SIZE).contains(&value))
    {
        return Err(diagnostic(format!(
            "{field} must be in {MIN_FILES_PREVIEW_SIZE}..={MAX_FILES_PREVIEW_SIZE}"
        )));
    }
    Ok(())
}

fn parse<'a, T>(path: &Path, source: &'a str) -> Result<T, ConfigDiagnostic>
where
    T: DecodeDocument<'a> + Default,
{
    tensor_kdl::read(source).map_err(|error| ConfigDiagnostic {
        message: tensor_kdl::format_error_named(&error, source, &path.display().to_string()),
    })
}

#[derive(Debug, Default, Decode)]
struct LauncherFile {
    #[kdl(child(name = "max-results"), unwrap(argument))]
    max_results: Option<u64>,
    #[kdl(child(name = "max-catalog-entries"), unwrap(argument))]
    max_catalog_entries: Option<u64>,
    #[kdl(child(name = "max-diagnostics"), unwrap(argument))]
    max_diagnostics: Option<u64>,
    #[kdl(children(name = "application-directory"))]
    application_directories: Vec<LauncherDirectory>,
}

#[derive(Debug, Decode)]
struct LauncherDirectory {
    #[kdl(argument)]
    path: String,
}

fn validate_launcher(path: &Path, source: &str) -> Result<LauncherConfigPreview, ConfigDiagnostic> {
    let file: LauncherFile = parse(path, source)?;
    Ok(LauncherConfigPreview {
        max_results: bounded("max-results", file.max_results, 10, MAX_QUERY_RESULTS)? as u32,
        max_catalog_entries: bounded(
            "max-catalog-entries",
            file.max_catalog_entries,
            32_768,
            MAX_CATALOG_ENTRIES,
        )? as u32,
        max_diagnostics: bounded(
            "max-diagnostics",
            file.max_diagnostics,
            32,
            MAX_CATALOG_DIAGNOSTICS,
        )? as u32,
        application_directories: file
            .application_directories
            .into_iter()
            .map(|directory| directory.path)
            .collect(),
    })
}

#[derive(Debug, Default, Decode)]
struct GreeterFile {
    #[kdl(child(name = "greetd-socket"), unwrap(argument))]
    greetd_socket: Option<String>,
    #[kdl(child(name = "max-users"), unwrap(argument))]
    max_users: Option<u64>,
    #[kdl(child(name = "max-auth-message-bytes"), unwrap(argument))]
    max_auth_message_bytes: Option<u64>,
    #[kdl(children(name = "session"))]
    sessions: Vec<GreeterSession>,
}

#[derive(Debug, Decode)]
struct GreeterSession {
    #[kdl(argument)]
    id: String,
    #[kdl(child, unwrap(argument))]
    label: String,
    #[kdl(child)]
    command: GreeterArguments,
    #[kdl(child)]
    environment: Option<GreeterArguments>,
}

#[derive(Debug, Decode)]
struct GreeterArguments {
    #[kdl(arguments)]
    values: Vec<String>,
}

fn validate_greeter(path: &Path, source: &str) -> Result<GreeterConfigPreview, ConfigDiagnostic> {
    let file: GreeterFile = parse(path, source)?;
    if file.greetd_socket.as_deref().is_some_and(str::is_empty) {
        return Err(diagnostic("greetd-socket must not be empty"));
    }
    let max_users = bounded("max-users", file.max_users, 128, MAX_USERS)? as u32;
    let max_auth_message_bytes = bounded(
        "max-auth-message-bytes",
        file.max_auth_message_bytes,
        4096,
        MAX_AUTH_MESSAGE_BYTES,
    )? as u32;
    if file.sessions.len() > 64 {
        return Err(diagnostic("at most 64 sessions may be configured"));
    }
    let mut session_ids = BTreeSet::new();
    for session in &file.sessions {
        if session.id.trim().is_empty() || session.label.trim().is_empty() {
            return Err(diagnostic("session ids and labels must not be empty"));
        }
        if !session_ids.insert(&session.id) {
            return Err(diagnostic(format!(
                "session `{}` is configured more than once",
                session.id
            )));
        }
        if session
            .command
            .values
            .first()
            .is_none_or(|value| value.is_empty())
        {
            return Err(diagnostic(format!(
                "session `{}` requires a non-empty command",
                session.id
            )));
        }
        if let Some(environment) = &session.environment {
            for value in &environment.values {
                let Some((name, _)) = value.split_once('=') else {
                    return Err(diagnostic(format!(
                        "session `{}` has an environment entry without `=`",
                        session.id
                    )));
                };
                if name.is_empty() || name.contains('\0') {
                    return Err(diagnostic(format!(
                        "session `{}` has an invalid environment name",
                        session.id
                    )));
                }
            }
        }
    }
    Ok(GreeterConfigPreview {
        max_users,
        max_auth_message_bytes,
        session_count: file.sessions.len() as u32,
    })
}

#[derive(Debug, Default, Decode)]
struct IdleFile {
    #[kdl(child, unwrap(argument))]
    enabled: Option<bool>,
    #[kdl(child(name = "respect-inhibitors"), unwrap(argument))]
    respect_inhibitors: Option<bool>,
    #[kdl(child)]
    ac: Option<IdlePowerFile>,
    #[kdl(child)]
    battery: Option<IdlePowerFile>,
}

#[derive(Debug, Default, Decode)]
struct IdlePowerFile {
    #[kdl(child(name = "monitor-off-after-seconds"), unwrap(argument))]
    monitor_off_after_seconds: Option<u64>,
    #[kdl(child(name = "lock-after-seconds"), unwrap(argument))]
    lock_after_seconds: Option<u64>,
    #[kdl(child(name = "suspend-after-seconds"), unwrap(argument))]
    suspend_after_seconds: Option<u64>,
    #[kdl(child(name = "post-lock-monitor-off-after-seconds"), unwrap(argument))]
    post_lock_monitor_off_after_seconds: Option<u64>,
}

fn validate_idle(path: &Path, source: &str) -> Result<IdleConfigPreview, ConfigDiagnostic> {
    let file: IdleFile = parse(path, source)?;
    let defaults = IdleConfigPreview {
        enabled: true,
        respect_inhibitors: true,
        ac: PowerPolicyPreview {
            monitor_off_after_seconds: Some(600),
            lock_after_seconds: Some(900),
            suspend_after_seconds: Some(1800),
            post_lock_monitor_off_after_seconds: Some(30),
        },
        battery: PowerPolicyPreview {
            monitor_off_after_seconds: Some(300),
            lock_after_seconds: Some(600),
            suspend_after_seconds: Some(900),
            post_lock_monitor_off_after_seconds: Some(15),
        },
    };
    Ok(IdleConfigPreview {
        enabled: file.enabled.unwrap_or(defaults.enabled),
        respect_inhibitors: file
            .respect_inhibitors
            .unwrap_or(defaults.respect_inhibitors),
        ac: resolve_power(file.ac, defaults.ac)?,
        battery: resolve_power(file.battery, defaults.battery)?,
    })
}

fn resolve_power(
    file: Option<IdlePowerFile>,
    defaults: PowerPolicyPreview,
) -> Result<PowerPolicyPreview, ConfigDiagnostic> {
    let Some(file) = file else {
        return Ok(defaults);
    };
    Ok(PowerPolicyPreview {
        monitor_off_after_seconds: timeout(
            file.monitor_off_after_seconds,
            defaults.monitor_off_after_seconds,
        )?,
        lock_after_seconds: timeout(file.lock_after_seconds, defaults.lock_after_seconds)?,
        suspend_after_seconds: timeout(file.suspend_after_seconds, defaults.suspend_after_seconds)?,
        post_lock_monitor_off_after_seconds: timeout(
            file.post_lock_monitor_off_after_seconds,
            defaults.post_lock_monitor_off_after_seconds,
        )?,
    })
}

fn timeout(configured: Option<u64>, default: Option<u32>) -> Result<Option<u32>, ConfigDiagnostic> {
    let Some(seconds) = configured else {
        return Ok(default);
    };
    if seconds > MAX_TIMEOUT_SECONDS {
        return Err(diagnostic(format!(
            "idle timeout must be at most {MAX_TIMEOUT_SECONDS} seconds, got {seconds}"
        )));
    }
    Ok((seconds != 0).then_some(seconds as u32))
}

#[derive(Debug, Default, Decode)]
struct XdpFile {
    #[kdl(child)]
    appearance: Option<XdpAppearanceFile>,
}

#[derive(Debug, Default, Decode)]
struct XdpAppearanceFile {
    #[kdl(property(name = "color-scheme"))]
    color_scheme: Option<String>,
    #[kdl(property)]
    contrast: Option<String>,
    #[kdl(property(name = "reduced-motion"))]
    reduced_motion: Option<bool>,
}

fn validate_xdp(path: &Path, source: &str) -> Result<XdpConfigPreview, ConfigDiagnostic> {
    let file: XdpFile = parse(path, source)?;
    let appearance = file.appearance.unwrap_or_default();
    let color_scheme = match appearance
        .color_scheme
        .as_deref()
        .unwrap_or("no-preference")
    {
        "no-preference" => XdpColorScheme::NoPreference,
        "dark" => XdpColorScheme::Dark,
        "light" => XdpColorScheme::Light,
        value => {
            return Err(diagnostic(format!(
                "appearance.color-scheme is invalid: {value:?}"
            )));
        }
    };
    let contrast = match appearance.contrast.as_deref().unwrap_or("normal") {
        "normal" => XdpContrast::Normal,
        "high" => XdpContrast::High,
        value => {
            return Err(diagnostic(format!(
                "appearance.contrast is invalid: {value:?}"
            )));
        }
    };
    Ok(XdpConfigPreview {
        color_scheme,
        contrast,
        reduced_motion: appearance.reduced_motion.unwrap_or(false),
    })
}

fn bounded(
    field: &'static str,
    configured: Option<u64>,
    default: u64,
    maximum: u64,
) -> Result<u64, ConfigDiagnostic> {
    let value = configured.unwrap_or(default);
    if value == 0 || value > maximum {
        return Err(diagnostic(format!(
            "{field} must be in 1..={maximum}, got {value}"
        )));
    }
    Ok(value)
}

fn diagnostic(message: impl Into<String>) -> ConfigDiagnostic {
    ConfigDiagnostic {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_preview_matches_runtime_bounds() {
        let preview = validate_launcher(
            Path::new("launcher.kdl"),
            "max-results 64\napplication-directory \"/usr/share/applications\"",
        )
        .unwrap();
        assert_eq!(preview.max_results, 64);
        assert_eq!(preview.application_directories, ["/usr/share/applications"]);
        assert!(validate_launcher(Path::new("launcher.kdl"), "max-results 65").is_err());
    }

    #[test]
    fn files_preview_matches_runtime_schema() {
        let preview = validate_files(
            Path::new("files.kdl"),
            r#"
places { sidebar { width 288.5; visible #true; } }
view { mode "details"; show-hidden #true; icons-preview-size 96; }
appearance { dark-mode #true; background-opacity 0.8; }
"#,
        )
        .unwrap();
        assert_eq!(preview.places_sidebar_width, Some(288.5));
        assert_eq!(preview.view_mode.as_deref(), Some("details"));
        assert_eq!(preview.icons_preview_size, Some(96));
        assert_eq!(preview.background_opacity, Some(0.8));
        assert!(validate_files(Path::new("files.kdl"), "view { mode \"grid\" }").is_err());
        assert!(validate_files(Path::new("files.kdl"), "places { sidebar { width 0 } }").is_err());
        assert!(
            validate_files(Path::new("files.kdl"), "view { compact-preview-size 512 }").is_err()
        );
        assert!(
            validate_files(
                Path::new("files.kdl"),
                "appearance { background-opacity #nan }"
            )
            .is_err()
        );
        assert!(
            validate_files(
                Path::new("files.kdl"),
                "appearance { background-opacity -0.1 }"
            )
            .is_err()
        );
    }

    #[test]
    fn greeter_preview_rejects_runtime_invalid_sessions() {
        let duplicate = r#"
            session "land" { label "Land"; command "tensor-session"; }
            session "land" { label "Other"; command "other"; }
        "#;
        assert!(validate_greeter(Path::new("greeter.kdl"), duplicate).is_err());
        let invalid_environment = r#"
            session "land" {
                label "Land"
                command "tensor-session"
                environment "=value"
            }
        "#;
        assert!(validate_greeter(Path::new("greeter.kdl"), invalid_environment).is_err());
    }

    #[test]
    fn idle_preview_preserves_zero_as_disabled_and_checks_wire_range() {
        let preview = validate_idle(
            Path::new("idle.kdl"),
            "ac { lock-after-seconds 0 }\nbattery { suspend-after-seconds 120 }",
        )
        .unwrap();
        assert_eq!(preview.ac.lock_after_seconds, None);
        assert_eq!(preview.battery.suspend_after_seconds, Some(120));
        assert!(validate_idle(Path::new("idle.kdl"), "ac { lock-after-seconds 4294968 }").is_err());
    }

    #[test]
    fn xdp_preview_uses_standardized_appearance_values() {
        let preview = validate_xdp(
            Path::new("xdp.kdl"),
            r#"appearance color-scheme="dark" contrast="high" reduced-motion=#true"#,
        )
        .unwrap();
        assert_eq!(preview.color_scheme, XdpColorScheme::Dark);
        assert_eq!(preview.contrast, XdpContrast::High);
        assert!(preview.reduced_motion);
    }
}
