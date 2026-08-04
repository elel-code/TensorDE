//! Strict codec for retained dynamic-text atlas metadata.

use super::{Decoder, SceneBinaryError, checked_u32, put_f32, put_u32};
use crate::engine::scene::abi::{
    SceneDynamicTextGlyphRecord, SceneDynamicTextRecord, SceneObjectHandle, SceneResourceId,
    SceneTextHorizontalAlign, SceneTextVerticalAlign,
};

pub(super) fn encode_dynamic_text(
    texts: &[SceneDynamicTextRecord],
    glyphs: &[SceneDynamicTextGlyphRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(texts.len(), "dynamic text count")?);
    for text in texts {
        put_u32(&mut out, text.object.0);
        put_u32(&mut out, text.font_resource.0);
        put_u32(&mut out, text.atlas_resource.0);
        put_u32(&mut out, text.glyph_start);
        put_u32(&mut out, text.glyph_count);
        put_u32(&mut out, text.max_glyph_count);
        put_f32(&mut out, text.pixels_per_em);
        put_f32(&mut out, text.spacing[0]);
        put_f32(&mut out, text.spacing[1]);
        put_f32(&mut out, text.padding[0]);
        put_f32(&mut out, text.padding[1]);
        put_u32(&mut out, text.horizontal_align.to_u32());
        put_u32(&mut out, text.vertical_align.to_u32());
    }
    put_u32(
        &mut out,
        checked_u32(glyphs.len(), "dynamic text glyph count")?,
    );
    for glyph in glyphs {
        put_u32(&mut out, glyph.codepoint);
        for value in glyph.atlas_uv.into_iter().chain(glyph.plane_bounds) {
            put_f32(&mut out, value);
        }
    }
    Ok(out)
}

pub(super) fn decode_dynamic_text(
    data: &[u8],
) -> Result<
    (
        Vec<SceneDynamicTextRecord>,
        Vec<SceneDynamicTextGlyphRecord>,
    ),
    SceneBinaryError,
> {
    let mut decoder = Decoder::new(data);
    let text_count = decoder.u32()? as usize;
    let mut texts = Vec::with_capacity(text_count);
    for _ in 0..text_count {
        let object = SceneObjectHandle(decoder.u32()?);
        let font_resource = SceneResourceId(decoder.u32()?);
        let atlas_resource = SceneResourceId(decoder.u32()?);
        let glyph_start = decoder.u32()?;
        let glyph_count = decoder.u32()?;
        let max_glyph_count = decoder.u32()?;
        let pixels_per_em = decoder.f32()?;
        let spacing = [decoder.f32()?, decoder.f32()?];
        let padding = [decoder.f32()?, decoder.f32()?];
        let horizontal_raw = decoder.u32()?;
        let vertical_raw = decoder.u32()?;
        texts.push(SceneDynamicTextRecord {
            object,
            font_resource,
            atlas_resource,
            glyph_start,
            glyph_count,
            max_glyph_count,
            pixels_per_em,
            spacing,
            padding,
            horizontal_align: SceneTextHorizontalAlign::from_u32(horizontal_raw).ok_or(
                SceneBinaryError::InvalidChunkValue(
                    "dynamic text horizontal alignment",
                    horizontal_raw,
                ),
            )?,
            vertical_align: SceneTextVerticalAlign::from_u32(vertical_raw).ok_or(
                SceneBinaryError::InvalidChunkValue(
                    "dynamic text vertical alignment",
                    vertical_raw,
                ),
            )?,
        });
    }
    let glyph_count = decoder.u32()? as usize;
    let mut glyphs = Vec::with_capacity(glyph_count);
    for _ in 0..glyph_count {
        glyphs.push(SceneDynamicTextGlyphRecord {
            codepoint: decoder.u32()?,
            atlas_uv: decoder.f32_array4()?,
            plane_bounds: decoder.f32_array4()?,
        });
    }
    Ok((texts, glyphs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_text_chunk_round_trips_atlas_and_layout_contract() {
        let texts = [SceneDynamicTextRecord {
            object: SceneObjectHandle(3),
            font_resource: SceneResourceId(5),
            atlas_resource: SceneResourceId(8),
            glyph_start: 0,
            glyph_count: 1,
            max_glyph_count: 1024,
            pixels_per_em: 75.0,
            spacing: [2.0, 3.0],
            padding: [1.0, 0.5],
            horizontal_align: SceneTextHorizontalAlign::Right,
            vertical_align: SceneTextVerticalAlign::Center,
        }];
        let glyphs = [SceneDynamicTextGlyphRecord {
            codepoint: '秒' as u32,
            atlas_uv: [0.1, 0.2, 0.3, 0.4],
            plane_bounds: [-1.0, -2.0, 7.0, 9.0],
        }];
        let encoded = encode_dynamic_text(&texts, &glyphs).expect("encode");
        let decoded = decode_dynamic_text(&encoded).expect("decode");
        assert_eq!(decoded, (texts.to_vec(), glyphs.to_vec()));
    }
}
