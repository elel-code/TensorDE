//! Retained projection domains for scene and authored-texture targets.

use super::{
    ResolvedSemanticFrame, SceneObjectKind, SceneProjectRecord, SceneRenderPassDrawPrimitive,
    SceneRenderPassRecord, SceneRenderingDeviceProjectionDomain, SceneStorage,
    target_extent::authored_texture_space_target_extent,
};

pub(crate) fn scene_clip_transform(
    project: &SceneProjectRecord,
    world_matrix: [f32; 16],
) -> [[f32; 4]; 4] {
    let width = project.logical_width.max(1) as f32;
    let height = project.logical_height.max(1) as f32;
    [
        [
            2.0 * world_matrix[0] / width - world_matrix[3],
            2.0 * world_matrix[4] / width - world_matrix[7],
            2.0 * world_matrix[8] / width - world_matrix[11],
            2.0 * world_matrix[12] / width - world_matrix[15],
        ],
        [
            -2.0 * world_matrix[1] / height + world_matrix[3],
            -2.0 * world_matrix[5] / height + world_matrix[7],
            -2.0 * world_matrix[9] / height + world_matrix[11],
            -2.0 * world_matrix[13] / height + world_matrix[15],
        ],
        [
            world_matrix[2],
            world_matrix[6],
            world_matrix[10],
            world_matrix[14],
        ],
        [
            world_matrix[3],
            world_matrix[7],
            world_matrix[11],
            world_matrix[15],
        ],
    ]
}

/// Composes the authored 2D camera layer with the logical-scene projection.
///
/// Wallpaper Engine camera-layer positions are offsets from the centre of the orthographic
/// canvas. The live output aspect remains a later viewport concern, so this function only
/// applies authored zoom and camera translation. The fixed 4,000-unit depth span is the WE 2D
/// camera projection contract recovered from the complete D3D11 MVP stream.
pub(crate) fn scene_clip_transform_for_frame(
    storage: &SceneStorage,
    semantic_frame: &ResolvedSemanticFrame,
    world_matrix: [f32; 16],
) -> [[f32; 4]; 4] {
    let project = storage.project();
    let mut clip = scene_clip_transform(project, world_matrix);
    let camera = storage
        .objects()
        .iter()
        .find(|object| object.kind == SceneObjectKind::Camera)
        .and_then(|object| {
            semantic_frame
                .object(object.id)
                .map(|state| (state, object.camera_zoom))
        });
    let zoom = camera.map_or(1.0, |(state, authored_zoom)| {
        debug_assert!(authored_zoom.is_finite() && authored_zoom > 0.0);
        state.camera_zoom
    });
    let width = project.logical_width.max(1) as f32;
    let height = project.logical_height.max(1) as f32;
    let camera_position = camera.map_or([0.0; 3], |(state, _)| {
        [
            state.world_matrix[12],
            state.world_matrix[13],
            state.world_matrix[14],
        ]
    });
    let homogeneous = clip[3];
    for value in &mut clip[0] {
        *value *= zoom;
    }
    for (value, homogeneous) in clip[0].iter_mut().zip(homogeneous) {
        *value -= 2.0 * zoom * camera_position[0] / width * homogeneous;
    }
    for value in &mut clip[1] {
        *value *= zoom;
    }
    for (value, homogeneous) in clip[1].iter_mut().zip(homogeneous) {
        *value += 2.0 * zoom * camera_position[1] / height * homogeneous;
    }
    for (index, homogeneous) in homogeneous.into_iter().enumerate() {
        clip[2][index] = world_matrix[index * 4 + 2] / 4_000.0
            + (0.5 - camera_position[2] / 4_000.0) * homogeneous;
    }
    clip
}

pub(crate) fn effect_texture_projection_matrix(
    storage: &SceneStorage,
    semantic_frame: &ResolvedSemanticFrame,
    world_matrix: [f32; 16],
    authored_source_extent: [f32; 2],
) -> [[f32; 4]; 4] {
    let mut projection = scene_clip_transform_for_frame(storage, semantic_frame, world_matrix);
    let half_width = authored_source_extent[0] * 0.5;
    let half_height = authored_source_extent[1] * 0.5;
    for row in &mut projection {
        row[0] *= half_width;
        row[1] *= half_height;
    }
    for value in &mut projection[1] {
        *value = -*value;
    }
    projection
}

impl SceneRenderingDeviceProjectionDomain {
    pub(crate) fn clip_transform(
        self,
        storage: &SceneStorage,
        semantic_frame: &ResolvedSemanticFrame,
        world_matrix: [f32; 16],
    ) -> [[f32; 4]; 4] {
        match self {
            Self::Scene => scene_clip_transform_for_frame(storage, semantic_frame, world_matrix),
            Self::AuthoredTexture { width, height } => {
                authored_texture_clip_transform(width, height)
            }
        }
    }
}

pub(super) fn pass_projection_domain(
    storage: &SceneStorage,
    graph_index: u32,
    pass: &SceneRenderPassRecord,
) -> SceneRenderingDeviceProjectionDomain {
    if pass.draw_primitive == SceneRenderPassDrawPrimitive::FramebufferCompositeMesh {
        return SceneRenderingDeviceProjectionDomain::Scene;
    }
    if let Some([width, height]) =
        authored_texture_space_target_extent(storage, graph_index, pass.target, pass.target_name)
    {
        SceneRenderingDeviceProjectionDomain::AuthoredTexture { width, height }
    } else {
        SceneRenderingDeviceProjectionDomain::Scene
    }
}

pub(crate) fn authored_texture_clip_transform(width: u32, height: u32) -> [[f32; 4]; 4] {
    let width = width.max(1) as f32;
    let height = height.max(1) as f32;
    [
        [2.0 / width, 0.0, 0.0, 0.0],
        [0.0, -2.0 / height, 0.0, 0.0],
        [0.0, 0.0, 1.0 / 2_000.0, 0.5],
        [0.0, 0.0, 0.0, 1.0],
    ]
}
