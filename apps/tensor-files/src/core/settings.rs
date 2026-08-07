#![allow(unexpected_cfgs)] // `tensor-kdl` derive emits optional downstream DOM impls.

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tensor_kdl::{Decode, Encode};

use super::pane::{MAX_ZOOM_LEVEL, MIN_ZOOM_LEVEL, ViewMode, icon_size_for_zoom_level};

const SETTINGS_FILE_NAME: &str = "files.kdl";
static NEXT_SETTINGS_TEMP_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppSettings {
    pub places_sidebar: PlacesSidebarSettings,
    pub view: ViewSettings,
    pub appearance: AppearanceSettings,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlacesSidebarSettings {
    pub width: Option<f32>,
    pub visible: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewSettings {
    pub mode: Option<ViewMode>,
    pub show_hidden: Option<bool>,
    pub icons_preview_size: Option<u16>,
    pub compact_preview_size: Option<u16>,
    pub details_preview_size: Option<u16>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AppearanceSettings {
    pub dark_mode: Option<bool>,
    pub background_blur: Option<bool>,
    pub background_opacity: Option<f32>,
}

pub fn default_app_settings_path() -> PathBuf {
    env::var_os("TENSOR_FILES_CONFIG")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            let config_home = env::var_os("XDG_CONFIG_HOME")
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    env::var_os("HOME")
                        .filter(|path| !path.is_empty())
                        .map(|home| PathBuf::from(home).join(".config"))
                })?;
            Some(app_settings_path_for_config_home(config_home))
        })
        .unwrap_or_else(|| PathBuf::from("/etc/tensor").join(SETTINGS_FILE_NAME))
}

fn app_settings_path_for_config_home(config_home: PathBuf) -> PathBuf {
    config_home.join("tensor").join(SETTINGS_FILE_NAME)
}

pub fn load_app_settings(path: &Path) -> io::Result<AppSettings> {
    match fs::read_to_string(path) {
        Ok(contents) => parse_app_settings(&contents),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(AppSettings::default()),
        Err(err) => Err(err),
    }
}

pub fn save_app_settings(path: &Path, settings: &AppSettings) -> io::Result<()> {
    let contents = app_settings_kdl(settings)?;
    atomic_replace_app_settings(path, contents.as_bytes())
}

pub fn parse_app_settings(contents: &str) -> io::Result<AppSettings> {
    let file: SettingsFile = tensor_kdl::read(contents).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            tensor_kdl::format_error(&error, contents),
        )
    })?;
    file.resolve()
}

pub fn app_settings_kdl(settings: &AppSettings) -> io::Result<String> {
    validate_app_settings(settings)?;
    tensor_kdl::to_string(&SettingsFile::from(settings)).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("encode Tensor Files KDL settings: {error}"),
        )
    })
}

#[derive(Debug, Default, Decode, Encode)]
struct SettingsFile {
    #[kdl(child)]
    places: Option<PlacesFile>,
    #[kdl(child)]
    view: Option<ViewFile>,
    #[kdl(child)]
    appearance: Option<AppearanceFile>,
}

#[derive(Debug, Default, Decode, Encode)]
struct PlacesFile {
    #[kdl(child)]
    sidebar: Option<PlacesSidebarFile>,
}

#[derive(Debug, Default, Decode, Encode)]
struct PlacesSidebarFile {
    #[kdl(child, unwrap(argument))]
    width: Option<f32>,
    #[kdl(child, unwrap(argument))]
    visible: Option<bool>,
}

