use std::{mem, slice};

use vulkan_renderer::{
    ColorAttachment, CommandEncoder, Error as RendererError, GraphicsPipeline, LoadOp, Rect2D,
    RenderingDescriptor, ResolveMode, StoreOp, TextureLayout, Viewport,
};

use crate::render::vulkan::target::NativeOutputImageInfo;
use crate::render::{FrameSubmission, cursor::MAX_CURSOR_OVERLAYS};

use super::{
    CursorPushData, DrawPushData, FrameRecordError, PreparedDraw, descriptor_index,
    destination_to_ndc, draw_over_damage, has_render_damage, validate_viewport,
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

    pub(super) fn is_empty(&self) -> bool {
        self.len == 0
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
                view_id: None,
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
                color: None,
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

pub(in crate::render::vulkan::frame) fn record_cursor_draws(
    rendering: &mut vulkan_renderer::RenderingEncoder<'_>,
    frame: &FrameSubmission,
    cursors: &PreparedCursorDraws,
    client_pipeline: Option<&GraphicsPipeline>,
    cursor_pipeline: Option<&GraphicsPipeline>,
) -> Result<(), RendererError> {
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum BoundPipeline {
        Client,
        Cursor,
    }

    let mut bound = None;
    for cursor in cursors.iter() {
        match cursor {
            PreparedCursorDraw::Vector { push, scissor } => {
                let Some(pipeline) = cursor_pipeline else {
                    continue;
                };
                if !has_render_damage(scissor, frame) {
                    continue;
                }
                if bound != Some(BoundPipeline::Cursor) {
                    rendering.bind_pipeline(pipeline)?;
                    bound = Some(BoundPipeline::Cursor);
                }
                push_struct(rendering, &push)?;
                draw_over_damage(rendering, scissor, frame)?;
            }
            PreparedCursorDraw::Texture(draw) => {
                let Some(pipeline) = client_pipeline else {
                    continue;
                };
                if !has_render_damage(draw.scissor, frame) {
                    continue;
                }
                if bound != Some(BoundPipeline::Client) {
                    rendering.bind_pipeline(pipeline)?;
                    bound = Some(BoundPipeline::Client);
                }
                push_struct(rendering, &draw.push)?;
                draw_over_damage(rendering, draw.scissor, frame)?;
            }
        }
    }
    Ok(())
}

pub(in crate::render::vulkan::frame) fn record_cursor_scope(
    encoder: &mut CommandEncoder,
    frame: &FrameSubmission,
    output: &NativeOutputImageInfo,
    cursors: &PreparedCursorDraws,
    client_pipeline: Option<&GraphicsPipeline>,
    cursor_pipeline: Option<&GraphicsPipeline>,
) -> Result<(), RendererError> {
    if cursors.is_empty() {
        return Ok(());
    }
    let attachments = [Some(ColorAttachment {
        view: output.image.as_attachment(),
        layout: TextureLayout::ColorAttachment,
        resolve_target: None,
        resolve_layout: TextureLayout::Undefined,
        resolve_mode: ResolveMode::None,
        load_op: LoadOp::Load,
        store_op: StoreOp::Store,
    })];
    let descriptor = RenderingDescriptor {
        label: Some("tensor-post-capture-cursors"),
        render_area: Rect2D::new(
            0,
            0,
            frame.target.viewport.width,
            frame.target.viewport.height,
        ),
        layer_count: 1,
        view_mask: 0,
        color_attachments: &attachments,
        depth_attachment: None,
        stencil_attachment: None,
        multisampled_render_to_single_sampled: None,
    };
    let mut rendering = unsafe { encoder.begin_rendering(&descriptor)? };
    rendering.set_viewport(Viewport {
        x: 0.0,
        y: 0.0,
        width: frame.target.viewport.width as f32,
        height: frame.target.viewport.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    })?;
    record_cursor_draws(
        &mut rendering,
        frame,
        cursors,
        client_pipeline,
        cursor_pipeline,
    )?;
    rendering.end();
    Ok(())
}

fn push_struct<T: Copy>(
    rendering: &mut vulkan_renderer::RenderingEncoder<'_>,
    value: &T,
) -> Result<(), RendererError> {
    let bytes =
        unsafe { slice::from_raw_parts((value as *const T).cast::<u8>(), mem::size_of::<T>()) };
    rendering.push_data(0, bytes)
}
