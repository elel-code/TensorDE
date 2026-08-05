use wayland_server::Display;

use super::*;
use crate::protocol::state::RuntimeState;

#[test]
fn authority_starts_empty_and_source_tokens_never_use_zero() {
    let display = Display::<RuntimeState>::new().unwrap();
    let mut selection = SelectionProtocol::new(&display.handle());

    assert_eq!(selection.counts(), (0, 0, 0, 0, 0));
    assert_ne!(selection.allocate_source(), SourceToken(0));
    assert_ne!(selection.allocate_source(), SourceToken(0));
}

#[test]
fn focus_without_devices_is_a_noop() {
    let display = Display::<RuntimeState>::new().unwrap();
    let mut selection = SelectionProtocol::new(&display.handle());

    selection.set_focus(None);
    assert_eq!(selection.counts(), (0, 0, 0, 0, 0));
}
