use serde_json::{Map, Value};

use super::super::{normalize_project_key, string_field, value_field, value_to_bool, value_to_f64};
use super::timeline::SceneTimelineIr;

#[derive(Debug, Clone, PartialEq)]
pub(in crate::convert::wallpaper_engine) struct SceneAnimationLayerIr {
    timelines: Vec<SceneAnimationLayerTimelineIr>,
    unlowered_layer_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::convert::wallpaper_engine) struct SceneAnimationLayerTimelineIr {
    hint: Option<String>,
    timeline: SceneTimelineIr,
}

impl SceneAnimationLayerIr {
    pub(in crate::convert::wallpaper_engine) fn from_wallpaper_engine_value(
        value: &Value,
        target_node: &str,
    ) -> Self {
        let mut state = SceneAnimationLayerIrState::default();
        scene_animation_layer_collect(value, target_node, None, &mut state);
        Self {
            timelines: state.timelines,
            unlowered_layer_count: state.unlowered_layer_count,
        }
    }

    pub(in crate::convert::wallpaper_engine) fn into_timelines(
        self,
    ) -> Vec<SceneAnimationLayerTimelineIr> {
        self.timelines
    }

    pub(in crate::convert::wallpaper_engine) fn unlowered_layer_count(&self) -> usize {
        self.unlowered_layer_count
    }
}

impl SceneAnimationLayerTimelineIr {
    pub(in crate::convert::wallpaper_engine) fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }

    pub(in crate::convert::wallpaper_engine) fn timeline_value(
        &self,
        timeline_id: String,
    ) -> Value {
        self.timeline.timeline_value(timeline_id)
    }
}

#[derive(Default)]
struct SceneAnimationLayerIrState {
    timelines: Vec<SceneAnimationLayerTimelineIr>,
    unlowered_layer_count: usize,
}

fn scene_animation_layer_collect(
    value: &Value,
    target_node: &str,
    inherited_hint: Option<&str>,
    state: &mut SceneAnimationLayerIrState,
) {
    match value {
        Value::Array(layers) => {
            for layer in layers {
                scene_animation_layer_collect(layer, target_node, inherited_hint, state);
            }
        }
        Value::Object(object) => {
            scene_animation_layer_collect_object(object, target_node, inherited_hint, state)
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }
}

fn scene_animation_layer_collect_object(
    object: &Map<String, Value>,
    target_node: &str,
    inherited_hint: Option<&str>,
    state: &mut SceneAnimationLayerIrState,
) {
    if scene_animation_layer_disabled(object) {
        return;
    }
    let layer_hint = string_field(object, &["timeline_id", "timelineId", "name", "id"])
        .or_else(|| inherited_hint.map(str::to_owned));
    let before = state.timelines.len();

    if let Some(timeline) =
        SceneTimelineIr::from_wallpaper_engine_object(object, target_node.to_owned())
    {
        state.timelines.push(SceneAnimationLayerTimelineIr {
            hint: layer_hint.clone(),
            timeline,
        });
    }

    for key in ["timeline", "timelines", "animation", "animations"] {
        if let Some(value) = object.get(key) {
            scene_animation_layer_collect(value, target_node, layer_hint.as_deref(), state);
        }
    }

    let lowered = state.timelines.len() > before;
    if !lowered || scene_animation_layer_has_unlowered_blending(object) {
        state.unlowered_layer_count += 1;
    }
}

fn scene_animation_layer_disabled(object: &Map<String, Value>) -> bool {
    object
        .get("visible")
        .or_else(|| object.get("enabled"))
        .and_then(value_to_bool)
        .is_some_and(|enabled| !enabled)
}

fn scene_animation_layer_has_unlowered_blending(object: &Map<String, Value>) -> bool {
    object.iter().any(|(key, value)| {
        let normalized = normalize_project_key(key);
        match normalized.as_str() {
            "blend" | "blendmode" | "blending" | "blendfunction" => {
                scene_animation_layer_blend_is_complex(value)
            }
            "weight" | "strength" | "layeropacity" => {
                value_to_f64(value).is_some_and(|value| (value - 1.0).abs() > f64::EPSILON)
            }
            "rate" | "speed" | "timescale" => {
                value_to_f64(value).is_some_and(|value| (value - 1.0).abs() > f64::EPSILON)
            }
            "script" | "scenescript" | "scriptproperties" => true,
            _ => false,
        }
    })
}

fn scene_animation_layer_blend_is_complex(value: &Value) -> bool {
    let Some(blend) = value_field_from_value(value) else {
        return false;
    };
    !matches!(
        normalize_project_key(&blend).as_str(),
        "normal" | "replace" | "override" | "none"
    )
}

fn value_field_from_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Object(object) => value_field(object, &["value", "mode", "type"]),
        Value::Number(_) | Value::Bool(_) | Value::Array(_) | Value::Null => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn animation_layer_ir_lowers_direct_keyframe_layer() {
        let layers = json!([{
            "name": "slide",
            "property": "origin",
            "keyframes": [
                { "time_ms": 0, "value": [0, 0, 0] },
                { "time_ms": 1000, "value": [80, 40, 0] }
            ]
        }]);
        let ir = SceneAnimationLayerIr::from_wallpaper_engine_value(&layers, "node-panel");

        assert_eq!(ir.unlowered_layer_count(), 0);
        let timelines = ir.into_timelines();
        assert_eq!(timelines.len(), 1);
        assert_eq!(timelines[0].hint(), Some("slide"));
        assert_eq!(
            timelines[0].timeline_value("timeline-1-slide".to_owned()),
            json!({
                "id": "timeline-1-slide",
                "target_node": "node-panel",
                "channels": [
                    {
                        "property": "x",
                        "loop": false,
                        "keyframes": [
                            { "time_ms": 0, "value": 0.0 },
                            { "time_ms": 1000, "value": 80.0 }
                        ]
                    },
                    {
                        "property": "y",
                        "loop": false,
                        "keyframes": [
                            { "time_ms": 0, "value": 0.0 },
                            { "time_ms": 1000, "value": 40.0 }
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn animation_layer_ir_marks_complex_blend_after_lowering_timeline() {
        let layers = json!([{
            "name": "additive-slide",
            "blendMode": "add",
            "animations": [{
                "property": "alpha",
                "keyframes": [
                    { "time_ms": 0, "value": 0.25 },
                    { "time_ms": 1000, "value": 0.75 }
                ]
            }]
        }]);
        let ir = SceneAnimationLayerIr::from_wallpaper_engine_value(&layers, "node-panel");

        assert_eq!(ir.unlowered_layer_count(), 1);
        assert_eq!(ir.into_timelines().len(), 1);
    }
}
