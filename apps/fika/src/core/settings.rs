use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::pane::ViewMode;

const SETTINGS_FILE_NAME: &str = "settings.tsv";
const PLACES_SIDEBAR_WIDTH_KEY: &str = "places.sidebar.width";
const PLACES_SIDEBAR_VISIBLE_KEY: &str = "places.sidebar.visible";
const VIEW_MODE_KEY: &str = "view.mode";
const VIEW_SHOW_HIDDEN_KEY: &str = "view.show_hidden";
const APPEARANCE_DARK_MODE_KEY: &str = "appearance.dark_mode";
const APPEARANCE_BACKGROUND_BLUR_KEY: &str = "appearance.background_blur";
const APPEARANCE_BACKGROUND_OPACITY_KEY: &str = "appearance.background_opacity";

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
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AppearanceSettings {
    pub dark_mode: Option<bool>,
    pub background_blur: Option<bool>,
    pub background_opacity: Option<f32>,
}

pub fn default_app_settings_path() -> PathBuf {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    app_settings_path_for_config_home(config_home)
}

fn app_settings_path_for_config_home(config_home: PathBuf) -> PathBuf {
    config_home.join("fika").join(SETTINGS_FILE_NAME)
}

pub fn load_app_settings(path: &Path) -> io::Result<AppSettings> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(parse_app_settings(&contents)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(AppSettings::default()),
        Err(err) => Err(err),
    }
}

pub fn save_app_settings(path: &Path, settings: &AppSettings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = app_settings_tsv(settings);
    let tmp_path = path.with_extension("tsv.tmp");
    fs::write(&tmp_path, contents)?;
    fs::rename(&tmp_path, path).or_else(|_| {
        fs::write(path, app_settings_tsv(settings))?;
        let _ = fs::remove_file(&tmp_path);
        Ok(())
    })
}

pub fn parse_app_settings(contents: &str) -> AppSettings {
    let mut settings = AppSettings::default();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('\t') else {
            continue;
        };
        match key.trim() {
            PLACES_SIDEBAR_WIDTH_KEY => {
                if let Some(width) = parse_finite_f32(value) {
                    settings.places_sidebar.width = Some(width);
                }
            }
            PLACES_SIDEBAR_VISIBLE_KEY => {
                if let Some(visible) = parse_bool(value) {
                    settings.places_sidebar.visible = Some(visible);
                }
            }
            VIEW_MODE_KEY => {
                if let Ok(mode) = ViewMode::parse(value.trim()) {
                    settings.view.mode = Some(mode);
                }
            }
            VIEW_SHOW_HIDDEN_KEY => {
                if let Some(show_hidden) = parse_bool(value) {
                    settings.view.show_hidden = Some(show_hidden);
                }
            }
            APPEARANCE_DARK_MODE_KEY => {
                if let Some(dark_mode) = parse_bool(value) {
                    settings.appearance.dark_mode = Some(dark_mode);
                }
            }
            APPEARANCE_BACKGROUND_BLUR_KEY => {
                if let Some(background_blur) = parse_bool(value) {
                    settings.appearance.background_blur = Some(background_blur);
                }
            }
            APPEARANCE_BACKGROUND_OPACITY_KEY => {
                if let Some(background_opacity) = parse_finite_f32(value) {
                    settings.appearance.background_opacity = Some(background_opacity);
                }
            }
            _ => {}
        }
    }
    settings
}

pub fn app_settings_tsv(settings: &AppSettings) -> String {
    let mut lines = Vec::new();
    if let Some(width) = settings.places_sidebar.width {
        lines.push(format!("{PLACES_SIDEBAR_WIDTH_KEY}\t{width:.3}"));
    }
    if let Some(visible) = settings.places_sidebar.visible {
        lines.push(format!("{PLACES_SIDEBAR_VISIBLE_KEY}\t{visible}"));
    }
    if let Some(mode) = settings.view.mode {
        lines.push(format!("{VIEW_MODE_KEY}\t{}", mode.as_str()));
    }
    if let Some(show_hidden) = settings.view.show_hidden {
        lines.push(format!("{VIEW_SHOW_HIDDEN_KEY}\t{show_hidden}"));
    }
    if let Some(dark_mode) = settings.appearance.dark_mode {
        lines.push(format!("{APPEARANCE_DARK_MODE_KEY}\t{dark_mode}"));
    }
    if let Some(background_blur) = settings.appearance.background_blur {
        lines.push(format!(
            "{APPEARANCE_BACKGROUND_BLUR_KEY}\t{background_blur}"
        ));
    }
    if let Some(background_opacity) = settings.appearance.background_opacity {
        lines.push(format!(
            "{APPEARANCE_BACKGROUND_OPACITY_KEY}\t{background_opacity:.3}"
        ));
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn parse_finite_f32(value: &str) -> Option<f32> {
    let value = value.trim().parse::<f32>().ok()?;
    value.is_finite().then_some(value)
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_app_settings_path_is_fika_scoped() {
        assert_eq!(
            app_settings_path_for_config_home(PathBuf::from("/xdg/config")),
            PathBuf::from("/xdg/config/fika/settings.tsv")
        );
    }

    #[test]
    fn parse_app_settings_accepts_known_places_sidebar_keys() {
        let settings = parse_app_settings(
            "\
places.sidebar.width\t276.5
places.sidebar.visible\tfalse
view.mode\tdetails
view.show_hidden\ttrue
appearance.dark_mode\ttrue
appearance.background_blur\ttrue
appearance.background_opacity\t0.825
ignored.key\tvalue
places.sidebar.width\tnan
places.sidebar.visible\tmaybe
view.mode\tunknown
view.show_hidden\tmaybe
appearance.dark_mode\tmaybe
appearance.background_blur\tmaybe
appearance.background_opacity\tnan
",
        );

        assert_eq!(settings.places_sidebar.width, Some(276.5));
        assert_eq!(settings.places_sidebar.visible, Some(false));
        assert_eq!(settings.view.mode, Some(ViewMode::Details));
        assert_eq!(settings.view.show_hidden, Some(true));
        assert_eq!(settings.appearance.dark_mode, Some(true));
        assert_eq!(settings.appearance.background_blur, Some(true));
        assert_eq!(settings.appearance.background_opacity, Some(0.825));
    }

    #[test]
    fn legacy_window_opacity_is_not_migrated() {
        let settings = parse_app_settings("appearance.window_opacity\t0.7\n");

        assert_eq!(settings.appearance.background_opacity, None);
        assert!(!app_settings_tsv(&settings).contains("window_opacity"));
    }

    #[test]
    fn save_and_load_app_settings_round_trips() {
        let root = env::temp_dir().join(format!(
            "fika-settings-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("nested/settings.tsv");
        let settings = AppSettings {
            places_sidebar: PlacesSidebarSettings {
                width: Some(311.25),
                visible: Some(false),
            },
            view: ViewSettings {
                mode: Some(ViewMode::Compact),
                show_hidden: Some(true),
            },
            appearance: AppearanceSettings {
                dark_mode: Some(true),
                background_blur: Some(true),
                background_opacity: Some(0.8),
            },
        };

        save_app_settings(&path, &settings).unwrap();
        assert_eq!(load_app_settings(&path).unwrap(), settings);
        let _ = fs::remove_dir_all(root);
    }
}
