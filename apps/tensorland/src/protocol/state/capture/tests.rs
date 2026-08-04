use super::*;

#[test]
fn shm_constraints_advertise_standard_formats() {
    let constraints = shm_constraints(BufferSize::from((1920, 1080)));
    assert_eq!(constraints.size.w, 1920);
    assert!(constraints.shm.contains(&wl_shm::Format::Xrgb8888));
}

#[test]
fn view_color_is_opaque() {
    assert_eq!(view_color(ViewId::new(42)) >> 24, 0xFF);
}

#[test]
fn capture_pixel_budget_rejects_absurd_sizes() {
    let kind = CaptureKind::Toplevel {
        size: BufferSize::from((16_000, 16_000)),
        geometry: Rect::new(0, 0, 16_000, 16_000),
        draw_cursors: false,
        gpu_target: None,
    };
    assert!(capture_pixel_count(kind) > MAX_CAPTURE_PIXELS);
}

#[test]
fn fill_rect_writes_expected_pixel() {
    let mut buf = vec![0u8; 8 * 4];
    let stride = 8;
    fill_rect(
        &mut buf,
        stride,
        2,
        2,
        &Rect::new(1, 0, 1, 1),
        0xFF_12_34_56,
        Rect::new(0, 0, 2, 2),
    );
    assert_eq!(&buf[4..8], &0xFF_12_34_56u32.to_le_bytes());
}
