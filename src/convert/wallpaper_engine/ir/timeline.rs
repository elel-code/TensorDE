use serde_json::{Map, Value, json};

use super::super::{
    normalize_project_key, scene_bool_value_field, value_field, value_to_bool, value_to_f64,
    vector3_components_from_value,
};

#[derive(Debug, Clone, PartialEq)]
pub(in crate::convert::wallpaper_engine) struct SceneTimelineIr {
    target_node: String,
    channels: Vec<SceneTimelineChannelIr>,
}

#[derive(Debug, Clone, PartialEq)]
struct SceneTimelineChannelIr {
    property: &'static str,
    loop_playback: bool,
    keyframes: Vec<SceneTimelineKeyframeIr>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SceneTimelineKeyframeIr {
    time_ms: u64,
    value: f64,
    curve: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
struct SceneTimelinePropertyMapping {
    property: &'static str,
    component: Option<usize>,
    value_scale: f64,
    min_value: Option<f64>,
    max_value: Option<f64>,
}

impl SceneTimelineIr {
    pub(in crate::convert::wallpaper_engine) fn from_wallpaper_engine_object(
        object: &Map<String, Value>,
        target_node: String,
    ) -> Option<Self> {
        let loop_playback = scene_bool_value_field(object, &["loop", "repeat", "loop_playback"])
            .or_else(|| scene_bool_value_field(object, &["loopPlayback"]))
            .unwrap_or(false);
        let inherited_curve = scene_timeline_curve_from_object(object);
        let mut channels = Vec::new();

        for key in ["channels", "tracks"] {
            if let Some(value) = object.get(key) {
                channels.extend(scene_timeline_channels_from_value(
                    value,
                    loop_playback,
                    inherited_curve,
                ));
            }
        }

        if channels.is_empty()
            && let Some(property) = value_field(object, &["property", "path", "target_property"])
                .or_else(|| value_field(object, &["targetProperty"]))
            && let Some(keyframes) = scene_timeline_keyframe_source(object)
        {
            channels.extend(scene_timeline_channels_from_property(
                &property,
                keyframes,
                loop_playback,
                inherited_curve,
            ));
        }

        if channels.is_empty() {
            channels.extend(scene_timeline_channels_from_property_map(
                object,
                loop_playback,
                inherited_curve,
            ));
        }

        (!channels.is_empty()).then_some(Self {
            target_node,
            channels,
        })
    }

    pub(in crate::convert::wallpaper_engine) fn timeline_value(
        &self,
        timeline_id: String,
    ) -> Value {
        json!({
            "id": timeline_id,
            "target_node": self.target_node,
            "channels": self.channels.iter().map(SceneTimelineChannelIr::channel_value).collect::<Vec<_>>()
        })
    }
}

impl SceneTimelineChannelIr {
    fn channel_value(&self) -> Value {
        json!({
            "property": self.property,
            "loop": self.loop_playback,
            "keyframes": self.keyframes.iter().map(SceneTimelineKeyframeIr::keyframe_value).collect::<Vec<_>>()
        })
    }
}

impl SceneTimelineKeyframeIr {
    fn keyframe_value(&self) -> Value {
        let mut value = json!({
            "time_ms": self.time_ms,
            "value": self.value
        });
        if let Some(curve) = self.curve
            && let Some(object) = value.as_object_mut()
        {
            object.insert("curve".to_owned(), Value::String(curve.to_owned()));
        }
        value
    }
}

fn scene_timeline_channels_from_value(
    value: &Value,
    inherited_loop: bool,
    inherited_curve: Option<&'static str>,
) -> Vec<SceneTimelineChannelIr> {
    match value {
        Value::Array(entries) => entries
            .iter()
            .flat_map(|entry| {
                scene_timeline_channels_from_value(entry, inherited_loop, inherited_curve)
            })
            .collect(),
        Value::Object(object) => {
            let loop_playback =
                scene_bool_value_field(object, &["loop", "repeat", "loop_playback"])
                    .or_else(|| scene_bool_value_field(object, &["loopPlayback"]))
                    .unwrap_or(inherited_loop);
            let curve = scene_timeline_curve_from_object(object).or(inherited_curve);
            if let Some(property) = value_field(object, &["property", "path", "target_property"])
                .or_else(|| value_field(object, &["targetProperty"]))
                && let Some(keyframes) = scene_timeline_keyframe_source(object)
            {
                return scene_timeline_channels_from_property(
                    &property,
                    keyframes,
                    loop_playback,
                    curve,
                );
            }
            scene_timeline_channels_from_property_map(object, loop_playback, curve)
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => Vec::new(),
    }
}

fn scene_timeline_channels_from_property_map(
    object: &Map<String, Value>,
    loop_playback: bool,
    inherited_curve: Option<&'static str>,
) -> Vec<SceneTimelineChannelIr> {
    object
        .iter()
        .filter(|(key, _)| !scene_timeline_metadata_key(key))
        .flat_map(|(property, keyframes)| {
            scene_timeline_channels_from_property(
                property,
                keyframes,
                loop_playback,
                inherited_curve,
            )
        })
        .collect()
}

fn scene_timeline_channels_from_property(
    property: &str,
    keyframes: &Value,
    loop_playback: bool,
    inherited_curve: Option<&'static str>,
) -> Vec<SceneTimelineChannelIr> {
    let mappings = scene_timeline_property_mappings(property);
    if mappings.is_empty() {
        return Vec::new();
    }
    mappings
        .into_iter()
        .filter_map(|mapping| {
            let keyframes =
                scene_timeline_keyframes_from_value(keyframes, property, mapping, inherited_curve);
            if keyframes.is_empty() {
                None
            } else {
                Some(SceneTimelineChannelIr {
                    property: mapping.property,
                    loop_playback,
                    keyframes,
                })
            }
        })
        .collect()
}

fn scene_timeline_keyframe_source(object: &Map<String, Value>) -> Option<&Value> {
    ["keyframes", "frames", "values", "points"]
        .iter()
        .filter_map(|key| object.get(*key))
        .next()
}

fn scene_timeline_keyframes_from_value(
    value: &Value,
    source_property: &str,
    mapping: SceneTimelinePropertyMapping,
    inherited_curve: Option<&'static str>,
) -> Vec<SceneTimelineKeyframeIr> {
    let entries = match value {
        Value::Array(entries) => entries.as_slice(),
        Value::Object(object) => match scene_timeline_keyframe_source(object) {
            Some(Value::Array(entries)) => entries.as_slice(),
            _ => return Vec::new(),
        },
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => return Vec::new(),
    };
    let mut keyframes = entries
        .iter()
        .filter_map(|entry| {
            let object = entry.as_object()?;
            let time_ms = scene_timeline_keyframe_time_ms(object)?;
            let value = scene_timeline_keyframe_value(object, source_property, mapping)?;
            let curve = scene_timeline_curve_from_object(object).or(inherited_curve);
            Some((
                time_ms,
                SceneTimelineKeyframeIr {
                    time_ms,
                    value,
                    curve,
                },
            ))
        })
        .collect::<Vec<_>>();
    keyframes.sort_by_key(|(time_ms, _)| *time_ms);
    keyframes
        .into_iter()
        .map(|(_, keyframe)| keyframe)
        .collect()
}

fn scene_timeline_keyframe_time_ms(object: &Map<String, Value>) -> Option<u64> {
    for key in [
        "time_ms",
        "timeMs",
        "timestamp_ms",
        "timestampMs",
        "at_ms",
        "atMs",
        "milliseconds",
        "millis",
        "ms",
    ] {
        if let Some(time_ms) = object.get(key).and_then(value_to_f64) {
            return scene_time_ms_from_f64(time_ms);
        }
    }
    for key in ["time_seconds", "timeSeconds", "seconds", "secs", "sec"] {
        if let Some(seconds) = object.get(key).and_then(value_to_f64) {
            return scene_time_ms_from_f64(seconds * 1000.0);
        }
    }
    let time = object.get("time").and_then(value_to_f64)?;
    let unit = value_field(object, &["unit", "time_unit", "timeUnit"])?;
    let normalized = normalize_project_key(&unit);
    if matches!(normalized.as_str(), "ms" | "millis" | "milliseconds") {
        scene_time_ms_from_f64(time)
    } else if matches!(normalized.as_str(), "s" | "sec" | "secs" | "seconds") {
        scene_time_ms_from_f64(time * 1000.0)
    } else {
        None
    }
}

fn scene_time_ms_from_f64(value: f64) -> Option<u64> {
    if value.is_finite() && value >= 0.0 && value <= u64::MAX as f64 {
        Some(value.round() as u64)
    } else {
        None
    }
}

fn scene_timeline_keyframe_value(
    object: &Map<String, Value>,
    source_property: &str,
    mapping: SceneTimelinePropertyMapping,
) -> Option<f64> {
    let value = scene_timeline_keyframe_raw_value(object, source_property, mapping)?;
    let mut value = value * mapping.value_scale;
    if let Some(min_value) = mapping.min_value {
        value = value.max(min_value);
    }
    if let Some(max_value) = mapping.max_value {
        value = value.min(max_value);
    }
    if value.is_finite() { Some(value) } else { None }
}

fn scene_timeline_keyframe_raw_value(
    object: &Map<String, Value>,
    source_property: &str,
    mapping: SceneTimelinePropertyMapping,
) -> Option<f64> {
    let value = ["value", "val", "v"]
        .iter()
        .filter_map(|key| object.get(*key))
        .next()
        .or_else(|| object.get(source_property))
        .or_else(|| scene_timeline_property_value_from_object(object, source_property))?;
    if let Some(component) = mapping.component {
        let components = vector3_components_from_value(value)?;
        return match component {
            0 => Some(components.0),
            1 => Some(components.1),
            2 => Some(components.2),
            _ => None,
        };
    }
    value_to_f64(value).or_else(|| value_to_bool(value).map(|value| if value { 1.0 } else { 0.0 }))
}

fn scene_timeline_property_value_from_object<'a>(
    object: &'a Map<String, Value>,
    source_property: &str,
) -> Option<&'a Value> {
    let normalized_source = normalize_project_key(source_property);
    object
        .iter()
        .find(|(key, _)| normalize_project_key(key) == normalized_source)
        .map(|(_, value)| value)
}

fn scene_timeline_curve_from_object(object: &Map<String, Value>) -> Option<&'static str> {
    let curve = value_field(object, &["curve", "easing", "interpolation"])?;
    match normalize_project_key(&curve).as_str() {
        "step" | "constant" | "hold" => Some("step"),
        "easein" => Some("ease-in"),
        "easeout" => Some("ease-out"),
        "easeinout" | "smooth" | "smoothstep" => Some("ease-in-out"),
        "linear" => Some("linear"),
        _ => None,
    }
}

