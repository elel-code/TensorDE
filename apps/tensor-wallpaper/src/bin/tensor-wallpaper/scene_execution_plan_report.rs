use tensor_wallpaper::engine::scene::{
    INVALID_OBJECT_ID, ResolvedSemanticFrame, SceneCameraParallaxRecord,
    SceneDynamicTextGlyphRecord, SceneDynamicTextRecord, SceneImageTargetRecord,
    SceneMaterialConstantRecord, SceneMaterialPassRecord, SceneMaterialRecord,
    SceneMeshClippingSliceRecord, SceneMeshClippingSubdrawRecord, SceneMeshRecord,
    SceneMeshSourceRecord, SceneObjectHandle, SceneObjectParallaxDepthRecord, SceneObjectRecord,
    SceneRenderPassRecord, SceneResourceRecord, SceneScriptProgramRecord, SceneShaderBindingRecord,
    SceneShaderProgramRecord, SceneShaderStageIoRecord, SceneShaderUniformBufferRecord,
    SceneShaderUniformMemberRecord, SceneStorage, SceneTextureRecord,
};
use tensor_wallpaper::renderer::rendering_device::{
    SceneExecutionPlan, RenderingDeviceSceneOwnedUniformArenaPlanSnapshot,
    scene_execution_plan_from_semantic_frame,
    rendering_device_scene_owned_uniform_arena_plan,
};
use serde::Serialize;

pub(super) const SCENE_EXECUTION_PLAN_REPORT_VERSION: u32 = 14;
const SCENE_EXECUTION_PLAN_UNIFORM_ALIGNMENT: u64 = 256;

#[derive(Debug, Serialize)]
pub(super) struct SceneExecutionPlanReport<'a> {
    pub scene_execution_plan_report_version: u32,
    #[serde(flatten)]
    pub execution_plan: SceneExecutionPlan,
    pub scene_objects: &'a [SceneObjectRecord],
    pub scene_resources: &'a [SceneResourceRecord],
    pub scene_textures: &'a [SceneTextureRecord],
    pub scene_image_targets: &'a [SceneImageTargetRecord],
    pub scene_render_passes: &'a [SceneRenderPassRecord],
    pub scene_materials: &'a [SceneMaterialRecord],
    pub scene_material_passes: &'a [SceneMaterialPassRecord],
    pub scene_material_constants: &'a [SceneMaterialConstantRecord],
    pub scene_meshes: &'a [SceneMeshRecord],
    pub scene_mesh_source_records: &'a [SceneMeshSourceRecord],
    pub scene_mesh_clipping_subdraws: &'a [SceneMeshClippingSubdrawRecord],
    pub scene_mesh_clipping_source_ordinals: &'a [u32],
    pub scene_mesh_clipping_slices: &'a [SceneMeshClippingSliceRecord],
    pub scene_shader_programs: &'a [SceneShaderProgramRecord],
    pub scene_shader_spirv: &'a [u32],
    pub scene_shader_bindings: &'a [SceneShaderBindingRecord],
    pub scene_shader_stage_io: &'a [SceneShaderStageIoRecord],
    pub scene_shader_uniform_buffers: &'a [SceneShaderUniformBufferRecord],
    pub scene_shader_uniform_members: &'a [SceneShaderUniformMemberRecord],
    pub scene_script_programs: &'a [SceneScriptProgramRecord],
    pub scene_dynamic_texts: &'a [SceneDynamicTextRecord],
    pub scene_dynamic_text_glyphs: &'a [SceneDynamicTextGlyphRecord],
    pub scene_camera_parallax: SceneCameraParallaxRecord,
    pub scene_object_parallax_depths: &'a [SceneObjectParallaxDepthRecord],
    pub scene_strings: &'a [String],
    pub checkpoint_scene_time_seconds: f32,
    pub checkpoint_draw_visibility: Vec<SceneExecutionPlanDrawVisibility>,
    pub scene_owned_uniform_arena: RenderingDeviceSceneOwnedUniformArenaPlanSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct SceneExecutionPlanDrawVisibility {
    pub draw_index: u32,
    pub object: SceneObjectHandle,
    pub resolved_object_index: u32,
    pub object_resolved_visible: Option<bool>,
    pub object_visibility_allows_draw: bool,
}

