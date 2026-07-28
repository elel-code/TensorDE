use crate::windowing::PhysicalSize;
use bytemuck::{Pod, Zeroable};
use fika_core::ViewRect;

#[derive(Clone, Copy, Debug)]
pub(crate) struct AtlasRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct TextVertex {
    pub(crate) position: [f32; 2],
    pub(crate) uv: [f32; 2],
    pub(crate) color: [f32; 4],
}

impl TextVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

pub(crate) fn push_textured_rect(
    vertices: &mut Vec<TextVertex>,
    rect: ViewRect,
    atlas: AtlasRect,
    atlas_width: u32,
    atlas_height: u32,
    size: PhysicalSize<u32>,
    color: [f32; 4],
) {
    if rect.width <= 0.0 || rect.height <= 0.0 || atlas.width <= 0.0 || atlas.height <= 0.0 {
        return;
    }
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let left = rect.x / width * 2.0 - 1.0;
    let right = rect.right() / width * 2.0 - 1.0;
    let top = 1.0 - rect.y / height * 2.0;
    let bottom = 1.0 - rect.bottom() / height * 2.0;

    let atlas_width = atlas_width.max(1) as f32;
    let atlas_height = atlas_height.max(1) as f32;
    let u0 = atlas.x / atlas_width;
    let v0 = atlas.y / atlas_height;
    let u1 = (atlas.x + atlas.width) / atlas_width;
    let v1 = (atlas.y + atlas.height) / atlas_height;

    vertices.extend_from_slice(&[
        TextVertex {
            position: [left, top],
            uv: [u0, v0],
            color,
        },
        TextVertex {
            position: [left, bottom],
            uv: [u0, v1],
            color,
        },
        TextVertex {
            position: [right, bottom],
            uv: [u1, v1],
            color,
        },
        TextVertex {
            position: [left, top],
            uv: [u0, v0],
            color,
        },
        TextVertex {
            position: [right, bottom],
            uv: [u1, v1],
            color,
        },
        TextVertex {
            position: [right, top],
            uv: [u1, v0],
            color,
        },
    ]);
}
