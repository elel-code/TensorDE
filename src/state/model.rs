use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default)]
    pub default_wallpaper: Option<WallpaperAssignment>,
    #[serde(default)]
    pub outputs: BTreeMap<String, OutputState>,
}

impl AppState {
    pub fn set_wallpaper(&mut self, output: Option<&str>, wallpaper: impl Into<String>) {
        let assignment = WallpaperAssignment {
            path: wallpaper.into(),
            variant: None,
        };
        match output {
            Some(output) => {
                self.outputs.entry(output.to_owned()).or_default().wallpaper = Some(assignment);
            }
            None => {
                self.default_wallpaper = Some(assignment.clone());
                for output in self.outputs.values_mut() {
                    output.wallpaper = Some(assignment.clone());
                }
            }
        }
    }

    pub fn pause(&mut self, output: Option<&str>, paused: bool) {
        match output {
            Some(output) => {
                self.outputs.entry(output.to_owned()).or_default().paused = paused;
            }
            None => {
                for output in self.outputs.values_mut() {
                    output.paused = paused;
                }
            }
        }
    }

    pub fn stop(&mut self, output: Option<&str>) {
        match output {
            Some(output) => {
                if let Some(state) = self.outputs.get_mut(output) {
                    state.wallpaper = None;
                    state.paused = false;
                }
            }
            None => {
                self.default_wallpaper = None;
                for state in self.outputs.values_mut() {
                    state.wallpaper = None;
                    state.paused = false;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OutputState {
    #[serde(default)]
    pub wallpaper: Option<WallpaperAssignment>,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WallpaperAssignment {
    pub path: String,
    #[serde(default)]
    pub variant: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_wallpaper_for_specific_output() {
        let mut state = AppState::default();
        state.set_wallpaper(Some("eDP-1"), "demo.gwpdir");
        assert_eq!(
            state.outputs["eDP-1"].wallpaper.as_ref().unwrap().path,
            "demo.gwpdir"
        );
    }

    #[test]
    fn pause_all_outputs_does_not_create_outputs() {
        let mut state = AppState::default();
        state.pause(None, true);
        assert!(state.outputs.is_empty());
    }
}
