use std::borrow::Cow;

use cosmic_text::{Align, Attrs, Buffer, Ellipsize, Family, FontSystem, Metrics, Shaping, Wrap};

const FILE_MANAGER_ELLIPSIS: &str = "…";
const FILE_MANAGER_RETURN_SYMBOL: char = '\u{21B5}';
const FILE_MANAGER_WRAP_HINT: char = '\u{200B}';

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FileManagerIconsFilenameLayout<'a> {
    pub(crate) display: Cow<'a, str>,
    pub(crate) line_count: usize,
    pub(crate) elided: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FileManagerTextLine {
    start: usize,
}

pub(crate) fn file_manager_layout_icons_filename<'a>(
    font_system: &mut FontSystem,
    buffer: &mut Buffer,
    label: &'a str,
    max_width: f32,
    max_lines: usize,
    font_size: f32,
    line_height: f32,
) -> FileManagerIconsFilenameLayout<'a> {
    let max_width = max_width.max(1.0);
    let max_lines = max_lines.max(1);
    let display_text = file_manager_escape_and_preprocess_wrap(label);
    let lines = file_manager_wrapped_text_lines(
        font_system,
        buffer,
        display_text.as_ref(),
        max_width,
        font_size,
        line_height,
    );
    let line_count = lines.len().max(1).min(max_lines);
    if lines.len() <= max_lines {
        return FileManagerIconsFilenameLayout {
            display: display_text,
            line_count,
            elided: false,
        };
    }

    let last_line_start = lines
        .get(max_lines.saturating_sub(1))
        .map(|line| line.start)
        .unwrap_or(0)
        .min(display_text.len());
    let mut display = String::from(&display_text[..last_line_start]);
    let mut eliding_width = max_width;
    let last_line = loop {
        let candidate = file_manager_elide_filename_text_to_width_shaped(
            font_system,
            buffer,
            &display_text[last_line_start..],
            eliding_width,
            font_size,
            line_height,
        )
        .into_owned();
        if file_manager_text_width_no_wrap(font_system, buffer, &candidate, font_size, line_height)
            <= max_width
            || eliding_width <= 1.0
        {
            break candidate;
        }
        eliding_width -= 1.0;
    };
    display.push_str(&last_line);

    FileManagerIconsFilenameLayout {
        display: Cow::Owned(display),
        line_count,
        elided: true,
    }
}

pub(crate) fn file_manager_icons_filename_line_count(
    font_system: &mut FontSystem,
    buffer: &mut Buffer,
    label: &str,
    max_width: f32,
    max_lines: usize,
    font_size: f32,
    line_height: f32,
) -> usize {
    file_manager_layout_icons_filename(
        font_system,
        buffer,
        label,
        max_width,
        max_lines,
        font_size,
        line_height,
    )
    .line_count
}

pub(crate) fn file_manager_elide_filename_to_width_shaped<'a>(
    font_system: &mut FontSystem,
    buffer: &mut Buffer,
    label: &'a str,
    max_width: f32,
    font_size: f32,
    line_height: f32,
) -> Cow<'a, str> {
    let escaped = file_manager_escape_text(label);
    match file_manager_elide_filename_text_to_width_shaped(
        font_system,
        buffer,
        escaped.as_ref(),
        max_width,
        font_size,
        line_height,
    ) {
        Cow::Borrowed(_) => escaped,
        Cow::Owned(display) => Cow::Owned(display),
    }
}

fn file_manager_wrapped_text_lines(
    font_system: &mut FontSystem,
    buffer: &mut Buffer,
    label: &str,
    max_width: f32,
    font_size: f32,
    line_height: f32,
) -> Vec<FileManagerTextLine> {
    if label.is_empty() {
        return Vec::new();
    }

    configure_text_buffer(
        buffer,
        label,
        TextBufferConfig {
            width: Some(max_width.max(1.0)),
            height: None,
            wrap: Wrap::WordOrGlyph,
            align: Align::Center,
            font_size,
            line_height,
        },
    );
    buffer.shape_until_scroll(font_system, false);

    let mut lines = Vec::new();
    for run in buffer.layout_runs() {
        let Some(start) = run.glyphs.iter().map(|glyph| glyph.start).min() else {
            continue;
        };
        lines.push(FileManagerTextLine { start });
    }
    if lines.is_empty() {
        lines.push(FileManagerTextLine { start: 0 });
    }
    lines
}

fn file_manager_elide_filename_text_to_width_shaped<'a>(
    font_system: &mut FontSystem,
    buffer: &mut Buffer,
    label: &'a str,
    max_width: f32,
    font_size: f32,
    line_height: f32,
) -> Cow<'a, str> {
    let max_width = max_width.max(1.0);
    if file_manager_text_width_no_wrap(font_system, buffer, label, font_size, line_height)
        <= max_width
    {
        return Cow::Borrowed(label);
    }

    let ellipsis_width = file_manager_text_width_no_wrap(
        font_system,
        buffer,
        FILE_MANAGER_ELLIPSIS,
        font_size,
        line_height,
    );
    if ellipsis_width >= max_width {
        return Cow::Owned(FILE_MANAGER_ELLIPSIS.to_string());
    }

    let (base, extension) = match filename_base_and_extension(label) {
        Some((base, extension)) => {
            let ellipsis_extension = format!("{FILE_MANAGER_ELLIPSIS}{extension}");
            if file_manager_text_width_no_wrap(
                font_system,
                buffer,
                &ellipsis_extension,
                font_size,
                line_height,
            ) > max_width
            {
                (label, "")
            } else {
                (base, extension)
            }
        }
        None => (label, ""),
    };

    let extension_width =
        file_manager_text_width_no_wrap(font_system, buffer, extension, font_size, line_height);
    let prefix_budget = (max_width - extension_width - ellipsis_width).max(0.0);
    let mut result = file_manager_prefix_to_width_shaped(
        font_system,
        buffer,
        base,
        prefix_budget,
        font_size,
        line_height,
    );
    result.push_str(FILE_MANAGER_ELLIPSIS);
    result.push_str(extension);
    Cow::Owned(result)
}

