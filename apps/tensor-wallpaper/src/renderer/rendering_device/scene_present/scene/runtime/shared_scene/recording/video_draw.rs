//! Exact scene-video pipeline binding and indexed draw recording.

use vulkan_renderer::{Extent2D, RenderingEncoder};

use super::super::super::draw_recording::SceneGpuDrawCommand;
use super::super::super::shared_resources::SharedSceneFrameResources;
use super::super::SharedSceneGpuResources;

impl SharedSceneGpuResources {
    pub(super) fn record_video_draw(
        &self,
        rendering: &mut RenderingEncoder<'_>,
        frame: &SharedSceneFrameResources,
        draw: &SceneGpuDrawCommand,
        extent: Extent2D,
    ) -> Result<(), String> {
        let media_instance = draw
            .video_media_instance
            .ok_or_else(|| "scene-video recording received a non-video draw".to_owned())?;
        let pipeline = self.pipelines.video.as_ref().ok_or_else(|| {
            format!("scene-video media instance {media_instance} has no exact pipeline")
        })?;
        let vertex = frame.video_vertex.as_ref().ok_or_else(|| {
            format!("scene-video media instance {media_instance} has no frame vertex buffer")
        })?;
        let byte_offset = draw.video_vertex_byte_offset.ok_or_else(|| {
            format!("scene-video media instance {media_instance} has no vertex offset")
        })?;
        if !byte_offset.is_multiple_of(u64::from(
            super::super::super::SCENE_VIDEO_VERTEX_STRIDE_BYTES,
        )) {
            return Err(format!(
                "scene-video media instance {media_instance} vertex offset is misaligned"
            ));
        }
        let push = video_descriptor_push(self, draw, extent)?;
        rendering
            .bind_machine_code_pipeline(&pipeline.pipeline)
            .map_err(|error| format!("bind scene-video pipeline: {error}"))?;
        unsafe {
            rendering
                .set_vertex_buffer(0, vertex, byte_offset)
                .map_err(|error| format!("bind scene-video vertices: {error}"))?;
        }
        rendering
            .push_data(0, &push)
            .map_err(|error| format!("push scene-video descriptor indices: {error}"))?;
        rendering
            .set_scissor(super::scene_scissor(draw.scissor, extent))
            .map_err(|error| format!("set scene-video scissor: {error}"))?;
        let index_end = draw
            .first_index
            .checked_add(draw.index_count)
            .ok_or_else(|| "scene-video index range overflows".to_owned())?;
        unsafe {
            rendering
                .draw_indexed(draw.first_index..index_end, 0, 0..1)
                .map_err(|error| format!("record scene-video indexed draw: {error}"))?;
        }
        Ok(())
    }
}

fn video_descriptor_push(
    scene: &SharedSceneGpuResources,
    draw: &SceneGpuDrawCommand,
    extent: Extent2D,
) -> Result<[u8; 24], String> {
    let indices = video_descriptor_indices(
        &scene.descriptor_layout.sampled_slots,
        draw.sampled_resource_descriptor_base,
        draw.sampler_descriptor_base,
    )?;
    let mut push = [0; 24];
    push[0..4].copy_from_slice(&(extent.width as f32).to_ne_bytes());
    push[4..8].copy_from_slice(&(extent.height as f32).to_ne_bytes());
    for (index, value) in indices.into_iter().enumerate() {
        let offset = 8 + index * 4;
        push[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
    }
    Ok(push)
}

fn video_descriptor_indices(
    sampled_slots: &[u32],
    resource_base: usize,
    sampler_base: usize,
) -> Result<[u32; 4], String> {
    let y = sampled_slots
        .iter()
        .position(|slot| *slot == 0)
        .ok_or_else(|| "scene-video descriptor layout is missing Y slot 0".to_owned())?;
    let uv = sampled_slots
        .iter()
        .position(|slot| *slot == 1)
        .ok_or_else(|| "scene-video descriptor layout is missing UV slot 1".to_owned())?;
    let word = |value: usize, label: &str| {
        u32::try_from(value).map_err(|_| format!("scene-video {label} heap index exceeds u32"))
    };
    Ok([
        word(resource_base + y, "Y image")?,
        word(resource_base + uv, "UV image")?,
        word(sampler_base + y, "Y sampler")?,
        word(sampler_base + uv, "UV sampler")?,
    ])
}

#[cfg(test)]
mod tests {
    use super::video_descriptor_indices;

    #[test]
    fn video_push_uses_fixed_plane_registers_and_absolute_dense_indices() {
        assert_eq!(
            video_descriptor_indices(&[0, 1, 4], 12, 6).unwrap(),
            [12, 13, 6, 7]
        );
        assert!(video_descriptor_indices(&[0, 4], 12, 6).is_err());
    }
}
