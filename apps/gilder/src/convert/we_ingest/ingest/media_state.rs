//! Typed containment for groups whose authored transform is driven by media-session events.

use serde_json::Value;

use super::super::script_analysis::analyze_scene_script;

pub(super) fn group_starts_hidden_without_media_session(object: &Value) -> Result<bool, String> {
    for property in ["origin", "scale", "angles", "visible"] {
        let Some(source) = object
            .get(property)
            .and_then(|property| property.get("script"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if analyze_scene_script(source)
            .map_err(|error| error.to_string())?
            .handles_media
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::group_starts_hidden_without_media_session;

    #[test]
    fn media_playback_transform_group_starts_hidden_without_a_session() {
        let object = json!({
            "scale": {
                "value": "1 1 1",
                "script": "export function mediaPlaybackChanged(event) { if (event.state == 1) {} }"
            }
        });

        assert!(group_starts_hidden_without_media_session(&object).expect("media analysis"));
    }

    #[test]
    fn callback_name_in_a_comment_does_not_hide_the_group() {
        let object = json!({
            "origin": {
                "value": "0 0 0",
                "script": "// mediaPlaybackChanged\nexport function update(value) { return value; }"
            }
        });

        assert!(!group_starts_hidden_without_media_session(&object).expect("media analysis"));
    }
}
