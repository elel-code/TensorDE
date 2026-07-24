use crate::{layout::LayoutEngine, scene::SceneAppearance};

use super::WaylandRuntime;

#[test]
fn socket_name_survives_event_source_registration() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let socket_name = runtime.socket_name().to_os_string();

    runtime.prepare(false).unwrap();

    assert_eq!(runtime.socket_name(), socket_name);
}
