use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default)]
    pub default_wallpaper: Option<WallpaperAssignment>,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
    #[serde(default)]
    pub outputs: BTreeMap<String, OutputState>,
}

impl AppState {
    pub fn set_wallpaper(&mut self, output: Option<&str>, wallpaper: impl Into<String>) {
        self.set_wallpaper_with_variant(output, wallpaper, None);
    }

    pub fn set_wallpaper_with_variant(
        &mut self,
        output: Option<&str>,
        wallpaper: impl Into<String>,
        variant: Option<String>,
    ) {
        let assignment = WallpaperAssignment {
            path: wallpaper.into(),
            variant,
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

    pub fn properties(&self, output: Option<&str>) -> BTreeMap<String, Value> {
        match output {
            Some(output) => self
                .outputs
                .get(output)
                .map(|state| state.properties.clone())
                .unwrap_or_default(),
            None => self.properties.clone(),
        }
    }

    pub fn get_property(&self, output: Option<&str>, key: &str) -> Option<Value> {
        match output {
            Some(output) => self
                .outputs
                .get(output)
                .and_then(|state| state.properties.get(key).cloned()),
            None => self.properties.get(key).cloned(),
        }
    }

    pub fn set_property(&mut self, output: Option<&str>, key: impl Into<String>, value: Value) {
        match output {
            Some(output) => {
                self.outputs
                    .entry(output.to_owned())
                    .or_default()
                    .properties
                    .insert(key.into(), value);
            }
            None => {
                self.properties.insert(key.into(), value);
            }
        }
    }

    pub fn unset_property(&mut self, output: Option<&str>, key: &str) -> Option<Value> {
        match output {
            Some(output) => self
                .outputs
                .get_mut(output)
                .and_then(|state| state.properties.remove(key)),
            None => self.properties.remove(key),
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
                self.properties.clear();
                for state in self.outputs.values_mut() {
                    state.wallpaper = None;
                    state.paused = false;
                    state.properties.clear();
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
    fn stores_wallpaper_variant() {
        let mut state = AppState::default();
        state.set_wallpaper_with_variant(Some("eDP-1"), "demo.gwpdir", Some("uhd".to_owned()));
        assert_eq!(
            state.outputs["eDP-1"]
                .wallpaper
                .as_ref()
                .unwrap()
                .variant
                .as_deref(),
            Some("uhd")
        );
    }

    #[test]
    fn pause_all_outputs_does_not_create_outputs() {
        let mut state = AppState::default();
        state.pause(None, true);
        assert!(state.outputs.is_empty());
    }

    #[test]
    fn stores_global_and_output_properties_separately() {
        let mut state = AppState::default();
        state.set_property(None, "speed", Value::from(0.75));
        state.set_property(Some("eDP-1"), "speed", Value::from(0.25));

        assert_eq!(state.get_property(None, "speed"), Some(Value::from(0.75)));
        assert_eq!(
            state.get_property(Some("eDP-1"), "speed"),
            Some(Value::from(0.25))
        );
    }

    #[test]
    fn unsets_properties() {
        let mut state = AppState::default();
        state.set_property(Some("eDP-1"), "enabled", Value::from(true));
        assert_eq!(
            state.unset_property(Some("eDP-1"), "enabled"),
            Some(Value::from(true))
        );
        assert_eq!(state.get_property(Some("eDP-1"), "enabled"), None);
    }
}
