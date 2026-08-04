use super::*;

#[test]
fn compact_layout_fills_rows_before_columns() {
    let layout = CompactLayout::new(
        7,
        CompactLayoutOptions {
            viewport_height: 188.0,
            item_width: 100.0,
            item_height: 50.0,
            gap: 10.0,
            padding: 4.0,
            side_padding: 4.0,
            ..CompactLayoutOptions::default()
        },
    );

    assert_eq!(layout.rows_per_column(), 3);
    assert_eq!(layout.item(0).unwrap().column, 0);
    assert_eq!(layout.item(2).unwrap().row, 2);
    assert_eq!(layout.item(3).unwrap().column, 1);
    assert_eq!(layout.item(3).unwrap().row, 0);
}

#[test]
fn compact_layout_hit_test_uses_model_index_not_row_index() {
    let layout = CompactLayout::new(
        6,
        CompactLayoutOptions {
            viewport_height: 128.0,
            item_width: 100.0,
            item_height: 50.0,
            gap: 10.0,
            padding: 4.0,
            ..CompactLayoutOptions::default()
        },
    );

    assert_eq!(layout.rows_per_column(), 2);
    assert_eq!(
        layout.hit_test_content_point(ViewPoint { x: 118.0, y: 10.0 }),
        Some(2)
    );
    assert_eq!(
        layout.hit_test_content_point(ViewPoint { x: 118.0, y: 70.0 }),
        Some(3)
    );
}

#[test]
fn compact_layout_distributes_unused_height_between_rows() {
    let layout = CompactLayout::new(
        6,
        CompactLayoutOptions {
            viewport_height: 128.0,
            item_width: 100.0,
            item_height: 50.0,
            gap: 10.0,
            padding: 4.0,
            ..CompactLayoutOptions::default()
        },
    );

    let first = layout.item(0).unwrap().item_rect;
    let second = layout.item(1).unwrap().item_rect;
    let distributed_gap = (128.0 - 2.0 * 50.0) / 3.0;

    assert!((first.y - distributed_gap).abs() < 0.01);
    assert!((second.y - (50.0 + 2.0 * distributed_gap)).abs() < 0.01);
    assert!((128.0 - second.bottom() - distributed_gap).abs() < 0.01);
}

#[test]
fn compact_layout_visible_items_respect_horizontal_scroll() {
    let layout = CompactLayout::new(
        12,
        CompactLayoutOptions {
            viewport_width: 110.0,
            viewport_height: 128.0,
            scroll_x: 114.0,
            item_width: 100.0,
            item_height: 50.0,
            gap: 10.0,
            padding: 4.0,
            ..CompactLayoutOptions::default()
        },
    );

    let indexes = layout
        .visible_items()
        .map(|item| item.model_index)
        .collect::<Vec<_>>();

    assert_eq!(indexes, vec![2, 3]);
}

#[test]
fn compact_layout_uses_variable_column_widths() {
    let layout = CompactLayout::new_with_column_widths(
        6,
        CompactLayoutOptions {
            viewport_height: 128.0,
            item_width: 100.0,
            item_height: 50.0,
            gap: 10.0,
            padding: 4.0,
            side_padding: 4.0,
            ..CompactLayoutOptions::default()
        },
        vec![100.0, 180.0, 120.0],
    );

    assert_eq!(layout.rows_per_column(), 2);
    assert_eq!(layout.item(2).unwrap().item_rect.width, 180.0);
    assert_eq!(layout.item(4).unwrap().item_rect.x, 304.0);
    assert_eq!(
        layout.hit_test_content_point(ViewPoint { x: 108.0, y: 8.0 }),
        None
    );
}

