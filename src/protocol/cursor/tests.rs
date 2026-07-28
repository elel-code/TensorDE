use super::*;

fn output() -> LogicalRect<i32> {
    LogicalRect::new((10, 20).into(), (100, 80).into())
}

#[test]
fn visible_cursor_uses_output_local_physical_coordinates() {
    let mut cursor = CursorState::default();
    let overlays = cursor.overlays_for_output(
        Some((20.4, 30.4).into()),
        output(),
        OutputScale::from_f64(1.25).unwrap(),
        Rect::new(0, 0, 125, 100),
        |_, _| None,
    );
    let overlay = overlays.as_slice()[0];

    assert_eq!(overlay.destination, Rect::new(13, 13, 30, 30));
    assert_eq!(overlay.clip, overlay.destination);
}

#[test]
fn hidden_and_off_output_cursors_do_not_create_an_overlay() {
    let mut cursor = CursorState::default();
    assert!(!cursor.set_image(CursorImage::default_named()));
    assert!(cursor.set_image(CursorImage::Hidden));
    assert_eq!(
        cursor
            .overlays_for_output(
                Some((20.0, 30.0).into()),
                output(),
                OutputScale::ONE,
                Rect::new(0, 0, 100, 80),
                |_, _| None,
            )
            .as_slice(),
        []
    );
    cursor.set_image(CursorImage::default_named());
    assert_eq!(
        cursor
            .overlays_for_output(
                Some((110.0, 30.0).into()),
                output(),
                OutputScale::ONE,
                Rect::new(0, 0, 100, 80),
                |_, _| None,
            )
            .as_slice(),
        []
    );
}

#[test]
fn cursor_extent_can_cross_an_output_boundary_before_its_hotspot() {
    let mut cursor = CursorState::default();
    let adjacent = LogicalRect::new((110, 20).into(), (100, 80).into());
    let overlays = cursor.overlays_for_output(
        Some((105.0, 30.0).into()),
        adjacent,
        OutputScale::ONE,
        Rect::new(0, 0, 100, 80),
        |_, _| None,
    );

    assert_eq!(overlays.as_slice().len(), 1);
    assert_eq!(
        overlays.as_slice()[0].destination,
        Rect::new(-5, 10, 24, 24)
    );
    assert_eq!(overlays.as_slice()[0].clip, Rect::new(0, 10, 19, 24));
}

#[test]
fn pointer_and_tablet_cursors_remain_independent() {
    let mut cursor = CursorState::default();
    assert!(cursor.note_tablet_activity(tensor_event::TabletToolId::new(1), (40.0, 50.0).into()));
    let overlays = cursor.overlays_for_output(
        Some((20.0, 30.0).into()),
        output(),
        OutputScale::ONE,
        Rect::new(0, 0, 100, 80),
        |_, _| None,
    );
    assert_eq!(overlays.as_slice().len(), 2);
    assert_eq!(
        overlays.as_slice()[0].destination,
        Rect::new(10, 10, 24, 24)
    );
    assert_eq!(
        overlays.as_slice()[1].destination,
        Rect::new(30, 30, 24, 24)
    );
    assert!(cursor.clear_tablet(tensor_event::TabletToolId::new(1)));
    assert_eq!(
        cursor
            .overlays_for_output(
                Some((20.0, 30.0).into()),
                output(),
                OutputScale::ONE,
                Rect::new(0, 0, 100, 80),
                |_, _| None,
            )
            .as_slice()
            .len(),
        1
    );
}

#[test]
fn named_raster_uses_physical_hotspot_and_sample_transform() {
    let mut cursor = CursorState::default();
    let scale = OutputScale::from_f64(1.25).unwrap();
    let transform = SurfaceSampleTransform::new((0.25, 0.5), (0.5, 0.0), (0.0, 0.25));
    cursor.named_rasters.insert(
        (CursorIcon::Default, scale),
        Some(CursorRasterSequence {
            frames: vec![CursorRasterFrame {
                raster: CursorRaster {
                    buffer_id: SurfaceBufferId::new(7),
                    size: Size::new(32, 40),
                    hotspot: Point::new(3, 4),
                    sample_transform: transform,
                },
                delay_ms: 0,
            }],
            duration_ms: 0,
            current: 0,
        }),
    );

    let overlays = cursor.overlays_for_output(
        Some((20.0, 30.0).into()),
        output(),
        scale,
        Rect::new(0, 0, 125, 100),
        |_, _| None,
    );
    let overlay = overlays.as_slice()[0];

    assert_eq!(overlay.destination, Rect::new(10, 9, 32, 40));
    assert_eq!(
        overlay.texture,
        Some(CursorTexture {
            buffer_id: SurfaceBufferId::new(7),
            sample_transform: transform,
        })
    );
}

