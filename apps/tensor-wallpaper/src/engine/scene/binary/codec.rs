use super::{SceneBinaryError, SceneResourceId, SceneStringId, SceneVec3};

pub(super) fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn put_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn put_bool(out: &mut Vec<u8>, value: bool) {
    out.push(u8::from(value));
}

pub(super) fn put_string_id(out: &mut Vec<u8>, value: SceneStringId) {
    put_u32(out, value.0);
}

pub(super) fn put_resource_id(out: &mut Vec<u8>, value: SceneResourceId) {
    put_u32(out, value.0);
}

pub(super) fn put_vec3(out: &mut Vec<u8>, value: SceneVec3) {
    put_f32(out, value.x);
    put_f32(out, value.y);
    put_f32(out, value.z);
}

pub(super) fn put_f32_array(out: &mut Vec<u8>, values: &[f32; 4]) {
    for value in values {
        put_f32(out, *value);
    }
}

pub(super) fn read_u32_at(
    data: &[u8],
    offset: usize,
    name: &'static str,
) -> Result<u32, SceneBinaryError> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or(SceneBinaryError::Truncated(name))?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("u32 slice")))
}

pub(super) fn read_u64_at(
    data: &[u8],
    offset: usize,
    name: &'static str,
) -> Result<u64, SceneBinaryError> {
    let bytes = data
        .get(offset..offset + 8)
        .ok_or(SceneBinaryError::Truncated(name))?;
    Ok(u64::from_le_bytes(bytes.try_into().expect("u64 slice")))
}

pub(super) fn checked_u32(value: usize, name: &'static str) -> Result<u32, SceneBinaryError> {
    u32::try_from(value).map_err(|_| SceneBinaryError::SizeOverflow(name))
}

pub(super) fn checked_u64(value: usize, name: &'static str) -> Result<u64, SceneBinaryError> {
    u64::try_from(value).map_err(|_| SceneBinaryError::SizeOverflow(name))
}

pub(super) struct Decoder<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    pub(super) fn bytes(&mut self, len: usize) -> Result<&'a [u8], SceneBinaryError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(SceneBinaryError::SizeOverflow("decode offset"))?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or(SceneBinaryError::Truncated("chunk payload"))?;
        self.offset = end;
        Ok(bytes)
    }

    pub(super) fn u32(&mut self) -> Result<u32, SceneBinaryError> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?.try_into().expect("u32 slice"),
        ))
    }

    pub(super) fn i32(&mut self) -> Result<i32, SceneBinaryError> {
        Ok(i32::from_le_bytes(
            self.bytes(4)?.try_into().expect("i32 slice"),
        ))
    }

    pub(super) fn i64(&mut self) -> Result<i64, SceneBinaryError> {
        Ok(i64::from_le_bytes(
            self.bytes(8)?.try_into().expect("i64 slice"),
        ))
    }

    pub(super) fn u64(&mut self) -> Result<u64, SceneBinaryError> {
        Ok(u64::from_le_bytes(
            self.bytes(8)?.try_into().expect("u64 slice"),
        ))
    }

    pub(super) fn f32(&mut self) -> Result<f32, SceneBinaryError> {
        Ok(f32::from_le_bytes(
            self.bytes(4)?.try_into().expect("f32 slice"),
        ))
    }

    pub(super) fn bool(&mut self) -> Result<bool, SceneBinaryError> {
        Ok(*self
            .bytes(1)?
            .first()
            .ok_or(SceneBinaryError::Truncated("bool"))?
            != 0)
    }

    pub(super) fn string_id(&mut self) -> Result<SceneStringId, SceneBinaryError> {
        Ok(SceneStringId(self.u32()?))
    }

    pub(super) fn resource_id(&mut self) -> Result<SceneResourceId, SceneBinaryError> {
        Ok(SceneResourceId(self.u32()?))
    }

    pub(super) fn vec3(&mut self) -> Result<SceneVec3, SceneBinaryError> {
        Ok(SceneVec3 {
            x: self.f32()?,
            y: self.f32()?,
            z: self.f32()?,
        })
    }

    pub(super) fn f32_array4(&mut self) -> Result<[f32; 4], SceneBinaryError> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }
}
