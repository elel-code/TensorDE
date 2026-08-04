//! Strict retained dynamic-text resource validation.

use super::*;

pub(super) fn validate_dynamic_text(
    document: &SceneBinaryDocument,
) -> Result<(), SceneStorageError> {
    if let Some(object) = duplicate_object(&document.dynamic_texts) {
        return Err(SceneStorageError::InvalidScriptProgram {
            object,
            reason: "duplicate retained dynamic-text object contract",
        });
    }
    for text in &document.dynamic_texts {
        validate_range(
            "dynamic_text.object",
            text.object.0,
            1,
            document.objects.len(),
        )?;
        validate_resource(document, "dynamic_text.font_resource", text.font_resource)?;
        validate_resource(document, "dynamic_text.atlas_resource", text.atlas_resource)?;
        validate_range(
            "dynamic_text.glyph_range",
            text.glyph_start,
            text.glyph_count,
            document.dynamic_text_glyphs.len(),
        )?;
        let valid = text.glyph_count != 0
            && (1..=SCENE_DYNAMIC_TEXT_MAX_GLYPHS).contains(&text.max_glyph_count)
            && text.pixels_per_em.is_finite()
            && text.pixels_per_em > 0.0
            && text.spacing.into_iter().all(f32::is_finite)
            && text
                .padding
                .into_iter()
                .all(|padding| padding.is_finite() && (0.0..=1.0).contains(&padding))
            && document
                .resources
                .get(text.font_resource.0 as usize)
                .is_some_and(|resource| resource.kind == SceneResourceKind::Font)
            && document
                .textures
                .iter()
                .any(|texture| texture.resource == text.atlas_resource)
            && document.script_programs.iter().any(|program| {
                program.object == text.object && program.target == SceneScriptTarget::Text
            });
        if !valid {
            return Err(SceneStorageError::InvalidScriptProgram {
                object: text.object,
                reason: "invalid retained dynamic-text atlas contract",
            });
        }
        let glyphs = document
            .dynamic_text_glyphs
            .get(
                text.glyph_start as usize
                    ..text.glyph_start.saturating_add(text.glyph_count) as usize,
            )
            .expect("dynamic text glyph range was validated");
        if !glyphs
            .windows(2)
            .all(|pair| pair[0].codepoint < pair[1].codepoint)
            || glyphs.iter().any(|glyph| {
                char::from_u32(glyph.codepoint).is_none()
                    || !valid_atlas_uv(glyph.atlas_uv)
                    || !valid_plane_bounds(glyph.plane_bounds)
            })
        {
            return Err(SceneStorageError::InvalidScriptProgram {
                object: text.object,
                reason: "dynamic-text glyphs must be ordered bounded Unicode scalar records",
            });
        }
    }
    Ok(())
}

fn valid_atlas_uv(bounds: [f32; 4]) -> bool {
    bounds.into_iter().all(f32::is_finite)
        && 0.0 <= bounds[0]
        && bounds[0] < bounds[2]
        && bounds[2] <= 1.0
        && 0.0 <= bounds[1]
        && bounds[1] < bounds[3]
        && bounds[3] <= 1.0
}

fn valid_plane_bounds(bounds: [f32; 4]) -> bool {
    bounds.into_iter().all(f32::is_finite) && bounds[0] < bounds[2] && bounds[1] < bounds[3]
}

fn duplicate_object(texts: &[SceneDynamicTextRecord]) -> Option<SceneObjectHandle> {
    let mut objects = std::collections::BTreeSet::new();
    texts
        .iter()
        .map(|text| text.object)
        .find(|object| !objects.insert(*object))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(object: u32) -> SceneDynamicTextRecord {
        SceneDynamicTextRecord {
            object: SceneObjectHandle(object),
            font_resource: SceneResourceId(0),
            atlas_resource: SceneResourceId(1),
            glyph_start: 0,
            glyph_count: 1,
            max_glyph_count: 8,
            pixels_per_em: 16.0,
            spacing: [0.0; 2],
            padding: [1.0; 2],
            horizontal_align: SceneTextHorizontalAlign::Left,
            vertical_align: SceneTextVerticalAlign::Top,
        }
    }

    #[test]
    fn rejects_duplicate_objects() {
        assert_eq!(
            duplicate_object(&[text(3), text(7), text(3)]),
            Some(SceneObjectHandle(3))
        );
        assert_eq!(duplicate_object(&[text(3), text(7)]), None);
    }

    #[test]
    fn glyph_bounds_are_strict_and_ordered() {
        assert!(valid_atlas_uv([0.1, 0.2, 0.3, 0.4]));
        assert!(!valid_atlas_uv([0.3, 0.2, 0.1, 0.4]));
        assert!(!valid_atlas_uv([-0.1, 0.2, 0.3, 0.4]));
        assert!(valid_plane_bounds([-2.0, -5.0, 7.0, 9.0]));
        assert!(!valid_plane_bounds([2.0, -5.0, 2.0, 9.0]));
    }
}