#[test]
fn compact_layout_visual_rect_follows_required_text_width() {
    let layout = CompactLayout::new_with_column_widths(
        2,
        CompactLayoutOptions {
            viewport_height: 128.0,
            item_width: 160.0,
            item_height: 50.0,
            icon_size: 24.0,
            text_height: 20.0,
            gap: 10.0,
            padding: 4.0,
            ..CompactLayoutOptions::default()
        },
        vec![240.0],
    );

    let full = layout.item(0).unwrap();
    let narrow = layout.item_with_required_text_width(0, Some(28.0)).unwrap();

    assert!(narrow.visual_rect.width < full.visual_rect.width);
    assert!(narrow.visual_rect.width >= narrow.icon_rect.width + narrow.text_rect.width);
    assert!(narrow.visual_rect.contains(ViewPoint {
        x: narrow.icon_rect.x,
        y: narrow.icon_rect.y
    }));
    assert!(!narrow.visual_rect.contains(ViewPoint {
        x: full.item_rect.right() - 2.0,
        y: narrow.visual_rect.y + 1.0
    }));
}

#[test]
fn compact_layout_visible_items_scale_with_viewport_not_model_size() {
    let layout = CompactLayout::new(
        1_000_000,
        CompactLayoutOptions {
            viewport_width: 220.0,
            viewport_height: 128.0,
            scroll_x: 100_000.0,
            item_width: 100.0,
            item_height: 50.0,
            gap: 10.0,
            padding: 4.0,
            ..CompactLayoutOptions::default()
        },
    );

    let indexes = layout
        .visible_items()
        .map(|item| item.model_index)
        .collect::<Vec<_>>();

    assert!(!indexes.is_empty());
    assert!(indexes.len() <= layout.rows_per_column() * 4);
    assert!(indexes.iter().all(|index| *index < 1_000_000));
}

#[test]
fn selection_rect_returns_model_indexes_in_layout_order() {
    let layout = CompactLayout::new(
        8,
        CompactLayoutOptions {
            viewport_height: 128.0,
            item_width: 100.0,
            item_height: 50.0,
            gap: 10.0,
            padding: 4.0,
            ..CompactLayoutOptions::default()
        },
    );

    let selection = layout.indexes_intersecting(ViewRect {
        x: 0.0,
        y: 60.0,
        width: 220.0,
        height: 60.0,
    });

    assert_eq!(selection.indexes(), &[1, 3]);
    assert_eq!(selection.range(), Some(1..4));
}

#[test]
fn compact_layout_rows_reserve_bottom_space() {
    let layout = CompactLayout::new(
        8,
        CompactLayoutOptions {
            viewport_height: 140.0,
            reserved_bottom: 20.0,
            item_height: 50.0,
            gap: 10.0,
            padding: 0.0,
            ..CompactLayoutOptions::default()
        },
    );

    assert_eq!(layout.rows_per_column(), 2);
    assert_eq!(layout.item(2).unwrap().column, 1);
    assert_eq!(layout.item(2).unwrap().row, 0);
}

#[test]
fn compact_empty_layout_does_not_inherit_viewport_extent() {
    let layout = CompactLayout::new(
        0,
        CompactLayoutOptions {
            viewport_width: 720.0,
            viewport_height: 520.0,
            ..CompactLayoutOptions::default()
        },
    );

    assert_eq!(layout.content_size().width, EMPTY_CONTENT_EXTENT);
    assert_eq!(layout.content_size().height, EMPTY_CONTENT_EXTENT);
    assert_eq!(layout.visible_items().count(), 0);
}

#[test]
fn icons_layout_fills_columns_before_rows() {
    let layout = IconsLayout::new(
        7,
        IconsLayoutOptions {
            viewport_width: 340.0,
            item_width: 100.0,
            item_height: 80.0,
            gap: 10.0,
            padding: 4.0,
            ..IconsLayoutOptions::default()
        },
    );

    assert_eq!(layout.columns_per_row(), 3);
    assert_eq!(layout.item(0).unwrap().row, 0);
    assert_eq!(layout.item(2).unwrap().column, 2);
    assert_eq!(layout.item(3).unwrap().row, 1);
    assert_eq!(layout.item(3).unwrap().column, 0);
}

