use vulkan_renderer::{
    BarrierBatch, ColorImageCopy, CommandEncoder, Error, Image, Origin2D, RenderGraphImageState,
    ResourceBinding, ResourceState,
};

use crate::render::OutputCaptureRequest;

use super::{
    NativeOutputImageInfo, append_image_transition, color_attachment_state,
    transfer_destination_state, transfer_source_state,
};

#[derive(Clone, Copy)]
pub(in crate::render::vulkan::frame) struct CaptureRecord<'a> {
    pub(in crate::render::vulkan::frame) request: OutputCaptureRequest,
    pub(in crate::render::vulkan::frame) image: &'a Image,
}

pub(in crate::render::vulkan::frame) unsafe fn record_capture_tap(
    encoder: &mut CommandEncoder,
    output: &NativeOutputImageInfo,
    capture: CaptureRecord<'_>,
    queue_family: u32,
    barriers: &mut BarrierBatch,
) -> Result<(), Error> {
    barriers.clear();
    append_image_transition(
        barriers,
        output.image.resource_binding(),
        color_attachment_state(queue_family),
        transfer_source_state(queue_family),
    )?;
    append_image_transition(
        barriers,
        ResourceBinding::whole_color_image(capture.image),
        ResourceState::image(RenderGraphImageState::Undefined, queue_family),
        transfer_destination_state(queue_family),
    )?;
    unsafe { encoder.pipeline_barrier(barriers) };
    unsafe {
        encoder.copy_exported_color_image_to_image(
            &output.image,
            capture.image,
            &[ColorImageCopy {
                source_mip_level: 0,
                source_base_array_layer: 0,
                source_origin: Origin2D::new(capture.request.region.x, capture.request.region.y),
                destination_mip_level: 0,
                destination_base_array_layer: 0,
                destination_origin: Origin2D::new(0, 0),
                extent: vulkan_renderer::Extent2D::new(
                    capture.request.region.width,
                    capture.request.region.height,
                ),
                layer_count: 1,
            }],
        )?;
    }
    Ok(())
}
