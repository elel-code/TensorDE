use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

const CONTROLLER_SETTING_KEYS: &[&str] = &[
    "allowAutoPlay",
    "cooldownSec",
    "endTimePercent",
    "enableAutoControl",
    "fadeDuration",
    "fadeInDuration",
    "fadeOutDuration",
    "hideOnStart",
    "hideWhenPaused",
    "hideWhenStopped",
    "isClickable",
    "loopControl",
    "loopCount",
    "loopInterval",
    "loopPlay",
    "mouseInactiveSec",
    "playbackSpeed",
    "resetOnClick",
    "resetOnRestart",
    "startDelay",
    "startTimePercent",
    "showDuration",
    "togglePlay",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneControllerKind {
    TimedVisibility,
    IdleVideoSwitch,
    ClickVideoSwitch,
    PropertyVideoSwitch,
}

impl SceneControllerKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::TimedVisibility => "timed-visibility",
            Self::IdleVideoSwitch => "idle-video-switch",
            Self::ClickVideoSwitch => "click-video-switch",
            Self::PropertyVideoSwitch => "property-video-switch",
        }
    }

    fn from_wallpaper_engine_utility(
        utility: &str,
        script_properties: &Map<String, Value>,
    ) -> Self {
        if scene_controller_has_timed_visibility_signal(script_properties) {
            Self::TimedVisibility
        } else if utility == "fullscreenlayer" || script_properties.contains_key("mouseInactiveSec")
        {
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
pub(in crate::convert::wallpaper_engine) struct SceneControllerIr {
    controller_node_id: String,
    utility: String,
    target_layer: String,
    property: String,
    default_hide_target: bool,
    kind: SceneControllerKind,
    settings: BTreeMap<String, Value>,
}

impl SceneControllerIr {
    pub(in crate::convert::wallpaper_engine) fn from_wallpaper_engine_utility(
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

    pub(in crate::convert::wallpaper_engine) fn controller_node_id(&self) -> &str {
        &self.controller_node_id
    }

    pub(in crate::convert::wallpaper_engine) fn target_layer(&self) -> &str {
        &self.target_layer
    }

    pub(in crate::convert::wallpaper_engine) fn uses_native_timed_visibility_timeline(
        &self,
    ) -> bool {
        self.kind == SceneControllerKind::TimedVisibility
    }

    pub(in crate::convert::wallpaper_engine) fn uses_native_idle_input_source(&self) -> bool {
        self.kind == SceneControllerKind::IdleVideoSwitch
    }

    pub(in crate::convert::wallpaper_engine) fn uses_native_idle_fade_ramp(&self) -> bool {
        self.uses_native_idle_input_source()
            && self
                .settings
                .get("fade_in_duration")
                .and_then(scene_ir_setting_number)
                .is_some_and(|value| value > 0.0)
    }

    pub(in crate::convert::wallpaper_engine) fn requires_external_input_source(&self) -> bool {
        !self.uses_native_idle_input_source() && !self.uses_native_timed_visibility_timeline()
    }

    pub(in crate::convert::wallpaper_engine) fn metadata_value(&self) -> Value {
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
        controller.insert("input_aliases".to_owned(), self.input_aliases_value(None));
        controller.insert(
            "default_hide_target".to_owned(),
            json!(self.default_hide_target),
        );
        for (key, value) in &self.settings {
            controller.insert(key.clone(), value.clone());
        }
        Value::Object(controller)
    }

    pub(in crate::convert::wallpaper_engine) fn input_aliases_value(
        &self,
        target_node_id: Option<&str>,
    ) -> Value {
        Value::Array(
            self.input_aliases(target_node_id)
                .into_iter()
                .map(Value::String)
                .collect(),
        )
    }

    fn input_aliases(&self, target_node_id: Option<&str>) -> Vec<String> {
        let mut aliases = vec![
            self.property.clone(),
            format!("scene.input.{}.active", self.controller_node_id),
            format!("scene.input.controller.{}.active", self.controller_node_id),
        ];
        if let Some(target_node_id) = target_node_id {
            aliases.push(format!("scene.input.{target_node_id}.active"));
            aliases.push(format!("scene.input.controller.{target_node_id}.active"));
        }
        aliases
    }

    pub(in crate::convert::wallpaper_engine) fn property_binding_value(
        &self,
        target_node_id: &str,
    ) -> Value {
        json!({
            "property": self.property.clone(),
            "target_node": target_node_id,
            "target": "opacity",
            "scale": 1.0,
            "offset": 0.0
        })
    }

    pub(in crate::convert::wallpaper_engine) fn initial_target_opacity(&self) -> Option<f64> {
        if self.uses_native_timed_visibility_timeline()
            && !self
                .settings
                .get("enable_auto_control")
                .and_then(scene_ir_setting_bool)
                .unwrap_or(true)
        {
            return Some(0.0);
        }
        if self.default_hide_target
            || self
                .settings
                .get("hide_on_start")
                .and_then(scene_ir_setting_bool)
                .unwrap_or(false)
        {
            Some(0.0)
        } else if self.uses_native_timed_visibility_timeline() {
            Some(1.0)
        } else {
            None
        }
    }

    pub(in crate::convert::wallpaper_engine) fn timed_visibility_timeline_value(
        &self,
        timeline_id: String,
        target_node_id: &str,
    ) -> Option<Value> {
        if !self.uses_native_timed_visibility_timeline() {
            return None;
        }
        if !self
            .settings
            .get("enable_auto_control")
            .and_then(scene_ir_setting_bool)
            .unwrap_or(true)
        {
            return Some(scene_opacity_timeline_value(
                timeline_id,
                target_node_id,
                false,
                vec![(0, 0.0)],
            ));
        }

        let start_delay_ms = self.setting_seconds_ms("start_delay").unwrap_or(0);
        let fade_ms = self.setting_seconds_ms("fade_duration").unwrap_or(0);
        let show_ms = self.setting_seconds_ms("show_duration").unwrap_or(0);
        let loop_control = self
            .settings
            .get("loop_control")
            .and_then(scene_ir_setting_bool)
            .unwrap_or(false)
            && show_ms > 0;
        let loop_interval_ms = self.setting_seconds_ms("loop_interval").unwrap_or(0);
        let hide_on_start = self
            .settings
            .get("hide_on_start")
            .and_then(scene_ir_setting_bool)
            .unwrap_or(false);
        let initial = if hide_on_start { 0.0 } else { 1.0 };

        let mut keyframes = Vec::new();
        scene_push_opacity_keyframe(&mut keyframes, 0, initial);
        if start_delay_ms > 0 {
            scene_push_opacity_keyframe(
                &mut keyframes,
                start_delay_ms,
                if hide_on_start { 0.0 } else { initial },
            );
        } else if hide_on_start {
            scene_push_opacity_keyframe(&mut keyframes, 0, 0.0);
        }

        let visible_at_ms = start_delay_ms.saturating_add(fade_ms);
        scene_push_opacity_keyframe(&mut keyframes, visible_at_ms, 1.0);

        if show_ms == 0 {
            return Some(scene_opacity_timeline_value(
                timeline_id,
                target_node_id,
                false,
                keyframes,
            ));
        }

        let hide_begin_ms = visible_at_ms.saturating_add(show_ms);
        scene_push_opacity_keyframe(&mut keyframes, hide_begin_ms, 1.0);
        let hidden_at_ms = hide_begin_ms.saturating_add(fade_ms);
        scene_push_opacity_keyframe(&mut keyframes, hidden_at_ms, 0.0);
        if loop_control {
            let period_ms = hidden_at_ms.saturating_add(loop_interval_ms.max(1));
            scene_push_opacity_keyframe(&mut keyframes, period_ms, 0.0);
        }

        Some(scene_opacity_timeline_value(
            timeline_id,
            target_node_id,
            loop_control,
            keyframes,
        ))
    }

    fn setting_seconds_ms(&self, key: &str) -> Option<u64> {
        let seconds = self.settings.get(key).and_then(scene_ir_setting_number)?;
        Some((seconds.max(0.0) * 1000.0).round() as u64)
    }

    pub(in crate::convert::wallpaper_engine) fn completed_feature_name(&self) -> String {
        format!("native-scene-controller-{}", self.kind.as_str())
    }
}

fn scene_controller_has_timed_visibility_signal(script_properties: &Map<String, Value>) -> bool {
    script_properties.contains_key("targetLayerName")
        || script_properties.contains_key("target_layer_name")
        || script_properties.contains_key("targetlayername")
        || script_properties.contains_key("showDuration")
        || script_properties.contains_key("show_duration")
        || script_properties.contains_key("showduration")
        || script_properties.contains_key("enableAutoControl")
        || script_properties.contains_key("enable_auto_control")
        || script_properties.contains_key("enableautocontrol")
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

fn scene_ir_setting_bool(value: &Value) -> Option<bool> {
    let value = value.get("value").unwrap_or(value);
    match value {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => Some(value.as_f64()? != 0.0),
        Value::String(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "true" | "1" | "yes" | "on" => Some(true),
                "false" | "0" | "no" | "off" => Some(false),
                _ => None,
            }
        }
        Value::Array(_) | Value::Object(_) | Value::Null => None,
    }
}

fn scene_push_opacity_keyframe(keyframes: &mut Vec<(u64, f64)>, time_ms: u64, value: f64) {
    let value = value.clamp(0.0, 1.0);
    if let Some(last) = keyframes.last_mut()
        && last.0 == time_ms
    {
        last.1 = value;
        return;
    }
    keyframes.push((time_ms, value));
}

fn scene_opacity_timeline_value(
    timeline_id: String,
    target_node_id: &str,
    loop_playback: bool,
    keyframes: Vec<(u64, f64)>,
) -> Value {
    json!({
        "id": timeline_id,
        "target_node": target_node_id,
        "channels": [
            {
                "property": "opacity",
                "loop": loop_playback,
                "keyframes": keyframes
                    .into_iter()
                    .map(|(time_ms, value)| {
                        json!({
                            "time_ms": time_ms,
                            "value": value
                        })
                    })
                    .collect::<Vec<_>>()
            }
        ]
    })
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
                "input_aliases": [
                    "scene.controller.node-idle.active",
                    "scene.input.node-idle.active",
                    "scene.input.controller.node-idle.active"
                ],
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
        assert_eq!(
            controller.input_aliases_value(Some("target-node")),
            json!([
                "scene.controller.node-click.active",
                "scene.input.node-click.active",
                "scene.input.controller.node-click.active",
                "scene.input.target-node.active",
                "scene.input.controller.target-node.active"
            ])
        );
    }

    #[test]
    fn timed_visibility_controller_ir_lowers_to_opacity_timeline() {
        let mut script_properties = Map::new();
        script_properties.insert("targetLayerName".to_owned(), json!("Cloud"));
        script_properties.insert("enableAutoControl".to_owned(), json!({ "value": true }));
        script_properties.insert("startDelay".to_owned(), json!("0.25"));
        script_properties.insert("showDuration".to_owned(), json!("2"));
        script_properties.insert("fadeDuration".to_owned(), json!(0.5));
        script_properties.insert("hideOnStart".to_owned(), json!(true));
        script_properties.insert("loopControl".to_owned(), json!(true));
        script_properties.insert("loopInterval".to_owned(), json!(1));

        let controller = SceneControllerIr::from_wallpaper_engine_utility(
            "node-timer",
            "fullscreenlayer",
            "Cloud",
            false,
            &script_properties,
        );

        assert!(controller.uses_native_timed_visibility_timeline());
        assert!(!controller.requires_external_input_source());
        assert_eq!(controller.initial_target_opacity(), Some(0.0));
        assert_eq!(
            controller.completed_feature_name(),
            "native-scene-controller-timed-visibility"
        );
        assert_eq!(controller.metadata_value()["kind"], "timed-visibility");
        assert_eq!(
            controller.timed_visibility_timeline_value("timeline-cloud".to_owned(), "node-cloud"),
            Some(json!({
                "id": "timeline-cloud",
                "target_node": "node-cloud",
                "channels": [
                    {
                        "property": "opacity",
                        "loop": true,
                        "keyframes": [
                            { "time_ms": 0, "value": 0.0 },
                            { "time_ms": 250, "value": 0.0 },
                            { "time_ms": 750, "value": 1.0 },
                            { "time_ms": 2750, "value": 1.0 },
                            { "time_ms": 3250, "value": 0.0 },
                            { "time_ms": 4250, "value": 0.0 }
                        ]
                    }
                ]
            }))
        );
    }
}
