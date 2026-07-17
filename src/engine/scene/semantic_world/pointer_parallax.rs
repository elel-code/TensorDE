//! Retained Wallpaper Engine camera-parallax semantic system.

use crate::engine::scene::{SceneCameraParallaxRecord, SceneFrameEvents};

use super::{ResolvedSemanticFrame, SceneSemanticWorld};

#[derive(Debug)]
pub(super) struct RetainedPointerParallaxSystem {
    camera: SceneCameraParallaxRecord,
    object_depths: Vec<[f32; 2]>,
    displacement: [f32; 2],
    previous_scene_time_seconds: Option<f32>,
}

impl RetainedPointerParallaxSystem {
    pub(super) fn from_world(world: &SceneSemanticWorld<'_>) -> Self {
        let mut object_depths = vec![[0.0; 2]; world.storage.objects().len()];
        for binding in world.storage.object_parallax_depths() {
            object_depths[binding.object.0 as usize] = binding.depth;
        }
        Self {
            camera: world.storage.camera_parallax(),
            object_depths,
            displacement: [0.0; 2],
            previous_scene_time_seconds: None,
        }
    }

    pub(super) fn begin_frame(
        &mut self,
        world: &SceneSemanticWorld<'_>,
        scene_time_seconds: f32,
        events: &SceneFrameEvents,
    ) {
        let delta_seconds = self
            .previous_scene_time_seconds
            .map(|previous| (scene_time_seconds - previous).max(0.0))
            .unwrap_or(0.0);
        self.previous_scene_time_seconds = Some(scene_time_seconds);
        if !self.camera.enabled || self.object_depths.is_empty() {
            self.displacement = [0.0; 2];
            return;
        }
        let normalized = pointer_scene_position(world, events).unwrap_or([0.5; 2]);
        let target = [
            (normalized[0] - 0.5) * self.camera.amount * self.camera.mouse_influence,
            (normalized[1] - 0.5) * self.camera.amount * self.camera.mouse_influence,
        ];
        let response = (self.camera.delay * delta_seconds).clamp(0.0, 1.0);
        for (current, target) in self.displacement.iter_mut().zip(target) {
            *current += (target - *current) * response;
        }
    }

    pub(super) fn apply_frame(
        &self,
        world: &SceneSemanticWorld<'_>,
        frame: &mut ResolvedSemanticFrame,
    ) {
        for object in &mut frame.objects {
            object.render_world_matrix = object.world_matrix;
        }
        if !self.camera.enabled || self.displacement == [0.0; 2] {
            return;
        }
        let reference_size = world.storage.project().logical_width.max(1) as f32;
        for object in &mut frame.objects {
            let Some(depth) = self.object_depths.get(object.object_index as usize) else {
                continue;
            };
            object.render_world_matrix[12] +=
                (depth[0] + self.camera.amount) * self.displacement[0] * reference_size;
            object.render_world_matrix[13] +=
                (depth[1] + self.camera.amount) * self.displacement[1] * reference_size;
        }
    }
}

fn pointer_scene_position(
    world: &SceneSemanticWorld<'_>,
    events: &SceneFrameEvents,
) -> Option<[f32; 2]> {
    if !events.pointer.inside {
        return None;
    }
    let normalized = events.pointer.normalized_position_top_left()?;
    let project = world.storage.project();
    Some(cover_mapped_position(
        normalized,
        [project.logical_width, project.logical_height],
        events.pointer.surface_size,
    ))
}

fn cover_mapped_position(
    normalized: [f32; 2],
    scene_size: [u32; 2],
    surface_size: [u32; 2],
) -> [f32; 2] {
    let scene_aspect = scene_size[0].max(1) as f32 / scene_size[1].max(1) as f32;
    let surface_aspect = surface_size[0].max(1) as f32 / surface_size[1].max(1) as f32;
    if scene_aspect > surface_aspect {
        let visible = surface_aspect / scene_aspect;
        [
            0.5 * (1.0 - visible) + normalized[0] * visible,
            normalized[1],
        ]
    } else {
        let visible = scene_aspect / surface_aspect;
        [
            normalized[0],
            0.5 * (1.0 - visible) + normalized[1] * visible,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_mapping_preserves_center_and_crops_wide_scene_x() {
        assert_eq!(
            cover_mapped_position([0.5, 0.5], [3440, 1440], [1920, 1080]),
            [0.5, 0.5]
        );
        let left = cover_mapped_position([0.0, 0.5], [3440, 1440], [1920, 1080]);
        assert!(left[0] > 0.12 && left[0] < 0.13);
    }
}
