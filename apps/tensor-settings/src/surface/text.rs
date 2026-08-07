use wayland_client_runtime::MAX_SURROUNDING_TEXT_BYTES;

pub(super) fn surrounding_window(text: &str, cursor: usize) -> (usize, usize) {
    let cursor = cursor.min(text.len());
    if text.len() <= MAX_SURROUNDING_TEXT_BYTES {
        return (0, text.len());
    }
    let half = MAX_SURROUNDING_TEXT_BYTES / 2;
    let mut start = cursor.saturating_sub(half);
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = cursor.saturating_add(half).min(text.len());
    while end < text.len() && !text.is_char_boundary(end) {
        end -= 1;
    }
    if end.saturating_sub(start) > MAX_SURROUNDING_TEXT_BYTES {
        end = (start + MAX_SURROUNDING_TEXT_BYTES).min(text.len());
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
    }
    (start, end.max(cursor))
}
