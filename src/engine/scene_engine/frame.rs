//! Per-frame scene sampling and immutable frame plan.
//!
//! References:
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `reverse-engineered/docs/exe/model-and-animation.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;

use super::{
    SceneEffectPassGraphPlan, SceneEffectUniformFramePlan, SceneFinalCompositorPlan, SceneGraph,
    SceneLayerCompositorPlan, SceneResourceResidencyPlan,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneFrameContext {
    pub time_ms: u64,
    pub target_width: u32,
    pub target_height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneFramePlan {
    pub residency: SceneResourceResidencyPlan,
    pub graph: SceneGraph,
    pub effect_pass_graph: SceneEffectPassGraphPlan,
    pub effect_uniforms: SceneEffectUniformFramePlan,
    pub final_compositor: SceneFinalCompositorPlan,
    pub layer_compositor: SceneLayerCompositorPlan,
}
