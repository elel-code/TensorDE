//! Cold glyph-atlas construction for script-mutated text layers.

use std::collections::BTreeSet;

use ab_glyph::{Font, FontArc, point};
use serde_json::Value;

use crate::convert::we_ingest::ir::{
    WeIrDynamicText, WeIrDynamicTextGlyph, WeIrMaterial, WeIrMaterialPass, WeIrMaterialTexture,
    WeIrResourceSource, WeIrScriptProgram, WeIrShaderOrigin, WeIrTexture, WeIrTextureMip,
};
use crate::engine::scene::{
    SCENE_DYNAMIC_TEXT_MAX_GLYPHS, SceneCullMode, SceneDepthTest, ScenePipelineBlend,
    SceneResourceKind, SceneTextHorizontalAlign, SceneTextVerticalAlign,
};

use super::{
    WeIrBuilder, WeTextLayerRaster, bound_string, parse_vec3, retained_glyph_upload,
    text_point_size_pixels_per_em, value_f32,
};
use crate::convert::we_ingest::tex::{
    block_compression::transcode_texture_upload, texture_alpha_coverage_rows,
};

const MAX_ATLAS_DIMENSION: u32 = 8_192;
const GLYPH_GUTTER: u32 = 1;

struct AtlasGlyph {
    codepoint: u32,
    plane_bounds: [f32; 4],
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::convert::we_ingest::ingest) struct DynamicTextAtlasKey {
    font_resource: u32,
    pixels_per_em_bits: u32,
    outline_radius: u32,
    text_color_bits: [u32; 3],
    outline_color_bits: [u32; 3],
    repertoire: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::convert::we_ingest::ingest) struct DynamicTextAtlasEntry {
    atlas_resource: u32,
    material: u32,
    glyph_start: u32,
    glyph_count: u32,
}

pub(super) fn ingest_dynamic_text_layer(
    builder: &mut WeIrBuilder,
    object: u32,
    value: &Value,
    initial_text: &str,
    font_resource: u32,
    font_bytes: Vec<u8>,
    programs: &[WeIrScriptProgram],
) -> Result<(u32, u32), String> {
    let point_size = value_f32(value.get("pointsize"))
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(32.0);
    let pixels_per_em = text_point_size_pixels_per_em(point_size);
    let repertoire = dynamic_text_repertoire(initial_text, programs);
    let key = dynamic_text_atlas_key(font_resource, pixels_per_em, value, &repertoire);
    let entry = if let Some(entry) = builder.dynamic_text_atlases.get(&key).copied() {
        entry
    } else {
        let entry = create_dynamic_text_atlas(
            builder,
            object,
            value,
            font_bytes.clone(),
            pixels_per_em,
            &repertoire,
        )?;
        builder.dynamic_text_atlases.insert(key, entry);
        entry
    };
    let initial = super::rasterize_text_layer(value, initial_text, font_bytes)?;
    builder.add_image_plane_mesh(
        object,
        Some(entry.material),
        initial.width as f32,
        initial.height as f32,
    );
    let spacing = parse_vec3(value.get("spacing")).unwrap_or_default();
    let padding = super::retained_text_padding(value.get("padding"));
    builder.dynamic_texts.push(WeIrDynamicText {
        object,
        font_resource,
        atlas_resource: entry.atlas_resource,
        glyph_start: entry.glyph_start,
        glyph_count: entry.glyph_count,
        max_glyph_count: SCENE_DYNAMIC_TEXT_MAX_GLYPHS,
        pixels_per_em,
        spacing: [spacing.x, spacing.y],
        padding: [padding.0, padding.1],
        horizontal_align: horizontal_align(value),
        vertical_align: vertical_align(value),
    });
    Ok((font_resource, entry.material))
}

