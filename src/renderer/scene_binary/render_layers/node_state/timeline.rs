//! Transform timeline sampling for binary scene node state.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use crate::core::SceneTransform;
use crate::core::scene::binary::{
    SceneBinaryChunkKind, SceneBinaryGeometryRecord, SceneBinaryNodeRecord,
    SceneBinaryTransformTimelineRecord,
};
use crate::renderer::RendererPlanError;

use super::super::super::reader::{BinarySceneReader, binary_scene_cached_record_slice};
use super::super::super::schema::{
    BINARY_NODE_FLAG_CORNER_RADIUS, BINARY_TRANSFORM_FLAG_LOOP,
    BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS, BINARY_TRANSFORM_PROPERTY_DEFAULT,
    BINARY_TRANSFORM_PROPERTY_HEIGHT, BINARY_TRANSFORM_PROPERTY_OPACITY,
    BINARY_TRANSFORM_PROPERTY_ROTATION_DEG, BINARY_TRANSFORM_PROPERTY_SCALE_X,
    BINARY_TRANSFORM_PROPERTY_SCALE_Y, BINARY_TRANSFORM_PROPERTY_WIDTH,
    BINARY_TRANSFORM_PROPERTY_X, BINARY_TRANSFORM_PROPERTY_Y,
};
use super::BinarySceneNodeState;

pub(super) fn binary_scene_node_state(
    reader: &mut BinarySceneReader,
    node: SceneBinaryNodeRecord,
    geometry: Option<SceneBinaryGeometryRecord>,
    snapshot_time_ms: u64,
) -> Result<BinarySceneNodeState, RendererPlanError> {
    let mut state = BinarySceneNodeState {
        transform: SceneTransform::default(),
        opacity: f64::from(node.opacity),
        width: geometry
            .and_then(|geometry| (geometry.width > 0.0).then_some(f64::from(geometry.width))),
        height: geometry
            .and_then(|geometry| (geometry.height > 0.0).then_some(f64::from(geometry.height))),
        corner_radius: (node.flags & BINARY_NODE_FLAG_CORNER_RADIUS != 0)
            .then_some(f64::from(node.corner_radius)),
    };
    let timeline_record_count = reader.chunk_count(SceneBinaryChunkKind::TransformTimeline);
    let timeline_records = reader.transform_timeline_records_cached()?;
    let records = binary_scene_cached_record_slice(
        &timeline_records,
        SceneBinaryChunkKind::TransformTimeline,
        node.first_transform,
        node.transform_count,
        timeline_record_count,
    )?;
    for record in records.iter().copied() {
        if record.property == BINARY_TRANSFORM_PROPERTY_DEFAULT {
            state.transform = binary_scene_default_transform(record);
            continue;
        }
        if record.keyframe_count == 0 {
            continue;
        }
        let Some(value) = binary_scene_transform_timeline_value(reader, record, snapshot_time_ms)?
        else {
            continue;
        };
        binary_scene_apply_timeline_value(&mut state, record.property, value);
    }
    Ok(state)
}

pub(super) fn binary_scene_node_has_timed_transform(
    reader: &mut BinarySceneReader,
    node: SceneBinaryNodeRecord,
) -> Result<bool, RendererPlanError> {
    let timeline_record_count = reader.chunk_count(SceneBinaryChunkKind::TransformTimeline);
    let timeline_records = reader.transform_timeline_records_cached()?;
    let records = binary_scene_cached_record_slice(
        &timeline_records,
        SceneBinaryChunkKind::TransformTimeline,
        node.first_transform,
        node.transform_count,
        timeline_record_count,
    )?;
    Ok(records.iter().any(|record| {
        record.property != BINARY_TRANSFORM_PROPERTY_DEFAULT && record.keyframe_count > 0
    }))
}

pub(super) fn binary_scene_apply_timeline_value(
    state: &mut BinarySceneNodeState,
    property: u16,
    value: f64,
) {
    match property {
        BINARY_TRANSFORM_PROPERTY_X => state.transform.x = value,
        BINARY_TRANSFORM_PROPERTY_Y => state.transform.y = value,
        BINARY_TRANSFORM_PROPERTY_SCALE_X if value > 0.0 => state.transform.scale_x = value,
        BINARY_TRANSFORM_PROPERTY_SCALE_Y if value > 0.0 => state.transform.scale_y = value,
        BINARY_TRANSFORM_PROPERTY_OPACITY => state.opacity = value.clamp(0.0, 1.0),
        BINARY_TRANSFORM_PROPERTY_ROTATION_DEG => state.transform.rotation_deg = value,
        BINARY_TRANSFORM_PROPERTY_WIDTH => state.width = Some(value.max(0.0)),
        BINARY_TRANSFORM_PROPERTY_HEIGHT => state.height = Some(value.max(0.0)),
        BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS => state.corner_radius = Some(value.max(0.0)),
        _ => {}
    }
}

fn binary_scene_default_transform(record: SceneBinaryTransformTimelineRecord) -> SceneTransform {
    SceneTransform {
        x: f64::from(record.value0),
        y: f64::from(record.value1),
        scale_x: f64::from(record.value2),
        scale_y: f64::from(record.value3),
        rotation_deg: f64::from(record.value4),
        anchor_x: f64::from(record.value5),
        anchor_y: f64::from(record.value6),
    }
}

fn binary_scene_transform_timeline_value(
    reader: &mut BinarySceneReader,
    record: SceneBinaryTransformTimelineRecord,
    snapshot_time_ms: u64,
) -> Result<Option<f64>, RendererPlanError> {
    let keyframe_record_count = reader.chunk_count(SceneBinaryChunkKind::TransformKeyframes);
    let keyframe_records = reader.transform_keyframe_records_cached()?;
    let keyframes = binary_scene_cached_record_slice(
        &keyframe_records,
        SceneBinaryChunkKind::TransformKeyframes,
        record.first_keyframe,
        record.keyframe_count,
        keyframe_record_count,
    )?;
    let mut keyframes = keyframes.iter().copied();
    let Some(first) = keyframes.next() else {
        return Ok(None);
    };
    if record.keyframe_count == 1 {
        return Ok(Some(f64::from(first.value)));
    }
    let mut time_ms = snapshot_time_ms.saturating_add(record.time_offset_ms);
    if record.flags & BINARY_TRANSFORM_FLAG_LOOP != 0 && record.last_time_ms > 0 {
        time_ms %= record.last_time_ms;
    }
    if time_ms <= first.time_ms {
        return Ok(Some(f64::from(first.value)));
    }
    let mut previous = first;
    for next in keyframes {
        if time_ms <= next.time_ms {
            let span = next.time_ms.saturating_sub(previous.time_ms) as f64;
            let progress = if span > 0.0 {
                (time_ms.saturating_sub(previous.time_ms) as f64 / span).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let eased = binary_scene_curve_ease(next.curve, progress);
            return Ok(Some(
                f64::from(previous.value)
                    + (f64::from(next.value) - f64::from(previous.value)) * eased,
            ));
        }
        previous = next;
    }
    Ok(Some(f64::from(previous.value)))
}

fn binary_scene_curve_ease(code: u16, value: f64) -> f64 {
    match code {
        2 => {
            if value >= 1.0 {
                1.0
            } else {
                0.0
            }
        }
        3 => value * value,
        4 => 1.0 - (1.0 - value) * (1.0 - value),
        5 => {
            if value < 0.5 {
                2.0 * value * value
            } else {
                1.0 - (-2.0 * value + 2.0).powi(2) / 2.0
            }
        }
        _ => value,
    }
}
