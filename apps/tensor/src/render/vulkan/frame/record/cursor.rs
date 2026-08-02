use vulkan_renderer::Rect2D;

use crate::render::{FrameSubmission, cursor::MAX_CURSOR_OVERLAYS};

use super::{
    CursorPushData, DrawPushData, FrameRecordError, PreparedDraw, descriptor_index,
    destination_to_ndc, validate_viewport,
};

#[derive(Clone, Copy, Debug)]
pub(in crate::render::vulkan::frame) enum PreparedCursorDraw {
    Vector {
        push: CursorPushData,
        scissor: Rect2D,
    },
    Texture(PreparedDraw),
}

pub(in crate::render::vulkan::frame) struct PreparedCursorDraws {
    entries: [Option<PreparedCursorDraw>; MAX_CURSOR_OVERLAYS],
    len: usize,
}

impl PreparedCursorDraws {
    fn new() -> Self {
        Self {
            entries: [None; MAX_CURSOR_OVERLAYS],
            len: 0,
        }
    }

    pub(in crate::render::vulkan::frame) fn has_vectors(&self) -> bool {
        self.iter()
            .any(|draw| matches!(draw, PreparedCursorDraw::Vector { .. }))
    }

    pub(in crate::render::vulkan::frame) fn has_textures(&self) -> bool {
        self.iter()
            .any(|draw| matches!(draw, PreparedCursorDraw::Texture(_)))
    }

    fn push(&mut self, draw: PreparedCursorDraw) {
        self.entries[self.len] = Some(draw);
        self.len += 1;
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = PreparedCursorDraw> + '_ {
        self.entries[..self.len].iter().flatten().copied()
    }
}

pub(in crate::render::vulkan::frame) fn prepare_cursor_draws(
    frame: &FrameSubmission,
    descriptor_stride: u64,
    sampler_index: u32,
) -> Result<PreparedCursorDraws, FrameRecordError> {
    let mut draws = PreparedCursorDraws::new();
    if frame.draw_plan.cursors().is_empty() {
        return Ok(draws);
    }
    let viewport = validate_viewport(frame.target.viewport)?;
    for (index, cursor) in frame.draw_plan.cursor_batch().draw_order() {
        let image_descriptor = &frame.draw_plan.cursor_image_descriptors()[index];
        let scissor = Rect2D::new(
            cursor.clip.x,
            cursor.clip.y,
            cursor.clip.width,
            cursor.clip.height,
        );
        if let (Some(texture), Some(image_descriptor)) = (cursor.texture, image_descriptor) {
            let descriptor_index =
                descriptor_index(frame.descriptors, descriptor_stride, *image_descriptor)?;
            let origin = texture.sample_transform.origin();
            let axis_x = texture.sample_transform.axis_x();
            let axis_y = texture.sample_transform.axis_y();
            draws.push(PreparedCursorDraw::Texture(PreparedDraw {
                push: DrawPushData {
                    descriptor_index,
                    corner_radius: 0,
                    opacity: 1.0,
                    sampler_index,
                    destination: destination_to_ndc(cursor.destination, viewport),
                    uv_origin_axis_x: [origin.0, origin.1, axis_x.0, axis_x.1],
                    uv_axis_y_surface_size: [
                        axis_y.0,
                        axis_y.1,
                        cursor.destination.width as f32,
                        cursor.destination.height as f32,
                    ],
                },
                scissor,
            }));
        } else {
            draws.push(PreparedCursorDraw::Vector {
                push: CursorPushData {
                    destination: destination_to_ndc(cursor.destination, viewport),
                },
                scissor,
            });
        }
    }
    Ok(draws)
}
