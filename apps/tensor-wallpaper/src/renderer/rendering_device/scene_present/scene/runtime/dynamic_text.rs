//! Retained CPU layout and per-frame GPU glyph-instance payloads.
//!
//! Font parsing and atlas discovery happen once during runtime setup. Frame work only lays out a
//! changed string and writes compact instance records; glyph coverage remains a cold-path atlas.

use std::collections::BTreeMap;
use std::sync::Arc;

use ab_glyph::{Font, FontArc, ScaleFont};

use crate::engine::scene::semantic_world::ResolvedSemanticFrame;
use crate::engine::scene::{
    SceneDynamicTextGlyphRecord, SceneDynamicTextRecord, SceneObjectHandle, SceneStorage,
    SceneTextHorizontalAlign, SceneTextVerticalAlign,
};

pub(super) const DYNAMIC_TEXT_INSTANCE_STRIDE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SceneDynamicTextDrawState {
    pub object: SceneObjectHandle,
    pub first_instance: u32,
    pub instance_count: u32,
    pub extent: [f32; 2],
}

#[derive(Debug)]
pub(super) struct SceneDynamicTextRuntime {
    layouts: Vec<DynamicTextLayout>,
    instance_capacity: usize,
    payload: Vec<u8>,
    draw_states: Vec<SceneDynamicTextDrawState>,
}

#[derive(Debug)]
struct DynamicTextLayout {
    record: SceneDynamicTextRecord,
    authored_extent: [f32; 2],
    font_metrics: DynamicTextFontMetrics,
    glyphs: Arc<[SceneDynamicTextGlyphRecord]>,
    initial_text: String,
    first_instance: u32,
    last_text: String,
    positioned: Vec<(char, f32, f32)>,
    lines: Vec<(usize, usize, f32)>,
    instances: Vec<GlyphInstance>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GlyphInstance {
    position: [f32; 4],
    atlas_uv: [f32; 4],
}

#[derive(Debug)]
struct DynamicTextFontMetrics {
    characters: Vec<char>,
    advances: Vec<f32>,
    kernings: Vec<f32>,
    line_advance: f32,
}

const UNICODE_WHITESPACE: [char; 25] = [
    '\u{0009}', '\u{000a}', '\u{000b}', '\u{000c}', '\u{000d}', '\u{0020}', '\u{0085}', '\u{00a0}',
    '\u{1680}', '\u{2000}', '\u{2001}', '\u{2002}', '\u{2003}', '\u{2004}', '\u{2005}', '\u{2006}',
    '\u{2007}', '\u{2008}', '\u{2009}', '\u{200a}', '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}',
    '\u{3000}',
];

impl DynamicTextFontMetrics {
    fn from_font(
        record: &SceneDynamicTextRecord,
        font: &impl Font,
        glyphs: &[SceneDynamicTextGlyphRecord],
    ) -> Result<Self, String> {
        let mut characters = glyphs
            .iter()
            .filter_map(|glyph| char::from_u32(glyph.codepoint))
            .chain(UNICODE_WHITESPACE)
            .collect::<Vec<_>>();
        characters.sort_unstable();
        characters.dedup();
        let scaled = font.as_scaled(font_scale(font, record.pixels_per_em)?);
        let ids = characters
            .iter()
            .map(|character| font.glyph_id(*character))
            .collect::<Vec<_>>();
        let advances = ids
            .iter()
            .map(|id| scaled.h_advance(*id))
            .collect::<Vec<_>>();
        let mut kernings = Vec::with_capacity(ids.len().saturating_mul(ids.len()));
        for left in &ids {
            for right in &ids {
                kernings.push(scaled.kern(*left, *right));
            }
        }
        Ok(Self {
            characters,
            advances,
            kernings,
            line_advance: scaled.height() + scaled.line_gap(),
        })
    }

    fn index(&self, character: char) -> Result<usize, String> {
        self.characters.binary_search(&character).map_err(|_| {
            format!(
                "dynamic text has no retained metric for U+{:04X}",
                character as u32
            )
        })
    }