pub(super) fn scene_execution_plan_report<'a>(
    storage: &'a SceneStorage,
    semantic_frame: &'a ResolvedSemanticFrame,
    surface_extent: Option<(u32, u32)>,
) -> Result<SceneExecutionPlanReport<'a>, String> {
    let execution_plan =
        scene_execution_plan_from_semantic_frame(storage, semantic_frame);
    let project = storage.project();
    let output_extent = scene_execution_plan_output_extent(
        [project.logical_width.max(1), project.logical_height.max(1)],
        surface_extent,
    );
    let scene_owned_uniform_arena = rendering_device_scene_owned_uniform_arena_plan(
        storage,
        &execution_plan.rendering_device_graph,
        output_extent,
        SCENE_EXECUTION_PLAN_UNIFORM_ALIGNMENT,
    )?;
    let checkpoint_draw_visibility = execution_plan
        .rendering_device_graph
        .mesh_draws
        .iter()
        .enumerate()
        .map(|(draw_index, draw)| {
            let draw_index = u32::try_from(draw_index)
                .map_err(|_| "scene execution plan draw index exceeds u32".to_owned())?;
            let object_resolved_visible = if draw.object.0 == INVALID_OBJECT_ID {
                None
            } else {
                let object = semantic_frame.object(draw.object).ok_or_else(|| {
                    format!(
                        "scene execution plan draw {draw_index} references missing resolved object {}",
                        draw.object.0
                    )
                })?;
                if object.object_index != draw.resolved_object_index {
                    return Err(format!(
                        "scene execution plan draw {draw_index} resolved object index {} does not match semantic object index {}",
                        draw.resolved_object_index, object.object_index
                    ));
                }
                Some(object.resolved_visible)
            };
            Ok(SceneExecutionPlanDrawVisibility {
                draw_index,
                object: draw.object,
                resolved_object_index: draw.resolved_object_index,
                object_resolved_visible,
                object_visibility_allows_draw: object_resolved_visible.unwrap_or(true),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(SceneExecutionPlanReport {
        scene_execution_plan_report_version: SCENE_EXECUTION_PLAN_REPORT_VERSION,
        execution_plan,
        scene_objects: storage.objects(),
        scene_resources: storage.resources(),
        scene_textures: storage.textures(),
        scene_image_targets: &storage.document().image_targets,
        scene_render_passes: &storage.document().render_passes,
        scene_materials: &storage.document().materials,
        scene_material_passes: &storage.document().material_passes,
        scene_material_constants: &storage.document().material_constants,
        scene_meshes: &storage.document().meshes,
        scene_mesh_source_records: &storage.document().mesh_source_records,
        scene_mesh_clipping_subdraws: &storage.document().mesh_clipping_subdraws,
        scene_mesh_clipping_source_ordinals: &storage.document().mesh_clipping_source_ordinals,
        scene_mesh_clipping_slices: &storage.document().mesh_clipping_slices,
        scene_shader_programs: storage.shader_programs(),
        scene_shader_spirv: &storage.document().shader_spirv,
        scene_shader_bindings: &storage.document().shader_bindings,
        scene_shader_stage_io: &storage.document().shader_stage_io,
        scene_shader_uniform_buffers: &storage.document().shader_uniform_buffers,
        scene_shader_uniform_members: &storage.document().shader_uniform_members,
        scene_script_programs: storage.script_programs(),
        scene_dynamic_texts: storage.dynamic_texts(),
        scene_dynamic_text_glyphs: &storage.document().dynamic_text_glyphs,
        scene_camera_parallax: storage.camera_parallax(),
        scene_object_parallax_depths: storage.object_parallax_depths(),
        scene_strings: storage.strings(),
        checkpoint_scene_time_seconds: 0.0,
        checkpoint_draw_visibility,
        scene_owned_uniform_arena,
    })
}

fn scene_execution_plan_output_extent(
    authored_extent: [u32; 2],
    surface_extent: Option<(u32, u32)>,
) -> [u32; 2] {
    surface_extent.map_or(authored_extent, |(width, height)| [width, height])
}

#[cfg(test)]
mod tests {
    use super::scene_execution_plan_output_extent;

    #[test]
    fn explicit_surface_extent_replaces_only_the_offline_output_extent() {
        assert_eq!(
            scene_execution_plan_output_extent([3440, 1440], Some((3856, 2199))),
            [3856, 2199]
        );
        assert_eq!(
            scene_execution_plan_output_extent([3440, 1440], None),
            [3440, 1440]
        );
    }
}
