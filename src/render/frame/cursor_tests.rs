use tensor_host::{DrmFormat, Fourcc, Modifier};
use tensor_util::{OutputScale, Rect};

use crate::{
    ecs::{SurfaceBufferId, SurfaceId, ViewId, WorkspaceId},
    layout::LayoutPlacement,
    scene::{
        ContentRevision, ContentSpan, EffectStyle, SceneNode, SceneSnapshot, SurfaceContent,
        SurfaceLayer, SurfaceSampleTransform,
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
                code: Fourcc::XRGB8888,
                modifier: Modifier::from_raw(9),
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
        alpha: Default::default(),
        local_geometry: Rect::new(0, 0, 100, 100),
        sample_transform: SurfaceSampleTransform::IDENTITY,
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

fn cursor(source: u64, x: i32, y: i32) -> CursorOverlay {
    CursorOverlay::new(source, Rect::new(x, y, 24, 24), VIEWPORT).unwrap()
}

fn cursors(entries: &[(i32, i32)]) -> CursorOverlays {
    let mut cursors = CursorOverlays::default();
    for (source, &(x, y)) in entries.iter().enumerate() {
        assert!(cursors.push(cursor(source as u64, x, y)));
    }
    cursors
}

#[test]
fn cursor_motion_is_drawn_last_and_damages_old_and_new_physical_bounds() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target()).unwrap();
    let first = scheduler
        .prepare_with_cursors(OUTPUT, scene(), cursors(&[(10, 20)]), 0)
        .unwrap();
    scheduler.commit(&first).unwrap();
    scheduler.retire_completed(first.timeline_value);

    let second = scheduler
        .prepare_with_cursors(
            OUTPUT,
            scene(),
            cursors(&[(200, 120)]),
            first.timeline_value,
        )
        .unwrap();

    assert_eq!(
        second.draw_plan.cursors()[0].destination,
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
        .prepare_with_cursors(OUTPUT, scene(), cursors(&[(10, 20)]), 0)
        .unwrap();
    scheduler.commit(&first).unwrap();
    scheduler.retire_completed(first.timeline_value);

    let second = scheduler
        .prepare_with_cursors(
            OUTPUT,
            scene(),
            CursorOverlays::default(),
            first.timeline_value,
        )
        .unwrap();

    assert!(second.draw_plan.cursors().is_empty());
    assert_eq!(second.damage.regions(), [Rect::new(10, 20, 24, 24)]);
}

#[test]
fn independent_cursors_draw_in_order_and_damage_every_changed_slot() {
    let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
    scheduler.register_output(target()).unwrap();
    let first = scheduler
        .prepare_with_cursors(OUTPUT, scene(), cursors(&[(10, 20), (40, 50)]), 0)
        .unwrap();
    scheduler.commit(&first).unwrap();
    scheduler.retire_completed(first.timeline_value);

    let second = scheduler
        .prepare_with_cursors(
            OUTPUT,
            scene(),
            cursors(&[(10, 20), (80, 90)]),
            first.timeline_value,
        )
        .unwrap();

    assert_eq!(
        second
            .draw_plan
            .cursors()
            .iter()
            .map(|cursor| cursor.destination)
            .collect::<Vec<_>>(),
        [Rect::new(10, 20, 24, 24), Rect::new(80, 90, 24, 24)]
    );
    assert_eq!(
        second.damage.regions(),
        [Rect::new(40, 50, 24, 24), Rect::new(80, 90, 24, 24),]
    );
}