fn dynamic_text_atlas_key(
    font_resource: u32,
    pixels_per_em: f32,
    value: &Value,
    repertoire: &BTreeSet<char>,
) -> DynamicTextAtlasKey {
    let text_color = parse_vec3(value.get("color")).unwrap_or(crate::engine::scene::SceneVec3::ONE);
    let outline_color =
        parse_vec3(value.get("outlinecolor")).unwrap_or(crate::engine::scene::SceneVec3::ONE);
    DynamicTextAtlasKey {
        font_resource,
        pixels_per_em_bits: pixels_per_em.to_bits(),
        outline_radius: dynamic_text_outline_radius(value) as u32,
        text_color_bits: [
            text_color.x.to_bits(),
            text_color.y.to_bits(),
            text_color.z.to_bits(),
        ],
        outline_color_bits: [
            outline_color.x.to_bits(),
            outline_color.y.to_bits(),
            outline_color.z.to_bits(),
        ],
        repertoire: repertoire
            .iter()
            .map(|character| *character as u32)
            .collect(),
    }
}

fn create_dynamic_text_atlas(
    builder: &mut WeIrBuilder,
    object: u32,
    value: &Value,
    font_bytes: Vec<u8>,
    pixels_per_em: f32,
    repertoire: &BTreeSet<char>,
) -> Result<DynamicTextAtlasEntry, String> {
    let font = FontArc::try_from_vec(font_bytes)
        .map_err(|_| "dynamic text font is not a supported OpenType/TrueType face".to_owned())?;
    let scale = super::ab_glyph_scale_for_font(&font, pixels_per_em)?;
    let mut glyphs = Vec::with_capacity(repertoire.len());
    for character in repertoire.iter().copied() {
        if let Some(glyph) = rasterize_atlas_glyph(value, &font, scale, character)? {
            glyphs.push(glyph);
        }
    }
    if glyphs.is_empty() {
        return Err("dynamic text glyph repertoire has no renderable glyphs".to_owned());
    }
    let (atlas_width, atlas_height, placements) = pack_glyphs(&glyphs)?;
    let mut atlas = vec![0u8; atlas_width as usize * atlas_height as usize * 4];
    let glyph_start = builder.dynamic_text_glyphs.len() as u32;
    for (glyph, [x, y]) in glyphs.iter().zip(placements) {
        copy_glyph(&mut atlas, atlas_width, glyph, x, y);
        builder.dynamic_text_glyphs.push(WeIrDynamicTextGlyph {
            codepoint: glyph.codepoint,
            atlas_uv: [
                x as f32 / atlas_width as f32,
                y as f32 / atlas_height as f32,
                (x + glyph.width) as f32 / atlas_width as f32,
                (y + glyph.height) as f32 / atlas_height as f32,
            ],
            plane_bounds: glyph.plane_bounds,
        });
    }

    let texture_path = format!("generated/text/{object}.atlas.tex");
    let atlas_resource = builder.add_existing_resource(
        &texture_path,
        SceneResourceKind::TextureTex,
        WeIrResourceSource::Builtin,
        Vec::new(),
    );
    let upload = retained_glyph_upload(WeTextLayerRaster {
        width: atlas_width,
        height: atlas_height,
        rgba: atlas,
    })
    .map_err(|error| format!("build dynamic text atlas upload: {error}"))?;
    let alpha_coverage_rows = texture_alpha_coverage_rows(&upload);
    let upload = transcode_texture_upload(&texture_path, upload)
        .map_err(|error| format!("transcode dynamic text atlas: {error}"))?;
    builder.textures.push(WeIrTexture {
        resource: atlas_resource,
        format: upload.format,
        source_runtime_format: upload.metadata.runtime_format,
        payload_format: upload.metadata.payload_format,
        sampler_filter: upload.metadata.sampler_filter,
        sampler_address_mode: upload.metadata.sampler_address_mode,
        width: upload.metadata.width,
        height: upload.metadata.height,
        storage_width: upload.metadata.storage_width,
        storage_height: upload.metadata.storage_height,
        texv_tag: "TENSOR_WALLPAPER_TEXT_ATLAS".to_owned(),
        texb_tag: "RETAINED_GLYPH_ATLAS".to_owned(),
        sequence_tag: String::new(),
        sequence_cell_width: 0,
        sequence_cell_height: 0,
        sequence_frames: Vec::new(),
        mips: upload
            .mips
            .into_iter()
            .map(|mip| WeIrTextureMip {
                width: mip.width,
                height: mip.height,
                payload_offset: mip.payload_offset,
                payload_len: mip.payload_len,
            })
            .collect(),
        upload_payload: upload.payload,
        alpha_coverage_rows,
    });
    builder
        .texture_by_path
        .insert(texture_path.clone(), atlas_resource);
    let material = add_dynamic_text_material(builder, object, atlas_resource, texture_path);
    Ok(DynamicTextAtlasEntry {
        atlas_resource,
        material,
        glyph_start,
        glyph_count: builder.dynamic_text_glyphs.len() as u32 - glyph_start,
    })
}

