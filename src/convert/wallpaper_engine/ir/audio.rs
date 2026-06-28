use serde_json::{Map, Value, json};

#[derive(Debug, Clone, PartialEq)]
pub(in crate::convert::wallpaper_engine) struct SceneAudioCueConditionIr {
    property: String,
    equals: Option<f64>,
}

impl SceneAudioCueConditionIr {
    pub(in crate::convert::wallpaper_engine) fn truthy(property: impl Into<String>) -> Self {
        Self {
            property: property.into(),
            equals: None,
        }
    }

    pub(in crate::convert::wallpaper_engine) fn equals(
        property: impl Into<String>,
        value: f64,
    ) -> Self {
        Self {
            property: property.into(),
            equals: Some(value),
        }
    }

    pub(in crate::convert::wallpaper_engine) fn value(&self) -> Value {
        let mut condition = Map::new();
        condition.insert("property".to_owned(), Value::String(self.property.clone()));
        if let Some(equals) = self.equals {
            condition.insert("equals".to_owned(), json!(equals));
        }
        Value::Object(condition)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::convert::wallpaper_engine) enum SceneAudioControllerIr {
    LayerActiveCue {
        audio_layer: String,
        source_layer: String,
        enable_property: Option<String>,
    },
    UserChoiceCue {
        property: String,
        choices: Vec<String>,
    },
}

impl SceneAudioControllerIr {
    pub(in crate::convert::wallpaper_engine) fn from_wallpaper_engine_visible_script(
        visible: &Map<String, Value>,
    ) -> Option<Self> {
        let script = visible.get("script").and_then(Value::as_str)?;
        let script_properties = visible.get("scriptproperties").and_then(Value::as_object);
        Self::layer_active_cue(script, script_properties)
            .or_else(|| Self::user_choice_cue(script, script_properties))
    }

    pub(in crate::convert::wallpaper_engine) fn completed_feature_name(&self) -> &'static str {
        match self {
            Self::LayerActiveCue { .. } => "native-scene-audio-layer-active-controller",
            Self::UserChoiceCue { .. } => "native-scene-audio-choice-controller",
        }
    }

    pub(in crate::convert::wallpaper_engine) fn target_audio_layers(&self) -> Vec<&str> {
        match self {
            Self::LayerActiveCue { audio_layer, .. } => vec![audio_layer.as_str()],
            Self::UserChoiceCue { choices, .. } => choices.iter().map(String::as_str).collect(),
        }
    }

    pub(in crate::convert::wallpaper_engine) fn conditions_for_audio_layer(
        &self,
        audio_layer: &str,
        source_layer_active_property: Option<&str>,
    ) -> Option<Vec<SceneAudioCueConditionIr>> {
        match self {
            Self::LayerActiveCue {
                audio_layer: target,
                enable_property,
                ..
            } if target == audio_layer => {
                let mut conditions = Vec::new();
                if let Some(property) = source_layer_active_property {
                    conditions.push(SceneAudioCueConditionIr::truthy(property));
                }
                if let Some(property) = enable_property {
                    conditions.push(SceneAudioCueConditionIr::truthy(property));
                }
                (!conditions.is_empty()).then_some(conditions)
            }
            Self::UserChoiceCue { property, choices } => choices
                .iter()
                .position(|choice| choice == audio_layer)
                .map(|index| {
                    vec![SceneAudioCueConditionIr::equals(
                        property.clone(),
                        (index + 1) as f64,
                    )]
                }),
            _ => None,
        }
    }

    pub(in crate::convert::wallpaper_engine) fn source_layer(&self) -> Option<&str> {
        match self {
            Self::LayerActiveCue { source_layer, .. } => Some(source_layer.as_str()),
            Self::UserChoiceCue { .. } => None,
        }
    }

    fn layer_active_cue(
        script: &str,
        script_properties: Option<&Map<String, Value>>,
    ) -> Option<Self> {
        let properties = script_properties?;
        if !(script.contains("thisScene.getLayer")
            && script.contains(".play()")
            && script.contains(".pause()")
            && script.contains(".visible")
            && script.contains(".alpha"))
        {
            return None;
        }

        let mut audio_layer = None;
        let mut source_layer = None;
        let mut enable_property = None;
        for (key, value) in properties {
            if enable_property.is_none()
                && let Some(user) = value
                    .as_object()
                    .and_then(|object| object.get("user"))
                    .and_then(Value::as_str)
                    .filter(|user| !user.trim().is_empty())
            {
                enable_property = Some(user.trim().to_owned());
            }
            let Some(text) = scene_script_property_string(value) else {
                continue;
            };
            if scene_audio_layer_name_like(&text) {
                audio_layer = Some(text);
            } else if source_layer.is_none() && !key.trim().is_empty() && !text.trim().is_empty() {
                source_layer = Some(text);
            }
        }

        Some(Self::LayerActiveCue {
            audio_layer: audio_layer?,
            source_layer: source_layer?,
            enable_property,
        })
    }

