//! Texture record and mip table binary codec.

use super::*;

pub(super) fn encode_textures(textures: &[SceneTextureRecord]) -> Vec<u8> {
    let mut out = Vec::new();
    put_u32(&mut out, textures.len() as u32);
    for record in textures {
        put_resource_id(&mut out, record.resource);
        put_u32(&mut out, record.format.to_u32());
        put_u32(&mut out, record.source_runtime_format);
        put_u32(&mut out, record.payload_format);
        put_u32(&mut out, record.sampler_flags);
        put_u32(&mut out, record.width);
        put_u32(&mut out, record.height);
        put_u32(&mut out, record.storage_width);
        put_u32(&mut out, record.storage_height);
        put_u32(&mut out, record.mip_start);
        put_u32(&mut out, record.mip_count);
        put_string_id(&mut out, record.texv_tag);
        put_string_id(&mut out, record.texb_tag);
        put_u64(&mut out, record.payload_offset);
        put_u64(&mut out, record.payload_len);
    }
    out
}

pub(super) fn decode_textures(data: &[u8]) -> Result<Vec<SceneTextureRecord>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let count = decoder.u32()? as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let resource = decoder.resource_id()?;
        let format_raw = decoder.u32()?;
        records.push(SceneTextureRecord {
            resource,
            format: SceneTextureFormat::from_u32(format_raw).ok_or(
                SceneBinaryError::InvalidChunkValue("texture format", format_raw),
            )?,
            source_runtime_format: decoder.u32()?,
            payload_format: decoder.u32()?,
            sampler_flags: decoder.u32()?,
            width: decoder.u32()?,
            height: decoder.u32()?,
            storage_width: decoder.u32()?,
            storage_height: decoder.u32()?,
            mip_start: decoder.u32()?,
            mip_count: decoder.u32()?,
            texv_tag: decoder.string_id()?,
            texb_tag: decoder.string_id()?,
            payload_offset: decoder.u64()?,
            payload_len: decoder.u64()?,
        });
    }
    Ok(records)
}

pub(super) fn encode_texture_mips(mips: &[SceneTextureMipRecord]) -> Vec<u8> {
    let mut out = Vec::new();
    put_u32(&mut out, mips.len() as u32);
    for mip in mips {
        put_u32(&mut out, mip.width);
        put_u32(&mut out, mip.height);
        put_u64(&mut out, mip.payload_offset);
        put_u64(&mut out, mip.payload_len);
    }
    out
}

pub(super) fn decode_texture_mips(
    data: &[u8],
) -> Result<Vec<SceneTextureMipRecord>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let count = decoder.u32()? as usize;
    let mut mips = Vec::with_capacity(count);
    for _ in 0..count {
        mips.push(SceneTextureMipRecord {
            width: decoder.u32()?,
            height: decoder.u32()?,
            payload_offset: decoder.u64()?,
            payload_len: decoder.u64()?,
        });
    }
    Ok(mips)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_records_round_trip_gpu_format_and_mips() {
        let texture = SceneTextureRecord {
            resource: SceneResourceId(7),
            format: SceneTextureFormat::Bc7UnormBlock,
            source_runtime_format: 0,
            payload_format: 2,
            sampler_flags: 10,
            width: 8,
            height: 8,
            storage_width: 8,
            storage_height: 8,
            mip_start: 0,
            mip_count: 1,
            texv_tag: SceneStringId(1),
            texb_tag: SceneStringId(2),
            payload_offset: 0,
            payload_len: 64,
        };
        let mip = SceneTextureMipRecord {
            width: 8,
            height: 8,
            payload_offset: 0,
            payload_len: 64,
        };

        assert_eq!(
            decode_textures(&encode_textures(&[texture])).unwrap(),
            vec![texture]
        );
        assert_eq!(
            decode_texture_mips(&encode_texture_mips(&[mip])).unwrap(),
            vec![mip]
        );
    }
}
