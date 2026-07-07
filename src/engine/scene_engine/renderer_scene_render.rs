//! Godot-aligned RendererSceneRender boundary.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/scene-and-object.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use super::{
    RenderingDevice, SceneEffectPassGraphPlan, SceneFrameContext, SceneFramePlan, SceneGraph,
    SceneObject, SceneObjectEffectProgram, SceneResource, SceneResourceResidencyPlan,
};

pub trait RendererSceneRender {
    fn build_graph(
        &self,
        context: SceneFrameContext,
        resources: &[SceneResource],
        objects: &[SceneObject],
        effects: &[SceneObjectEffectProgram],
    ) -> Result<SceneGraph, String>;

    fn build_frame(
        &self,
        context: SceneFrameContext,
        resources: &[SceneResource],
        objects: &[SceneObject],
        effects: &[SceneObjectEffectProgram],
    ) -> Result<SceneFramePlan, String> {
        let residency = SceneResourceResidencyPlan::from_resources(resources);
        self.build_frame_with_residency(context, &residency, resources, objects, effects)
    }

    fn build_frame_with_residency(
        &self,
        context: SceneFrameContext,
        residency: &SceneResourceResidencyPlan,
        resources: &[SceneResource],
        objects: &[SceneObject],
        effects: &[SceneObjectEffectProgram],
    ) -> Result<SceneFramePlan, String> {
        Ok(SceneFramePlan {
            residency: residency.clone(),
            graph: self.build_graph(context, resources, objects, effects)?,
            effect_pass_graph: SceneEffectPassGraphPlan::from_scene(objects, effects)?,
        })
    }

    fn draw<D: RenderingDevice>(&self, frame: &SceneFramePlan, device: &mut D) {
        device.record_scene_frame(frame);
    }
}
