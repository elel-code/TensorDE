use std::borrow::Cow;

use crate::windowing::PhysicalSize;
use bytemuck::{Pod, Zeroable};
#[cfg(test)]
use fika_core::ViewPoint;
use fika_core::ViewRect;

use crate::ui::render::gpu::{
    VertexBufferUploadStats, create_vertex_buffer, upload_vertex_buffer_if_dirty,
};
use crate::ui::render::shaders::QUAD_SHADER;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct QuadVertex {
    pub(crate) position: [f32; 2],
    pub(crate) color: [f32; 4],
}

impl QuadVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

pub(crate) struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_count: usize,
    last_vertices_hash: Option<u64>,
}

impl QuadRenderer {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_blend(device, format, wgpu::BlendState::ALPHA_BLENDING)
    }

    pub(crate) fn new_transparent_clear(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let mut renderer = Self::new_with_blend(device, format, wgpu::BlendState::REPLACE);
        renderer.upload(device, queue, &transparent_fullscreen_quad());
        renderer
    }

    fn new_with_blend(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        blend: wgpu::BlendState,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fika-wgpu-quad-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(QUAD_SHADER)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fika-wgpu-quad-layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fika-wgpu-quad-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(QuadVertex::layout())],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertex_capacity = 6;
        let vertex_buffer = create_vertex_buffer(device, vertex_capacity);
        Self {
            pipeline,
            vertex_buffer,
            vertex_capacity,
            vertex_count: 0,
            last_vertices_hash: None,
        }
    }

    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[QuadVertex],
    ) -> VertexBufferUploadStats {
        if vertices.len() > self.vertex_capacity {
            self.vertex_capacity = vertices.len().next_power_of_two();
            self.vertex_buffer = create_vertex_buffer(device, self.vertex_capacity);
            self.last_vertices_hash = None;
        }

        self.vertex_count = vertices.len();
        upload_vertex_buffer_if_dirty(
            queue,
            &self.vertex_buffer,
            vertices,
            &mut self.last_vertices_hash,
        )
    }

    pub(crate) fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count as u32, 0..1);
    }

    pub(crate) fn batch_count(&self) -> usize {
        usize::from(self.vertex_count > 0)
    }
}

fn transparent_fullscreen_quad() -> [QuadVertex; 6] {
    let color = [0.0; 4];
    [
        QuadVertex {
            position: [-1.0, 1.0],
            color,
        },
        QuadVertex {
            position: [-1.0, -1.0],
            color,
        },
        QuadVertex {
            position: [1.0, -1.0],
            color,
        },
        QuadVertex {
            position: [-1.0, 1.0],
            color,
        },
        QuadVertex {
            position: [1.0, -1.0],
            color,
        },
        QuadVertex {
            position: [1.0, 1.0],
            color,
        },
    ]
}

pub(crate) fn push_clipped_rect(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    if let Some(rect) = intersect_rect(rect, clip) {
        push_rect(vertices, rect, color, size);
    }
}

pub(crate) fn push_clipped_rounded_rect(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    radius: f32,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 || color[3] <= 0.0 {
        return;
    }
    let radius = radius.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0);
    if radius <= 1.0 {
        push_clipped_rect(vertices, rect, clip, color, size);
        return;
    }

    let middle_height = (rect.height - radius * 2.0).max(0.0);
    if middle_height > 0.0 {
        push_clipped_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y + radius,
                width: rect.width,
                height: middle_height,
            },
            clip,
            color,
            size,
        );
    }

    let steps = radius.ceil().clamp(4.0, 16.0) as usize;
    let step_height = radius / steps as f32;
    for step in 0..steps {
        let y = rect.y + step as f32 * step_height;
        let midpoint_y = y + step_height / 2.0;
        let dy = rect.y + radius - midpoint_y;
        let inset = radius - (radius * radius - dy * dy).max(0.0).sqrt();
        let strip_width = rect.width - inset * 2.0;
        if strip_width <= 0.0 {
            continue;
        }
        let top = ViewRect {
            x: rect.x + inset,
            y,
            width: strip_width,
            height: step_height,
        };
        let bottom = ViewRect {
            x: rect.x + inset,
            y: rect.bottom() - (step + 1) as f32 * step_height,
            width: strip_width,
            height: step_height,
        };
        push_clipped_rect(vertices, top, clip, color, size);
        push_clipped_rect(vertices, bottom, clip, color, size);
    }
}

