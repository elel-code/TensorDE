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

#[test]
fn reconnecting_a_connector_gets_a_new_protocol_instance() {
    let mode = PhysicalMode::new(1920, 1080, 60_000);
    let make_output = || {
        Output::new(
            ConnectorId::new(7, 11),
            "DP-1".to_owned(),
            (600, 340),
            SubpixelLayout::Unknown,
            vec![mode],
            mode,
            mode,
            OutputScale::ONE,
        )
    };
    let first = make_output();
    let second = make_output();
    assert_eq!(first.id(), second.id());
    assert_ne!(first.instance_id(), second.instance_id());
    assert_eq!(first.logical_size(), (1920, 1080));
}
