//! Typed lowering of WE object transform property animation.

use serde_json::Value;

use crate::convert::we_ingest::ir::{
    WeIrObjectTransformChannel, WeIrObjectTransformChannelKind, WeIrObjectTransformKeyframe,
    WeIrObjectTransformProperty, WeIrObjectTransformTrack, WeIrUnsupported,
};

pub(super) fn ingest_object_transform_tracks(
    object: u32,
    value: &Value,
    tracks: &mut Vec<WeIrObjectTransformTrack>,
    channels: &mut Vec<WeIrObjectTransformChannel>,
    keyframes: &mut Vec<WeIrObjectTransformKeyframe>,
    unsupported: &mut Vec<WeIrUnsupported>,
) {
    for (property_name, property) in [
        ("origin", WeIrObjectTransformProperty::Origin),
        ("angles", WeIrObjectTransformProperty::Angles),
        ("scale", WeIrObjectTransformProperty::Scale),
    ] {
        let Some(binding) = value.get(property_name).and_then(Value::as_object) else {
            continue;
        };
        if let Some(animation) = binding.get("animation") {
            if !append_keyframed_track(object, property, animation, tracks, channels, keyframes) {
                unsupported.push(unsupported_property(
                    object,
                    property_name,
                    "malformed-keyframed-transform-animation",
                    "object-transform-animation-skipped",
                ));
            }
        }
        if let Some(script) = binding.get("script").and_then(Value::as_str) {
            if property == WeIrObjectTransformProperty::Angles {
                unsupported.push(unsupported_property(
                    object,
                    property_name,
                    "angle-script-evaluation-unit-unverified",
                    "static-bound-value-kept",
                ));
            } else if !append_sine_script_track(object, property, binding, script, tracks, channels)
            {
                unsupported.push(unsupported_property(
                    object,
                    property_name,
                    "unlowered-transform-property-script",
                    "static-bound-value-kept",
                ));
            }
        }
    }
}

fn append_keyframed_track(
    object: u32,
    property: WeIrObjectTransformProperty,
    animation: &Value,
    tracks: &mut Vec<WeIrObjectTransformTrack>,
    channels: &mut Vec<WeIrObjectTransformChannel>,
    keyframes: &mut Vec<WeIrObjectTransformKeyframe>,
) -> bool {
    let Some(animation) = animation.as_object() else {
        return false;
    };
    let Some(options) = animation.get("options").and_then(Value::as_object) else {
        return false;
    };
    let Some(fps) = finite_f32(options.get("fps")) else {
        return false;
    };
    let Some(frame_count) = options
        .get("length")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
    else {
        return false;
    };
    if fps <= 0.0 || frame_count == 0 {
        return false;
    }

    let mut parsed_channels = Vec::new();
    for component in 0..3 {
        let channel_name = format!("c{component}");
        let Some(values) = animation.get(&channel_name).and_then(Value::as_array) else {
            continue;
        };
        let Some(parsed) = parse_keyframes(values) else {
            return false;
        };
        if !parsed.is_empty() {
            parsed_channels.push((component, parsed));
        }
    }
    if parsed_channels.is_empty() {
        return false;
    }

    let track_index = tracks.len() as u32;
    let channel_start = channels.len() as u32;
    tracks.push(WeIrObjectTransformTrack {
        object,
        property,
        relative: animation
            .get("relative")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        wrap_loop: options
            .get("wraploop")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        playback: options
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("loop")
            .to_owned(),
        fps,
        frame_count,
        channel_start,
        channel_count: parsed_channels.len() as u32,
    });
    for (component, parsed) in parsed_channels {
        let keyframe_start = keyframes.len() as u32;
        let keyframe_count = parsed.len() as u32;
        keyframes.extend(parsed);
        channels.push(WeIrObjectTransformChannel {
            track: track_index,
            component,
            kind: WeIrObjectTransformChannelKind::Keyframed,
            offset: 0.0,
            amplitude: 0.0,
            frequency: 0.0,
            phase: 0.0,
            keyframe_start,
            keyframe_count,
        });
    }
    true
}

fn parse_keyframes(values: &[Value]) -> Option<Vec<WeIrObjectTransformKeyframe>> {
    let mut parsed = Vec::with_capacity(values.len());
    let mut previous_frame = None;
    for value in values {
        let frame = finite_f32(value.get("frame"))?;
        let sample = finite_f32(value.get("value"))?;
        if previous_frame.is_some_and(|previous| frame <= previous) {
            return None;
        }
        previous_frame = Some(frame);
        let (back, back_enabled, back_magic) = parse_handle(value.get("back"));
        let (front, front_enabled, front_magic) = parse_handle(value.get("front"));
        parsed.push(WeIrObjectTransformKeyframe {
            frame,
            value: sample,
            back,
            front,
            back_enabled,
            front_enabled,
            back_magic,
            front_magic,
        });
    }
    Some(parsed)
}

fn parse_handle(value: Option<&Value>) -> ([f32; 2], bool, bool) {
    let Some(value) = value.and_then(Value::as_object) else {
        return ([0.0; 2], false, false);
    };
    let x = finite_f32(value.get("x")).unwrap_or(0.0);
    let y = finite_f32(value.get("y")).unwrap_or(0.0);
    (
        [x, y],
        value
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        value.get("magic").and_then(Value::as_bool).unwrap_or(false),
    )
}

