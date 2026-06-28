use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

const CONTROLLER_SETTING_KEYS: &[&str] = &[
    "allowAutoPlay",
    "cooldownSec",
    "endTimePercent",
    "fadeInDuration",
    "fadeOutDuration",
    "hideWhenPaused",
    "hideWhenStopped",
    "isClickable",
    "loopCount",
    "loopPlay",
    "mouseInactiveSec",
    "playbackSpeed",
    "resetOnClick",
    "resetOnRestart",
    "startDelay",
    "startTimePercent",
    "togglePlay",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SceneControllerKind {
    IdleVideoSwitch,
    ClickVideoSwitch,
    PropertyVideoSwitch,
}

impl SceneControllerKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::IdleVideoSwitch => "idle-video-switch",
            Self::ClickVideoSwitch => "click-video-switch",
            Self::PropertyVideoSwitch => "property-video-switch",
        }
    }

    fn from_wallpaper_engine_utility(
        utility: &str,
        script_properties: &Map<String, Value>,
    ) -> Self {
        if utility == "fullscreenlayer" || script_properties.contains_key("mouseInactiveSec") {
            Self::IdleVideoSwitch
        } else if utility == "composelayer"
            || script_properties.contains_key("isClickable")
            || script_properties.contains_key("togglePlay")
        {
            Self::ClickVideoSwitch
        } else {
            Self::PropertyVideoSwitch
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SceneControllerIr {
    controller_node_id: String,
    utility: String,
    target_layer: String,
    property: String,
    default_hide_target: bool,
    kind: SceneControllerKind,
    settings: BTreeMap<String, Value>,
}

impl SceneControllerIr {
    pub(super) fn from_wallpaper_engine_utility(
        controller_node_id: &str,
        utility: &str,
        target_layer: &str,
        default_hide_target: bool,
        script_properties: &Map<String, Value>,
    ) -> Self {
        let settings = CONTROLLER_SETTING_KEYS
            .iter()
            .filter_map(|key| {
                script_properties
                    .get(*key)
                    .map(|value| (scene_controller_property_name(key), value.clone()))
            })
            .collect();
        Self {
            controller_node_id: controller_node_id.to_owned(),
            utility: utility.to_owned(),
            target_layer: target_layer.to_owned(),
            property: format!("scene.controller.{controller_node_id}.active"),
            default_hide_target,
            kind: SceneControllerKind::from_wallpaper_engine_utility(utility, script_properties),
            settings,
        }
    }

    pub(super) fn controller_node_id(&self) -> &str {
        &self.controller_node_id
    }

    pub(super) fn target_layer(&self) -> &str {
        &self.target_layer
    }

    pub(super) fn default_hide_target(&self) -> bool {
        self.default_hide_target
    }

    pub(super) fn uses_native_idle_input_source(&self) -> bool {
        self.kind == SceneControllerKind::IdleVideoSwitch
    }

    pub(super) fn uses_native_idle_fade_ramp(&self) -> bool {
        self.uses_native_idle_input_source()
            && self
                .settings
                .get("fade_in_duration")
                .and_then(scene_ir_setting_number)
                .is_some_and(|value| value > 0.0)
    }

    pub(super) fn requires_external_input_source(&self) -> bool {
        !self.uses_native_idle_input_source()
    }

    pub(super) fn metadata_value(&self) -> Value {
        let mut controller = Map::new();
        controller.insert("runtime".to_owned(), Value::String("native".to_owned()));
        controller.insert(
            "kind".to_owned(),
            Value::String(self.kind.as_str().to_owned()),
        );
        controller.insert("utility".to_owned(), Value::String(self.utility.clone()));
        controller.insert(
            "target_layer".to_owned(),
            Value::String(self.target_layer.clone()),
        );
        controller.insert("property".to_owned(), Value::String(self.property.clone()));
        controller.insert(
            "default_hide_target".to_owned(),
            json!(self.default_hide_target),
        );
        for (key, value) in &self.settings {
            controller.insert(key.clone(), value.clone());
        }
        Value::Object(controller)
    }

    pub(super) fn property_binding_value(&self, target_node_id: &str) -> Value {
        json!({
            "property": self.property.clone(),
            "target_node": target_node_id,
            "target": "opacity",
            "scale": 1.0,
            "offset": 0.0
        })
    }

    pub(super) fn completed_feature_name(&self) -> String {
        format!("native-scene-controller-{}", self.kind.as_str())
    }
}

fn scene_controller_property_name(key: &str) -> String {
    let mut output = String::new();
    for (index, character) in key.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn scene_ir_setting_number(value: &Value) -> Option<f64> {
    let value = value.get("value").unwrap_or(value);
    let number = match value {
        Value::Bool(value) => {
            if *value {
                1.0
            } else {
                0.0
            }
        }
        Value::Number(value) => value.as_f64()?,
        Value::String(value) => value.parse::<f64>().ok()?,
        _ => return None,
    };
    number.is_finite().then_some(number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_controller_ir_lowers_wallpaper_engine_utility_metadata() {
        let mut script_properties = Map::new();
        script_properties.insert("mouseInactiveSec".to_owned(), json!({ "value": 70 }));
        script_properties.insert("fadeInDuration".to_owned(), json!(0.5));
        let controller = SceneControllerIr::from_wallpaper_engine_utility(
            "node-idle",
            "fullscreenlayer",
            "Idle Layer",
            true,
            &script_properties,
        );

        assert!(controller.uses_native_idle_input_source());
        assert!(controller.uses_native_idle_fade_ramp());
        assert!(!controller.requires_external_input_source());
        assert_eq!(
            controller.completed_feature_name(),
            "native-scene-controller-idle-video-switch"
        );
        assert_eq!(
            controller.metadata_value(),
            json!({
                "runtime": "native",
                "kind": "idle-video-switch",
                "utility": "fullscreenlayer",
                "target_layer": "Idle Layer",
                "property": "scene.controller.node-idle.active",
                "default_hide_target": true,
                "fade_in_duration": 0.5,
                "mouse_inactive_sec": { "value": 70 }
            })
        );
        assert_eq!(
            controller.property_binding_value("target-node"),
            json!({
                "property": "scene.controller.node-idle.active",
                "target_node": "target-node",
                "target": "opacity",
                "scale": 1.0,
                "offset": 0.0
            })
        );
    }

    #[test]
    fn click_controller_ir_marks_external_input_requirement() {
        let mut script_properties = Map::new();
        script_properties.insert("togglePlay".to_owned(), json!(true));
        let controller = SceneControllerIr::from_wallpaper_engine_utility(
            "node-click",
            "composelayer",
            "Click Layer",
            true,
            &script_properties,
        );

        assert!(controller.requires_external_input_source());
        assert_eq!(
            controller.completed_feature_name(),
            "native-scene-controller-click-video-switch"
        );
        assert_eq!(controller.metadata_value()["kind"], "click-video-switch");
        assert_eq!(controller.metadata_value()["toggle_play"], true);
    }
}
