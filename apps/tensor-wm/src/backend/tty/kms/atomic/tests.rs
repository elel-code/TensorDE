use std::num::NonZeroU32;

use super::*;

fn handle<T: From<NonZeroU32>>(raw: u32) -> T {
    NonZeroU32::new(raw).unwrap().into()
}

#[test]
fn async_mode_is_the_only_path_that_sets_page_flip_async() {
    let vsync = page_flip_flags(PresentMode::Vsync);
    let asynchronous = page_flip_flags(PresentMode::Async);
    assert!(!vsync.contains(AtomicCommitFlags::PAGE_FLIP_ASYNC));
    assert!(asynchronous.contains(AtomicCommitFlags::PAGE_FLIP_ASYNC));
    assert!(asynchronous.contains(AtomicCommitFlags::PAGE_FLIP_EVENT));
    assert!(asynchronous.contains(AtomicCommitFlags::NONBLOCK));
    assert!(!asynchronous.contains(AtomicCommitFlags::ALLOW_MODESET));
}

fn plane_properties(base: u32) -> PlaneProperties {
    let mut next = base;
    PlaneProperties::resolve(|_, _| {
        let property = handle(next);
        next += 1;
        Ok(property)
    })
    .unwrap()
}

#[test]
fn clear_request_disables_selected_primary_and_cursor_planes_together() {
    let properties = AtomicProperties {
        connector_crtc: handle(10),
        crtc: CrtcProperties {
            active: handle(11),
            mode: handle(12),
        },
        plane: plane_properties(20),
        cursor: Some(plane_properties(40)),
    };
    let request = AtomicClearRequest::new(
        handle(1),
        handle(2),
        handle(3),
        Some(handle(4)),
        &properties,
    );

    assert_eq!(request.objects, [1, 2, 3, 4]);
    assert_eq!(request.property_counts, [1, 2, 2, 2]);
    assert_eq!(
        request.properties,
        [
            10,
            11,
            12,
            u32::from(properties.plane.crtc),
            u32::from(properties.plane.framebuffer),
            u32::from(properties.cursor.unwrap().crtc),
            u32::from(properties.cursor.unwrap().framebuffer),
        ]
    );
    assert_eq!(request.values, [0; 7]);
}
