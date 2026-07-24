use smithay::backend::allocator::{Format as DrmFormat, Fourcc, Modifier};
use tensor_util::{OutputScale, Rect, Size};

use crate::{
    ecs::{SurfaceBufferId, SurfaceId, ViewId, WorkspaceId},
    layout::LayoutPlacement,
    scene::{
        ContentRevision, ContentSpan, EffectStyle, SceneNode, SceneSnapshot, SurfaceContent,
        SurfaceLayer, SurfaceTransform,
    },
};

use super::*;

const OUTPUT: RenderOutputId = RenderOutputId {
    device_id: 3,
    connector_id: 4,
};
const VIEWPORT: Rect = Rect::new(0, 0, 320, 200);

fn target() -> NativeOutputTarget {
    NativeOutputTarget {
        output: OUTPUT,
        viewport: VIEWPORT,
        format: OutputFormat {
            format: DrmFormat {
                code: Fourcc::Xrgb8888,
                modifier: Modifier::from(9),
            },
            plane_count: 1,
        },
        scale: OutputScale::ONE,
    }
}

fn scene() -> SceneSnapshot {
    let content = SurfaceContent {
        surface_id: SurfaceId::new(1),
        buffer_id: SurfaceBufferId::new(1),
        revision: ContentRevision::new(1),
        layer: SurfaceLayer::View,
        buffer_size: Size::new(100, 100),
        local_geometry: Rect::new(0, 0, 100, 100),
        buffer_scale: 1,
        transform: SurfaceTransform::Normal,
    };
    SceneSnapshot::with_content(
        WorkspaceId::new(0),
        VIEWPORT,
        vec![
            SceneNode::new(
                ViewId::new(1),
                1,
                LayoutPlacement {
                    geometry: Rect::new(0, 0, 100, 100),
                    visible: Some(Rect::new(0, 0, 100, 100)),
                },
                EffectStyle::default(),
            )
            .with_content(ContentSpan::new(0, 1).unwrap()),
        ],
        vec![content],
    )
}

fn cursor(x: i32, y: i32) -> CursorOverlay {
    CursorOverlay::new(Rect::new(x, y, 24, 24), VIEWPORT).unwrap()
}

#[test]
fn cursor_motion_is_drawn_last_and_damages_old_and_new_physical_bounds() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target()).unwrap();
    let first = scheduler
        .prepare_with_cursor(OUTPUT, scene(), Some(cursor(10, 20)), 0)
        .unwrap();
    scheduler.commit(&first).unwrap();
    scheduler.retire_completed(first.timeline_value);

    let second = scheduler
        .prepare_with_cursor(
            OUTPUT,
            scene(),
            Some(cursor(200, 120)),
            first.timeline_value,
        )
        .unwrap();

    assert_eq!(
        second.draw_plan.cursor().unwrap().destination,
        Rect::new(200, 120, 24, 24)
    );
    assert_eq!(
        second.damage.regions(),
        [Rect::new(10, 20, 24, 24), Rect::new(200, 120, 24, 24)]
    );
}

#[test]
fn hiding_cursor_damages_its_previous_physical_bounds() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target()).unwrap();
    let first = scheduler
        .prepare_with_cursor(OUTPUT, scene(), Some(cursor(10, 20)), 0)
        .unwrap();
    scheduler.commit(&first).unwrap();
    scheduler.retire_completed(first.timeline_value);

    let second = scheduler
        .prepare_with_cursor(OUTPUT, scene(), None, first.timeline_value)
        .unwrap();

    assert_eq!(second.draw_plan.cursor(), None);
    assert_eq!(second.damage.regions(), [Rect::new(10, 20, 24, 24)]);
}