    fn advance(&self, character: char) -> Result<f32, String> {
        Ok(self.advances[self.index(character)?])
    }

    fn kern(&self, left: char, right: char) -> Result<f32, String> {
        let left = self.index(left)?;
        let right = self.index(right)?;
        Ok(self.kernings[left * self.characters.len() + right])
    }
}

impl SceneDynamicTextRuntime {
    pub(super) fn from_storage(storage: &SceneStorage) -> Result<Self, String> {
        let mut layouts = Vec::with_capacity(storage.dynamic_texts().len());
        let mut fonts = BTreeMap::new();
        let mut glyph_tables = BTreeMap::<(u32, u32), Arc<[SceneDynamicTextGlyphRecord]>>::new();
        let mut first_instance = 0u32;
        for record in storage.dynamic_texts() {
            let resource = storage.resource(record.font_resource).ok_or_else(|| {
                format!(
                    "dynamic text object {} references missing font resource {}",
                    record.object.0, record.font_resource.0
                )
            })?;
            let font = if let Some(font) = fonts.get(&record.font_resource) {
                FontArc::clone(font)
            } else {
                let font = FontArc::try_from_vec(
                    storage
                        .resource_payload(resource)
                        .ok_or_else(|| {
                            format!("dynamic text font {} payload is unavailable", resource.id.0)
                        })?
                        .to_vec(),
                )
                .map_err(|_| {
                    format!(
                        "dynamic text font {} is not OpenType/TrueType",
                        resource.id.0
                    )
                })?;
                fonts.insert(record.font_resource, FontArc::clone(&font));
                font
            };
            let initial_text = storage
                .script_programs()
                .iter()
                .find(|program| {
                    program.object == record.object
                        && program.target == crate::engine::scene::SceneScriptTarget::Text
                })
                .and_then(|program| storage.string(program.initial_text))
                .unwrap_or_default()
                .to_owned();
            let glyph_range = (record.glyph_start, record.glyph_count);
            let glyphs = glyph_tables
                .entry(glyph_range)
                .or_insert_with(|| Arc::from(storage.dynamic_text_glyphs(record)))
                .clone();
            let font_metrics = DynamicTextFontMetrics::from_font(record, &font, &glyphs)?;
            let authored_extent = storage
                .meshes()
                .iter()
                .find(|mesh| mesh.object == record.object)
                .map(|mesh| [mesh.width.max(1.0), mesh.height.max(1.0)])
                .unwrap_or([0.0; 2]);
            layouts.push(DynamicTextLayout {
                record: *record,
                authored_extent,
                font_metrics,
                glyphs,
                initial_text,
                first_instance,
                last_text: String::new(),
                positioned: Vec::with_capacity(record.max_glyph_count as usize),
                lines: Vec::with_capacity(record.max_glyph_count as usize + 1),
                instances: Vec::with_capacity(record.max_glyph_count as usize),
            });
            first_instance = first_instance
                .checked_add(record.max_glyph_count)
                .ok_or_else(|| "dynamic text instance capacity overflow".to_owned())?;
        }
        let instance_capacity = first_instance as usize;
        Ok(Self {
            layouts,
            instance_capacity,
            payload: vec![0; instance_capacity.saturating_mul(DYNAMIC_TEXT_INSTANCE_STRIDE)],
            draw_states: storage
                .dynamic_texts()
                .iter()
                .scan(0u32, |first_instance, record| {
                    let state = SceneDynamicTextDrawState {
                        object: record.object,
                        first_instance: *first_instance,
                        instance_count: 0,
                        extent: [0.0; 2],
                    };
                    *first_instance = first_instance.saturating_add(record.max_glyph_count);
                    Some(state)
                })
                .collect(),
        })
    }

    pub(super) fn byte_capacity(&self) -> usize {
        self.payload.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.layouts.is_empty()
    }

