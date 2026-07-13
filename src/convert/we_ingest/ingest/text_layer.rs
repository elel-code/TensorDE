//! Cold-path lowering of WE text layers into retained glyph textures.
//!
//! The generated texture is an ordinary typed-IR texture/material input. Runtime script-driven
//! text mutation will replace this retained fallback with a dirty atlas update, without adding a
//! text-specific branch to the Vulkan command path.

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use serde_json::Value;

use crate::engine::scene::SceneVec3;
use crate::engine::scene::{
    SceneCullMode, SceneDepthTest, ScenePipelineBlend, SceneResourceKind, SceneTextureFormat,
};

use super::super::ir::{
    WeIrMaterial, WeIrMaterialPass, WeIrMaterialTexture, WeIrResourceSource, WeIrTexture,
    WeIrTextureMip, WeIrUnsupported,
};
use super::super::tex::{
    TexMetadata, TexUpload, TexUploadMip, block_compression::transcode_texture_upload,
};
use super::{WeIngestError, WeIrBuilder, bound_bool, bound_string, parse_vec3, value_f32};

const TEXT_REFERENCE_HEIGHT: f32 = 1080.0;
const TEXT_VISUAL_SCALE: f32 = 1.5;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct WeTextLayerRaster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub(super) fn text_layer_value(object: &Value) -> Option<String> {
    bound_string(object.get("text")).filter(|text| !text.is_empty())
}

pub(super) fn text_layer_font_path(object: &Value) -> String {
    let font = bound_string(object.get("font")).unwrap_or_default();
    if font.is_empty() || font.starts_with("systemfont_") {
        "fonts/Jost-Medium.ttf".to_owned()
    } else {
        font
    }
}

pub(super) fn retained_text_effect_is_supported(builder: &WeIrBuilder, effect: u32) -> bool {
    builder
        .effects
        .get(effect as usize)
        .and_then(|effect| builder.resources.get(effect.resource as usize))
        .is_some_and(|resource| {
            resource.path == "effects/colorkey/effect.json"
                || resource.path == "effects/scroll/effect.json"
        })
}

pub(super) fn retained_text_effect_requires_dependency_composite(
    builder: &WeIrBuilder,
    effect: u32,
) -> bool {
    builder
        .effects
        .get(effect as usize)
        .and_then(|effect| builder.resources.get(effect.resource as usize))
        .is_some_and(|resource| {
            resource.path.ends_with("/clipping_mask/effect.json")
                || resource.path.ends_with("/clippingmask/effect.json")
        })
}

