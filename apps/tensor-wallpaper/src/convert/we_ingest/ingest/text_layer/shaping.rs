use ab_glyph::{Font, GlyphId, PxScale, ScaleFont, point};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ShapedGlyph {
    pub(super) id: GlyphId,
    pub(super) x_advance: f32,
    pub(super) x_offset: f32,
    pub(super) y_offset: f32,
}

pub(super) fn shape_line(
    font_bytes: &[u8],
    line: &str,
    pixels_per_em: f32,
) -> Result<Vec<ShapedGlyph>, String> {
    let font = harfrust::FontRef::from_index(font_bytes, 0)
        .map_err(|error| format!("text shaping font is invalid: {error}"))?;
    let data = harfrust::ShaperData::new(&font);
    let shaper = data.shaper(&font).build();
    let units_per_em = shaper.units_per_em();
    if units_per_em <= 0 {
        return Err("text shaping font has no positive units-per-em".to_owned());
    }
    let factor = pixels_per_em / units_per_em as f32;
    let mut buffer = harfrust::UnicodeBuffer::new();
    buffer.push_str(line);
    buffer.guess_segment_properties();
    let shaped = shaper.shape(buffer, &[]);
    Ok(shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .map(|(info, position)| ShapedGlyph {
            id: GlyphId(info.glyph_id as u16),
            x_advance: pixel_metric(position.x_advance, factor),
            x_offset: pixel_metric(position.x_offset, factor),
            y_offset: pixel_metric(position.y_offset, factor),
        })
        .collect())
}

fn pixel_metric(font_units: i32, factor: f32) -> f32 {
    (font_units as f32 * factor).floor()
}

#[cfg(test)]
pub(super) fn layout_glyphs(
    font: &impl Font,
    text: &str,
    scale: PxScale,
    spacing: [f32; 2],
    alignment: [Option<&str>; 2],
    canvas_extent: [f32; 2],
    padding: (f32, f32),
) -> Vec<ab_glyph::Glyph> {
    let scaled = font.as_scaled(scale);
    let line_advance = scaled.height() + scaled.line_gap() + spacing[1];
    let mut glyphs = Vec::with_capacity(text.chars().count());
    let mut lines = Vec::new();
    for (line_index, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let glyph_start = glyphs.len();
        let mut cursor_x = 0.0;
        let mut previous = None;
        for character in line.chars() {
            let id = font.glyph_id(character);
            if let Some(previous) = previous {
                cursor_x += scaled.kern(previous, id);
            }
            glyphs.push(
                id.with_scale_and_position(
                    scale,
                    point(cursor_x, line_index as f32 * line_advance),
                ),
            );
            cursor_x += scaled.h_advance(id) + spacing[0];
            previous = Some(id);
        }
        lines.push((glyph_start, glyphs.len(), cursor_x));
    }
    align_lines(
        font,
        scale,
        spacing,
        alignment,
        canvas_extent,
        padding,
        glyphs,
        lines,
    )
}

pub(super) fn layout_glyphs_with_shaping(
    font: &impl Font,
    text: &str,
    font_bytes: &[u8],
    pixels_per_em: f32,
    scale: PxScale,
    spacing: [f32; 2],
    alignment: [Option<&str>; 2],
    canvas_extent: [f32; 2],
    padding: (f32, f32),
) -> Result<Vec<ab_glyph::Glyph>, String> {
    let scaled = font.as_scaled(scale);
    let mut glyphs = Vec::with_capacity(text.chars().count());
    let mut lines = Vec::new();
    let line_advance = scaled.height() + scaled.line_gap() + spacing[1];
    for (line_index, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let glyph_start = glyphs.len();
        let mut cursor_x = 0.0;
        for shaped in shape_line(font_bytes, line, pixels_per_em)? {
            glyphs.push(shaped.id.with_scale_and_position(
                scale,
                point(
                    cursor_x + shaped.x_offset,
                    line_index as f32 * line_advance - shaped.y_offset,
                ),
            ));
            cursor_x += shaped.x_advance + spacing[0];
        }
        lines.push((glyph_start, glyphs.len(), cursor_x));
    }
    Ok(align_lines(
        font,
        scale,
        spacing,
        alignment,
        canvas_extent,
        padding,
        glyphs,
        lines,
    ))
}

fn align_lines(
    font: &impl Font,
    scale: PxScale,
    spacing: [f32; 2],
    alignment: [Option<&str>; 2],
    canvas_extent: [f32; 2],
    padding: (f32, f32),
    mut glyphs: Vec<ab_glyph::Glyph>,
    lines: Vec<(usize, usize, f32)>,
) -> Vec<ab_glyph::Glyph> {
    let scaled = font.as_scaled(scale);
    let line_advance = scaled.height() + scaled.line_gap() + spacing[1];
    let maximum_line_width = lines
        .iter()
        .map(|(_, _, width)| *width)
        .fold(0.0_f32, f32::max);
    let padding_x = padding.0.min(canvas_extent[0] * 0.5);
    let padding_y = padding.1.min(canvas_extent[1] * 0.5);
    let block_x = match alignment[0] {
        Some("left") => padding_x,
        Some("right") => (canvas_extent[0] - padding_x - maximum_line_width).max(0.0),
        _ => (canvas_extent[0] - maximum_line_width) * 0.5,
    };
    let block_height = scaled.height() + lines.len().saturating_sub(1) as f32 * line_advance;
    let block_y = match alignment[1] {
        Some("top") => padding_y,
        Some("bottom") => canvas_extent[1] - padding_y - block_height,
        _ => (canvas_extent[1] - block_height) * 0.5,
    };
    let baseline_y = block_y + scaled.ascent();
    for (start, end, width) in lines {
        let line_offset_x = match alignment[0] {
            Some("left") => 0.0,
            Some("right") => maximum_line_width - width,
            _ => (maximum_line_width - width) * 0.5,
        };
        for glyph in &mut glyphs[start..end] {
            glyph.position.x += block_x + line_offset_x;
            glyph.position.y += baseline_y;
        }
    }
    glyphs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn we_layout_truncates_each_shaped_metric_to_whole_pixels() {
        assert_eq!(pixel_metric(848, 75.0 / 1_000.0), 63.0);
        assert_eq!(pixel_metric(629, 75.0 / 1_000.0), 47.0);
    }
}
