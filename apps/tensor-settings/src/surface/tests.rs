use super::*;

#[test]
fn sidebar_hit_testing_is_bounded() {
    let extent = LogicalSize::new(980, 680);
    assert_eq!(product_at(extent, 8, (30.0, 90.0)), Some(0));
    assert_eq!(product_at(extent, 8, (400.0, 90.0)), None);
}

#[test]
fn every_document_state_has_visible_chrome() {
    for state in [
        ConfigDocumentState::Clean,
        ConfigDocumentState::Dirty,
        ConfigDocumentState::Invalid,
        ConfigDocumentState::ReadOnly,
        ConfigDocumentState::Unsupported,
    ] {
        assert!(state_color(state)[..3].iter().any(|channel| *channel > 0.0));
    }
}

#[test]
fn large_editor_documents_use_a_bounded_utf8_surrounding_window() {
    let text = "界".repeat(wayland_client_runtime::MAX_SURROUNDING_TEXT_BYTES);
    let cursor = text.len() / 2;
    let (start, end) = surrounding_window(&text, cursor);
    assert!(end - start <= wayland_client_runtime::MAX_SURROUNDING_TEXT_BYTES);
    assert!(text.is_char_boundary(start));
    assert!(text.is_char_boundary(end));
    assert!(start <= cursor && cursor <= end);
}
