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
        put_u32(&mut out, record.sampler_filter.to_u32());
        put_u32(&mut out, record.sampler_address_mode.to_u32());
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
        for row in record.alpha_coverage_rows {
            put_u32(&mut out, row);
        }
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
        let source_runtime_format = decoder.u32()?;
        let payload_format = decoder.u32()?;
        let sampler_filter_raw = decoder.u32()?;
        let sampler_address_mode_raw = decoder.u32()?;
        let width = decoder.u32()?;
        let height = decoder.u32()?;
        let storage_width = decoder.u32()?;
        let storage_height = decoder.u32()?;
        let mip_start = decoder.u32()?;
        let mip_count = decoder.u32()?;
        let texv_tag = decoder.string_id()?;
        let texb_tag = decoder.string_id()?;
        let payload_offset = decoder.u64()?;
        let payload_len = decoder.u64()?;
        let mut alpha_coverage_rows = [0u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE];
        for row in &mut alpha_coverage_rows {
            *row = decoder.u32()?;
        }
        records.push(SceneTextureRecord {
            resource,
            format: SceneTextureFormat::from_u32(format_raw).ok_or(
                SceneBinaryError::InvalidChunkValue("texture format", format_raw),
            )?,
            source_runtime_format,
            payload_format,
            sampler_filter: SceneTextureSamplerFilter::from_u32(sampler_filter_raw).ok_or(
                SceneBinaryError::InvalidChunkValue("texture sampler filter", sampler_filter_raw),
            )?,
            sampler_address_mode: SceneTextureSamplerAddressMode::from_u32(
                sampler_address_mode_raw,
            )
            .ok_or(SceneBinaryError::InvalidChunkValue(
                "texture sampler address mode",
                sampler_address_mode_raw,
            ))?,
            width,
            height,
            storage_width,
            storage_height,
            mip_start,
            mip_count,
            texv_tag,
            texb_tag,
            payload_offset,
            payload_len,
            alpha_coverage_rows,
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
            sampler_filter: SceneTextureSamplerFilter::Anisotropic8,
            sampler_address_mode: SceneTextureSamplerAddressMode::ClampToEdge,
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
            alpha_coverage_rows: [u32::MAX; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
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
