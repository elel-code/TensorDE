#[test]
fn folder_preview_gpu_draw_rect_stays_inside_folder_shell_slot() {
    let layout = ItemPixmapLayout {
        view_mode: ShellViewMode::Icons,
        icon_rect: ViewRect {
            x: 50.0,
            y: 20.0,
            width: 100.0,
            height: 100.0,
        },
        text_rect: ViewRect {
            x: 12.0,
            y: 124.0,
            width: 176.0,
            height: 16.0,
        },
        text_midline_shift: 0.0,
    };
    let shell = folder_preview_role_shell_rect(layout);
    let slot = folder_preview_role_slot(layout);
    let draw = folder_preview_gpu_draw_rect(layout);

    assert!(slot.x >= shell.x);
    assert!(slot.y >= shell.y);
    assert!(slot.right() <= shell.right() + f32::EPSILON);
    assert!(slot.bottom() <= shell.bottom() + f32::EPSILON);
    assert!(draw.width <= slot.width + f32::EPSILON);
    assert!(draw.height <= slot.height + f32::EPSILON);
    assert!((draw.width - draw.height).abs() < 0.05);
    assert!((draw.x + draw.width / 2.0 - (slot.x + slot.width / 2.0)).abs() < f32::EPSILON);
    assert!((draw.y + draw.height / 2.0 - (slot.y + slot.height / 2.0)).abs() < f32::EPSILON);
    assert_eq!(draw, slot);
}

#[test]
fn folder_preview_gpu_draw_rect_uses_file_manager_text_midline_shift_in_compact_area() {
    let layout = ItemPixmapLayout {
        view_mode: ShellViewMode::Compact,
        icon_rect: ViewRect {
            x: 4.0,
            y: 8.0,
            width: 48.0,
            height: 48.0,
        },
        text_rect: ViewRect {
            x: 60.0,
            y: 22.0,
            width: 140.0,
            height: 18.0,
        },
        text_midline_shift: 3.0,
    };
    let area = folder_preview_role_shell_rect(layout);
    let draw = folder_preview_gpu_draw_rect(layout);
    let expected_center_y = layout.text_rect.y + layout.text_rect.height / 2.0 + 3.0;

    assert!((area.y + area.height / 2.0 - expected_center_y).abs() < f32::EPSILON);
    assert!(draw.y >= area.y);
    assert!(draw.bottom() <= area.bottom() + f32::EPSILON);
    assert!((draw.x + draw.width / 2.0 - (area.x + area.width / 2.0)).abs() < f32::EPSILON);
    assert_eq!(draw, area);
}

#[test]
fn file_manager_text_midline_shift_matches_font_metrics_formula() {
    let shift =
        file_manager_text_midline_shift_from_metrics(18.0, 14.0, 1000, -200.0, Some(700.0));

    assert!((shift - 1.3).abs() < 0.01);
}
