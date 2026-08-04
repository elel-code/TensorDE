//! Typed replacement of a proven opaque full-output flat draw with an attachment clear.

use crate::engine::scene::{
    SceneCompositeBlend, SceneCullMode, SceneDepthTest, ScenePipelineBlend, SceneRenderTargetKind,
    SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan, SceneStorage,
};
use crate::renderer::rendering_device::RenderingDeviceClearColor;
use crate::renderer::rendering_device::scene::rendering_device_scene_shader_for_key;

use super::composite_scissor::SceneMeshCoveragePlans;
use super::composite_scissor::object_mesh_covers_output;
use super::draw_recording::{SceneGpuDrawRange, SceneGpuGraphDrawRange};
use super::material_uniform::resolved_standard_material_color;

static CLEAR_DIAGNOSTIC_EMITTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static CLEAR_DIAGNOSTIC_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SceneGpuSceneColorClear {
    pub graph_index: u32,
    pub draw_index: u32,
    pub color: RenderingDeviceClearColor,
}

impl SceneGpuSceneColorClear {
    pub(super) fn replaces(self, range: SceneGpuGraphDrawRange) -> bool {
        range.graph_index == self.graph_index
            && range.range
                == (SceneGpuDrawRange {
                    start: self.draw_index,
                    count: 1,
                })
    }
}

pub(super) fn resolve_scene_color_attachment_clear(
    storage: &SceneStorage,
    mesh_coverage: &SceneMeshCoveragePlans,
    graph: &SceneRenderingDeviceGraphPlan,
    graph_execution_order: &[u32],
    output_extent: [u32; 2],
    enabled: bool,
) -> Option<SceneGpuSceneColorClear> {
    match resolve_scene_color_attachment_clear_contract(
        storage,
        mesh_coverage,
        graph,
        graph_execution_order,
        output_extent,
        enabled,
    ) {
        Ok(clear) => Some(clear),
        Err(reason) => {
            if *CLEAR_DIAGNOSTIC_ENABLED.get_or_init(|| {
                std::env::var_os("TENSOR_WALLPAPER_RENDERING_DEVICE_SCENE_COLOR_CLEAR_DEBUG").is_some()
            }) && !CLEAR_DIAGNOSTIC_EMITTED.swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                eprintln!("tensor-wallpaper-scene-color-attachment-clear: rejected={reason}");
            }
            None
        }
    }
}

fn resolve_scene_color_attachment_clear_contract(
    storage: &SceneStorage,
    mesh_coverage: &SceneMeshCoveragePlans,
    graph: &SceneRenderingDeviceGraphPlan,
    graph_execution_order: &[u32],
    output_extent: [u32; 2],
    enabled: bool,
) -> Result<SceneGpuSceneColorClear, &'static str> {
    if !enabled {
        return Err("disabled");
    }
    let graph_index = *graph_execution_order
        .first()
        .ok_or("empty-execution-order")?;
    let mut passes = graph
        .pass_nodes
        .iter()
        .filter(|pass| pass.graph_index == graph_index && pass.mesh_draw_count != 0);
    let pass = passes.next().ok_or("first-graph-has-no-drawn-pass")?;
    if passes.next().is_some() {
        return Err("first-graph-has-multiple-drawn-passes");
    }
    if pass.mesh_draw_count != 1 {
        return Err("first-pass-is-not-a-single-draw");
    }
    if !matches!(
        pass.target,
        SceneRenderTargetKind::SceneColor | SceneRenderTargetKind::Swapchain
    ) {
        return Err("first-pass-does-not-target-scene-color");
    }
    let pass_record = storage
        .document()
        .render_passes
        .get(pass.pass_record_index as usize)
        .ok_or("missing-render-pass-record")?;
    let shader_key = storage
        .string(pass_record.shader_key)
        .ok_or("missing-shader-key")?;
    if !rendering_device_scene_shader_for_key(shader_key)
        .is_some_and(|shader| shader.key.eq_ignore_ascii_case("we/flat"))
    {
        return Err("first-pass-is-not-we-flat");
    }
    if pass_record.cull_mode != SceneCullMode::None {
        return Err("flat-pass-enables-culling");
    }
    if pass_record.depth_test != SceneDepthTest::Disabled {
        return Err("flat-pass-enables-depth-test");
    }
    if !opaque_source_replaces_destination(pass_record.scene_blend, pass_record.pipeline_blend) {
        return Err("flat-pass-blend-does-not-replace-when-opaque");
    }
    let draw = graph
        .mesh_draws
        .get(pass.mesh_draw_start as usize)
        .ok_or("missing-flat-draw")?;
    if draw.primitive != SceneRenderingDeviceDrawPrimitive::ObjectMesh {
        return Err("flat-draw-is-not-an-object-mesh");
    }
    if draw.skinning_palette_count != 0 {
        return Err("flat-draw-is-skinned");
    }
    if draw.material != pass_record.material {
        return Err("flat-draw-material-mismatch");
    }
    if !object_mesh_covers_output(storage, mesh_coverage, draw, output_extent) {
        return Err("flat-object-mesh-does-not-prove-full-output-coverage");
    }
    let [r, g, b, a] =
        resolved_standard_material_color(storage, draw).ok_or("flat-color-is-unresolved")?;
    if ![r, g, b, a].iter().all(|value| value.is_finite()) {
        return Err("flat-color-is-not-finite");
    }
    if a != 1.0 {
        return Err("flat-color-is-not-opaque");
    }
    Ok(SceneGpuSceneColorClear {
        graph_index,
        draw_index: pass.mesh_draw_start,
        color: RenderingDeviceClearColor { r, g, b, a },
    })
}

fn opaque_source_replaces_destination(
    scene_blend: SceneCompositeBlend,
    pipeline_blend: ScenePipelineBlend,
) -> bool {
    match scene_blend {
        SceneCompositeBlend::Alpha => true,
        SceneCompositeBlend::Normal => matches!(
            pipeline_blend,
            ScenePipelineBlend::Normal
                | ScenePipelineBlend::Disabled
                | ScenePipelineBlend::Translucent
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_replace_and_opaque_alpha_blends_accept_attachment_clear() {
        assert!(opaque_source_replaces_destination(
            SceneCompositeBlend::Alpha,
            ScenePipelineBlend::Additive,
        ));
        assert!(opaque_source_replaces_destination(
            SceneCompositeBlend::Normal,
            ScenePipelineBlend::Disabled,
        ));
        assert!(!opaque_source_replaces_destination(
            SceneCompositeBlend::Normal,
            ScenePipelineBlend::Additive,
        ));
        assert!(!opaque_source_replaces_destination(
            SceneCompositeBlend::Multiply,
            ScenePipelineBlend::Normal,
        ));
    }

    #[test]
    fn clear_replaces_only_its_single_typed_draw_range() {
        let clear = SceneGpuSceneColorClear {
            graph_index: 3,
            draw_index: 7,
            color: RenderingDeviceClearColor::default(),
        };
        assert!(clear.replaces(SceneGpuGraphDrawRange {
            graph_index: 3,
            range: SceneGpuDrawRange { start: 7, count: 1 },
        }));
        assert!(!clear.replaces(SceneGpuGraphDrawRange {
            graph_index: 3,
            range: SceneGpuDrawRange { start: 7, count: 2 },
        }));
    }
}