fn dynamic_text_repertoire(initial_text: &str, programs: &[WeIrScriptProgram]) -> BTreeSet<char> {
    let mut characters = (' '..='~').collect::<BTreeSet<_>>();
    for source in std::iter::once(initial_text).chain(
        programs
            .iter()
            .flat_map(|program| [&program.source[..], &program.properties_json[..]]),
    ) {
        characters.extend(source.chars().filter(|character| !character.is_control()));
    }
    characters
}

fn dynamic_text_outline_radius(object: &Value) -> f32 {
    if super::bound_bool(object.get("outline")).unwrap_or(false) {
        value_f32(object.get("outlinethickness")).unwrap_or(1.0)
    } else {
        0.0
    }
    .round()
    .clamp(0.0, 16.0)
}

fn rasterize_atlas_glyph(
    object: &Value,
    font: &FontArc,
    scale: ab_glyph::PxScale,
    character: char,
) -> Result<Option<AtlasGlyph>, String> {
    let id = font.glyph_id(character);
    if id.0 == 0 || character.is_whitespace() {
        return Ok(None);
    }
    let positioned = id.with_scale_and_position(scale, point(0.0, 0.0));
    let Some(outlined) = font.outline_glyph(positioned) else {
        return Ok(None);
    };
    let outline_radius = dynamic_text_outline_radius(object);
    let bounds = outlined.px_bounds();
    let min_x = (bounds.min.x - outline_radius).floor();
    let min_y = (bounds.min.y - outline_radius).floor();
    let max_x = (bounds.max.x + outline_radius).ceil();
    let max_y = (bounds.max.y + outline_radius).ceil();
    let width = (max_x - min_x).max(1.0) as u32;
    let height = (max_y - min_y).max(1.0) as u32;
    let positioned = id.with_scale_and_position(scale, point(-min_x, -min_y));
    let outlined = font
        .outline_glyph(positioned)
        .ok_or_else(|| format!("dynamic glyph U+{:04X} lost its outline", character as u32))?;
    let positioned_bounds = outlined.px_bounds();
    let mut alpha = vec![0u8; width as usize * height as usize];
    outlined.draw(|x, y, coverage| {
        let pixel_x = positioned_bounds.min.x.floor() as i32 + x as i32;
        let pixel_y = positioned_bounds.min.y.floor() as i32 + y as i32;
        if pixel_x >= 0 && pixel_y >= 0 && pixel_x < width as i32 && pixel_y < height as i32 {
            let index = pixel_y as usize * width as usize + pixel_x as usize;
            alpha[index] = alpha[index].max((coverage * 255.0).round() as u8);
        }
    });
    let outline = super::dilate_alpha(&alpha, width, height, outline_radius as i32);
    let text_color =
        parse_vec3(object.get("color")).unwrap_or(crate::engine::scene::SceneVec3::ONE);
    let outline_color =
        parse_vec3(object.get("outlinecolor")).unwrap_or(crate::engine::scene::SceneVec3::ONE);
    let mut rgba = vec![0u8; alpha.len() * 4];
    for index in 0..alpha.len() {
        let glyph_coverage = alpha[index] as f32 / 255.0;
        let outline_coverage = outline[index] as f32 / 255.0;
        let resolved_alpha = glyph_coverage + outline_coverage * (1.0 - glyph_coverage);
        let color = if resolved_alpha <= f32::EPSILON {
            crate::engine::scene::SceneVec3::default()
        } else {
            crate::engine::scene::SceneVec3 {
                x: (text_color.x * glyph_coverage
                    + outline_color.x * outline_coverage * (1.0 - glyph_coverage))
                    / resolved_alpha,
                y: (text_color.y * glyph_coverage
                    + outline_color.y * outline_coverage * (1.0 - glyph_coverage))
                    / resolved_alpha,
                z: (text_color.z * glyph_coverage
                    + outline_color.z * outline_coverage * (1.0 - glyph_coverage))
                    / resolved_alpha,
            }
        };
        rgba[index * 4..index * 4 + 4].copy_from_slice(&[
            super::color_byte(color.x),
            super::color_byte(color.y),
            super::color_byte(color.z),
            (resolved_alpha * 255.0).round() as u8,
        ]);
    }
    Ok(Some(AtlasGlyph {
        codepoint: character as u32,
        plane_bounds: [min_x, min_y, max_x, max_y],
        width,
        height,
        rgba,
    }))
}

