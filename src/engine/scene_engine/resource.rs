//! Engine-owned scene resources.
//!
//! References:
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `references/godot/servers/rendering/rendering_device.h`

use std::path::PathBuf;

use serde::Serialize;

use crate::core::scene::{
    SceneMeshPuppetClippingRecord, SceneMeshSkin, SceneMeshVertex, ScenePuppetAnimationClip,
    ScenePuppetAnimationLayer,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct SceneResourceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct SceneGeometryId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ScenePuppetId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SceneTextureFormat {
    Bc1RgbaUnormBlock,
    Bc3UnormBlock,
    Bc7UnormBlock,
    R8Unorm,
    R8G8B8A8Unorm,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SceneResource {
    Texture {
        id: SceneResourceId,
        source: PathBuf,
        width: Option<u32>,
        height: Option<u32>,
        format: Option<SceneTextureFormat>,
        mip_count: Option<u32>,
        payload_bytes: Option<u64>,
    },
    Buffer {
        id: SceneResourceId,
        bytes: u64,
    },
    MeshGeometry {
        id: SceneGeometryId,
        source_record: u32,
        vertices: Vec<SceneMeshVertex>,
        indices: Vec<u32>,
    },
    PuppetRig {
        id: ScenePuppetId,
        source_record: u32,
        skin: Option<SceneMeshSkin>,
        clips: Vec<ScenePuppetAnimationClip>,
        layers: Vec<ScenePuppetAnimationLayer>,
        clipping_records: Vec<SceneMeshPuppetClippingRecord>,
    },
}