    pub(super) fn update<'a>(
        &'a mut self,
        frame: &ResolvedSemanticFrame,
    ) -> Result<(bool, &'a [u8], &'a [SceneDynamicTextDrawState]), String> {
        let mut changed = false;
        for (layout, state) in self.layouts.iter_mut().zip(&mut self.draw_states) {
            let DynamicTextLayout {
                record,
                authored_extent,
                font_metrics,
                glyphs,
                initial_text,
                first_instance,
                last_text,
                positioned,
                lines,
                instances,
            } = layout;
            let text = frame
                .script_text_values
                .iter()
                .find(|value| value.object == record.object)
                .map(|value| value.text.as_str())
                .unwrap_or(initial_text.as_str());
            if text != last_text {
                let extent = layout_text_with_metrics_into(
                    record,
                    font_metrics,
                    glyphs,
                    text,
                    *authored_extent,
                    positioned,
                    lines,
                    instances,
                )?;
                let base = *first_instance as usize * DYNAMIC_TEXT_INSTANCE_STRIDE;
                let capacity = record.max_glyph_count as usize * DYNAMIC_TEXT_INSTANCE_STRIDE;
                self.payload[base..base + capacity].fill(0);
                for (index, instance) in instances.iter().enumerate() {
                    let start = base + index * DYNAMIC_TEXT_INSTANCE_STRIDE;
                    encode_instance(
                        instance,
                        &mut self.payload[start..start + DYNAMIC_TEXT_INSTANCE_STRIDE],
                    );
                }
                state.instance_count = instances.len() as u32;
                state.extent = extent;
                last_text.clear();
                last_text.push_str(text);
                changed = true;
            }
        }
        debug_assert_eq!(
            self.payload.len(),
            self.instance_capacity * DYNAMIC_TEXT_INSTANCE_STRIDE
        );
        Ok((changed, &self.payload, &self.draw_states))
    }
}

fn layout_text_with_font(
    record: &SceneDynamicTextRecord,
    font: &impl Font,
    glyphs: &[SceneDynamicTextGlyphRecord],
    text: &str,
) -> Result<(Vec<GlyphInstance>, [f32; 2]), String> {
    let mut positioned = Vec::with_capacity(record.max_glyph_count as usize);
    let mut lines = Vec::with_capacity(record.max_glyph_count as usize + 1);
    let mut instances = Vec::with_capacity(record.max_glyph_count as usize);
    let extent = layout_text_with_font_into(
        record,
        font,
        glyphs,
        text,
        [0.0; 2],
        &mut positioned,
        &mut lines,
        &mut instances,
    )?;
    Ok((instances, extent))
}

fn layout_text_with_font_into(
    record: &SceneDynamicTextRecord,
    font: &impl Font,
    glyphs: &[SceneDynamicTextGlyphRecord],
    text: &str,
    authored_extent: [f32; 2],
    positioned: &mut Vec<(char, f32, f32)>,
    lines: &mut Vec<(usize, usize, f32)>,
    instances: &mut Vec<GlyphInstance>,
) -> Result<[f32; 2], String> {
    let character_count = text.chars().count();
    if character_count > record.max_glyph_count as usize {
        return Err(format!(
            "dynamic text object {} produced {} scalars, exceeding the typed limit {}",
            record.object.0, character_count, record.max_glyph_count
        ));
    }
    let scale = font_scale(font, record.pixels_per_em)?;
    let scaled = font.as_scaled(scale);
    let line_advance = scaled.height() + scaled.line_gap() + record.spacing[1];
    positioned.clear();
    lines.clear();
    instances.clear();
    for (line_index, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let start = positioned.len();
        let mut cursor_x = 0.0;
        let mut previous = None;
        for character in line.chars() {
            let id = font.glyph_id(character);
            if id.0 == 0 && !character.is_whitespace() {
                return Err(format!(
                    "dynamic text object {} font has no glyph for U+{:04X}",
                    record.object.0, character as u32
                ));
            }
            if let Some(previous) = previous {
                cursor_x += scaled.kern(previous, id);
            }
            positioned.push((character, cursor_x, line_index as f32 * line_advance));
            cursor_x += scaled.h_advance(id) + record.spacing[0];
            previous = Some(id);
        }
        lines.push((start, positioned.len(), cursor_x));
    }
    let maximum_line_width = lines
        .iter()
        .map(|(_, _, width)| *width)
        .fold(0.0_f32, f32::max);
    for &(start, end, width) in lines.iter() {
        let offset = match record.horizontal_align {
            SceneTextHorizontalAlign::Left => 0.0,
            SceneTextHorizontalAlign::Center => (maximum_line_width - width) * 0.5,
            SceneTextHorizontalAlign::Right => maximum_line_width - width,
        };
        for (_, x, _) in &mut positioned[start..end] {
            *x += offset;
        }
    }

    layout_positioned_glyphs(record, glyphs, positioned, authored_extent, instances)
}