fn pack_glyphs(glyphs: &[AtlasGlyph]) -> Result<(u32, u32, Vec<[u32; 2]>), String> {
    let maximum_width = glyphs.iter().map(|glyph| glyph.width).max().unwrap_or(1);
    if maximum_width + GLYPH_GUTTER * 2 > MAX_ATLAS_DIMENSION {
        return Err("dynamic glyph exceeds the 8192-pixel atlas contract".to_owned());
    }
    let total_area = glyphs.iter().fold(0u64, |area, glyph| {
        area.saturating_add(
            u64::from(glyph.width + GLYPH_GUTTER) * u64::from(glyph.height + GLYPH_GUTTER),
        )
    });
    let target = (total_area as f64).sqrt().ceil() as u32;
    let width = target
        .next_power_of_two()
        .max((maximum_width + GLYPH_GUTTER * 2).next_power_of_two())
        .min(MAX_ATLAS_DIMENSION);
    let mut placements = Vec::with_capacity(glyphs.len());
    let mut x = GLYPH_GUTTER;
    let mut y = GLYPH_GUTTER;
    let mut row_height = 0;
    for glyph in glyphs {
        if x + glyph.width + GLYPH_GUTTER > width {
            x = GLYPH_GUTTER;
            y = y.saturating_add(row_height + GLYPH_GUTTER);
            row_height = 0;
        }
        if y + glyph.height + GLYPH_GUTTER > MAX_ATLAS_DIMENSION {
            return Err("dynamic glyph repertoire exceeds one 8192x8192 atlas".to_owned());
        }
        placements.push([x, y]);
        x += glyph.width + GLYPH_GUTTER;
        row_height = row_height.max(glyph.height);
    }
    let height = (y + row_height + GLYPH_GUTTER).next_power_of_two().max(1);
    if height > MAX_ATLAS_DIMENSION {
        return Err("dynamic glyph repertoire exceeds one 8192x8192 atlas".to_owned());
    }
    Ok((width, height, placements))
}

fn copy_glyph(atlas: &mut [u8], atlas_width: u32, glyph: &AtlasGlyph, x: u32, y: u32) {
    for row in 0..glyph.height {
        let source = row as usize * glyph.width as usize * 4;
        let destination = ((y + row) as usize * atlas_width as usize + x as usize) * 4;
        atlas[destination..destination + glyph.width as usize * 4]
            .copy_from_slice(&glyph.rgba[source..source + glyph.width as usize * 4]);
    }
}

