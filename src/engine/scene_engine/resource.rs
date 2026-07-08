//! Engine-owned scene resources.
//!
//! References:
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `references/godot/servers/rendering/rendering_device.h`

use std::path::PathBuf;

use serde::Serialize;

use crate::core::scene::{
    SceneMeshSkin, SceneMeshVertex, ScenePuppetAnimationClip, ScenePuppetAnimationLayer,
};

use super::object::SceneObjectId;
use super::{SceneLayerAuxCompositeTargets, ScenePuppetClippingProgram};

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
    LayerAlphaMaskRtMethod8MdlvGeometry {
        geometry: SceneLayerAlphaMaskRtMethod8MdlvGeometry,
    },
    LayerAuxCompositeTargets {
        targets: SceneLayerAuxCompositeTargets,
    },
    PuppetRig {
        id: ScenePuppetId,
        source_record: u32,
        skin: Option<SceneMeshSkin>,
        clips: Vec<ScenePuppetAnimationClip>,
        layers: Vec<ScenePuppetAnimationLayer>,
        clipping: ScenePuppetClippingProgram,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneLayerAlphaMaskRtMethod8MdlvGeometry {
    pub object: SceneObjectId,
    pub entry_owner_index: u32,
    pub layout_key: u32,
    pub vertex_stride_bytes: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub vertex_payload: Vec<u8>,
    pub index_payload: Vec<u8>,
    pub source_records: Vec<SceneLayerAlphaMaskRtMethod8MdlvSourceRecord>,
    pub subdraws: Vec<SceneLayerAlphaMaskRtMethod8MdlvSubdraw>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneLayerAlphaMaskRtMethod8MdlvSourceRecord {
    pub source_index: u32,
    pub local_offset: u32,
    pub index_span_offset: u32,
    pub index_span_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneLayerAlphaMaskRtMethod8MdlvSubdraw {
    pub source_qword: u64,
    pub mask_resource: String,
    pub raw_flags: u32,
    pub first_indices: Vec<u32>,
    pub second_indices: Vec<u32>,
    pub link: u32,
}
