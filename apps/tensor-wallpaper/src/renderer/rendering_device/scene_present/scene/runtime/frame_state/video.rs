//! Exact 20-byte screen-space vertex stream for scene-video layers.

use crate::engine::scene::{SceneRenderingDeviceGraphPlan, SceneStorage};

use super::super::draw_recording::SceneGpuDrawCommand;

pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn pack_scene_video_vertices_into(
    payload: &mut Vec<u8>,
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    commands: &[SceneGpuDrawCommand],
    extent: [u32; 2],
) -> Result<(), String> {
    if graph.mesh_draws.len() != commands.len() || extent.contains(&0) {
        return Err("scene-video vertex update has mismatched draws or empty extent".into());
    }
    payload.clear();
    for (draw_index, (draw, command)) in graph.mesh_draws.iter().zip(commands).enumerate() {
        let Some(expected_offset) = command.video_vertex_byte_offset else {
            continue;
        };
        if payload.len() as u64 != expected_offset {
            return Err(format!(
                "scene-video draw {draw_index} vertex offset {expected_offset} differs from packed byte {}",
                payload.len()
            ));
        }
        if draw.apply_resolved_visual && draw.resolved_color != crate::engine::scene::SceneVec3::ONE
        {
            return Err(format!(
                "scene-video draw {draw_index} has unsupported color modulation"
            ));
        }
        let opacity = if draw.apply_resolved_visual {
            draw.resolved_alpha
        } else {
            1.0
        };
        if !opacity.is_finite() {
            return Err(format!(
                "scene-video draw {draw_index} opacity is not finite"
            ));
        }
        let start = draw.vertex_start as usize;
        let end = start
            .checked_add(draw.vertex_count as usize)
            .ok_or_else(|| "scene-video vertex range overflows".to_owned())?;
        let vertices = storage
            .document()
            .mesh_vertices
            .get(start..end)
            .ok_or_else(|| {
                format!(
                    "scene-video draw {draw_index} vertex range {start}..{end} exceeds {} vertices",
                    storage.document().mesh_vertices.len()
                )
            })?;
        for vertex in vertices {
            let position = project_to_video_pixels(
                draw.clip_transform,
                [vertex.position.x, vertex.position.y],
                extent,
            )
            .ok_or_else(|| {
                format!("scene-video draw {draw_index} produces an invalid clip position")
            })?;
            for value in [
                position[0],
                position[1],
                vertex.uv[0],
                vertex.uv[1],
                opacity,
            ] {
                payload.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    Ok(())
}

fn project_to_video_pixels(
    transform: [[f32; 4]; 4],
    position: [f32; 2],
    extent: [u32; 2],
) -> Option<[f32; 2]> {
    let local = [position[0], position[1], 0.0, 1.0];
    let clip = transform.map(|row| {
        row.iter()
            .zip(local)
            .map(|(coefficient, value)| coefficient * value)
            .sum::<f32>()
    });
    if !clip.iter().all(|value| value.is_finite()) || clip[3].abs() <= f32::EPSILON {
        return None;
    }
    Some([
        (clip[0] / clip[3] * 0.5 + 0.5) * extent[0] as f32,
        (clip[1] / clip[3] * 0.5 + 0.5) * extent[1] as f32,
    ])
}

#[cfg(test)]
mod tests {
    use super::project_to_video_pixels;

    #[test]
    fn video_vertex_stride_remains_position_uv_opacity() {
        assert_eq!(super::super::super::SCENE_VIDEO_VERTEX_STRIDE_BYTES, 20);
    }

    #[test]
    fn row_dot_identity_projects_clip_origin_to_surface_center() {
        assert_eq!(
            project_to_video_pixels(
                [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
                [0.0, 0.0],
                [3840, 2160],
            ),
            Some([1920.0, 1080.0])
        );
    }
}