fn add_dynamic_text_material(
    builder: &mut WeIrBuilder,
    object: u32,
    atlas_resource: u32,
    texture_path: String,
) -> u32 {
    let material_path = format!("generated/text/{object}.atlas.material.json");
    let material_resource = builder.add_existing_resource(
        &material_path,
        SceneResourceKind::MaterialJson,
        WeIrResourceSource::Builtin,
        br#"{"passes":[{"shader":"tensor-wallpaper/dynamic-text","blending":"translucent"}]}"#
            .to_vec(),
    );
    let material = builder.materials.len() as u32;
    let texture_start = builder.material_textures.len() as u32;
    builder.material_textures.push(WeIrMaterialTexture {
        slot: 0,
        resource: Some(atlas_resource),
        path: texture_path,
    });
    let pass_start = builder.material_passes.len() as u32;
    builder.material_passes.push(WeIrMaterialPass {
        material,
        shader_key: "tensor-wallpaper/dynamic-text".to_owned(),
        shader_source_key: "tensor-wallpaper/dynamic-text".to_owned(),
        shader_origin: WeIrShaderOrigin::EngineBuiltIn,
        target: String::new(),
        texture_start,
        texture_count: 1,
        constant_start: builder.material_constants.len() as u32,
        constant_count: 0,
        pipeline_blend: ScenePipelineBlend::Translucent,
        depth_test: SceneDepthTest::Disabled,
        depth_write: false,
        cull_mode: SceneCullMode::None,
        alpha_writing: String::new(),
        clear_target: false,
    });
    builder.materials.push(WeIrMaterial {
        handle: material,
        resource: material_resource,
        pass_start,
        pass_count: 1,
    });
    builder.material_by_path.insert(material_path, material);
    material
}

fn horizontal_align(object: &Value) -> SceneTextHorizontalAlign {
    match bound_string(object.get("horizontalalign")).as_deref() {
        Some("left") => SceneTextHorizontalAlign::Left,
        Some("right") => SceneTextHorizontalAlign::Right,
        _ => SceneTextHorizontalAlign::Center,
    }
}

fn vertical_align(object: &Value) -> SceneTextVerticalAlign {
    match bound_string(object.get("verticalalign")).as_deref() {
        Some("top") => SceneTextVerticalAlign::Top,
        Some("bottom") => SceneTextVerticalAlign::Bottom,
        _ => SceneTextVerticalAlign::Center,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_repertoire_covers_numeric_results_and_authored_non_ascii_literals() {
        let programs = [WeIrScriptProgram {
            object: 0,
            target: crate::engine::scene::SceneScriptTarget::Text,
            selector: 0,
            updates_target_value: true,
            source: "export function update() { return '运行'+17; }".to_owned(),
            properties_json: "{}".to_owned(),
            initial_text: "23".to_owned(),
            subscriptions: crate::engine::scene::SceneScriptSubscriptions::FRAME,
            initial_numeric: [0.0; 4],
        }];
        let repertoire = dynamic_text_repertoire("23", &programs);
        assert!(('0'..='9').all(|character| repertoire.contains(&character)));
        assert!(repertoire.contains(&'运'));
        assert!(repertoire.contains(&'行'));
    }

    #[test]
    fn atlas_key_reuses_identical_style_and_repertoire() {
        let value = serde_json::json!({
            "color": "0.25 0.5 1.0",
            "outline": true,
            "outlinecolor": "1.0 0.5 0.25",
            "outlinethickness": 2.0,
        });
        let repertoire = BTreeSet::from(['0', '1', '秒']);
        assert_eq!(
            dynamic_text_atlas_key(7, 32.0, &value, &repertoire),
            dynamic_text_atlas_key(7, 32.0, &value, &repertoire)
        );
    }

    #[test]
    fn atlas_key_keeps_font_scale_style_and_repertoire_distinct() {
        let plain = serde_json::json!({"color": "1 1 1"});
        let outlined = serde_json::json!({"color": "1 1 1", "outline": true});
        let digits = BTreeSet::from(['0', '1']);
        let chinese = BTreeSet::from(['0', '1', '秒']);
        let base = dynamic_text_atlas_key(7, 32.0, &plain, &digits);
        assert_ne!(base, dynamic_text_atlas_key(8, 32.0, &plain, &digits));
        assert_ne!(base, dynamic_text_atlas_key(7, 64.0, &plain, &digits));
        assert_ne!(base, dynamic_text_atlas_key(7, 32.0, &outlined, &digits));
        assert_ne!(base, dynamic_text_atlas_key(7, 32.0, &plain, &chinese));
    }
}