fn append_sine_script_track(
    object: u32,
    property: WeIrObjectTransformProperty,
    binding: &serde_json::Map<String, Value>,
    script: &str,
    tracks: &mut Vec<WeIrObjectTransformTrack>,
    channels: &mut Vec<WeIrObjectTransformChannel>,
) -> bool {
    let Some(properties) = binding.get("scriptproperties").and_then(Value::as_object) else {
        return false;
    };
    let normalized: String = script
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let mut parsed = Vec::new();
    for (component, component_name, prefix) in [(0, "x", "x"), (1, "y", "y"), (2, "z", "z")] {
        let offset_name = format!("{prefix}a");
        let frequency_name = format!("{prefix}b");
        let amplitude_name = format!("{prefix}c");
        let expression = format!(
            "value.{component_name}=scriptProperties.{offset_name}+(Math.sin(engine.runtime*scriptProperties.{frequency_name})*scriptProperties.{amplitude_name});"
        );
        if !normalized.contains(&expression) {
            continue;
        }
        let Some(offset) = bound_f32(properties.get(&offset_name)) else {
            return false;
        };
        let Some(frequency) = bound_f32(properties.get(&frequency_name)) else {
            return false;
        };
        let Some(amplitude) = bound_f32(properties.get(&amplitude_name)) else {
            return false;
        };
        parsed.push((component, offset, frequency, amplitude));
    }
    if parsed.is_empty() {
        return false;
    }

    let track_index = tracks.len() as u32;
    let channel_start = channels.len() as u32;
    tracks.push(WeIrObjectTransformTrack {
        object,
        property,
        relative: false,
        wrap_loop: false,
        playback: "continuous".to_owned(),
        fps: 0.0,
        frame_count: 0,
        channel_start,
        channel_count: parsed.len() as u32,
    });
    for (component, offset, frequency, amplitude) in parsed {
        channels.push(WeIrObjectTransformChannel {
            track: track_index,
            component,
            kind: WeIrObjectTransformChannelKind::Sine,
            offset,
            amplitude,
            frequency,
            phase: 0.0,
            keyframe_start: 0,
            keyframe_count: 0,
        });
    }
    true
}

fn finite_f32(value: Option<&Value>) -> Option<f32> {
    let value = value?.as_f64()? as f32;
    value.is_finite().then_some(value)
}

fn bound_f32(value: Option<&Value>) -> Option<f32> {
    match value? {
        Value::Object(value) => finite_f32(value.get("value")),
        value => finite_f32(Some(value)),
    }
}

fn unsupported_property(
    object: u32,
    property: &str,
    feature: &str,
    containment: &str,
) -> WeIrUnsupported {
    WeIrUnsupported {
        object: Some(object),
        pass_index: None,
        feature: format!("{feature}:{property}"),
        expected_subsystem: "semantic ECS transform property system".to_owned(),
        containment: containment.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_relative_keyframes_and_sine_script_to_typed_tracks() {
        let object: Value = serde_json::from_str(
            r#"{
                "origin": {
                    "value": "10 20 0",
                    "animation": {
                        "c0": [
                            {"frame":0,"value":0,"front":{"enabled":true,"magic":true,"x":1,"y":0}},
                            {"frame":15,"value":4,"back":{"enabled":true,"magic":true,"x":-1,"y":0}}
                        ],
                        "options":{"fps":30,"length":30,"mode":"loop","wraploop":true},
                        "relative":true
                    }
                },
                "scale": {
                    "value": "0 0 0",
                    "script": "export function update(value) { value.z = scriptProperties.za + (Math.sin(engine.runtime * scriptProperties.zb) * scriptProperties.zc); return value; }",
                    "scriptproperties":{"za":2,"zb":0.5,"zc":8}
                }
            }"#,
        )
        .expect("object json");
        let mut tracks = Vec::new();
        let mut channels = Vec::new();
        let mut keyframes = Vec::new();
        let mut unsupported = Vec::new();

        ingest_object_transform_tracks(
            7,
            &object,
            &mut tracks,
            &mut channels,
            &mut keyframes,
            &mut unsupported,
        );

        assert_eq!(tracks.len(), 2);
        assert!(tracks[0].relative);
        assert!(tracks[0].wrap_loop);
        assert_eq!(channels[1].kind, WeIrObjectTransformChannelKind::Sine);
        assert_eq!(channels[1].component, 2);
        assert_eq!(channels[1].frequency, 0.5);
        assert_eq!(keyframes.len(), 2);
        assert!(unsupported.is_empty());
    }

    #[test]
    fn defers_angle_sine_script_until_its_evaluation_unit_is_verified() {
        let object: Value = serde_json::from_str(
            r#"{
                "angles": {
                    "value": "0 0 -0.61087",
                    "script": "export function update(value) { value.z = scriptProperties.za + (Math.sin(engine.runtime * scriptProperties.zb) * scriptProperties.zc); return value; }",
                    "scriptproperties":{"za":-35,"zb":0.47,"zc":10}
                }
            }"#,
        )
        .expect("object json");
        let mut tracks = Vec::new();
        let mut channels = Vec::new();
        let mut keyframes = Vec::new();
        let mut unsupported = Vec::new();

        ingest_object_transform_tracks(
            9,
            &object,
            &mut tracks,
            &mut channels,
            &mut keyframes,
            &mut unsupported,
        );

        assert!(tracks.is_empty());
        assert!(channels.is_empty());
        assert!(keyframes.is_empty());
        assert_eq!(unsupported.len(), 1);
        assert_eq!(
            unsupported[0].feature,
            "angle-script-evaluation-unit-unverified:angles"
        );
        assert_eq!(unsupported[0].containment, "static-bound-value-kept");
    }
}