pub(super) fn ingest_text_layer(
    builder: &mut WeIrBuilder,
    object: u32,
    value: &Value,
    text: &str,
) -> Result<Option<(u32, u32)>, WeIngestError> {
    let font_path = text_layer_font_path(value);
    let Some(font_resource) = builder.add_optional_resource(&font_path, SceneResourceKind::Font)?
    else {
        builder.unsupported.push(WeIrUnsupported {
            object: Some(object),
            pass_index: None,
            feature: format!("missing-text-font:{font_path}"),
            expected_subsystem: "convert/we_ingest asset source".to_owned(),
            containment: "text-object-kept-without-glyph-texture".to_owned(),
        });
        return Ok(None);
    };
    let font_bytes = builder.resources[font_resource as usize].payload.clone();
    let raster = match rasterize_text_layer(value, text, font_bytes, builder.scene.logical_height) {
        Ok(raster) => raster,
        Err(message) => {
            builder.unsupported.push(WeIrUnsupported {
                object: Some(object),
                pass_index: None,
                feature: format!("text-raster-failed:{message}"),
                expected_subsystem: "convert/we_ingest text glyph lowering".to_owned(),
                containment: "text-object-kept-without-glyph-texture".to_owned(),
            });
            return Ok(None);
        }
    };
    let texture_path = format!("generated/text/{object}.tex");
    let texture_resource = builder.add_existing_resource(
        &texture_path,
        SceneResourceKind::TextureTex,
        WeIrResourceSource::Builtin,
        Vec::new(),
    );
    let upload = retained_glyph_upload(raster);
    let upload =
        transcode_texture_upload(&texture_path, upload).map_err(|source| WeIngestError::Tex {
            path: texture_path.clone(),
            source,
        })?;
    let texture_width = upload.metadata.width;
    let texture_height = upload.metadata.height;
    builder.textures.push(WeIrTexture {
        resource: texture_resource,
        format: upload.format,
        source_runtime_format: upload.metadata.runtime_format,
        payload_format: upload.metadata.payload_format,
        sampler_flags: upload.metadata.sampler_flags,
        width: upload.metadata.width,
        height: upload.metadata.height,
        storage_width: upload.metadata.storage_width,
        storage_height: upload.metadata.storage_height,
        texv_tag: upload.metadata.texv_tag,
        texb_tag: upload.metadata.texb_tag,
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
    });
    builder
        .texture_by_path
        .insert(texture_path.clone(), texture_resource);

    let material_path = format!("generated/text/{object}.material.json");
    let material_resource = builder.add_existing_resource(
        &material_path,
        SceneResourceKind::MaterialJson,
        WeIrResourceSource::Builtin,
        br#"{"passes":[{"shader":"genericimage4","blending":"translucent"}]}"#.to_vec(),
    );
    let material = builder.materials.len() as u32;
    let texture_start = builder.material_textures.len() as u32;
    builder.material_textures.push(WeIrMaterialTexture {
        slot: 0,
        resource: Some(texture_resource),
        path: texture_path,
    });
    let pass_start = builder.material_passes.len() as u32;
    builder.material_passes.push(WeIrMaterialPass {
        material,
        shader_key: "we/genericimage4".to_owned(),
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
    let vertex_start = builder.mesh_vertices.len();
    builder.add_image_plane_mesh(
        object,
        Some(material),
        texture_width as f32,
        texture_height as f32,
    );
    apply_text_alignment_anchor(
        builder,
        value,
        vertex_start,
        texture_width as f32,
        texture_height as f32,
    );
    Ok(Some((font_resource, material)))
}

fn apply_text_alignment_anchor(
    builder: &mut WeIrBuilder,
    object: &Value,
    vertex_start: usize,
    width: f32,
    height: f32,
) {
    let offset_x = match bound_string(object.get("horizontalalign")).as_deref() {
        Some("left") => width * 0.5,
        Some("right") => -width * 0.5,
        _ => 0.0,
    };
    let offset_y = match bound_string(object.get("verticalalign")).as_deref() {
        Some("top") => -height * 0.5,
        Some("bottom") => height * 0.5,
        _ => 0.0,
    };
    for vertex in &mut builder.mesh_vertices[vertex_start..] {
        vertex.position.x += offset_x;
        vertex.position.y += offset_y;
    }
    if let Some(mesh) = builder.meshes.last_mut() {
        mesh.bounds_min.x += offset_x;
        mesh.bounds_min.y += offset_y;
        mesh.bounds_max.x += offset_x;
        mesh.bounds_max.y += offset_y;
    }
}

fn retained_glyph_upload(raster: WeTextLayerRaster) -> TexUpload {
    TexUpload {
        metadata: TexMetadata {
            texv_tag: "GILDER_TEXT".to_owned(),
            texi_tag: "RGBA8".to_owned(),
            texb_tag: "RETAINED_GLYPHS".to_owned(),
            runtime_format: 0,
            payload_format: 0,
            sampler_flags: 2,
            width: raster.width,
            height: raster.height,
            storage_width: raster.width,
            storage_height: raster.height,
            mip_count: 1,
        },
        format: SceneTextureFormat::Rgba8Unorm,
        mips: vec![TexUploadMip {
            width: raster.width,
            height: raster.height,
            payload_offset: 0,
            payload_len: raster.rgba.len() as u64,
        }],
        payload: raster.rgba,
    }
}

pub(super) fn rasterize_text_layer(
    object: &Value,
    text: &str,
    font_bytes: Vec<u8>,
    scene_height: u32,
) -> Result<WeTextLayerRaster, String> {
    let size = parse_vec3(object.get("size")).ok_or("text layer is missing a valid size")?;
    let width = checked_dimension(size.x, "width")?;
    let height = checked_dimension(size.y, "height")?;
    let font = FontArc::try_from_vec(font_bytes)
        .map_err(|_| "font payload is not a supported OpenType/TrueType face".to_owned())?;
    let point_size = text_point_size_pixels(
        value_f32(object.get("pointsize"))
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(32.0),
        scene_height,
    );
    let spacing = parse_vec3(object.get("spacing")).map_or(0.0, |spacing| spacing.x);
    let scale = PxScale::from(point_size);
    let scaled = font.as_scaled(scale);
    let glyphs = layout_glyphs(&font, text, scale, spacing);
    let text_width = glyphs
        .last()
        .map(|glyph| glyph.position.x + scaled.h_advance(glyph.id))
        .unwrap_or(0.0)
        .max(0.0);
    let horizontal_align = bound_string(object.get("horizontalalign")).unwrap_or_default();
    let start_x = match horizontal_align.as_str() {
        "right" => width as f32 - text_width,
        "center" => (width as f32 - text_width) * 0.5,
        _ => 0.0,
    };
    let vertical_align = bound_string(object.get("verticalalign")).unwrap_or_default();
    let line_height = scaled.height();
    let line_top = match vertical_align.as_str() {
        "bottom" => height as f32 - line_height,
        "center" => (height as f32 - line_height) * 0.5,
        _ => 0.0,
    };
    let baseline = line_top + scaled.ascent();
    let mut glyph_alpha = vec![0u8; width as usize * height as usize];
    for glyph in glyphs {
        let positioned = glyph
            .id
            .with_scale_and_position(scale, point(start_x + glyph.position.x, baseline));
        let Some(outlined) = font.outline_glyph(positioned) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|x, y, coverage| {
            let pixel_x = bounds.min.x.floor() as i32 + x as i32;
            let pixel_y = bounds.min.y.floor() as i32 + y as i32;
            if pixel_x < 0 || pixel_y < 0 || pixel_x >= width as i32 || pixel_y >= height as i32 {
                return;
            }
            let index = pixel_y as usize * width as usize + pixel_x as usize;
            glyph_alpha[index] = glyph_alpha[index].max((coverage * 255.0).round() as u8);
        });
    }

    let text_color = parse_vec3(object.get("color")).unwrap_or(SceneVec3::ONE);
    let outline_enabled = bound_bool(object.get("outline")).unwrap_or(false);
    let outline_radius = outline_enabled
        .then(|| value_f32(object.get("outlinethickness")).unwrap_or(1.0))
        .unwrap_or(0.0)
        .round()
        .clamp(0.0, 16.0) as i32;
    let outline_color = parse_vec3(object.get("outlinecolor")).unwrap_or(SceneVec3::ONE);
    let outline_alpha = dilate_alpha(&glyph_alpha, width, height, outline_radius);
    let mut rgba = vec![0u8; glyph_alpha.len() * 4];
    for index in 0..glyph_alpha.len() {
        let glyph_coverage = glyph_alpha[index] as f32 / 255.0;
        let outline_coverage = outline_alpha[index] as f32 / 255.0;
        let alpha = glyph_coverage + outline_coverage * (1.0 - glyph_coverage);
        let color = if alpha <= f32::EPSILON {
            SceneVec3::default()
        } else {
            SceneVec3 {
                x: (text_color.x * glyph_coverage
                    + outline_color.x * outline_coverage * (1.0 - glyph_coverage))
                    / alpha,
                y: (text_color.y * glyph_coverage
                    + outline_color.y * outline_coverage * (1.0 - glyph_coverage))
                    / alpha,
                z: (text_color.z * glyph_coverage
                    + outline_color.z * outline_coverage * (1.0 - glyph_coverage))
                    / alpha,
            }
        };
        let destination = &mut rgba[index * 4..index * 4 + 4];
        destination.copy_from_slice(&[
            color_byte(color.x),
            color_byte(color.y),
            color_byte(color.z),
            (alpha * 255.0).round() as u8,
        ]);
    }
    Ok(WeTextLayerRaster {
        width,
        height,
        rgba,
    })
}

fn layout_glyphs(font: &FontArc, text: &str, scale: PxScale, spacing: f32) -> Vec<ab_glyph::Glyph> {
    let scaled = font.as_scaled(scale);
    let mut cursor_x = 0.0;
    let mut previous = None;
    let mut glyphs = Vec::with_capacity(text.chars().count());
    for character in text.chars() {
        let id = font.glyph_id(character);
        if let Some(previous) = previous {
            cursor_x += scaled.kern(previous, id);
        }
        glyphs.push(id.with_scale_and_position(scale, point(cursor_x, 0.0)));
        cursor_x += scaled.h_advance(id) + spacing;
        previous = Some(id);
    }
    glyphs
}

fn dilate_alpha(source: &[u8], width: u32, height: u32, radius: i32) -> Vec<u8> {
    if radius <= 0 {
        return vec![0; source.len()];
    }
    let mut output = vec![0u8; source.len()];
    let radius_squared = radius * radius;
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let mut coverage = 0u8;
            for offset_y in -radius..=radius {
                for offset_x in -radius..=radius {
                    if offset_x * offset_x + offset_y * offset_y > radius_squared {
                        continue;
                    }
                    let sample_x = x + offset_x;
                    let sample_y = y + offset_y;
                    if sample_x < 0
                        || sample_y < 0
                        || sample_x >= width as i32
                        || sample_y >= height as i32
                    {
                        continue;
                    }
                    coverage = coverage
                        .max(source[sample_y as usize * width as usize + sample_x as usize]);
                }
            }
            output[y as usize * width as usize + x as usize] = coverage;
        }
    }
    output
}

