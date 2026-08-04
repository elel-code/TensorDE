use super::*;

pub(super) unsafe fn record_bound_draw(
    rendering: &mut RenderingEncoder<'_>,
    draw: &super::super::super::draw_recording::SceneGpuDrawCommand,
    particle_indirect: Option<&vulkan_renderer::Buffer>,
) -> Result<(), String> {
    if draw.dynamic_text {
        let instance_end = draw
            .first_instance
            .checked_add(draw.instance_count)
            .ok_or_else(|| "shared dynamic-text instance range overflows".to_owned())?;
        return unsafe { rendering.draw(0..6, draw.first_instance..instance_end) }
            .map_err(|error| format!("record shared dynamic-text draw: {error}"));
    }
    match draw.primitive {
        SceneRenderingDeviceDrawPrimitive::ObjectMesh => {
            let index_end = draw
                .first_index
                .checked_add(draw.index_count)
                .ok_or_else(|| "shared indexed draw range overflows".to_owned())?;
            unsafe { rendering.draw_indexed(draw.first_index..index_end, draw.vertex_offset, 0..1) }
                .map_err(|error| format!("record shared indexed scene draw: {error}"))
        }
        SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
        | SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad
        | SceneRenderingDeviceDrawPrimitive::ParticleBillboard => {
            if let Some(index) = draw.particle_indirect_index {
                let buffer = particle_indirect.ok_or_else(|| {
                    "shared particle draw has no retained indirect buffer".to_owned()
                })?;
                let stride =
                    std::mem::size_of::<crate::engine::scene::SceneParticleIndirectDraw>() as u64;
                let offset = u64::from(index)
                    .checked_mul(stride)
                    .ok_or_else(|| "shared particle indirect offset overflows".to_owned())?;
                return unsafe { rendering.draw_indirect(buffer, offset, 1, stride as u32) }
                    .map_err(|error| format!("record shared particle indirect draw: {error}"));
            }
            unsafe { rendering.draw(0..draw.vertex_count, 0..draw.instance_count) }
                .map_err(|error| format!("record shared non-indexed scene draw: {error}"))
        }
    }
}

pub(super) fn scene_scissor(scissor: Option<SceneGpuScissor>, extent: Extent2D) -> Rect2D {
    scissor.map_or_else(
        || Rect2D::new(0, 0, extent.width, extent.height),
        |scissor| {
            Rect2D::new(
                scissor.offset[0],
                scissor.offset[1],
                scissor.extent[0],
                scissor.extent[1],
            )
        },
    )
}
