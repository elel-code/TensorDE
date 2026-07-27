use super::*;

#[test]
fn atomic_snapshot_never_exposes_partial_reconfiguration() {
    let first = OutputSnapshot {
        mode: Some(PhysicalMode::new(1920, 1080, 60_000)),
        location: (0, 0),
        physical_size: (600, 340),
        subpixel: SubpixelLayout::HorizontalRgb,
        scale: OutputScale::ONE,
        transform: SurfaceTransform::Normal,
    };
    let snapshot = AtomicOutputSnapshot::new(first);
    let second = OutputSnapshot {
        mode: Some(PhysicalMode::new(2560, 1440, 144_000)),
        location: (1920, -40),
        physical_size: (700, 390),
        subpixel: SubpixelLayout::VerticalBgr,
        scale: OutputScale::from_f64(1.25).unwrap(),
        transform: SurfaceTransform::Rotate90,
    };
    snapshot.store(second);
    assert_eq!(snapshot.load(), second);
}

#[test]
fn fractional_scale_rounds_xdg_size_without_allocating() {
    let scale = OutputScale::from_f64(1.25).unwrap();
    assert_eq!(logical_length_round(1920, scale), 1536);
    assert_eq!(integer_scale(scale), 2);
    assert_eq!(
        transformed_dimensions(1536, 864, SurfaceTransform::Rotate90),
        (864, 1536)
    );
}