fn checked_dimension(value: f32, label: &str) -> Result<u32, String> {
    if !value.is_finite() || value <= 0.0 || value > 8192.0 {
        return Err(format!("text layer {label} {value} is outside 1..=8192"));
    }
    Ok(value.round().max(1.0) as u32)
}

fn text_point_size_pixels(point_size: f32, scene_height: u32) -> f32 {
    point_size * (scene_height.max(1) as f32 / TEXT_REFERENCE_HEIGHT).max(1.0) * TEXT_VISUAL_SCALE
}

fn color_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_value_reads_bound_default() {
        let object = serde_json::json!({"text": {"user": "title", "value": "DREAM"}});
        assert_eq!(text_layer_value(&object).as_deref(), Some("DREAM"));
    }

    #[test]
    fn system_font_uses_portable_scene_fallback() {
        let object = serde_json::json!({"font": "systemfont_arial"});
        assert_eq!(text_layer_font_path(&object), "fonts/Jost-Medium.ttf");
    }

    #[test]
    fn wallpaper_engine_point_size_tracks_authored_scene_resolution() {
        assert_eq!(text_point_size_pixels(96.0, 2160), 288.0);
        assert_eq!(text_point_size_pixels(96.0, 1080), 144.0);
        assert_eq!(text_point_size_pixels(96.0, 720), 144.0);
    }
}
