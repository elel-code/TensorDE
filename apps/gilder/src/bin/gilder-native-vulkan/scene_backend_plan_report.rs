use gilder::engine::scene::{
    INVALID_OBJECT_ID, ResolvedSemanticFrame, SceneObjectHandle, SceneObjectRecord,
    SceneRenderPassRecord, SceneResourceRecord, SceneStorage, SceneTextureRecord,
};
use gilder::renderer::native_vulkan::{
    NativeVulkanSceneBackendPlan, native_vulkan_scene_backend_plan_from_semantic_frame,
};
use serde::Serialize;

pub(super) const SCENE_BACKEND_PLAN_REPORT_VERSION: u32 = 4;

#[derive(Debug, Serialize)]
pub(super) struct SceneBackendPlanReport<'a> {
    pub scene_backend_plan_report_version: u32,
    #[serde(flatten)]
    pub backend_plan: NativeVulkanSceneBackendPlan,
    pub scene_objects: &'a [SceneObjectRecord],
    pub scene_resources: &'a [SceneResourceRecord],
    pub scene_textures: &'a [SceneTextureRecord],
    pub scene_render_passes: &'a [SceneRenderPassRecord],
    pub scene_strings: &'a [String],
    pub checkpoint_scene_time_seconds: f32,
    pub checkpoint_draw_visibility: Vec<SceneBackendPlanDrawVisibility>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct SceneBackendPlanDrawVisibility {
    pub draw_index: u32,
    pub object: SceneObjectHandle,
    pub resolved_object_index: u32,
    pub object_resolved_visible: Option<bool>,
    pub object_visibility_allows_draw: bool,
}

pub(super) fn scene_backend_plan_report<'a>(
    storage: &'a SceneStorage,
    semantic_frame: &ResolvedSemanticFrame,
) -> Result<SceneBackendPlanReport<'a>, String> {
    let backend_plan =
        native_vulkan_scene_backend_plan_from_semantic_frame(storage, semantic_frame);
    let checkpoint_draw_visibility = backend_plan
        .rendering_device_graph
        .mesh_draws
        .iter()
        .enumerate()
        .map(|(draw_index, draw)| {
            let draw_index = u32::try_from(draw_index)
                .map_err(|_| "scene backend plan draw index exceeds u32".to_owned())?;
            let object_resolved_visible = if draw.object.0 == INVALID_OBJECT_ID {
                None
            } else {
                let object = semantic_frame.object(draw.object).ok_or_else(|| {
                    format!(
                        "scene backend plan draw {draw_index} references missing resolved object {}",
                        draw.object.0
                    )
                })?;
                if object.object_index != draw.resolved_object_index {
                    return Err(format!(
                        "scene backend plan draw {draw_index} resolved object index {} does not match semantic object index {}",
                        draw.resolved_object_index, object.object_index
                    ));
                }
                Some(object.resolved_visible)
            };
            Ok(SceneBackendPlanDrawVisibility {
                draw_index,
                object: draw.object,
                resolved_object_index: draw.resolved_object_index,
                object_resolved_visible,
                object_visibility_allows_draw: object_resolved_visible.unwrap_or(true),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(SceneBackendPlanReport {
        scene_backend_plan_report_version: SCENE_BACKEND_PLAN_REPORT_VERSION,
        backend_plan,
        scene_objects: storage.objects(),
        scene_resources: storage.resources(),
        scene_textures: storage.textures(),
        scene_render_passes: &storage.document().render_passes,
        scene_strings: storage.strings(),
        checkpoint_scene_time_seconds: 0.0,
        checkpoint_draw_visibility,
    })
}