pub(crate) fn push_clipped_rect_outline(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    thickness: f32,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    let thickness = thickness.max(1.0).min(rect.width.min(rect.height) / 2.0);
    let top = ViewRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: thickness,
    };
    let bottom = ViewRect {
        x: rect.x,
        y: rect.bottom() - thickness,
        width: rect.width,
        height: thickness,
    };
    let left = ViewRect {
        x: rect.x,
        y: rect.y + thickness,
        width: thickness,
        height: (rect.height - thickness * 2.0).max(0.0),
    };
    let right = ViewRect {
        x: rect.right() - thickness,
        y: rect.y + thickness,
        width: thickness,
        height: (rect.height - thickness * 2.0).max(0.0),
    };
    push_clipped_rect(vertices, top, clip, color, size);
    push_clipped_rect(vertices, bottom, clip, color, size);
    push_clipped_rect(vertices, left, clip, color, size);
    push_clipped_rect(vertices, right, clip, color, size);
}

pub(crate) fn push_clipped_rounded_rect_outline(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    radius: f32,
    thickness: f32,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 || color[3] <= 0.0 {
        return;
    }
    let thickness = thickness.max(1.0).min(rect.width.min(rect.height) / 2.0);
    let radius = radius.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0);
    if radius <= 1.0 || radius <= thickness {
        push_clipped_rect_outline(vertices, rect, clip, thickness, color, size);
        return;
    }

    let center_width = (rect.width - radius * 2.0).max(0.0);
    if center_width > 0.0 {
        push_clipped_rect(
            vertices,
            ViewRect {
                x: rect.x + radius,
                y: rect.y,
                width: center_width,
                height: thickness,
            },
            clip,
            color,
            size,
        );
        push_clipped_rect(
            vertices,
            ViewRect {
                x: rect.x + radius,
                y: rect.bottom() - thickness,
                width: center_width,
                height: thickness,
            },
            clip,
            color,
            size,
        );
    }

    let center_height = (rect.height - radius * 2.0).max(0.0);
    if center_height > 0.0 {
        push_clipped_rect(
            vertices,
            ViewRect {
                x: rect.x,
                y: rect.y + radius,
                width: thickness,
                height: center_height,
            },
            clip,
            color,
            size,
        );
        push_clipped_rect(
            vertices,
            ViewRect {
                x: rect.right() - thickness,
                y: rect.y + radius,
                width: thickness,
                height: center_height,
            },
            clip,
            color,
            size,
        );
    }

    let steps = radius.ceil().clamp(4.0, 16.0) as usize;
    let step_height = radius / steps as f32;
    let inner_radius = (radius - thickness).max(0.0);
    let left_center_x = rect.x + radius;
    let right_center_x = rect.right() - radius;
    for step in 0..steps {
        let top_y = rect.y + step as f32 * step_height;
        let bottom_y = rect.bottom() - (step + 1) as f32 * step_height;
        for y in [top_y, bottom_y] {
            let midpoint_y = y + step_height / 2.0;
            let center_y = if midpoint_y < rect.y + radius {
                rect.y + radius
            } else {
                rect.bottom() - radius
            };
            let dy = (center_y - midpoint_y).abs();
            let outer_dx = (radius * radius - dy * dy).max(0.0).sqrt();
            let outer_left = left_center_x - outer_dx;
            let outer_right = right_center_x + outer_dx;
            let (inner_left, inner_right) = if inner_radius > 0.0 && dy <= inner_radius {
                let inner_dx = (inner_radius * inner_radius - dy * dy).max(0.0).sqrt();
                (left_center_x - inner_dx, right_center_x + inner_dx)
            } else {
                (left_center_x, right_center_x)
            };
            let left_width = (inner_left - outer_left).max(0.0);
            if left_width > 0.0 {
                push_clipped_rect(
                    vertices,
                    ViewRect {
                        x: outer_left,
                        y,
                        width: left_width,
                        height: step_height,
                    },
                    clip,
                    color,
                    size,
                );
            }
            let right_width = (outer_right - inner_right).max(0.0);
            if right_width > 0.0 {
                push_clipped_rect(
                    vertices,
                    ViewRect {
                        x: inner_right,
                        y,
                        width: right_width,
                        height: step_height,
                    },
                    clip,
                    color,
                    size,
                );
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RoundedHighlightStyle {
    pub(crate) fill: [f32; 4],
    pub(crate) border: [f32; 4],
    pub(crate) border_width: f32,
}

pub(crate) fn push_clipped_rounded_highlight(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    radius: f32,
    style: RoundedHighlightStyle,
    size: PhysicalSize<u32>,
) {
    if style.fill[3] > 0.0 {
        push_clipped_rounded_rect(vertices, rect, clip, radius, style.fill, size);
    }
    push_clipped_rounded_rect_outline(
        vertices,
        rect,
        clip,
        radius,
        style.border_width,
        style.border,
        size,
    );
}

pub(crate) fn push_rect(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let left = rect.x / width * 2.0 - 1.0;
    let right = rect.right() / width * 2.0 - 1.0;
    let top = 1.0 - rect.y / height * 2.0;
    let bottom = 1.0 - rect.bottom() / height * 2.0;

    vertices.extend_from_slice(&[
        QuadVertex {
            position: [left, top],
            color,
        },
        QuadVertex {
            position: [left, bottom],
            color,
        },
        QuadVertex {
            position: [right, bottom],
            color,
        },
        QuadVertex {
            position: [left, top],
            color,
        },
        QuadVertex {
            position: [right, bottom],
            color,
        },
        QuadVertex {
            position: [right, top],
            color,
        },
    ]);
}

fn intersect_rect(rect: ViewRect, clip: ViewRect) -> Option<ViewRect> {
    let x = rect.x.max(clip.x);
    let y = rect.y.max(clip.y);
    let right = rect.right().min(clip.right());
    let bottom = rect.bottom().min(clip.bottom());
    (right > x && bottom > y).then_some(ViewRect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_clear_quad_covers_the_full_target_without_color() {
        let vertices = transparent_fullscreen_quad();

        assert!(vertices.iter().all(|vertex| vertex.color == [0.0; 4]));
        assert_eq!(
            quad_screen_rect(&vertices, PhysicalSize::new(120, 80)),
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 80.0,
            }
        );
    }

    #[test]
    fn rounded_highlight_with_transparent_fill_paints_only_outline() {
        let size = PhysicalSize::new(120, 80);
        let rect = ViewRect {
            x: 20.0,
            y: 20.0,
            width: 80.0,
            height: 36.0,
        };
        let center = ViewPoint { x: 60.0, y: 38.0 };
        let border = [0.2, 0.3, 0.4, 0.8];
        let mut vertices = Vec::new();

        push_clipped_rounded_highlight(
            &mut vertices,
            rect,
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 80.0,
            },
            5.0,
            RoundedHighlightStyle {
                fill: [0.0, 0.0, 0.0, 0.0],
                border,
                border_width: 1.25,
            },
            size,
        );

        assert!(!vertices.is_empty());
        assert!(!quad_vertices_contain_point_with_color(
            &vertices, size, center, border
        ));
    }

    #[test]
    fn rounded_highlight_with_fill_keeps_center_fill() {
        let size = PhysicalSize::new(120, 80);
        let rect = ViewRect {
            x: 20.0,
            y: 20.0,
            width: 80.0,
            height: 36.0,
        };
        let center = ViewPoint { x: 60.0, y: 38.0 };
        let fill = [0.8, 0.7, 0.2, 0.5];
        let border = [0.2, 0.3, 0.4, 0.8];
        let mut vertices = Vec::new();

        push_clipped_rounded_highlight(
            &mut vertices,
            rect,
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 80.0,
            },
            5.0,
            RoundedHighlightStyle {
                fill,
                border,
                border_width: 1.25,
            },
            size,
        );

        assert!(quad_vertices_contain_point_with_color(
            &vertices, size, center, fill
        ));
        assert!(!quad_vertices_contain_point_with_color(
            &vertices, size, center, border
        ));
    }

    fn quad_vertices_contain_point_with_color(
        vertices: &[QuadVertex],
        size: PhysicalSize<u32>,
        point: ViewPoint,
        color: [f32; 4],
    ) -> bool {
        vertices
            .chunks_exact(6)
            .any(|quad| quad[0].color == color && quad_screen_rect(quad, size).contains(point))
    }

    fn quad_screen_rect(quad: &[QuadVertex], size: PhysicalSize<u32>) -> ViewRect {
        let min_x = quad
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = quad
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = quad
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::INFINITY, f32::min);
        let max_y = quad
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let width = size.width.max(1) as f32;
        let height = size.height.max(1) as f32;
        let left = (min_x + 1.0) * width / 2.0;
        let right = (max_x + 1.0) * width / 2.0;
        let top = (1.0 - max_y) * height / 2.0;
        let bottom = (1.0 - min_y) * height / 2.0;
        ViewRect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }
}