fn file_manager_prefix_to_width_shaped(
    font_system: &mut FontSystem,
    buffer: &mut Buffer,
    label: &str,
    max_width: f32,
    font_size: f32,
    line_height: f32,
) -> String {
    if label.is_empty() || max_width <= 0.0 {
        return String::new();
    }

    let mut boundaries = label
        .char_indices()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    boundaries.push(label.len());
    let mut low = 0;
    let mut high = boundaries.len() - 1;
    while low < high {
        let mid = (low + high).div_ceil(2);
        let prefix = &label[..boundaries[mid]];
        if file_manager_text_width_no_wrap(font_system, buffer, prefix, font_size, line_height)
            <= max_width
        {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    label[..boundaries[low]].to_string()
}

pub(crate) fn file_manager_text_width_no_wrap(
    font_system: &mut FontSystem,
    buffer: &mut Buffer,
    label: &str,
    font_size: f32,
    line_height: f32,
) -> f32 {
    if label.is_empty() {
        return 0.0;
    }

    configure_text_buffer(
        buffer,
        label,
        TextBufferConfig {
            width: None,
            height: None,
            wrap: Wrap::None,
            align: Align::Left,
            font_size,
            line_height,
        },
    );
    buffer.shape_until_scroll(font_system, false);
    buffer
        .layout_runs()
        .next()
        .map(|run| run.line_w)
        .unwrap_or(0.0)
}

struct TextBufferConfig {
    width: Option<f32>,
    height: Option<f32>,
    wrap: Wrap,
    align: Align,
    font_size: f32,
    line_height: f32,
}

fn configure_text_buffer(buffer: &mut Buffer, label: &str, config: TextBufferConfig) {
    let attrs = Attrs::new().family(Family::SansSerif);
    buffer.set_metrics(Metrics::new(config.font_size, config.line_height));
    buffer.set_wrap(config.wrap);
    buffer.set_ellipsize(Ellipsize::None);
    buffer.set_size(config.width, config.height);
    buffer.set_text(label, &attrs, Shaping::Advanced, Some(config.align));
}

fn file_manager_escape_text(label: &str) -> Cow<'_, str> {
    if !label.contains('\n') {
        return Cow::Borrowed(label);
    }

    Cow::Owned(label.replace('\n', &FILE_MANAGER_RETURN_SYMBOL.to_string()))
}

fn file_manager_escape_and_preprocess_wrap(label: &str) -> Cow<'_, str> {
    let mut output: Option<String> = None;
    for (index, ch) in label.char_indices() {
        let next = label[index + ch.len_utf8()..].chars().next();
        let mapped = if ch == '\n' {
            FILE_MANAGER_RETURN_SYMBOL
        } else {
            ch
        };
        let insert_wrap_hint = should_insert_file_manager_wrap_hint(mapped, next);
        if output.is_none() && (mapped != ch || insert_wrap_hint) {
            let mut string = String::with_capacity(label.len() + 8);
            string.push_str(&label[..index]);
            output = Some(string);
        }
        if let Some(string) = &mut output {
            string.push(mapped);
            if insert_wrap_hint {
                string.push(FILE_MANAGER_WRAP_HINT);
            }
        }
    }

    output.map(Cow::Owned).unwrap_or(Cow::Borrowed(label))
}

fn should_insert_file_manager_wrap_hint(ch: char, next: Option<char>) -> bool {
    let Some(next) = next else {
        return false;
    };
    if next.is_whitespace() || next == FILE_MANAGER_WRAP_HINT || ch == FILE_MANAGER_WRAP_HINT {
        return false;
    }
    ch.is_ascii_punctuation()
}

fn filename_base_and_extension(label: &str) -> Option<(&str, &str)> {
    let dot = label.rfind('.')?;
    if dot == 0 || dot + 1 >= label.len() {
        return None;
    }
    let (base, extension) = label.split_at(dot);
    (!base.is_empty() && extension.len() > 1).then_some((base, extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_manager_wrap_preprocessing_inserts_zero_width_breaks_after_punctuation() {
        let display = file_manager_escape_and_preprocess_wrap("alpha-beta.gamma_txt");

        assert_eq!(
            display.as_ref(),
            "alpha-\u{200B}beta.\u{200B}gamma_\u{200B}txt"
        );
    }

    #[test]
    fn shaped_icons_filename_elides_only_when_wrapping_exceeds_max_lines() {
        let mut font_system = FontSystem::new();
        let mut buffer = Buffer::new_empty(Metrics::new(13.0, 18.0));
        let layout = file_manager_layout_icons_filename(
            &mut font_system,
            &mut buffer,
            "very-long-folder-preview-name-that-needs-more-than-three-lines.png",
            60.0,
            3,
            13.0,
            18.0,
        );

        assert!(layout.elided);
        assert_eq!(layout.line_count, 3);
        assert!(layout.display.contains('…'));
        assert!(!layout.display.contains("..."));
        assert!(
            layout
                .display
                .replace(FILE_MANAGER_WRAP_HINT, "")
                .ends_with(".png")
        );
    }
}
