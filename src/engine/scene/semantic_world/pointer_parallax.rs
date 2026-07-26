//! Retained Wallpaper Engine camera-parallax semantic system.

use crate::engine::scene::{SceneCameraParallaxRecord, SceneFrameEvents};

use super::{ResolvedSemanticFrame, SceneSemanticWorld};

#[derive(Debug, Clone, Copy)]
struct RootParallaxBinding {
    root_object_index: usize,
    depth: [f32; 2],
}

#[derive(Debug)]
pub(super) struct RetainedPointerParallaxSystem {
    camera: SceneCameraParallaxRecord,
    root_bindings: Vec<RootParallaxBinding>,
    camera_position: [f32; 2],
}

impl RetainedPointerParallaxSystem {
    pub(super) fn from_world(world: &SceneSemanticWorld<'_>) -> Self {
        let mut object_depths = vec![[0.0; 2]; world.storage.objects().len()];
        for binding in world.storage.object_parallax_depths() {
            object_depths[binding.object.0 as usize] = binding.depth;
        }
        let root_bindings = world
            .storage
            .objects()
            .iter()
            .map(|object| {
                let root_object_index = topmost_object_index(world, object.id);
                RootParallaxBinding {
                    root_object_index,
                    depth: object_depths[root_object_index],
                }
            })
            .collect();
        let project = world.storage.project();
        Self {
            camera: world.storage.camera_parallax(),
            root_bindings,
            camera_position: [
                project.camera_eye.x + project.logical_width as f32 * 0.5,
                project.camera_eye.y + project.logical_height as f32 * 0.5,
            ],
        }
    }

    pub(super) fn begin_frame(
        &mut self,
        world: &SceneSemanticWorld<'_>,
        frame_delta_seconds: f32,
        events: &SceneFrameEvents,
    ) {
        if !self.camera.enabled || self.root_bindings.is_empty() {
            return;
        }
        let normalized = pointer_scene_position(world, events).unwrap_or([0.5; 2]);
        let project = world.storage.project();
        let target = [
            project.camera_eye.x
                + project.logical_width as f32
                    * (0.5 * (1.0 - self.camera.mouse_influence)
                        + normalized[0] * self.camera.mouse_influence),
            project.camera_eye.y
                + project.logical_height as f32
                    * (0.5 * (1.0 - self.camera.mouse_influence)
                        + normalized[1] * self.camera.mouse_influence),
        ];
        let response = if self.camera.delay <= 0.0 {
            1.0
        } else {
            ((1.0 - self.camera.delay / 3.0) * 10.0 * frame_delta_seconds).min(1.0)
        };
        for (current, target) in self.camera_position.iter_mut().zip(target) {
            *current += (target - *current) * response;
        }
    }

    pub(super) fn apply_frame(&self, frame: &mut ResolvedSemanticFrame) {
        for object in &mut frame.objects {
            object.render_world_matrix = object.world_matrix;
        }
        if !self.camera.enabled {
            return;
        }
        for object_index in 0..frame.objects.len() {
            let Some(binding) = self.root_bindings.get(object_index).copied() else {
                continue;
            };
            if binding.depth == [0.0; 2] {
                continue;
            }
            let root = frame.objects[binding.root_object_index];
            let translation = [
                self.camera.amount
                    * (root.world_matrix[12] - self.camera_position[0])
                    * binding.depth[0],
                self.camera.amount
                    * (root.world_matrix[13] - self.camera_position[1])
                    * binding.depth[1],
            ];
            let object = &mut frame.objects[object_index];
            object.render_world_matrix[12] += translation[0];
            object.render_world_matrix[13] += translation[1];
        }
    }
}

fn topmost_object_index(
    world: &SceneSemanticWorld<'_>,
    object: crate::engine::scene::SceneObjectHandle,
) -> usize {
    let mut root = object;
    while let Some(parent) = world.parent(root) {
        let parent_entity = world
            .entity_for_we_id(parent.parent_we_id)
            .expect("validated semantic parent exists");
        root = world
            .entity_record(parent_entity)
            .expect("semantic parent entity exists")
            .object;
    }
    root.0 as usize
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
    let mapped = cover_mapped_position(
        normalized,
        [project.logical_width, project.logical_height],
        events.pointer.surface_size,
    );
    Some([mapped[0], 1.0 - mapped[1]])
}

pub(super) fn cover_mapped_position(
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
