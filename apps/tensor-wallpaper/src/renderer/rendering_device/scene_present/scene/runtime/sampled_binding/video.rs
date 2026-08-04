//! Explicit graph-to-draw video semantics.

use crate::engine::scene::{
    SceneRenderBindingKind, SceneRenderingDeviceGraphPlan, SceneRenderingDeviceImageAccess,
    SceneRenderingDeviceSampledBinding,
};

use super::super::draw_recording::SceneGpuDrawCommand;

pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn apply_scene_video_draw_semantics(
    graph: &SceneRenderingDeviceGraphPlan,
    commands: &mut [SceneGpuDrawCommand],
) -> Result<(), String> {
    if commands.len() != graph.mesh_draws.len() {
        return Err(format!(
            "scene video semantic draw count {} differs from graph draw count {}",
            commands.len(),
            graph.mesh_draws.len()
        ));
    }
    let media_instances = video_draw_media_instances(commands.len(), &graph.sampled_bindings)?;
    let mut vertex_byte_offset = 0u64;
    for ((command, draw), media_instance) in commands
        .iter_mut()
        .zip(&graph.mesh_draws)
        .zip(media_instances)
    {
        command.video_media_instance = media_instance;
        command.video_vertex_byte_offset = if media_instance.is_some() {
            if draw.primitive != crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh
                || draw.vertex_count == 0
                || draw.index_count == 0
            {
                return Err(
                    "scene-video draw requires a non-empty indexed ObjectMesh primitive".into(),
                );
            }
            if draw.skinning_palette_count != 0 {
                return Err("scene-video draw does not support a skinned vertex stream".into());
            }
            let offset = vertex_byte_offset;
            let byte_count = u64::from(draw.vertex_count)
                .checked_mul(u64::from(super::super::SCENE_VIDEO_VERTEX_STRIDE_BYTES))
                .ok_or_else(|| "scene-video vertex byte count overflows".to_owned())?;
            vertex_byte_offset = vertex_byte_offset
                .checked_add(byte_count)
                .ok_or_else(|| "scene-video vertex arena size overflows".to_owned())?;
            Some(offset)
        } else {
            None
        };
    }
    Ok(())
}

fn video_draw_media_instances(
    draw_count: usize,
    bindings: &[SceneRenderingDeviceSampledBinding],
) -> Result<Vec<Option<u32>>, String> {
    let mut instances = vec![None; draw_count];
    for binding in bindings
        .iter()
        .filter(|binding| binding.kind == SceneRenderBindingKind::VideoFrame)
    {
        if binding.access != SceneRenderingDeviceImageAccess::SampledImage {
            return Err(format!(
                "scene video media instance {} must use sampled-image access",
                binding.slot
            ));
        }
        if binding.mesh_draw_count == 0 {
            return Err(format!(
                "scene video media instance {} has an empty draw range",
                binding.slot
            ));
        }
        let start = binding.mesh_draw_start as usize;
        let end = start
            .checked_add(binding.mesh_draw_count as usize)
            .ok_or_else(|| "scene video draw range overflows".to_owned())?;
        let targets = instances.get_mut(start..end).ok_or_else(|| {
            format!(
                "scene video media instance {} draw range {start}..{end} exceeds {draw_count} draws",
                binding.slot
            )
        })?;
        for (offset, target) in targets.iter_mut().enumerate() {
            if let Some(previous) = target {
                return Err(format!(
                    "scene draw {} has video media instances {previous} and {}",
                    start + offset,
                    binding.slot
                ));
            }
            *target = Some(binding.slot);
        }
    }
    Ok(instances)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{SceneRenderTargetKind, SceneStringId};

    fn binding(media_instance: u32, start: u32, count: u32) -> SceneRenderingDeviceSampledBinding {
        SceneRenderingDeviceSampledBinding {
            pass_node_index: 0,
            graph_index: 0,
            mesh_draw_start: start,
            mesh_draw_count: count,
            kind: SceneRenderBindingKind::VideoFrame,
            slot: media_instance,
            target: SceneRenderTargetKind::VideoExternalImage,
            target_name: SceneStringId::NONE,
            access: SceneRenderingDeviceImageAccess::SampledImage,
        }
    }

    #[test]
    fn media_instance_marks_only_the_explicit_binding_draw_range() {
        assert_eq!(
            video_draw_media_instances(4, &[binding(7, 1, 2)]).unwrap(),
            vec![None, Some(7), Some(7), None]
        );
    }

    #[test]
    fn overlapping_video_bindings_are_rejected_instead_of_guessed() {
        let error =
            video_draw_media_instances(3, &[binding(2, 0, 2), binding(3, 1, 2)]).unwrap_err();
        assert!(error.contains("media instances 2 and 3"));
    }
}