fn layout_text_with_metrics_into(
    record: &SceneDynamicTextRecord,
    metrics: &DynamicTextFontMetrics,
    glyphs: &[SceneDynamicTextGlyphRecord],
    text: &str,
    authored_extent: [f32; 2],
    positioned: &mut Vec<(char, f32, f32)>,
    lines: &mut Vec<(usize, usize, f32)>,
    instances: &mut Vec<GlyphInstance>,
) -> Result<[f32; 2], String> {
    let character_count = text.chars().count();
    if character_count > record.max_glyph_count as usize {
        return Err(format!(
            "dynamic text object {} produced {} scalars, exceeding the typed limit {}",
            record.object.0, character_count, record.max_glyph_count
        ));
    }
    positioned.clear();
    lines.clear();
    instances.clear();
    for (line_index, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let start = positioned.len();
        let mut cursor_x = 0.0;
        let mut previous = None;
        for character in line.chars() {
            if let Some(previous) = previous {
                cursor_x += metrics.kern(previous, character)?;
            }
            positioned.push((
                character,
                cursor_x,
                line_index as f32 * (metrics.line_advance + record.spacing[1]),
            ));
            cursor_x += metrics.advance(character)? + record.spacing[0];
            previous = Some(character);
        }
        lines.push((start, positioned.len(), cursor_x));
    }
    align_positioned_lines(record, positioned, lines);
    layout_positioned_glyphs(record, glyphs, positioned, authored_extent, instances)
}

fn align_positioned_lines(
    record: &SceneDynamicTextRecord,
    positioned: &mut [(char, f32, f32)],
    lines: &[(usize, usize, f32)],
) {
    let maximum_line_width = lines
        .iter()
        .map(|(_, _, width)| *width)
        .fold(0.0_f32, f32::max);
    for &(start, end, width) in lines {
        let offset = match record.horizontal_align {
            SceneTextHorizontalAlign::Left => 0.0,
            SceneTextHorizontalAlign::Center => (maximum_line_width - width) * 0.5,
            SceneTextHorizontalAlign::Right => maximum_line_width - width,
        };
        for (_, x, _) in &mut positioned[start..end] {
            *x += offset;
        }
    }
}