fn scene_timeline_property_mappings(property: &str) -> Vec<SceneTimelinePropertyMapping> {
    let normalized = normalize_project_key(property);
    let to_degrees = 180.0 / std::f64::consts::PI;
    let x = SceneTimelinePropertyMapping {
        property: "x",
        component: None,
        value_scale: 1.0,
        min_value: None,
        max_value: None,
    };
    let y = SceneTimelinePropertyMapping {
        property: "y",
        component: None,
        value_scale: 1.0,
        min_value: None,
        max_value: None,
    };
    let scale_x = SceneTimelinePropertyMapping {
        property: "scale-x",
        component: None,
        value_scale: 1.0,
        min_value: Some(f64::EPSILON),
        max_value: None,
    };
    let scale_y = SceneTimelinePropertyMapping {
        property: "scale-y",
        component: None,
        value_scale: 1.0,
        min_value: Some(f64::EPSILON),
        max_value: None,
    };
    let opacity = SceneTimelinePropertyMapping {
        property: "opacity",
        component: None,
        value_scale: 1.0,
        min_value: Some(0.0),
        max_value: Some(1.0),
    };
    let rotation_deg = SceneTimelinePropertyMapping {
        property: "rotation-deg",
        component: None,
        value_scale: 1.0,
        min_value: None,
        max_value: None,
    };
    let width = SceneTimelinePropertyMapping {
        property: "width",
        component: None,
        value_scale: 1.0,
        min_value: Some(0.0),
        max_value: None,
    };
    let height = SceneTimelinePropertyMapping {
        property: "height",
        component: None,
        value_scale: 1.0,
        min_value: Some(0.0),
        max_value: None,
    };
    let corner_radius = SceneTimelinePropertyMapping {
        property: "corner-radius",
        component: None,
        value_scale: 1.0,
        min_value: Some(0.0),
        max_value: None,
    };
    match normalized.as_str() {
        "x" | "left" | "originx" | "positionx" | "translationx" => vec![x],
        "y" | "top" | "originy" | "positiony" | "translationy" => vec![y],
        "origin" | "position" | "translation" => vec![
            SceneTimelinePropertyMapping {
                component: Some(0),
                ..x
            },
            SceneTimelinePropertyMapping {
                component: Some(1),
                ..y
            },
        ],
        "scalex" => vec![scale_x],
        "scaley" => vec![scale_y],
        "scale" => vec![
            SceneTimelinePropertyMapping {
                component: Some(0),
                ..scale_x
            },
            SceneTimelinePropertyMapping {
                component: Some(1),
                ..scale_y
            },
        ],
        "opacity" | "alpha" | "visible" | "visibility" => vec![opacity],
        "rotation" | "rotationdeg" | "angle" | "rotationz" => vec![rotation_deg],
        "anglesz" => vec![SceneTimelinePropertyMapping {
            value_scale: to_degrees,
            ..rotation_deg
        }],
        "angles" => vec![SceneTimelinePropertyMapping {
            component: Some(2),
            value_scale: to_degrees,
            ..rotation_deg
        }],
        "width" | "w" | "sizex" => vec![width],
        "height" | "h" | "sizey" => vec![height],
        "size" | "dimensions" => vec![
            SceneTimelinePropertyMapping {
                component: Some(0),
                ..width
            },
            SceneTimelinePropertyMapping {
                component: Some(1),
                ..height
            },
        ],
        "radius" | "cornerradius" | "borderradius" => vec![corner_radius],
        _ => Vec::new(),
    }
}

