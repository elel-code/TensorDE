//! Godot-aligned RenderingServer front door.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/rendering_server_default.h`

use super::{
    RendererSceneRender, SceneFrameContext, SceneFramePlan, SceneObject, SceneObjectEffectProgram,
    SceneResource, SceneResourceResidencyPlan,
};

#[derive(Debug, Default)]
pub struct RenderingServer {
    resources: Vec<SceneResource>,
    residency: SceneResourceResidencyPlan,
    objects: Vec<SceneObject>,
    effects: Vec<SceneObjectEffectProgram>,
}

impl RenderingServer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn replace_resources(&mut self, resources: Vec<SceneResource>) {
        self.residency = SceneResourceResidencyPlan::from_resources(&resources);
        self.resources = resources;
    }

    pub fn replace_objects(&mut self, objects: Vec<SceneObject>) {
        self.objects = objects;
    }

    pub fn replace_effects(&mut self, effects: Vec<SceneObjectEffectProgram>) {
        self.effects = effects;
    }

    pub fn replace_scene(
        &mut self,
        resources: Vec<SceneResource>,
        objects: Vec<SceneObject>,
        effects: Vec<SceneObjectEffectProgram>,
    ) {
        self.residency = SceneResourceResidencyPlan::from_resources(&resources);
        self.resources = resources;
        self.objects = objects;
        self.effects = effects;
    }

    pub fn resources(&self) -> &[SceneResource] {
        &self.resources
    }

    pub fn objects(&self) -> &[SceneObject] {
        &self.objects
    }

    pub fn effects(&self) -> &[SceneObjectEffectProgram] {
        &self.effects
    }

    pub fn draw<R: RendererSceneRender>(
        &self,
        renderer: &R,
        context: SceneFrameContext,
    ) -> Result<SceneFramePlan, String> {
        renderer.build_frame_with_residency(
            context,
            &self.residency,
            &self.resources,
            &self.objects,
            &self.effects,
        )
    }
}
