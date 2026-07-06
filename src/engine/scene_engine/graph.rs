//! Backend-independent frame graph.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use super::{SceneGeometryId, SceneMaterialKey, SceneObjectId, ScenePuppetId, SceneResourceId};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SceneGraphTarget {
    Swapchain,
    ImageLocalMain(u32),
    ImageLocalSub(u32),
    NamedFbo(u32),
    EffectTarget(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SceneGraphPipelineClass {
    Quad,
    Mesh,
    PuppetSkinning,
    ParticleEmitter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SceneGraphResourceRole {
    BaseColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneGraphResourceBinding {
    pub slot: u32,
    pub role: SceneGraphResourceRole,
    pub resource: SceneResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneGraphDraw {
    pub object: SceneObjectId,
    pub pipeline: SceneGraphPipelineClass,
    pub material: SceneMaterialKey,
    pub geometry: Option<SceneGeometryId>,
    pub puppet: Option<ScenePuppetId>,
    pub resources: Vec<SceneGraphResourceBinding>,
    pub index_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneGraphPass {
    pub name: String,
    pub input: Option<SceneGraphTarget>,
    pub output: SceneGraphTarget,
    pub draws: Vec<SceneGraphDraw>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SceneGraph {
    pub passes: Vec<SceneGraphPass>,
}
