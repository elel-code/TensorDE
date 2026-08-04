use tensor_files_core::CompactLayoutOptions;
#[cfg(test)]
use tensor_files_core::Entry;

use crate::ui::metrics::TEXT_FONT_SIZE;
#[cfg(test)]
use crate::ui::metrics::normalized_scale_factor;

pub(crate) fn estimated_text_cursor_for_offset(
    label: &str,
    offset_x: f32,
    font_size: f32,
) -> usize {
    if label.is_empty() || offset_x <= 0.0 {
        return 0;
    }

    let scale = font_size / TEXT_FONT_SIZE.max(1.0);
    let mut x = 0.0;
    for (index, ch) in label.char_indices() {
        let next = index + ch.len_utf8();
        let width = estimated_name_char_width(ch) * scale;
        if offset_x <= x + width / 2.0 {
            return index;
        }
        x += width;
        if offset_x <= x {
            return next;
        }
    }

    label.len()
}

#[cfg(test)]
pub(crate) fn estimated_text_cursor_x(label: &str, cursor: usize, font_size: f32) -> f32 {
    let cursor = normalized_text_cursor(label, cursor);
    let scale = font_size / TEXT_FONT_SIZE.max(1.0);
    label[..cursor]
        .chars()
        .map(estimated_name_char_width)
        .sum::<f32>()
        * scale
}

#[cfg(test)]
fn estimated_text_width_without_padding(label: &str, font_size: f32) -> f32 {
    let scale = font_size / TEXT_FONT_SIZE.max(1.0);
    label.chars().map(estimated_name_char_width).sum::<f32>() * scale
}

pub(crate) fn required_compact_item_width(options: CompactLayoutOptions, text_width: f32) -> f32 {
    // CompactLayout adds horizontal padding to both the item and the text raster.
    (options.padding * 4.0 + options.icon_size + options.text_gap + text_width).ceil()
}

#[cfg(test)]
pub(crate) fn compact_entry_text_width(entry: &Entry, scale_factor: f32) -> f32 {
    estimated_text_width_without_padding(
        entry.name.as_ref(),
        TEXT_FONT_SIZE * normalized_scale_factor(scale_factor),
    )
}

fn estimated_name_char_width(ch: char) -> f32 {
    match ch {
        '\u{200B}' => 0.0,
        '\u{2026}' => 8.0,
        'i' | 'l' | 'I' | '!' | '.' | ',' | ':' | ';' | '\'' | '`' | '|' => 4.0,
        ' ' | '-' | '_' => 5.0,
        'm' | 'w' | 'M' | 'W' | '@' | '%' | '#' => 11.0,
        'A'..='Z' => 9.0,
        '0'..='9' => 8.0,
        ch if ch.is_ascii() => 7.5,
        _ => 14.0,
    }
}

#[cfg(test)]
fn normalized_text_cursor(value: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(value.len());
    while !value.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimated_text_cursor_uses_glyph_widths_and_empty_tail() {
        let font_size = TEXT_FONT_SIZE;
        let label = "imβ";
        let i_tail = estimated_text_cursor_x(label, "i".len(), font_size);
        let m_tail = estimated_text_cursor_x(label, "im".len(), font_size);
        let end = estimated_text_cursor_x(label, label.len(), font_size);

        assert_eq!(
            estimated_text_cursor_for_offset(label, i_tail / 3.0, font_size),
            0
        );
        assert_eq!(
            estimated_text_cursor_for_offset(label, i_tail + (m_tail - i_tail) * 0.75, font_size),
            "im".len()
        );
        assert_eq!(
            estimated_text_cursor_for_offset(label, end + 24.0, font_size),
            label.len()
        );
    }
}