#[derive(Debug, Default, Decode, Encode)]
struct ViewFile {
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

#[derive(Debug, Default, Decode, Encode)]
struct AppearanceFile {
    #[kdl(child(name = "dark-mode"), unwrap(argument))]
    dark_mode: Option<bool>,
    #[kdl(child(name = "background-blur"), unwrap(argument))]
    background_blur: Option<bool>,
    #[kdl(child(name = "background-opacity"), unwrap(argument))]
    background_opacity: Option<f32>,
}

impl SettingsFile {
    fn resolve(self) -> io::Result<AppSettings> {
        let sidebar = self
            .places
            .and_then(|places| places.sidebar)
            .unwrap_or_default();
        let view = self.view.unwrap_or_default();
        let mode = view
            .mode
            .as_deref()
            .map(ViewMode::parse)
            .transpose()
            .map_err(invalid_setting)?;
        let appearance = self.appearance.unwrap_or_default();
        let settings = AppSettings {
            places_sidebar: PlacesSidebarSettings {
                width: sidebar.width,
                visible: sidebar.visible,
            },
            view: ViewSettings {
                mode,
                show_hidden: view.show_hidden,
                icons_preview_size: view.icons_preview_size,
                compact_preview_size: view.compact_preview_size,
                details_preview_size: view.details_preview_size,
            },
            appearance: AppearanceSettings {
                dark_mode: appearance.dark_mode,
                background_blur: appearance.background_blur,
                background_opacity: appearance.background_opacity,
            },
        };
        validate_app_settings(&settings)?;
        Ok(settings)
    }
}

impl From<&AppSettings> for SettingsFile {
    fn from(settings: &AppSettings) -> Self {
        let sidebar = settings.places_sidebar;
        let view = settings.view;
        let appearance = settings.appearance;
        Self {
            places: (sidebar != PlacesSidebarSettings::default()).then_some(PlacesFile {
                sidebar: Some(PlacesSidebarFile {
                    width: sidebar.width,
                    visible: sidebar.visible,
                }),
            }),
            view: (view != ViewSettings::default()).then_some(ViewFile {
                mode: view.mode.map(|mode| mode.as_str().to_string()),
                show_hidden: view.show_hidden,
                icons_preview_size: view.icons_preview_size,
                compact_preview_size: view.compact_preview_size,
                details_preview_size: view.details_preview_size,
            }),
            appearance: (appearance != AppearanceSettings::default()).then_some(AppearanceFile {
                dark_mode: appearance.dark_mode,
                background_blur: appearance.background_blur,
                background_opacity: appearance.background_opacity,
            }),
        }
    }
}

fn atomic_replace_app_settings(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = settings_temporary_path(path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        copy_settings_permissions(path, &file)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn settings_temporary_path(path: &Path) -> PathBuf {
    let id = NEXT_SETTINGS_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("config");
    path.with_extension(format!(
        "{extension}.tensor-files-{}-{id}.tmp",
        std::process::id()
    ))
}

#[cfg(unix)]
fn copy_settings_permissions(path: &Path, temporary: &File) -> io::Result<()> {
    match fs::metadata(path) {
        Ok(metadata) => temporary.set_permissions(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(not(unix))]
fn copy_settings_permissions(_path: &Path, _temporary: &File) -> io::Result<()> {
    Ok(())
}

fn validate_app_settings(settings: &AppSettings) -> io::Result<()> {
    validate_positive_finite("places.sidebar.width", settings.places_sidebar.width)?;
    validate_preview_size("view.icons-preview-size", settings.view.icons_preview_size)?;
    validate_preview_size(
        "view.compact-preview-size",
        settings.view.compact_preview_size,
    )?;
    validate_preview_size(
        "view.details-preview-size",
        settings.view.details_preview_size,
    )?;
    validate_unit_interval(
        "appearance.background-opacity",
        settings.appearance.background_opacity,
    )
}

fn validate_positive_finite(field: &str, value: Option<f32>) -> io::Result<()> {
    if value.is_some_and(|value| !value.is_finite() || value <= 0.0) {
        return Err(invalid_setting(format!(
            "{field} must be finite and greater than zero"
        )));
    }
    Ok(())
}

fn validate_unit_interval(field: &str, value: Option<f32>) -> io::Result<()> {
    if value.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err(invalid_setting(format!(
            "{field} must be finite and in 0.0..=1.0"
        )));
    }
    Ok(())
}

fn validate_preview_size(field: &str, value: Option<u16>) -> io::Result<()> {
    let minimum = icon_size_for_zoom_level(MIN_ZOOM_LEVEL) as u16;
    let maximum = icon_size_for_zoom_level(MAX_ZOOM_LEVEL) as u16;
    if value.is_some_and(|value| !(minimum..=maximum).contains(&value)) {
        return Err(invalid_setting(format!(
            "{field} must be in {minimum}..={maximum}"
        )));
    }
    Ok(())
}

fn invalid_setting(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_app_settings_path_is_tensor_files_scoped() {
        assert_eq!(
            app_settings_path_for_config_home(PathBuf::from("/xdg/config")),
            PathBuf::from("/xdg/config/tensor/files.kdl")
        );
    }

    #[test]
    fn parse_app_settings_accepts_typed_kdl_sections() {
        let settings = parse_app_settings(
            r#"
places {
    sidebar {
        width 276.5
        visible #false
    }
}
view {
    mode "details"
    show-hidden #true
    icons-preview-size 80
    compact-preview-size 64
    details-preview-size 32
}
appearance {
    dark-mode #true
    background-blur #true
    background-opacity 0.825
}
"#,
        )
        .unwrap();

        assert_eq!(settings.places_sidebar.width, Some(276.5));
        assert_eq!(settings.places_sidebar.visible, Some(false));
        assert_eq!(settings.view.mode, Some(ViewMode::Details));
        assert_eq!(settings.view.show_hidden, Some(true));
        assert_eq!(settings.view.icons_preview_size, Some(80));
        assert_eq!(settings.view.compact_preview_size, Some(64));
        assert_eq!(settings.view.details_preview_size, Some(32));
        assert_eq!(settings.appearance.dark_mode, Some(true));
        assert_eq!(settings.appearance.background_blur, Some(true));
        assert_eq!(settings.appearance.background_opacity, Some(0.825));
    }

    #[test]
    fn invalid_kdl_and_values_are_reported() {
        assert!(parse_app_settings("view {").is_err());
        assert!(parse_app_settings("ignored \"value\"").is_err());
        assert!(parse_app_settings("view { mode \"unknown\" }").is_err());
        assert!(parse_app_settings("places { sidebar { width 0.0 } }").is_err());
        assert!(parse_app_settings("view { icons-preview-size 8 }").is_err());
        assert!(parse_app_settings("view { details-preview-size 512 }").is_err());
        assert!(parse_app_settings("appearance { background-opacity #nan }").is_err());
        assert!(parse_app_settings("appearance { background-opacity 1.01 }").is_err());
    }

    #[test]
    fn save_and_load_app_settings_round_trips() {
        let root = env::temp_dir().join(format!(
            "tensor-files-settings-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("nested/files.kdl");
        let settings = AppSettings {
            places_sidebar: PlacesSidebarSettings {
                width: Some(311.25),
                visible: Some(false),
            },
            view: ViewSettings {
                mode: Some(ViewMode::Compact),
                show_hidden: Some(true),
                icons_preview_size: Some(96),
                compact_preview_size: Some(64),
                details_preview_size: Some(32),
            },
            appearance: AppearanceSettings {
                dark_mode: Some(true),
                background_blur: Some(true),
                background_opacity: Some(0.8),
            },
        };

        save_app_settings(&path, &settings).unwrap();
        assert_eq!(load_app_settings(&path).unwrap(), settings);
        let encoded = fs::read_to_string(&path).unwrap();
        assert!(encoded.contains("places"));
        assert!(encoded.contains("show-hidden #true"));
        assert!(encoded.contains("background-opacity 0.8"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_typed_settings_do_not_replace_existing_kdl() {
        let root = env::temp_dir().join(format!(
            "tensor-files-invalid-settings-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("files.kdl");
        let settings = AppSettings {
            view: ViewSettings {
                mode: Some(ViewMode::Icons),
                ..ViewSettings::default()
            },
            ..AppSettings::default()
        };
        save_app_settings(&path, &settings).unwrap();

        let mut invalid = settings.clone();
        invalid.appearance.background_opacity = Some(1.5);
        let error = save_app_settings(&path, &invalid).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(load_app_settings(&path).unwrap(), settings);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        let _ = fs::remove_dir_all(root);
    }
}