#[test]
fn animated_raster_selects_frames_and_exact_next_deadlines() {
    let frame = |buffer_id, delay_ms| CursorRasterFrame {
        raster: CursorRaster {
            buffer_id: SurfaceBufferId::new(buffer_id),
            size: Size::new(24, 24),
            hotspot: Point::new(1, 2),
            sample_transform: SurfaceSampleTransform::IDENTITY,
        },
        delay_ms,
    };
    let sequence = CursorRasterSequence {
        frames: vec![frame(1, 10), frame(2, 20), frame(3, 30)],
        duration_ms: 60,
        current: 0,
    };

    assert_eq!(
        sequence.frame_at(Duration::from_millis(0)),
        Some((0, Duration::from_millis(10)))
    );
    assert_eq!(
        sequence.frame_at(Duration::from_millis(10)),
        Some((1, Duration::from_millis(20)))
    );
    assert_eq!(
        sequence.frame_at(Duration::from_millis(59)),
        Some((2, Duration::from_millis(1)))
    );
    assert_eq!(
        sequence.frame_at(Duration::from_millis(60)),
        Some((0, Duration::from_millis(10)))
    );
}

#[test]
fn animation_tick_advances_only_active_named_shapes() {
    let frame = |buffer_id, delay_ms| CursorRasterFrame {
        raster: CursorRaster {
            buffer_id: SurfaceBufferId::new(buffer_id),
            size: Size::new(24, 24),
            hotspot: Point::new(0, 0),
            sample_transform: SurfaceSampleTransform::IDENTITY,
        },
        delay_ms,
    };
    let sequence = |first, second| {
        Some(CursorRasterSequence {
            frames: vec![frame(first, 10), frame(second, 20)],
            duration_ms: 30,
            current: 0,
        })
    };
    let mut cursor = CursorState::default();
    cursor
        .named_rasters
        .insert((CursorIcon::Default, OutputScale::ONE), sequence(1, 2));
    cursor
        .named_rasters
        .insert((CursorIcon::Wait, OutputScale::ONE), sequence(3, 4));
    let now = Instant::now();
    cursor.animation_epoch = now - Duration::from_millis(15);

    assert!(cursor.named_animation_will_change(now));
    cursor.advance_named_animation(now);

    assert_eq!(
        cursor.named_rasters[&(CursorIcon::Default, OutputScale::ONE)]
            .as_ref()
            .unwrap()
            .current,
        1
    );
    assert_eq!(
        cursor.named_rasters[&(CursorIcon::Wait, OutputScale::ONE)]
            .as_ref()
            .unwrap()
            .current,
        0
    );
}

#[test]
fn theme_change_releases_every_uploaded_animation_frame() {
    let raster = |buffer_id| CursorRasterFrame {
        raster: CursorRaster {
            buffer_id: SurfaceBufferId::new(buffer_id),
            size: Size::new(24, 24),
            hotspot: Point::new(0, 0),
            sample_transform: SurfaceSampleTransform::IDENTITY,
        },
        delay_ms: 10,
    };
    let mut cursor = CursorState::default();
    cursor.named_rasters.insert(
        (CursorIcon::Default, OutputScale::ONE),
        Some(CursorRasterSequence {
            frames: vec![raster(3), raster(4)],
            duration_ms: 20,
            current: 0,
        }),
    );

    let released = cursor.configure("another-theme".to_owned(), 24, false);

    assert_eq!(released, [SurfaceBufferId::new(3), SurfaceBufferId::new(4)]);
    assert!(cursor.named_rasters.is_empty());
}
