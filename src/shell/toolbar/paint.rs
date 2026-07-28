use crate::windowing::PhysicalSize;
use fika_core::ViewRect;

use crate::shell::options::ShellViewMode;
use crate::shell::pane::ShellPaneId;
use crate::shell::render::quad::{
    QuadVertex, push_clipped_rect_outline, push_clipped_rounded_rect,
    push_clipped_rounded_rect_outline,
};
use crate::shell::theme::ShellTheme;
use crate::vulkan_rect::VulkanRectInstance;
use crate::{ShellScene, shell::toolbar::ShellToolbarViewModeControl};

/// The application toolbar is a small, texture-free scene. Both renderers use
/// this sink so hover/active state and icon geometry have one source of truth.
trait ToolbarRectSink {
    fn fill(&mut self, rect: ViewRect, clip: ViewRect, radius: f32, color: [f32; 4]);

    fn outline(
        &mut self,
        rect: ViewRect,
        clip: ViewRect,
        radius: f32,
        stroke_width: f32,
        color: [f32; 4],
    );
}

struct CpuToolbarRectSink<'a> {
    vertices: &'a mut Vec<QuadVertex>,
    size: PhysicalSize<u32>,
}

impl ToolbarRectSink for CpuToolbarRectSink<'_> {
    fn fill(&mut self, rect: ViewRect, clip: ViewRect, radius: f32, color: [f32; 4]) {
        push_clipped_rounded_rect(self.vertices, rect, clip, radius, color, self.size);
    }

    fn outline(
        &mut self,
        rect: ViewRect,
        clip: ViewRect,
        radius: f32,
        stroke_width: f32,
        color: [f32; 4],
    ) {
        if radius <= 1.0 {
            push_clipped_rect_outline(self.vertices, rect, clip, stroke_width, color, self.size);
        } else {
            push_clipped_rounded_rect_outline(
                self.vertices,
                rect,
                clip,
                radius,
                stroke_width,
                color,
                self.size,
            );
        }
    }
}

struct NativeToolbarRectSink<'a> {
    instances: &'a mut Vec<VulkanRectInstance>,
    size: PhysicalSize<u32>,
}

impl ToolbarRectSink for NativeToolbarRectSink<'_> {
    fn fill(&mut self, rect: ViewRect, clip: ViewRect, radius: f32, color: [f32; 4]) {
        if let Some(instance) = VulkanRectInstance::fill(rect, clip, radius, color, self.size) {
            self.instances.push(instance);
        }
    }

    fn outline(
        &mut self,
        rect: ViewRect,
        clip: ViewRect,
        radius: f32,
        stroke_width: f32,
        color: [f32; 4],
    ) {
        if let Some(instance) =
            VulkanRectInstance::outline(rect, clip, radius, stroke_width, color, self.size)
        {
            self.instances.push(instance);
        }
    }
}

impl ShellScene {
    pub(crate) fn push_app_toolbar(
        &self,
        vertices: &mut Vec<QuadVertex>,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let mut sink = CpuToolbarRectSink { vertices, size };
        self.paint_app_toolbar(&mut sink, size, theme);
    }

    /// Emits the exact same toolbar scene as [`Self::push_app_toolbar`] into
    /// Vulkan analytic-rectangle instances instead of CPU-tessellated quads.
    pub(crate) fn push_native_app_toolbar(
        &self,
        instances: &mut Vec<VulkanRectInstance>,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let mut sink = NativeToolbarRectSink { instances, size };
        self.paint_app_toolbar(&mut sink, size, theme);
    }