    fn user_choice_cue(
        script: &str,
        _script_properties: Option<&Map<String, Value>>,
    ) -> Option<Self> {
        if !(script.contains("changedUserProperties.")
            && script.contains("songNames")
            && script.contains("playTargetMusic")
            && script.contains(".play()"))
        {
            return None;
        }
        let property = scene_changed_user_property_name(script)?;
        let choices = scene_script_string_array(script, "songNames")?;
        (!choices.is_empty()).then_some(Self::UserChoiceCue { property, choices })
    }
}

fn scene_script_property_value(value: &Value) -> &Value {
    value
        .as_object()
        .and_then(|object| object.get("value"))
        .unwrap_or(value)
}

fn scene_script_property_string(value: &Value) -> Option<String> {
    let value = scene_script_property_value(value);
    match value {
        Value::String(value) => {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_owned())
        }
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(_) | Value::Array(_) | Value::Object(_) | Value::Null => None,
    }
}

fn scene_audio_layer_name_like(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    ["mp3", "ogg", "wav", "flac", "m4a", "aac", "opus"]
        .iter()
        .any(|extension| normalized.ends_with(&format!(".{extension}")))
}

fn scene_changed_user_property_name(script: &str) -> Option<String> {
    let marker = "changedUserProperties.";
    let start = script.find(marker)? + marker.len();
    let name = script[start..]
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect::<String>();
    (!name.is_empty()).then_some(name)
}

fn scene_script_string_array(script: &str, name: &str) -> Option<Vec<String>> {
    let assignment = script.find(name)?;
    let start = script[assignment..].find('[')? + assignment + 1;
    let end = scene_matching_array_end(script, start)?;
    let mut strings = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut current = String::new();
    for character in script[start..end].chars() {
        if let Some(active_quote) = quote {
            if escaped {
                current.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                let value = current.trim();
                if !value.is_empty() {
                    strings.push(value.to_owned());
                }
                current.clear();
                quote = None;
            } else {
                current.push(character);
            }
            continue;
        }
        if character == '\'' || character == '"' {
            quote = Some(character);
        }
    }
    Some(strings)
}

fn scene_matching_array_end(script: &str, content_start: usize) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (offset, character) in script[content_start..].char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            ']' => return Some(content_start + offset),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lowers_layer_active_audio_controller_script() {
        let visible = json!({
            "script": "let t=thisScene.getLayer(scriptProperties.target?.trim()),e=thisScene.getLayer(scriptProperties.audio?.trim());let i=t.visible&&t.alpha>0;i&&!q&&e.play(),!i&&q&&e.pause()",
            "scriptproperties": {
                "target": "Idle Video",
                "audio": "voice.mp3",
                "enabled": { "user": "voice_enabled", "value": true }
            }
        });
        let controller = SceneAudioControllerIr::from_wallpaper_engine_visible_script(
            visible.as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(
            controller,
            SceneAudioControllerIr::LayerActiveCue {
                audio_layer: "voice.mp3".to_owned(),
                source_layer: "Idle Video".to_owned(),
                enable_property: Some("voice_enabled".to_owned())
            }
        );
    }

    #[test]
    fn lowers_music_choice_audio_controller_script() {
        let visible = json!({
            "script": "let songNames = [\"a.mp3\", 'b.mp3', \"random\"]; export function applyUserProperties(changedUserProperties) { if (changedUserProperties.music === undefined) return; playTargetMusic(); targetSong.play(); }"
        });
        let controller = SceneAudioControllerIr::from_wallpaper_engine_visible_script(
            visible.as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(
            controller,
            SceneAudioControllerIr::UserChoiceCue {
                property: "music".to_owned(),
                choices: vec!["a.mp3".to_owned(), "b.mp3".to_owned(), "random".to_owned()]
            }
        );
    }
}
