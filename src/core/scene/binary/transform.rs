use super::{
    SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE, SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
    SceneBinaryError, read_f32, read_u16, read_u32, read_u64, write_f32, write_u16, write_u32,
    write_u64,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryTransformTimelineRecord {
    pub owner_name: u32,
    pub timeline_name: u32,
    pub property: u16,
    pub flags: u16,
    pub keyframe_count: u32,
    pub first_keyframe: u32,
    pub time_offset_ms: u64,
    pub first_time_ms: u64,
    pub last_time_ms: u64,
    pub value0: f32,
    pub value1: f32,
    pub value2: f32,
    pub value3: f32,
    pub value4: f32,
    pub value5: f32,
    pub value6: f32,
}

impl SceneBinaryTransformTimelineRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.timeline_name);
        write_u16(out, self.property);
        write_u16(out, self.flags);
        write_u32(out, self.keyframe_count);
        write_u32(out, self.first_keyframe);
        write_u32(out, 0);
        write_u64(out, self.time_offset_ms);
        write_u64(out, self.first_time_ms);
        write_u64(out, self.last_time_ms);
        write_f32(out, self.value0);
        write_f32(out, self.value1);
        write_f32(out, self.value2);
        write_f32(out, self.value3);
        write_f32(out, self.value4);
        write_f32(out, self.value5);
        write_f32(out, self.value6);
        write_u32(out, 0);
        debug_assert_eq!(SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE, 80);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryTransformKeyframeRecord {
    pub time_ms: u64,
    pub value: f32,
    pub curve: u16,
    pub flags: u16,
}

impl SceneBinaryTransformKeyframeRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u64(out, self.time_ms);
        write_f32(out, self.value);
        write_u16(out, self.curve);
        write_u16(out, self.flags);
        debug_assert_eq!(SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE, 16);
    }
}

pub(crate) fn decode_transform_timeline_record(
    bytes: &[u8],
) -> Result<SceneBinaryTransformTimelineRecord, SceneBinaryError> {
    Ok(SceneBinaryTransformTimelineRecord {
        owner_name: read_u32(bytes, 0)?,
        timeline_name: read_u32(bytes, 4)?,
        property: read_u16(bytes, 8)?,
        flags: read_u16(bytes, 10)?,
        keyframe_count: read_u32(bytes, 12)?,
        first_keyframe: read_u32(bytes, 16)?,
        time_offset_ms: read_u64(bytes, 24)?,
        first_time_ms: read_u64(bytes, 32)?,
        last_time_ms: read_u64(bytes, 40)?,
        value0: read_f32(bytes, 48)?,
        value1: read_f32(bytes, 52)?,
        value2: read_f32(bytes, 56)?,
        value3: read_f32(bytes, 60)?,
        value4: read_f32(bytes, 64)?,
        value5: read_f32(bytes, 68)?,
        value6: read_f32(bytes, 72)?,
    })
}

pub(crate) fn decode_transform_keyframe_record(
    bytes: &[u8],
) -> Result<SceneBinaryTransformKeyframeRecord, SceneBinaryError> {
    Ok(SceneBinaryTransformKeyframeRecord {
        time_ms: read_u64(bytes, 0)?,
        value: read_f32(bytes, 8)?,
        curve: read_u16(bytes, 12)?,
        flags: read_u16(bytes, 14)?,
    })
}
