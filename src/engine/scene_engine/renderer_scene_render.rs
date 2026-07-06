//! Godot-aligned RendererSceneRender boundary.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/scene-and-object.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use super::{
    RenderingDevice, SceneFrameContext, SceneFramePlan, SceneGraph, SceneObject, SceneResource,
    SceneResourceResidencyPlan,
};

pub trait RendererSceneRender {
    fn build_graph(
        &self,
        context: SceneFrameContext,
        resources: &[SceneResource],
        objects: &[SceneObject],
    ) -> SceneGraph;

    fn build_frame(
        &self,
        context: SceneFrameContext,
        resources: &[SceneResource],
        objects: &[SceneObject],
    ) -> SceneFramePlan {
        let residency = SceneResourceResidencyPlan::from_resources(resources);
        self.build_frame_with_residency(context, &residency, resources, objects)
    }

    fn build_frame_with_residency(
        &self,
        context: SceneFrameContext,
        residency: &SceneResourceResidencyPlan,
        resources: &[SceneResource],
        objects: &[SceneObject],
    ) -> SceneFramePlan {
        SceneFramePlan {
            residency: residency.clone(),
            graph: self.build_graph(context, resources, objects),
        }
    }

    fn draw<D: RenderingDevice>(&self, frame: &SceneFramePlan, device: &mut D) {
        device.record_scene_frame(frame);
    }
}
