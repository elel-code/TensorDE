//! Retained Wallpaper Engine camera-parallax semantic system.

use crate::engine::scene::{
    SceneCameraParallaxRecord, SceneFrameEvents, SceneObjectHandle, SceneObjectKind,
};

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
    active_camera: Option<SceneObjectHandle>,
    fallback_camera_eye: [f32; 2],
    logical_half_extent: [f32; 2],
    camera_position: [f32; 2],
    pointer_scene_offset: [f32; 2],
    shader_position: [f32; 2],
    response: f32,
}

impl RetainedPointerParallaxSystem {
    pub(super) fn from_world(
        world: &SceneSemanticWorld<'_>,
        initial_frame: &ResolvedSemanticFrame,
    ) -> Self {
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
        let active_camera = world
            .storage
            .objects()
            .iter()
            .find(|object| object.kind == SceneObjectKind::Camera)
            .map(|object| object.id);
        let fallback_camera_eye = [project.camera_eye.x, project.camera_eye.y];
        let camera_eye = resolved_camera_eye(initial_frame, active_camera, fallback_camera_eye);
        let pointer_scene_offset = [
            project.logical_width as f32 * 0.5,
            project.logical_height as f32 * 0.5,
        ];
        Self {
            camera: world.storage.camera_parallax(),
            root_bindings,
            active_camera,
            fallback_camera_eye,
            logical_half_extent: pointer_scene_offset,
            camera_position: [
                camera_eye[0] + pointer_scene_offset[0],
                camera_eye[1] + pointer_scene_offset[1],
            ],
            pointer_scene_offset,
            shader_position: [0.5; 2],
            response: 0.0,
        }
    }

    pub(super) fn begin_frame(
        &mut self,
        world: &SceneSemanticWorld<'_>,
        frame_delta_seconds: f32,
        events: &SceneFrameEvents,
    ) {
        let shader_pointer = events
            .pointer
            .normalized_position_top_left()
            .unwrap_or([0.5; 2]);
        self.shader_position = if self.camera.enabled {
            [
                0.5 * (1.0 - self.camera.mouse_influence)
                    + shader_pointer[0] * self.camera.mouse_influence,
                0.5 * (1.0 - self.camera.mouse_influence)
                    + shader_pointer[1] * self.camera.mouse_influence,
            ]
        } else {
            shader_pointer
        };
        if !self.camera.enabled || self.root_bindings.is_empty() {
            return;
        }
        let normalized = retained_camera_pointer_position(events).unwrap_or([0.5; 2]);
        let project = world.storage.project();
        self.pointer_scene_offset = [
            project.logical_width as f32
                * (0.5 * (1.0 - self.camera.mouse_influence)
                    + normalized[0] * self.camera.mouse_influence),
            project.logical_height as f32
                * (0.5 * (1.0 - self.camera.mouse_influence)
                    + normalized[1] * self.camera.mouse_influence),
        ];
        self.response = if self.camera.delay <= 0.0 {
            1.0
        } else {
            ((1.0 - self.camera.delay / 3.0) * 10.0 * frame_delta_seconds).min(1.0)
        };
    }

