use crate::windowing::PhysicalSize;
use bytemuck::{Pod, Zeroable};
use tensor_files_core::ViewRect;

use super::coordinates::rect_to_vulkan_ndc;

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
    let [left, top, right, bottom] = rect_to_vulkan_ndc(rect, size);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textured_rect_keeps_top_left_uv_on_vulkan_top_left_vertex() {
        let mut vertices = Vec::new();
        push_textured_rect(
            &mut vertices,
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 50.0,
            },
            AtlasRect {
                x: 10.0,
                y: 20.0,
                width: 30.0,
                height: 40.0,
            },
            100,
            100,
            PhysicalSize::new(100, 100),
            [1.0; 4],
        );

        assert_eq!(vertices[0].position, [-1.0, -1.0]);
        assert_eq!(vertices[0].uv, [0.1, 0.2]);
        assert_eq!(vertices[1].position, [-1.0, 0.0]);
        assert_eq!(vertices[1].uv, [0.1, 0.6]);
    }
}