fn layout_positioned_glyphs(
    record: &SceneDynamicTextRecord,
    glyphs: &[SceneDynamicTextGlyphRecord],
    positioned: &[(char, f32, f32)],
    authored_extent: [f32; 2],
    instances: &mut Vec<GlyphInstance>,
) -> Result<[f32; 2], String> {
    let mut minimum = [f32::INFINITY; 2];
    let mut maximum = [f32::NEG_INFINITY; 2];
    for &(character, x, y) in positioned.iter() {
        if character.is_whitespace() {
            continue;
        }
        let glyph = glyphs
            .binary_search_by_key(&(character as u32), |glyph| glyph.codepoint)
            .ok()
            .map(|index| glyphs[index])
            .ok_or_else(|| {
                format!(
                    "dynamic text object {} atlas has no glyph for U+{:04X}",
                    record.object.0, character as u32
                )
            })?;
        let bounds = [
            x + glyph.plane_bounds[0],
            y + glyph.plane_bounds[1],
            x + glyph.plane_bounds[2],
            y + glyph.plane_bounds[3],
        ];
        minimum[0] = minimum[0].min(bounds[0]);
        minimum[1] = minimum[1].min(bounds[1]);
        maximum[0] = maximum[0].max(bounds[2]);
        maximum[1] = maximum[1].max(bounds[3]);
        instances.push(GlyphInstance {
            position: bounds,
            atlas_uv: glyph.atlas_uv,
        });
    }
    if instances.is_empty() {
        return Ok([0.0; 2]);
    }
    let extent = [
        maximum[0] - minimum[0] + record.padding[0] * 2.0,
        maximum[1] - minimum[1] + record.padding[1] * 2.0,
    ];
    // WE's local/composite text target is metrics-driven. If the ink run exceeds the
    // serialized canvas, the right-aligned target grows to retain that overflow instead of
    // clipping it back to the authored width. Keep the normal tight-target anchor for runs that
    // fit inside the authored canvas; this also preserves stable script-driven re-layout.
    let right_overflow = if record.horizontal_align == SceneTextHorizontalAlign::Right {
        (extent[0] - authored_extent[0].max(0.0)).max(0.0)
    } else {
        0.0
    };
    let anchor_x = match record.horizontal_align {
        SceneTextHorizontalAlign::Left => 0.0,
        SceneTextHorizontalAlign::Center => -extent[0] * 0.5,
        SceneTextHorizontalAlign::Right => -extent[0] + right_overflow,
    };
    let anchor_y = match record.vertical_align {
        SceneTextVerticalAlign::Top => 0.0,
        SceneTextVerticalAlign::Center => extent[1] * 0.5,
        SceneTextVerticalAlign::Bottom => extent[1],
    };
    for instance in instances {
        let bounds = instance.position;
        instance.position = [
            bounds[0] - minimum[0] + anchor_x + record.padding[0],
            anchor_y - (bounds[3] - minimum[1] + record.padding[1]),
            bounds[2] - minimum[0] + anchor_x + record.padding[0],
            anchor_y - (bounds[1] - minimum[1] + record.padding[1]),
        ];
    }
    Ok(extent)
}

fn font_scale(font: &impl Font, pixels_per_em: f32) -> Result<ab_glyph::PxScale, String> {
    let units_per_em = font
        .units_per_em()
        .ok_or("dynamic text font has no valid units-per-em")?;
    let scale = pixels_per_em * font.height_unscaled() / units_per_em;
    if !scale.is_finite() || scale <= 0.0 {
        return Err("dynamic text font produced an invalid layout scale".to_owned());
    }
    Ok(ab_glyph::PxScale::from(scale))
}