fn scene_timeline_metadata_key(key: &str) -> bool {
    matches!(
        normalize_project_key(key).as_str(),
        "id" | "name"
            | "target"
            | "targetnode"
            | "targetid"
            | "object"
            | "objectid"
            | "node"
            | "nodeid"
            | "property"
            | "targetproperty"
            | "path"
            | "channels"
            | "tracks"
            | "keyframes"
            | "frames"
            | "values"
            | "points"
            | "loop"
            | "repeat"
            | "loopplayback"
            | "curve"
            | "easing"
            | "interpolation"
            | "unit"
            | "timeunit"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_ir_lowers_vector_channels_and_sorts_keyframes() {
        let timeline = json!({
            "property": "origin",
            "easing": "easeInOut",
            "keyframes": [
                { "time_ms": 1000, "value": [100, 50, 0] },
                { "time_ms": 0, "value": [0, 0, 0] }
            ]
        });
        let ir = SceneTimelineIr::from_wallpaper_engine_object(
            timeline.as_object().unwrap(),
            "node-panel".to_owned(),
        )
        .unwrap();

        assert_eq!(
            ir.timeline_value("timeline-1".to_owned()),
            json!({
                "id": "timeline-1",
                "target_node": "node-panel",
                "channels": [
                    {
                        "property": "x",
                        "loop": false,
                        "keyframes": [
                            { "time_ms": 0, "value": 0.0, "curve": "ease-in-out" },
                            { "time_ms": 1000, "value": 100.0, "curve": "ease-in-out" }
                        ]
                    },
                    {
                        "property": "y",
                        "loop": false,
                        "keyframes": [
                            { "time_ms": 0, "value": 0.0, "curve": "ease-in-out" },
                            { "time_ms": 1000, "value": 50.0, "curve": "ease-in-out" }
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn timeline_ir_lowers_track_map_loop_and_radian_rotation() {
        let timeline = json!({
            "loopPlayback": true,
            "tracks": [{
                "anglesz": [
                    { "time_seconds": 0, "value": 0.0 },
                    { "time_seconds": 1.5, "value": std::f64::consts::PI }
                ]
            }]
        });
        let ir = SceneTimelineIr::from_wallpaper_engine_object(
            timeline.as_object().unwrap(),
            "node-spinner".to_owned(),
        )
        .unwrap();

        assert_eq!(
            ir.timeline_value("timeline-spin".to_owned())["channels"],
            json!([
                {
                    "property": "rotation-deg",
                    "loop": true,
                    "keyframes": [
                        { "time_ms": 0, "value": 0.0 },
                        { "time_ms": 1500, "value": 180.0 }
                    ]
                }
            ])
        );
    }
}