#[test]
fn icons_empty_layout_does_not_inherit_viewport_extent() {
    let layout = IconsLayout::new(
        0,
        IconsLayoutOptions {
            viewport_width: 720.0,
            viewport_height: 520.0,
            ..IconsLayoutOptions::default()
        },
    );

    assert_eq!(layout.content_size().width, EMPTY_CONTENT_EXTENT);
    assert_eq!(layout.content_size().height, EMPTY_CONTENT_EXTENT);
    assert_eq!(layout.visible_items().count(), 0);
}

#[test]
fn icons_layout_visible_items_respect_vertical_scroll() {
    let layout = IconsLayout::new(
        12,
        IconsLayoutOptions {
            viewport_width: 230.0,
            viewport_height: 90.0,
            scroll_y: 94.0,
            item_width: 100.0,
            item_height: 80.0,
            gap: 10.0,
            padding: 4.0,
            ..IconsLayoutOptions::default()
        },
    );

    let indexes = layout
        .visible_items()
        .map(|item| item.model_index)
        .collect::<Vec<_>>();

    assert_eq!(indexes, vec![2, 3]);
}

#[test]
fn icons_layout_visible_items_scale_with_viewport_not_model_size() {
    let layout = IconsLayout::new(
        1_000_000,
        IconsLayoutOptions {
            viewport_width: 340.0,
            viewport_height: 190.0,
            scroll_y: 100_000.0,
            item_width: 100.0,
            item_height: 80.0,
            gap: 10.0,
            padding: 4.0,
            ..IconsLayoutOptions::default()
        },
    );

    let indexes = layout
        .visible_items()
        .map(|item| item.model_index)
        .collect::<Vec<_>>();

    assert!(!indexes.is_empty());
    assert!(indexes.len() <= layout.columns_per_row() * 4);
    assert!(indexes.iter().all(|index| *index < 1_000_000));
}

#[test]
fn icons_layout_hit_test_uses_row_major_index() {
    let layout = IconsLayout::new(
        6,
        IconsLayoutOptions {
            viewport_width: 230.0,
            item_width: 100.0,
            item_height: 80.0,
            gap: 10.0,
            padding: 4.0,
            ..IconsLayoutOptions::default()
        },
    );

    assert_eq!(
        layout.hit_test_content_point(ViewPoint { x: 128.0, y: 12.0 }),
        Some(1)
    );
    assert_eq!(
        layout.hit_test_content_point(ViewPoint { x: 18.0, y: 102.0 }),
        Some(2)
    );
}

#[test]
fn icons_layout_uses_row_maximum_item_height() {
    let layout = IconsLayout::new_with_item_heights(
        6,
        IconsLayoutOptions {
            viewport_width: 230.0,
            item_width: 100.0,
            item_height: 80.0,
            gap: 10.0,
            padding: 4.0,
            ..IconsLayoutOptions::default()
        },
        vec![120.0, 80.0, 80.0, 80.0, 80.0, 80.0],
    );

    assert_eq!(layout.columns_per_row(), 2);
    assert_eq!(layout.item(0).unwrap().item_rect.height, 120.0);
    assert_eq!(layout.item(1).unwrap().item_rect.height, 80.0);
    assert_eq!(layout.item(2).unwrap().item_rect.y, 140.0);
    assert_eq!(
        layout.hit_test_content_point(ViewPoint { x: 18.0, y: 135.0 }),
        None
    );
}

#[test]
fn icons_layout_item_height_only_offsets_following_rows() {
    let layout = IconsLayout::new_with_item_heights(
        6,
        IconsLayoutOptions {
            viewport_width: 230.0,
            item_width: 100.0,
            item_height: 80.0,
            gap: 10.0,
            padding: 4.0,
            ..IconsLayoutOptions::default()
        },
        vec![80.0, 80.0, 120.0, 80.0, 80.0, 80.0],
    );

    assert_eq!(layout.item(0).unwrap().item_rect.y, 10.0);
    assert_eq!(layout.item(2).unwrap().item_rect.y, 100.0);
    assert_eq!(layout.item(4).unwrap().item_rect.y, 230.0);
}