fn encode_instance(instance: &GlyphInstance, output: &mut [u8]) {
    for (index, value) in instance
        .position
        .into_iter()
        .chain(instance.atlas_uv)
        .enumerate()
    {
        output[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_glyph::{GlyphId, Outline};

    struct TestFont;

    impl Font for TestFont {
        fn units_per_em(&self) -> Option<f32> {
            Some(1_000.0)
        }

        fn ascent_unscaled(&self) -> f32 {
            800.0
        }

        fn descent_unscaled(&self) -> f32 {
            -200.0
        }

        fn line_gap_unscaled(&self) -> f32 {
            0.0
        }

        fn glyph_id(&self, character: char) -> GlyphId {
            GlyphId(u16::try_from(character as u32).unwrap_or(0))
        }

        fn h_advance_unscaled(&self, _id: GlyphId) -> f32 {
            500.0
        }

        fn h_side_bearing_unscaled(&self, _id: GlyphId) -> f32 {
            0.0
        }

        fn v_advance_unscaled(&self, _id: GlyphId) -> f32 {
            1_000.0
        }

        fn v_side_bearing_unscaled(&self, _id: GlyphId) -> f32 {
            0.0
        }

        fn kern_unscaled(&self, _first: GlyphId, _second: GlyphId) -> f32 {
            0.0
        }

        fn outline(&self, _id: GlyphId) -> Option<Outline> {
            None
        }

        fn glyph_count(&self) -> usize {
            usize::from(u16::MAX)
        }

        fn codepoint_ids(&self) -> ab_glyph::CodepointIdIter<'_> {
            unimplemented!("layout does not enumerate the test font")
        }

        fn glyph_raster_image2(
            &self,
            _id: GlyphId,
            _pixel_size: u16,
        ) -> Option<ab_glyph::v2::GlyphImage<'_>> {
            None
        }
    }

    fn glyph(character: char, atlas_left: f32) -> SceneDynamicTextGlyphRecord {
        SceneDynamicTextGlyphRecord {
            codepoint: character as u32,
            atlas_uv: [atlas_left, 0.0, atlas_left + 0.1, 0.2],
            plane_bounds: [0.0, -8.0, 5.0, 2.0],
        }
    }

    #[test]
    fn instance_encoding_is_exactly_two_vec4_values() {
        let instance = GlyphInstance {
            position: [-1.0, -2.0, 3.0, 4.0],
            atlas_uv: [0.1, 0.2, 0.3, 0.4],
        };
        let mut bytes = [0; DYNAMIC_TEXT_INSTANCE_STRIDE];
        encode_instance(&instance, &mut bytes);
        assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), -1.0);
        assert_eq!(f32::from_le_bytes(bytes[28..32].try_into().unwrap()), 0.4);
    }

    #[test]
    fn changed_clock_text_rewrites_instances_without_replacing_the_atlas() {
        let record = SceneDynamicTextRecord {
            object: SceneObjectHandle(4),
            font_resource: crate::engine::scene::SceneResourceId(7),
            atlas_resource: crate::engine::scene::SceneResourceId(9),
            glyph_start: 0,
            glyph_count: 3,
            max_glyph_count: 8,
            pixels_per_em: 20.0,
            spacing: [0.0; 2],
            padding: [1.0; 2],
            horizontal_align: SceneTextHorizontalAlign::Right,
            vertical_align: SceneTextVerticalAlign::Center,
        };
        let glyphs = [glyph('1', 0.0), glyph('2', 0.2), glyph('3', 0.4)];
        let (first, first_extent) =
            layout_text_with_font(&record, &TestFont, &glyphs, "12").expect("first clock");
        let (second, _) =
            layout_text_with_font(&record, &TestFont, &glyphs, "23").expect("second clock");
        let mut first_bytes = vec![0; first.len() * DYNAMIC_TEXT_INSTANCE_STRIDE];
        let mut second_bytes = vec![0; second.len() * DYNAMIC_TEXT_INSTANCE_STRIDE];
        for (instance, output) in first.iter().zip(first_bytes.chunks_exact_mut(32)) {
            encode_instance(instance, output);
        }
        for (instance, output) in second.iter().zip(second_bytes.chunks_exact_mut(32)) {
            encode_instance(instance, output);
        }

        assert_eq!(
            record.atlas_resource,
            crate::engine::scene::SceneResourceId(9)
        );
        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 2);
        assert_eq!(first_extent, [17.0, 12.0]);
        assert_ne!(first_bytes, second_bytes);
    }

    #[test]
    fn repeated_clock_layout_reuses_bounded_scratch_capacity() {
        let record = SceneDynamicTextRecord {
            object: SceneObjectHandle(4),
            font_resource: crate::engine::scene::SceneResourceId(7),
            atlas_resource: crate::engine::scene::SceneResourceId(9),
            glyph_start: 0,
            glyph_count: 3,
            max_glyph_count: 8,
            pixels_per_em: 20.0,
            spacing: [0.0; 2],
            padding: [1.0; 2],
            horizontal_align: SceneTextHorizontalAlign::Right,
            vertical_align: SceneTextVerticalAlign::Center,
        };
        let glyphs = [glyph('1', 0.0), glyph('2', 0.2), glyph('3', 0.4)];
        let mut positioned = Vec::with_capacity(record.max_glyph_count as usize);
        let mut lines = Vec::with_capacity(record.max_glyph_count as usize + 1);
        let mut instances = Vec::with_capacity(record.max_glyph_count as usize);
        let capacities = (
            positioned.capacity(),
            lines.capacity(),
            instances.capacity(),
        );
        for text in ["12", "23", "31", "12\n3"].into_iter().cycle().take(4_096) {
            layout_text_with_font_into(
                &record,
                &TestFont,
                &glyphs,
                text,
                [0.0; 2],
                &mut positioned,
                &mut lines,
                &mut instances,
            )
            .expect("bounded retained clock layout");
        }
        assert_eq!(
            (
                positioned.capacity(),
                lines.capacity(),
                instances.capacity(),
            ),
            capacities
        );
    }

    #[test]
    fn retained_metrics_match_the_cold_font_layout() {
        let record = SceneDynamicTextRecord {
            object: SceneObjectHandle(4),
            font_resource: crate::engine::scene::SceneResourceId(7),
            atlas_resource: crate::engine::scene::SceneResourceId(9),
            glyph_start: 0,
            glyph_count: 3,
            max_glyph_count: 8,
            pixels_per_em: 20.0,
            spacing: [1.0, 2.0],
            padding: [1.0; 2],
            horizontal_align: SceneTextHorizontalAlign::Center,
            vertical_align: SceneTextVerticalAlign::Center,
        };
        let glyphs = [glyph('1', 0.0), glyph('2', 0.2), glyph('3', 0.4)];
        let metrics = DynamicTextFontMetrics::from_font(&record, &TestFont, &glyphs)
            .expect("retained font metrics");
        for text in ["12", "1 2", "1\t2", "12\n3"] {
            let (font_instances, font_extent) =
                layout_text_with_font(&record, &TestFont, &glyphs, text).expect("font layout");
            let mut positioned = Vec::with_capacity(record.max_glyph_count as usize);
            let mut lines = Vec::with_capacity(record.max_glyph_count as usize + 1);
            let mut metric_instances = Vec::with_capacity(record.max_glyph_count as usize);
            let metric_extent = layout_text_with_metrics_into(
                &record,
                &metrics,
                &glyphs,
                text,
                [0.0; 2],
                &mut positioned,
                &mut lines,
                &mut metric_instances,
            )
            .expect("metric layout");
            assert_eq!(metric_instances, font_instances, "text {text:?}");
            assert_eq!(metric_extent, font_extent, "text {text:?}");
        }
    }

    #[test]
    fn retained_kerning_matrix_uses_ordered_character_pairs() {
        let metrics = DynamicTextFontMetrics {
            characters: vec!['a', 'b'],
            advances: vec![4.0, 5.0],
            kernings: vec![0.0, -1.5, 2.0, 0.0],
            line_advance: 10.0,
        };
        assert_eq!(metrics.advance('a').unwrap(), 4.0);
        assert_eq!(metrics.kern('a', 'b').unwrap(), -1.5);
        assert_eq!(metrics.kern('b', 'a').unwrap(), 2.0);
        assert!(metrics.advance('c').is_err());
    }

    #[test]
    fn right_aligned_metrics_overflow_is_not_clipped_to_the_authored_canvas() {
        let record = SceneDynamicTextRecord {
            object: SceneObjectHandle(4),
            font_resource: crate::engine::scene::SceneResourceId(7),
            atlas_resource: crate::engine::scene::SceneResourceId(9),
            glyph_start: 0,
            glyph_count: 3,
            max_glyph_count: 8,
            pixels_per_em: 20.0,
            spacing: [0.0; 2],
            padding: [1.0; 2],
            horizontal_align: SceneTextHorizontalAlign::Right,
            vertical_align: SceneTextVerticalAlign::Center,
        };
        let glyphs = [glyph('1', 0.0), glyph('2', 0.2), glyph('3', 0.4)];
        let mut positioned = Vec::with_capacity(record.max_glyph_count as usize);
        let mut lines = Vec::with_capacity(record.max_glyph_count as usize + 1);
        let mut instances = Vec::with_capacity(record.max_glyph_count as usize);

        let extent = layout_text_with_font_into(
            &record,
            &TestFont,
            &glyphs,
            "12",
            [12.0, 20.0],
            &mut positioned,
            &mut lines,
            &mut instances,
        )
        .expect("right-aligned overflow layout");

        assert_eq!(extent, [17.0, 12.0]);
        assert_eq!(instances.last().unwrap().position[2], 4.0);
    }
}
