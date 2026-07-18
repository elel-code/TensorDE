//! Typed lowering of SceneScript user-property font selection into convert-time defaults.

use std::collections::BTreeMap;

use serde_json::Value;

use super::normalize_we_path;

pub(super) fn text_font_overrides(scene: &Value, project: &Value) -> BTreeMap<String, String> {
    let mut overrides = BTreeMap::new();
    visit_scripts(scene, &mut |script| {
        if !script.contains("engine.registerAsset") || !script.contains("setLayerFont") {
            return;
        }
        let fonts = registered_font_assets(script);
        for (layer, property) in font_property_targets(script) {
            let Some(index) = project_property_index(project, &property) else {
                continue;
            };
            let Some(font) = index.checked_sub(1).and_then(|index| fonts.get(index)) else {
                continue;
            };
            overrides.insert(layer, font.clone());
        }
    });
    overrides
}

fn visit_scripts(value: &Value, visitor: &mut impl FnMut(&str)) {
    match value {
        Value::Object(object) => {
            if let Some(script) = object.get("script").and_then(Value::as_str) {
                visitor(script);
            }
            for child in object.values() {
                visit_scripts(child, visitor);
            }
        }
        Value::Array(values) => {
            for child in values {
                visit_scripts(child, visitor);
            }
        }
        _ => {}
    }
}

fn registered_font_assets(script: &str) -> Vec<String> {
    quoted_arguments(script, "engine.registerAsset(")
        .into_iter()
        .filter(|path| {
            let lower = path.to_ascii_lowercase();
            lower.ends_with(".ttf") || lower.ends_with(".otf")
        })
        .map(|path| normalize_we_path(&path))
        .collect()
}

fn font_property_targets(script: &str) -> Vec<(String, String)> {
    let mut targets = Vec::new();
    let mut tail = script;
    while let Some(offset) = tail.find("setLayerFont(") {
        tail = &tail[offset + "setLayerFont(".len()..];
        let Some((layer, after_layer)) = leading_quoted_value(tail) else {
            continue;
        };
        let Some(call_end) = after_layer.find(')') else {
            break;
        };
        let arguments = &after_layer[..call_end];
        let Some(property_start) = arguments.find("userProperties.") else {
            tail = &after_layer[call_end + 1..];
            continue;
        };
        let property = arguments[property_start + "userProperties.".len()..]
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect::<String>();
        if !property.is_empty() {
            targets.push((layer, property));
        }
        tail = &after_layer[call_end + 1..];
    }
    targets
}

fn quoted_arguments(script: &str, call: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut tail = script;
    while let Some(offset) = tail.find(call) {
        tail = &tail[offset + call.len()..];
        let Some((value, after)) = leading_quoted_value(tail) else {
            continue;
        };
        values.push(value);
        tail = after;
    }
    values
}

fn leading_quoted_value(value: &str) -> Option<(String, &str)> {
    let value = value.trim_start();
    let quote = value.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let content = &value[quote.len_utf8()..];
    let end = content.find(quote)?;
    Some((
        content[..end].to_owned(),
        &content[end + quote.len_utf8()..],
    ))
}

fn project_property_index(project: &Value, property: &str) -> Option<usize> {
    let value = project.pointer(&format!("/general/properties/{property}/value"))?;
    value
        .as_str()
        .and_then(|value| value.parse().ok())
        .or_else(|| value.as_u64().and_then(|value| usize::try_from(value).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_authored_combo_default_to_registered_font() {
        let scene = serde_json::json!({
            "objects": [{
                "visible": {"script": r#"
                    let fonts = [null,
                        engine.registerAsset("fonts/Latin.ttf"),
                        engine.registerAsset("fonts/中文.otf")];
                    setLayerFont('年', parseInt(userProperties.text1));
                    setLayerFont("时间", parseInt(userProperties.text1));
                "#}
            }]
        });
        let project = serde_json::json!({
            "general": {"properties": {"text1": {"value": "2"}}}
        });
        assert_eq!(
            text_font_overrides(&scene, &project),
            BTreeMap::from([
                ("年".to_owned(), "fonts/中文.otf".to_owned()),
                ("时间".to_owned(), "fonts/中文.otf".to_owned()),
            ])
        );
    }

    #[test]
    fn ignores_out_of_range_or_missing_property_defaults() {
        let scene = serde_json::json!({
            "script": "engine.registerAsset('fonts/a.ttf'); setLayerFont('年', userProperties.font)"
        });
        assert!(text_font_overrides(&scene, &serde_json::json!({})).is_empty());
    }
}
