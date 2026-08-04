use tensor_files_core::ViewRect;

use crate::ui::options::ShellViewMode;

use super::style::{
    BREEZE_FOCUS_PEN_WIDTH, BREEZE_ITEM_ROUNDNESS, FileManagerItemPalette, UiColor,
    details_row_background_color_for_palette,
    item_background_color_for_palette_with_hover_progress, item_focus_color_for_palette,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FileManagerItemFill {
    pub(crate) rect: ViewRect,
    pub(crate) radius: f32,
    pub(crate) color: UiColor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FileManagerItemFocus {
    pub(crate) rect: ViewRect,
    pub(crate) radius: f32,
    pub(crate) color: UiColor,
    pub(crate) stroke_width: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct FileManagerItemPaint {
    pub(crate) alternate_background: Option<FileManagerItemFill>,
    pub(crate) background: Option<FileManagerItemFill>,
    pub(crate) focus: Option<FileManagerItemFocus>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FileManagerItemGeometry {
    pub(crate) item: ViewRect,
    pub(crate) visual: ViewRect,
    pub(crate) content: ViewRect,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FileManagerItemInteraction {
    pub(crate) selected: bool,
    pub(crate) hovered: bool,
    pub(crate) current: bool,
    pub(crate) alternate: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn file_manager_item_paint(
    view_mode: ShellViewMode,
    geometry: FileManagerItemGeometry,
    interaction: FileManagerItemInteraction,
    scale: f32,
) -> FileManagerItemPaint {
    file_manager_item_paint_with_palette(
        view_mode,
        geometry,
        interaction,
        scale,
        FileManagerItemPalette::light(),
    )
}

pub(crate) fn file_manager_item_paint_with_palette(
    view_mode: ShellViewMode,
    geometry: FileManagerItemGeometry,
    interaction: FileManagerItemInteraction,
    scale: f32,
    palette: FileManagerItemPalette,
) -> FileManagerItemPaint {
    file_manager_item_paint_with_palette_and_hover_progress(
        view_mode,
        geometry,
        interaction,
        scale,
        palette,
        1.0,
    )
}

pub(crate) fn file_manager_item_paint_with_palette_and_hover_progress(
    view_mode: ShellViewMode,
    geometry: FileManagerItemGeometry,
    interaction: FileManagerItemInteraction,
    scale: f32,
    palette: FileManagerItemPalette,
    hover_progress: f32,
) -> FileManagerItemPaint {
    let FileManagerItemGeometry {
        item: item_rect,
        visual: visual_rect,
        content: content_rect,
    } = geometry;
    let FileManagerItemInteraction {
        selected,
        hovered,
        current,
        alternate,
    } = interaction;
    let radius = BREEZE_ITEM_ROUNDNESS * scale.max(1.0);
    let highlight_rect = match view_mode {
        ShellViewMode::Details => item_rect,
        ShellViewMode::Compact => content_rect,
        ShellViewMode::Icons => {
            file_manager_selection_full_rect(view_mode, item_rect, visual_rect, scale)
        }
    };
    let alternate_background =
        (view_mode == ShellViewMode::Details && alternate).then(|| FileManagerItemFill {
            rect: item_rect,
            radius: 0.0,
            color: details_row_background_color_for_palette(false, false, true, palette),
        });
    let background = match view_mode {
        ShellViewMode::Details => (selected || hovered).then(|| FileManagerItemFill {
            rect: highlight_rect,
            radius: 0.0,
            color: item_background_color_for_palette_with_hover_progress(
                selected,
                hovered,
                palette,
                hover_progress,
            ),
        }),
        ShellViewMode::Compact | ShellViewMode::Icons => {
            (selected || hovered).then(|| FileManagerItemFill {
                rect: highlight_rect,
                radius,
                color: item_background_color_for_palette_with_hover_progress(
                    selected,
                    hovered,
                    palette,
                    hover_progress,
                ),
            })
        }
    };

    let focus = current.then(|| {
        let stroke_width = BREEZE_FOCUS_PEN_WIDTH * scale.max(1.0);
        FileManagerItemFocus {
            rect: inset_rect(highlight_rect, stroke_width * 0.5).unwrap_or(highlight_rect),
            radius: (radius - stroke_width * 0.5).max(1.0),
            color: item_focus_color_for_palette(selected, hovered, palette),
            stroke_width,
        }
    });

    FileManagerItemPaint {
        alternate_background,
        background,
        focus,
    }
}

pub(crate) fn file_manager_selection_full_rect(
    view_mode: ShellViewMode,
    item_rect: ViewRect,
    _visual_rect: ViewRect,
    scale: f32,
) -> ViewRect {
    match view_mode {
        ShellViewMode::Details => item_rect,
        ShellViewMode::Compact => {
            let padding = 2.0 * scale.max(1.0);
            ViewRect {
                x: item_rect.x - padding,
                y: item_rect.y,
                width: item_rect.width + padding * 2.0,
                height: item_rect.height,
            }
        }
        ShellViewMode::Icons => item_rect,
    }
}

pub(crate) fn file_manager_selection_core_rect(
    view_mode: ShellViewMode,
    item_rect: ViewRect,
    visual_rect: ViewRect,
    icon_rect: ViewRect,
    text_rect: ViewRect,
    selected: bool,
) -> ViewRect {
    match view_mode {
        ShellViewMode::Details if !selected => union_rect(icon_rect, text_rect),
        ShellViewMode::Details => item_rect,
        ShellViewMode::Compact | ShellViewMode::Icons => visual_rect,
    }
}

fn union_rect(left: ViewRect, right: ViewRect) -> ViewRect {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let rect_right = left.right().max(right.right());
    let bottom = left.bottom().max(right.bottom());
    ViewRect {
        x,
        y,
        width: (rect_right - x).max(0.0),
        height: (bottom - y).max(0.0),
    }
}

fn inset_rect(rect: ViewRect, inset: f32) -> Option<ViewRect> {
    let inset = inset.max(0.0);
    let width = rect.width - inset * 2.0;
    let height = rect.height - inset * 2.0;
    (width > 0.0 && height > 0.0).then_some(ViewRect {
        x: rect.x + inset,
        y: rect.y + inset,
        width,
        height,
    })
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

    fn geometry(item: ViewRect, visual: ViewRect, content: ViewRect) -> FileManagerItemGeometry {
        FileManagerItemGeometry {
            item,
            visual,
            content,
        }
    }

    fn interaction(
        selected: bool,
        hovered: bool,
        current: bool,
        alternate: bool,
    ) -> FileManagerItemInteraction {
        FileManagerItemInteraction {
            selected,
            hovered,
            current,
            alternate,
        }
    }

    #[test]
    fn details_item_paint_uses_alternate_row_base_without_idle_highlight() {
        let paint = file_manager_item_paint(
            ShellViewMode::Details,
            geometry(
                rect(0.0, 0.0, 320.0, 28.0),
                rect(8.0, 2.0, 180.0, 24.0),
                rect(8.0, 2.0, 180.0, 24.0),
            ),
            interaction(false, false, false, true),
            1.0,
        );

        assert_eq!(
            paint.alternate_background,
            Some(FileManagerItemFill {
                rect: rect(0.0, 0.0, 320.0, 28.0),
                radius: 0.0,
                color: [0.949, 0.957, 0.969, 1.0],
            })
        );
        assert_eq!(paint.background, None);
        assert_eq!(paint.focus, None);
    }

    #[test]
    fn details_item_paint_layers_selection_highlight_over_alternate_row() {
        let paint = file_manager_item_paint(
            ShellViewMode::Details,
            geometry(
                rect(0.0, 0.0, 320.0, 28.0),
                rect(8.0, 2.0, 180.0, 24.0),
                rect(8.0, 2.0, 180.0, 24.0),
            ),
            interaction(true, false, false, true),
            1.0,
        );

        assert_eq!(
            paint.alternate_background,
            Some(FileManagerItemFill {
                rect: rect(0.0, 0.0, 320.0, 28.0),
                radius: 0.0,
                color: [0.949, 0.957, 0.969, 1.0],
            })
        );
        assert_eq!(
            paint.background,
            Some(FileManagerItemFill {
                rect: rect(0.0, 0.0, 320.0, 28.0),
                radius: 0.0,
                color: [0.239, 0.502, 0.710, 0.32],
            })
        );
    }

    #[test]
    fn icons_item_paint_only_fills_interactive_items() {
        let idle = file_manager_item_paint(
            ShellViewMode::Icons,
            geometry(
                rect(0.0, 0.0, 120.0, 120.0),
                rect(4.0, 4.0, 112.0, 112.0),
                rect(4.0, 4.0, 112.0, 112.0),
            ),
            interaction(false, false, false, false),
            1.0,
        );
        assert_eq!(idle.background, None);

        let selected = file_manager_item_paint(
            ShellViewMode::Icons,
            geometry(
                rect(0.0, 0.0, 120.0, 120.0),
                rect(4.0, 4.0, 112.0, 112.0),
                rect(4.0, 4.0, 112.0, 112.0),
            ),
            interaction(true, false, false, false),
            1.0,
        );
        assert_eq!(
            selected.background,
            Some(FileManagerItemFill {
                rect: rect(0.0, 0.0, 120.0, 120.0),
                radius: BREEZE_ITEM_ROUNDNESS,
                color: [0.239, 0.502, 0.710, 0.32],
            })
        );
    }

    #[test]
    fn compact_item_paint_uses_item_content_rect() {
        let paint = file_manager_item_paint(
            ShellViewMode::Compact,
            geometry(
                rect(10.0, 0.0, 180.0, 32.0),
                rect(12.0, 2.0, 176.0, 28.0),
                rect(12.0, 2.0, 176.0, 28.0),
            ),
            interaction(true, false, false, false),
            1.0,
        );

        assert_eq!(
            paint.background,
            Some(FileManagerItemFill {
                rect: rect(12.0, 2.0, 176.0, 28.0),
                radius: BREEZE_ITEM_ROUNDNESS,
                color: [0.239, 0.502, 0.710, 0.32],
            })
        );
    }

    #[test]
    fn current_compact_item_paint_uses_inset_breeze_focus_stroke_on_content_rect() {
        let paint = file_manager_item_paint(
            ShellViewMode::Compact,
            geometry(
                rect(10.0, 0.0, 180.0, 32.0),
                rect(2.0, 2.0, 176.0, 28.0),
                rect(2.0, 2.0, 176.0, 28.0),
            ),
            interaction(false, false, true, false),
            1.0,
        );

        assert_eq!(
            paint.focus,
            Some(FileManagerItemFocus {
                rect: rect(2.625, 2.625, 174.75, 26.75),
                radius: 4.375,
                color: [0.217, 0.456, 0.645, 0.8],
                stroke_width: BREEZE_FOCUS_PEN_WIDTH,
            })
        );
    }

    #[test]
    fn details_selection_core_uses_icon_and_text_for_details_rows() {
        let item = rect(0.0, 20.0, 480.0, 28.0);
        let visual = item;
        let icon = rect(8.0, 22.0, 24.0, 24.0);
        let text = rect(40.0, 24.0, 160.0, 18.0);

        assert_eq!(
            file_manager_selection_core_rect(
                ShellViewMode::Details,
                item,
                visual,
                icon,
                text,
                false
            ),
            rect(8.0, 22.0, 192.0, 24.0)
        );
        assert_eq!(
            file_manager_selection_core_rect(
                ShellViewMode::Details,
                item,
                visual,
                icon,
                text,
                true
            ),
            item
        );
    }

    #[test]
    fn icon_selection_core_matches_visual_rect() {
        let item = rect(0.0, 0.0, 120.0, 120.0);
        let visual = rect(8.0, 8.0, 104.0, 100.0);
        let icon = rect(36.0, 8.0, 48.0, 48.0);
        let text = rect(8.0, 64.0, 104.0, 36.0);

        assert_eq!(
            file_manager_selection_core_rect(ShellViewMode::Icons, item, visual, icon, text, false),
            visual
        );
    }
}