    pub(super) fn apply_frame(&mut self, frame: &mut ResolvedSemanticFrame) {
        frame.parallax_position = self.shader_position;
        frame.particle_camera_parallax_translation = [0.0; 2];
        for object in &mut frame.objects {
            object.render_world_matrix = object.world_matrix;
        }
        if !self.camera.enabled {
            return;
        }
        let camera_eye = resolved_camera_eye(frame, self.active_camera, self.fallback_camera_eye);
        let target = [
            camera_eye[0] + self.pointer_scene_offset[0],
            camera_eye[1] + self.pointer_scene_offset[1],
        ];
        for (current, target) in self.camera_position.iter_mut().zip(target) {
            *current += (target - *current) * self.response;
        }
        let project_center = [
            camera_eye[0] + self.logical_half_extent[0],
            camera_eye[1] + self.logical_half_extent[1],
        ];
        frame.particle_camera_parallax_translation = [
            self.camera.amount * (self.camera_position[0] - project_center[0]),
            self.camera.amount * (self.camera_position[1] - project_center[1]),
        ];
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

fn resolved_camera_eye(
    frame: &ResolvedSemanticFrame,
    active_camera: Option<SceneObjectHandle>,
    fallback: [f32; 2],
) -> [f32; 2] {
    active_camera
        .and_then(|camera| frame.object(camera))
        .map(|camera| [camera.world_matrix[12], camera.world_matrix[13]])
        .unwrap_or(fallback)
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

fn retained_camera_pointer_position(events: &SceneFrameEvents) -> Option<[f32; 2]> {
    // Wallpaper Engine samples a retained desktop pointer position; a Wayland
    // wl_pointer.leave only means that another surface gained protocol focus.
    // Keep the last surface-local position for camera parallax after leave.
    // Script hover/click dispatch still observes `inside` independently.
    let normalized = events.pointer.normalized_position_top_left()?;
    // Camera parallax consumes the complete surface-normalized input domain.
    // Cover mapping belongs to scene projection and pointer hit-testing; using
    // its cropped visible range here weakens edge motion on unlike aspects.
    Some([normalized[0], 1.0 - normalized[1]])
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
    use crate::engine::scene::{
        INVALID_MATERIAL_ID, INVALID_OBJECT_ID, SceneBinaryDocument, SceneMaterialHandle,
        SceneObjectParallaxDepthRecord, SceneObjectRecord, SceneResourceId, SceneStorage,
        SceneStringId, SceneVec3,
    };

    #[test]
    fn retained_camera_uses_complete_surface_normalized_domain() {
        let events = SceneFrameEvents {
            pointer: crate::engine::scene::ScenePointerState {
                position: [90.0, 10.0],
                surface_size: [100, 200],
                ..crate::engine::scene::ScenePointerState::default()
            },
            ..SceneFrameEvents::default()
        };

        assert_eq!(retained_camera_pointer_position(&events), Some([0.9, 0.95]));
    }

    #[test]
    fn shader_parallax_position_applies_authored_mouse_influence_in_top_left_space() {
        let mut document = SceneBinaryDocument::default();
        document.camera_parallax = SceneCameraParallaxRecord {
            enabled: true,
            amount: 0.5,
            delay: 0.1,
            mouse_influence: 0.5,
        };
        let storage = SceneStorage::from_document(document).expect("parallax storage");
        let world = SceneSemanticWorld::from_storage(&storage).expect("parallax world");
        let mut frame = world.resolve_frame().expect("initial semantic frame");
        let mut parallax = RetainedPointerParallaxSystem::from_world(&world, &frame);
        let events = SceneFrameEvents {
            pointer: crate::engine::scene::ScenePointerState {
                position: [90.0, 10.0],
                surface_size: [100, 200],
                ..crate::engine::scene::ScenePointerState::default()
            },
            ..SceneFrameEvents::default()
        };

        parallax.begin_frame(&world, 1.0 / 60.0, &events);
        parallax.apply_frame(&mut frame);

        assert_eq!(frame.parallax_position, [0.7, 0.275]);
    }

    #[test]
    fn retained_particle_camera_translation_is_independent_of_zero_root_depth() {
        let image = SceneObjectHandle(0);
        let mut document = SceneBinaryDocument::default();
        document.project.logical_width = 100;
        document.project.logical_height = 100;
        document.camera_parallax = SceneCameraParallaxRecord {
            enabled: true,
            amount: 0.1,
            delay: 0.0,
            mouse_influence: 1.0,
        };
        document.objects = vec![test_object(image, SceneObjectKind::Image, [50.0, 50.0])];
        let storage = SceneStorage::from_document(document).expect("camera parallax storage");
        let world = SceneSemanticWorld::from_storage(&storage).expect("camera parallax world");
        let mut frame = world.resolve_frame().expect("initial semantic frame");
        let mut parallax = RetainedPointerParallaxSystem::from_world(&world, &frame);
        let events = SceneFrameEvents {
            pointer: crate::engine::scene::ScenePointerState {
                position: [0.0, 50.0],
                surface_size: [100, 100],
                ..crate::engine::scene::ScenePointerState::default()
            },
            ..SceneFrameEvents::default()
        };

        parallax.begin_frame(&world, 0.0, &events);
        parallax.apply_frame(&mut frame);

        assert_eq!(frame.particle_camera_parallax_translation, [-5.0, 0.0]);
        assert_eq!(
            frame.object(image).expect("image").render_world_matrix[12],
            50.0,
            "zero root depth leaves the regular object at its authored position"
        );
    }

    #[test]
    fn root_depth_translation_is_independent_of_particle_camera_translation() {
        let image = SceneObjectHandle(0);
        let mut document = SceneBinaryDocument::default();
        document.project.logical_width = 100;
        document.project.logical_height = 100;
        document.camera_parallax = SceneCameraParallaxRecord {
            enabled: true,
            amount: 0.1,
            delay: 0.0,
            mouse_influence: 1.0,
        };
        document.objects = vec![test_object(image, SceneObjectKind::Image, [50.0, 50.0])];
        document.object_parallax_depths = vec![SceneObjectParallaxDepthRecord {
            object: image,
            depth: [-0.2, -0.2],
        }];
        let storage = SceneStorage::from_document(document).expect("camera parallax storage");
        let world = SceneSemanticWorld::from_storage(&storage).expect("camera parallax world");
        let mut frame = world.resolve_frame().expect("initial semantic frame");
        let mut parallax = RetainedPointerParallaxSystem::from_world(&world, &frame);
        let events = SceneFrameEvents {
            pointer: crate::engine::scene::ScenePointerState {
                position: [0.0, 50.0],
                surface_size: [100, 100],
                ..crate::engine::scene::ScenePointerState::default()
            },
            ..SceneFrameEvents::default()
        };

        parallax.begin_frame(&world, 0.0, &events);
        parallax.apply_frame(&mut frame);

        assert_eq!(frame.particle_camera_parallax_translation, [-5.0, 0.0]);
        assert_eq!(
            frame.object(image).expect("image").render_world_matrix[12],
            49.0
        );
    }

    #[test]
    fn cover_mapping_preserves_center_and_crops_wide_scene_x() {
        assert_eq!(
            cover_mapped_position([0.5, 0.5], [3440, 1440], [1920, 1080]),
            [0.5, 0.5]
        );
        let left = cover_mapped_position([0.0, 0.5], [3440, 1440], [1920, 1080]);
        assert!(left[0] > 0.12 && left[0] < 0.13);
    }

    #[test]
    fn resolved_camera_motion_updates_retained_parallax_eye() {
        let camera = SceneObjectHandle(0);
        let image = SceneObjectHandle(1);
        let mut document = SceneBinaryDocument::default();
        document.project.logical_width = 100;
        document.project.logical_height = 100;
        document.camera_parallax = SceneCameraParallaxRecord {
            enabled: true,
            amount: 0.5,
            delay: 0.0,
            mouse_influence: 1.0,
        };
        document.objects = vec![
            test_object(camera, SceneObjectKind::Camera, [-20.0, -30.0]),
            test_object(image, SceneObjectKind::Image, [10.0, 20.0]),
        ];
        document.object_parallax_depths = vec![SceneObjectParallaxDepthRecord {
            object: image,
            depth: [-0.1, -0.2],
        }];
        let storage = SceneStorage::from_document(document).expect("camera parallax storage");
        let world = SceneSemanticWorld::from_storage(&storage).expect("camera parallax world");
        let mut frame = world.resolve_frame().expect("initial semantic frame");
        let mut parallax = RetainedPointerParallaxSystem::from_world(&world, &frame);

        frame.objects[camera.0 as usize].world_matrix[12] = -10.0;
        frame.objects[camera.0 as usize].world_matrix[13] = -5.0;
        let events = SceneFrameEvents {
            pointer: crate::engine::scene::ScenePointerState {
                position: [50.0, 50.0],
                surface_size: [100, 100],
                ..crate::engine::scene::ScenePointerState::default()
            },
            ..SceneFrameEvents::default()
        };
        parallax.begin_frame(&world, 0.0, &events);
        parallax.apply_frame(&mut frame);

        let image = frame.object(image).expect("parallax image");
        assert!((image.render_world_matrix[12] - 11.5).abs() < 1.0e-6);
        assert!((image.render_world_matrix[13] - 22.5).abs() < 1.0e-6);
    }

    fn test_object(
        id: SceneObjectHandle,
        kind: SceneObjectKind,
        origin: [f32; 2],
    ) -> SceneObjectRecord {
        SceneObjectRecord {
            id,
            we_id: id.0 + 100,
            name: SceneStringId::NONE,
            kind,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3 {
                x: origin[0],
                y: origin[1],
                z: 0.0,
            },
            angles: SceneVec3::default(),
            scale: SceneVec3::ONE,
            camera_zoom: 1.0,
            color: SceneVec3::ONE,
            alpha: 1.0,
            visible: true,
            color_blend_mode: 0,
            sort_order: id.0 as i32,
            effect_start: INVALID_OBJECT_ID,
            effect_count: 0,
            render_graph: INVALID_OBJECT_ID,
        }
    }
}