    fn paint_app_toolbar<S: ToolbarRectSink>(
        &self,
        sink: &mut S,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let layout = self.app_toolbar_layout(size);
        let toolbar = layout.toolbar;
        sink.fill(
            ViewRect {
                x: toolbar.x,
                y: toolbar.bottom() - self.scale_metric(1.0).max(1.0),
                width: toolbar.width,
                height: self.scale_metric(1.0).max(1.0),
            },
            toolbar,
            0.0,
            theme.divider(),
        );

        let button = layout.places_toggle;
        let split_button = layout.split_view;
        let overflow_button = layout.overflow;
        let view_mode = layout.view_mode;
        let places_hovered = self.pointer.is_some_and(|point| button.contains(point));
        let places_active = self.places_visible || places_hovered;
        let button_colors = theme.toolbar_button(places_active);
        if places_active {
            sink.fill(button, toolbar, self.scale_metric(6.0), button_colors.fill);
        }

        let icon = ViewRect {
            x: button.x + (button.width - self.scale_metric(18.0)) / 2.0,
            y: button.y + (button.height - self.scale_metric(18.0)) / 2.0,
            width: self.scale_metric(18.0),
            height: self.scale_metric(18.0),
        };
        let rail = self.scale_metric(2.0);
        sink.fill(
            ViewRect {
                x: icon.x + self.scale_metric(2.0),
                y: icon.y + self.scale_metric(2.0),
                width: rail,
                height: icon.height - self.scale_metric(4.0),
            },
            toolbar,
            0.0,
            button_colors.icon,
        );
        sink.outline(
            ViewRect {
                x: icon.x + self.scale_metric(1.0),
                y: icon.y + self.scale_metric(3.0),
                width: icon.width - self.scale_metric(2.0),
                height: icon.height - self.scale_metric(6.0),
            },
            toolbar,
            0.0,
            self.scale_metric(1.0),
            button_colors.icon,
        );

        if let Some(control) = view_mode {
            self.paint_toolbar_view_mode_control(sink, control, toolbar, theme);
        }

        let split_open = self.panes.is_open(ShellPaneId::SLOT_1);
        let split_hovered = self
            .pointer
            .is_some_and(|point| split_button.contains(point));
        let split_active = split_open || split_hovered;
        let split_colors = theme.toolbar_button(split_active);
        if split_active {
            sink.fill(
                split_button,
                toolbar,
                self.scale_metric(6.0),
                split_colors.fill,
            );
        }
        let split_icon = ViewRect {
            x: split_button.x + (split_button.width - self.scale_metric(18.0)) / 2.0,
            y: split_button.y + (split_button.height - self.scale_metric(18.0)) / 2.0,
            width: self.scale_metric(18.0),
            height: self.scale_metric(18.0),
        };
        sink.outline(
            ViewRect {
                x: split_icon.x + self.scale_metric(1.0),
                y: split_icon.y + self.scale_metric(2.0),
                width: split_icon.width - self.scale_metric(2.0),
                height: split_icon.height - self.scale_metric(4.0),
            },
            toolbar,
            0.0,
            self.scale_metric(1.0),
            split_colors.icon,
        );
        sink.fill(
            ViewRect {
                x: split_icon.x + split_icon.width / 2.0 - self.scale_metric(0.5),
                y: split_icon.y + self.scale_metric(2.0),
                width: self.scale_metric(1.0),
                height: split_icon.height - self.scale_metric(4.0),
            },
            toolbar,
            0.0,
            split_colors.icon,
        );
        if split_open {
            let close_center_x = if self.active_pane() == ShellPaneId::SLOT_0 {
                split_icon.x + split_icon.width * 0.25
            } else {
                split_icon.x + split_icon.width * 0.75
            };
            sink.fill(
                ViewRect {
                    x: close_center_x - self.scale_metric(3.0),
                    y: split_icon.y + split_icon.height / 2.0 - self.scale_metric(0.5),
                    width: self.scale_metric(6.0),
                    height: self.scale_metric(1.0),
                },
                toolbar,
                0.0,
                split_colors.icon,
            );
        }

        let overflow_hovered = self
            .pointer
            .is_some_and(|point| overflow_button.contains(point));
        let overflow_colors = theme.toolbar_button(overflow_hovered);
        if overflow_hovered {
            sink.fill(
                overflow_button,
                toolbar,
                self.scale_metric(6.0),
                overflow_colors.fill,
            );
        }
        let dot_size = self.scale_metric(3.0);
        let dot_gap = self.scale_metric(3.0);
        let dots_width = dot_size * 3.0 + dot_gap * 2.0;
        let dot_y = overflow_button.y + (overflow_button.height - dot_size) / 2.0;
        let dot_x = overflow_button.x + (overflow_button.width - dots_width) / 2.0;
        for index in 0..3 {
            sink.fill(
                ViewRect {
                    x: dot_x + index as f32 * (dot_size + dot_gap),
                    y: dot_y,
                    width: dot_size,
                    height: dot_size,
                },
                toolbar,
                dot_size / 2.0,
                overflow_colors.icon,
            );
        }
    }

