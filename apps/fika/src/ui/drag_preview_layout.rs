use fika_core::{ItemLayout, ViewPoint, ViewRect};

use crate::ui::metrics::COMPACT_MIN_TEXT_WIDTH;
use crate::ui::options::ShellViewMode;

/// The text treatment used by the item delegates in FileManager's drag pixmaps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DragPreviewLabelStyle {
    FilenameWrapped,
    FilenameSingleLine,
    PlainSingleLine,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DragPreviewBackgroundStyle {
    SelectedItem,
    HoveredPlace,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DragPreviewLabelLayout {
    pub(crate) rect: ViewRect,
    pub(crate) style: DragPreviewLabelStyle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SingleDragPreviewLayout {
    /// The local pixmap bounds. The origin is always `(0, 0)`.
    pub(crate) bounds: ViewRect,
    pub(crate) icon: ViewRect,
    pub(crate) label: Option<DragPreviewLabelLayout>,
    pub(crate) background: ViewRect,
    pub(crate) background_style: DragPreviewBackgroundStyle,
    pub(crate) radius: f32,
    /// Hotspot in local scene coordinates. FileManager pane previews use the top
    /// centre, while Places retains the pointer's original row grab point.
    pub(crate) hotspot: ViewPoint,
    pub(crate) view_mode: Option<ShellViewMode>,
}

/// The fixed-size icon grid FileManager uses for a multi-item drag.
///
/// FileManager intentionally keeps this preview independent of the current item
/// view mode.  The grid is capped at 5x5 and contains no filename or count
/// label, so it remains useful when a selection contains unrelated names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MultiDragPreviewLayout {
    pub(crate) bounds: ViewRect,
    pub(crate) columns: usize,
    pub(crate) rows: usize,
    pub(crate) item_count: usize,
    pub(crate) icon_size: f32,
    pub(crate) gap: f32,
    pub(crate) hotspot: ViewPoint,
}

impl MultiDragPreviewLayout {
    pub(crate) fn cell_rect(self, index: usize) -> Option<ViewRect> {
        if index >= self.item_count || index >= self.columns * self.rows {
            return None;
        }
        let column = index % self.columns;
        let row = index / self.columns;
        Some(ViewRect {
            x: column as f32 * (self.icon_size + self.gap),
            y: row as f32 * (self.icon_size + self.gap),
            width: self.icon_size,
            height: self.icon_size,
        })
    }
}

/// Build FileManager's 3x3, 4x4, or 5x5 multi-item drag grid.
pub(crate) fn multi_drag_preview_layout(count: usize, scale: f32) -> MultiDragPreviewLayout {
    let count = count.max(1);
    let (mut columns, logical_icon_size) = if count > 16 {
        (5, 16.0)
    } else if count > 9 {
        (4, 22.0)
    } else {
        (3, 32.0)
    };
    columns = columns.min(count);
    let rows = count.div_ceil(columns).min(columns);
    let item_count = count.min(columns * rows);
    let scale = scale.max(1.0);
    let icon_size = logical_icon_size * scale;
    let gap = scale;
    let bounds = ViewRect {
        x: 0.0,
        y: 0.0,
        width: columns as f32 * (icon_size + gap),
        height: rows as f32 * (icon_size + gap),
    };
    MultiDragPreviewLayout {
        bounds,
        columns,
        rows,
        item_count,
        icon_size,
        gap,
        hotspot: ViewPoint {
            x: bounds.width / 2.0,
            y: 0.0,
        },
    }
}

/// Build the single-item pixmap geometry used by FileManager's item widgets.
///
/// `item_layout` is the layout already used by the live view. Keeping it as
/// the input means a drag preview cannot silently drift from the item the user
/// just dragged when zoom, view mode, or compact-column widths change.
///
/// Pane drag hotspot matches FileManager `KItemListController::startDragging`:
/// `(pixmap.width / devicePixelRatio) / 2`, `0` — top centre of the pixmap.
/// (Places uses press-relative hotspot via [`place_single_drag_preview_layout`].)
///
/// `press_point` is accepted for API symmetry with the places path but is not
/// used for pane items; FileManager never anchors pane previews to the grab point.
pub(crate) fn pane_single_drag_preview_layout(
    view_mode: ShellViewMode,
    item_layout: Option<ItemLayout>,
    icon_size: f32,
    natural_text_width: f32,
    text_line_height: f32,
    scale: f32,
    _press_point: Option<ViewPoint>,
) -> SingleDragPreviewLayout {
    let scale = scale.max(1.0);
    let item = item_layout.unwrap_or_else(|| {
        fallback_item_layout(
            view_mode,
            icon_size,
            natural_text_width,
            text_line_height,
            scale,
        )
    });
    // Crop to the painted content, not the layout slot:
    // - Icons: full cell (`item_rect`)
    // - Compact: per-name `visual_rect` (column `item_rect` is wider than the
    //   icon+label chip; using the slot width misplaces the top-centre hotspot)
    // - Details: name-column crop (icon + natural text width)
    let crop = match view_mode {
        ShellViewMode::Details => details_drag_crop(item, natural_text_width, scale),
        ShellViewMode::Compact => item.visual_rect,
        ShellViewMode::Icons => item.item_rect,
    };
    let bounds = ViewRect {
        x: 0.0,
        y: 0.0,
        width: crop.width.max(1.0),
        height: crop.height.max(1.0),
    };
    let icon = translate_clipped_rect(item.icon_rect, crop);
    let label_rect = translate_clipped_rect(item.text_rect, crop);
    let label = Some(DragPreviewLabelLayout {
        rect: label_rect,
        style: match view_mode {
            ShellViewMode::Icons => DragPreviewLabelStyle::FilenameWrapped,
            ShellViewMode::Compact | ShellViewMode::Details => {
                DragPreviewLabelStyle::FilenameSingleLine
            }
        },
    });
    let background = match view_mode {
        ShellViewMode::Icons | ShellViewMode::Details => item.item_rect,
        ShellViewMode::Compact => item.visual_rect,
    };
    // FileManager: setHotSpot(QPoint(pixmap.width()/dpr/2, 0))
    let hotspot = ViewPoint {
        x: bounds.width / 2.0,
        y: 0.0,
    };
    SingleDragPreviewLayout {
        bounds,
        icon,
        label,
        background: translate_clipped_rect(background, crop),
        background_style: DragPreviewBackgroundStyle::SelectedItem,
        radius: if view_mode == ShellViewMode::Details {
            0.0
        } else {
            5.0 * scale
        },
        hotspot,
        view_mode: Some(view_mode),
    }
}

/// Build the horizontal row used by `KFilePlacesView`'s delegate.
///
/// `hotspot` is the pointer press position relative to the source row, matching
/// `QAbstractItemView::startDrag` rather than FileManager's pane-specific top-centre
/// hotspot.
pub(crate) fn place_single_drag_preview_layout(
    row_width: f32,
    row_height: f32,
    icon_size: f32,
    text_line_height: f32,
    hotspot: ViewPoint,
    scale: f32,
) -> SingleDragPreviewLayout {
    let scale = scale.max(1.0);
    let row_width = row_width.max(icon_size + 32.0 * scale);
    let row_height = row_height.max(icon_size + 8.0 * scale);
    let text_line_height = text_line_height.max(1.0).min(row_height);
    let side_padding = 8.0 * scale;
    let gap = 8.0 * scale;
    let icon = ViewRect {
        x: side_padding,
        y: (row_height - icon_size) / 2.0,
        width: icon_size.max(1.0),
        height: icon_size.max(1.0),
    };
    let label = ViewRect {
        x: icon.right() + gap,
        y: (row_height - text_line_height) / 2.0,
        width: (row_width - icon.right() - gap - side_padding).max(1.0),
        height: text_line_height,
    };
    let bounds = ViewRect {
        x: 0.0,
        y: 0.0,
        width: row_width,
        height: row_height,
    };
    SingleDragPreviewLayout {
        bounds,
        icon,
        label: Some(DragPreviewLabelLayout {
            rect: label,
            style: DragPreviewLabelStyle::PlainSingleLine,
        }),
        background: bounds,
        background_style: DragPreviewBackgroundStyle::HoveredPlace,
        radius: 5.0 * scale,
        hotspot: ViewPoint {
            x: hotspot.x.clamp(0.0, bounds.width),
            y: hotspot.y.clamp(0.0, bounds.height),
        },
        view_mode: None,
    }
}

fn translate_clipped_rect(rect: ViewRect, crop: ViewRect) -> ViewRect {
    let left = rect.x.max(crop.x);
    let top = rect.y.max(crop.y);
    let right = rect.right().min(crop.right()).max(left);
    let bottom = rect.bottom().min(crop.bottom()).max(top);
    ViewRect {
        x: left - crop.x,
        y: top - crop.y,
        width: right - left,
        height: bottom - top,
    }
}

fn details_drag_crop(item: ItemLayout, natural_text_width: f32, scale: f32) -> ViewRect {
    let padding = 4.0 * scale.max(1.0);
    let left = item.icon_rect.x.min(item.item_rect.right());
    let text_width = natural_text_width
        .max(1.0)
        .min(item.text_rect.width.max(1.0));
    let right = (item.text_rect.x + text_width + padding * 2.0)
        .max(item.icon_rect.right() + padding)
        .min(item.item_rect.right());
    ViewRect {
        x: left,
        y: item.item_rect.y,
        width: (right - left).max(1.0),
        height: item.item_rect.height.max(1.0),
    }
}

fn fallback_item_layout(
    view_mode: ShellViewMode,
    icon_size: f32,
    natural_text_width: f32,
    text_line_height: f32,
    scale: f32,
) -> ItemLayout {
    let padding = 2.0 * scale;
    let gap = 8.0 * scale;
    match view_mode {
        ShellViewMode::Icons => {
            let width = (icon_size + padding * 2.0).max(64.0 * scale);
            let height = padding * 3.0 + icon_size + text_line_height;
            ItemLayout {
                model_index: 0,
                column: 0,
                row: 0,
                item_rect: ViewRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                },
                visual_rect: ViewRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                },
                icon_rect: ViewRect {
                    x: (width - icon_size) / 2.0,
                    y: padding,
                    width: icon_size,
                    height: icon_size,
                },
                text_rect: ViewRect {
                    x: padding,
                    y: padding * 2.0 + icon_size,
                    width: (width - padding * 2.0).max(1.0),
                    height: text_line_height,
                },
            }
        }
        ShellViewMode::Compact => {
            let width = (padding * 4.0
                + icon_size
                + gap
                + natural_text_width.max(COMPACT_MIN_TEXT_WIDTH * scale))
            .max(1.0);
            let height = (padding * 2.0 + icon_size.max(text_line_height)).max(1.0);
            ItemLayout {
                model_index: 0,
                column: 0,
                row: 0,
                item_rect: ViewRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                },
                visual_rect: ViewRect {
                    x: padding,
                    y: padding,
                    width: (width - padding * 2.0).max(1.0),
                    height: (height - padding * 2.0).max(1.0),
                },
                icon_rect: ViewRect {
                    x: padding,
                    y: (height - icon_size) / 2.0,
                    width: icon_size,
                    height: icon_size,
                },
                text_rect: ViewRect {
                    x: padding + icon_size + gap,
                    y: (height - text_line_height) / 2.0,
                    width: natural_text_width.max(1.0),
                    height: text_line_height,
                },
            }
        }
        ShellViewMode::Details => {
            let height = (8.0 * scale + icon_size.max(text_line_height)).max(1.0);
            let text_x = 8.0 * scale + icon_size + gap;
            let width = text_x + natural_text_width.max(1.0) + 8.0 * scale;
            ItemLayout {
                model_index: 0,
                column: 0,
                row: 0,
                item_rect: ViewRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                },
                visual_rect: ViewRect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                },
                icon_rect: ViewRect {
                    x: 8.0 * scale,
                    y: (height - icon_size) / 2.0,
                    width: icon_size,
                    height: icon_size,
                },
                text_rect: ViewRect {
                    x: text_x,
                    y: (height - text_line_height) / 2.0,
                    width: natural_text_width.max(1.0),
                    height: text_line_height,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> ViewRect {
        ViewRect {
            x,
            y,
            width,
            height,
        }
    }

    fn item() -> ItemLayout {
        ItemLayout {
            model_index: 0,
            column: 0,
            row: 0,
            item_rect: rect(10.0, 20.0, 140.0, 86.0),
            visual_rect: rect(12.0, 22.0, 136.0, 82.0),
            icon_rect: rect(52.0, 24.0, 48.0, 48.0),
            text_rect: rect(20.0, 76.0, 120.0, 18.0),
        }
    }

    #[test]
    fn icons_preview_keeps_item_shape_and_places_name_below_icon() {
        let layout = pane_single_drag_preview_layout(
            ShellViewMode::Icons,
            Some(item()),
            48.0,
            80.0,
            18.0,
            1.0,
            None,
        );
        assert_eq!(layout.bounds, rect(0.0, 0.0, 140.0, 86.0));
        assert_eq!(layout.icon, rect(42.0, 4.0, 48.0, 48.0));
        assert_eq!(layout.label.unwrap().rect, rect(10.0, 56.0, 120.0, 18.0));
        assert_eq!(layout.hotspot, ViewPoint { x: 70.0, y: 0.0 });
    }

    #[test]
    fn compact_preview_keeps_name_to_the_right() {
        let compact_item = ItemLayout {
            model_index: 0,
            column: 0,
            row: 0,
            // Column slot is wider than the painted chip (visual_rect).
            item_rect: rect(10.0, 20.0, 240.0, 40.0),
            visual_rect: rect(12.0, 22.0, 176.0, 36.0),
            icon_rect: rect(14.0, 26.0, 28.0, 28.0),
            text_rect: rect(50.0, 30.0, 128.0, 18.0),
        };
        let layout = pane_single_drag_preview_layout(
            ShellViewMode::Compact,
            Some(compact_item),
            28.0,
            80.0,
            18.0,
            1.0,
            None,
        );
        assert!(layout.label.unwrap().rect.x > layout.icon.right());
        // Pixmap bounds follow per-item visual width, not the column slot.
        assert_eq!(layout.bounds, rect(0.0, 0.0, 176.0, 36.0));
        assert_eq!(layout.background, rect(0.0, 0.0, 176.0, 36.0));
        assert_eq!(layout.icon, rect(2.0, 4.0, 28.0, 28.0));
        // FileManager pane hotspot is top-centre of the pixmap (visual width / 2).
        assert_eq!(layout.hotspot, ViewPoint { x: 88.0, y: 0.0 });
    }

    #[test]
    fn compact_preview_hotspot_is_file_manager_top_centre_even_with_press() {
        let compact_item = ItemLayout {
            model_index: 0,
            column: 0,
            row: 0,
            item_rect: rect(10.0, 20.0, 240.0, 40.0),
            visual_rect: rect(12.0, 22.0, 176.0, 36.0),
            icon_rect: rect(14.0, 26.0, 28.0, 28.0),
            text_rect: rect(50.0, 30.0, 128.0, 18.0),
        };
        // Press point is ignored for pane items (FileManager never uses it).
        let layout = pane_single_drag_preview_layout(
            ShellViewMode::Compact,
            Some(compact_item),
            28.0,
            80.0,
            18.0,
            1.0,
            Some(ViewPoint { x: 120.0, y: 42.0 }),
        );
        assert_eq!(layout.bounds.width, 176.0);
        assert_eq!(layout.hotspot, ViewPoint { x: 88.0, y: 0.0 });
        // Must not use half the column slot width (240/2 = 120).
        assert_ne!(layout.hotspot.x, compact_item.item_rect.width / 2.0);
    }

    #[test]
    fn compact_preview_uses_per_item_visual_width_not_column_slot() {
        let short = ItemLayout {
            model_index: 0,
            column: 0,
            row: 0,
            item_rect: rect(0.0, 0.0, 200.0, 40.0),
            visual_rect: rect(2.0, 2.0, 60.0, 36.0),
            icon_rect: rect(4.0, 6.0, 28.0, 28.0),
            text_rect: rect(40.0, 10.0, 18.0, 18.0),
        };
        let long = ItemLayout {
            model_index: 1,
            column: 0,
            row: 1,
            item_rect: rect(0.0, 40.0, 200.0, 40.0),
            visual_rect: rect(2.0, 42.0, 180.0, 36.0),
            icon_rect: rect(4.0, 46.0, 28.0, 28.0),
            text_rect: rect(40.0, 50.0, 138.0, 18.0),
        };
        let short_layout = pane_single_drag_preview_layout(
            ShellViewMode::Compact,
            Some(short),
            28.0,
            20.0,
            18.0,
            1.0,
            None,
        );
        let long_layout = pane_single_drag_preview_layout(
            ShellViewMode::Compact,
            Some(long),
            28.0,
            120.0,
            18.0,
            1.0,
            None,
        );
        assert_eq!(short_layout.bounds.width, 60.0);
        assert_eq!(long_layout.bounds.width, 180.0);
        assert_eq!(short_layout.hotspot.x, 30.0);
        assert_eq!(long_layout.hotspot.x, 90.0);
    }

    #[test]
    fn details_preview_crops_to_name_column() {
        let layout = pane_single_drag_preview_layout(
            ShellViewMode::Details,
            Some(item()),
            18.0,
            40.0,
            18.0,
            1.0,
            None,
        );
        assert!(layout.bounds.width < item().item_rect.width);
        assert!(layout.label.unwrap().rect.right() <= layout.bounds.right());
        assert_eq!(layout.background, layout.bounds);
        assert_eq!(
            layout.label.unwrap().style,
            DragPreviewLabelStyle::FilenameSingleLine
        );
        assert_eq!(layout.radius, 0.0);
    }

    #[test]
    fn places_preview_is_a_horizontal_row() {
        let layout = place_single_drag_preview_layout(
            212.0,
            30.0,
            22.0,
            18.0,
            ViewPoint { x: 44.0, y: 15.0 },
            1.0,
        );
        assert_eq!(layout.bounds, rect(0.0, 0.0, 212.0, 30.0));
        assert_eq!(layout.icon, rect(8.0, 4.0, 22.0, 22.0));
        assert_eq!(layout.label.unwrap().rect, rect(38.0, 6.0, 166.0, 18.0));
        assert_eq!(layout.hotspot, ViewPoint { x: 44.0, y: 15.0 });
    }

    #[test]
    fn multi_preview_matches_file_manager_grid_thresholds() {
        let three = multi_drag_preview_layout(3, 1.0);
        assert_eq!((three.columns, three.rows), (3, 1));
        assert_eq!(three.icon_size, 32.0);
        assert_eq!(three.cell_rect(2).unwrap(), rect(66.0, 0.0, 32.0, 32.0));

        let ten = multi_drag_preview_layout(10, 1.0);
        assert_eq!((ten.columns, ten.rows), (4, 3));
        assert_eq!(ten.icon_size, 22.0);

        let twenty_five = multi_drag_preview_layout(25, 1.5);
        assert_eq!((twenty_five.columns, twenty_five.rows), (5, 5));
        assert_eq!(twenty_five.item_count, 25);
        assert!(twenty_five.cell_rect(25).is_none());
        assert_eq!(twenty_five.hotspot.y, 0.0);
    }
}
