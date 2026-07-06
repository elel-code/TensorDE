//! Scene object model after WE parsing and before render graph lowering.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/scene-and-object.md`

use super::{SceneGeometryId, SceneMaterialContract, ScenePuppetId, SceneResourceId};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct SceneObjectId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SceneObjectGeometry {
    Quad,
    Mesh {
        geometry: SceneGeometryId,
        vertex_count: u32,
        index_count: u32,
    },
    Puppet {
        geometry: SceneGeometryId,
        puppet: ScenePuppetId,
        vertex_count: u32,
        index_count: u32,
    },
    ParticleEmitter,
}

impl SceneObjectGeometry {
    pub fn index_count(&self) -> u32 {
        match self {
            Self::Quad | Self::ParticleEmitter => 6,
            Self::Mesh { index_count, .. } | Self::Puppet { index_count, .. } => {
                (*index_count).max(1)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneObject {
    pub id: SceneObjectId,
    pub geometry: SceneObjectGeometry,
    pub material: SceneMaterialContract,
    pub source: Option<SceneResourceId>,
}