    fn paint_toolbar_view_mode_control<S: ToolbarRectSink>(
        &self,
        sink: &mut S,
        control: ShellToolbarViewModeControl,
        clip: ViewRect,
        theme: ShellTheme,
    ) {
        let rect = control.outer;
        let hovered = self.pointer.is_some_and(|point| rect.contains(point));
        let colors = theme.toolbar_button(hovered);
        if hovered {
            sink.fill(rect, clip, self.scale_metric(7.0), colors.fill);
        }
        for segment in control.segments {
            let segment_hovered = self
                .pointer
                .is_some_and(|point| segment.rect.contains(point));
            let active = segment.mode == self.active_view_mode();
            if active || segment_hovered {
                sink.fill(
                    segment.rect,
                    rect,
                    self.scale_metric(5.0),
                    theme.toolbar_button(active || segment_hovered).fill,
                );
            }
            let glyph_size = self.scale_metric(15.0).min(segment.rect.height).max(1.0);
            let icon_rect = ViewRect {
                x: segment.rect.x + (segment.rect.width - glyph_size) / 2.0,
                y: segment.rect.y + (segment.rect.height - glyph_size) / 2.0,
                width: glyph_size,
                height: glyph_size,
            };
            self.paint_view_mode_glyph(sink, segment.mode, icon_rect, rect, theme);
        }
    }

    fn paint_view_mode_glyph<S: ToolbarRectSink>(
        &self,
        sink: &mut S,
        mode: ShellViewMode,
        rect: ViewRect,
        clip: ViewRect,
        theme: ShellTheme,
    ) {
        let active = mode == self.active_view_mode();
        let color = if active {
            theme.accent()
        } else {
            theme.toolbar_button(false).icon
        };
        match mode {
            ShellViewMode::Icons => {
                let dot = self.scale_metric(4.0).max(2.0);
                let gap = self.scale_metric(3.0).max(1.0);
                let content_size = dot * 2.0 + gap;
                let origin_x = rect.x + (rect.width - content_size) / 2.0;
                let origin_y = rect.y + (rect.height - content_size) / 2.0;
                for row in 0..2 {
                    for column in 0..2 {
                        sink.fill(
                            ViewRect {
                                x: origin_x + column as f32 * (dot + gap),
                                y: origin_y + row as f32 * (dot + gap),
                                width: dot,
                                height: dot,
                            },
                            clip,
                            dot / 2.0,
                            color,
                        );
                    }
                }
            }
            ShellViewMode::Compact => {
                let row_height = self.scale_metric(3.0).max(2.0);
                let row_step = self.scale_metric(5.0);
                let content_height = row_height + row_step * 2.0;
                let content_y = rect.y + (rect.height - content_height) / 2.0;
                let marker_width = self.scale_metric(5.0).min(rect.width).max(1.0);
                let marker_gap = self
                    .scale_metric(3.0)
                    .min((rect.width - marker_width).max(0.0));
                let line_x = rect.x + marker_width + marker_gap;
                let line_width = (rect.right() - line_x).max(1.0);
                for row in 0..3 {
                    let y = content_y + row as f32 * row_step;
                    sink.fill(
                        ViewRect {
                            x: rect.x,
                            y,
                            width: marker_width,
                            height: row_height,
                        },
                        clip,
                        self.scale_metric(1.5),
                        color,
                    );
                    sink.fill(
                        ViewRect {
                            x: line_x,
                            y,
                            width: line_width,
                            height: row_height,
                        },
                        clip,
                        self.scale_metric(1.5),
                        if active {
                            theme.field_separator()
                        } else {
                            color
                        },
                    );
                }
            }
            ShellViewMode::Details => {
                let row_height = self.scale_metric(3.0).max(2.0);
                let row_step = self.scale_metric(5.0);
                let content_height = row_height + row_step * 2.0;
                let content_y = rect.y + (rect.height - content_height) / 2.0;
                for row in 0..3 {
                    sink.fill(
                        ViewRect {
                            x: rect.x,
                            y: content_y + row as f32 * row_step,
                            width: rect.width,
                            height: row_height,
                        },
                        clip,
                        self.scale_metric(1.5),
                        if row == 0 || active {
                            color
                        } else {
                            theme.field_separator()
                        },
                    );
                }
            }
        }
    }
}
