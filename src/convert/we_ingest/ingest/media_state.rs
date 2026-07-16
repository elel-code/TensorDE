//! Typed containment for groups whose authored transform is driven by media-session events.

use serde_json::Value;

pub(super) fn group_starts_hidden_without_media_session(object: &Value) -> bool {
    ["origin", "scale", "angles", "visible"]
        .into_iter()
        .filter_map(|property| object.get(property))
        .filter_map(|property| property.get("script"))
        .filter_map(Value::as_str)
        .any(|script| script.contains("mediaPlaybackChanged"))
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

        assert!(group_starts_hidden_without_media_session(&object));
    }

    #[test]
    fn ordinary_transform_script_remains_visible() {
        let object = json!({
            "origin": {
                "value": "0 0 0",
                "script": "export function update(value) { value.y += engine.frametime; return value; }"
            }
        });

        assert!(!group_starts_hidden_without_media_session(&object));
    }
}
