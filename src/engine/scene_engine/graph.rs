//! Backend-independent frame graph.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/shaders/genericimage4.frag`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use super::{SceneGeometryId, SceneMaterialKey, SceneObjectId, ScenePuppetId, SceneResourceId};
use serde::Serialize;

pub const SCENE_WE_MAX_SHADER_TEXTURE_SLOTS: u32 = 32;
pub const SCENE_WE_PASS_INPUT_TEXTURE_SLOT: u32 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SceneGraphTarget {
    Swapchain,
    ImageLocalMain(u32),
    ImageLocalSub(u32),
    NamedFbo(u32),
    EffectTarget(u32),
    ImageLayerCompositeA(SceneObjectId),
    ImageLayerSource(SceneObjectId),
    ObjectFinal(SceneObjectId),
    FullAlphaMask,
    FullAlphaMaskIntermediate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SceneGraphPipelineClass {
    Quad,
    Mesh,
    PuppetSkinning,
    LayerUtilityIndexed,
    ParticleEmitter,
}

impl SceneGraphPipelineClass {
    pub const fn is_indexed_mesh_graphics(self) -> bool {
        matches!(self, Self::Mesh | Self::PuppetSkinning)
    }

    pub const fn uses_scene_mesh_vertex_layout(self) -> bool {
        matches!(self, Self::Mesh | Self::PuppetSkinning)
    }

    pub const fn is_layer_utility_indexed(self) -> bool {
        matches!(self, Self::LayerUtilityIndexed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SceneGraphResourceRole {
    ShaderTexture { index: u32 },
}

impl SceneGraphResourceRole {
    pub const fn shader_texture(index: u32) -> Self {
        Self::ShaderTexture { index }
    }

    pub const fn shader_texture_index(self) -> u32 {
        match self {
            Self::ShaderTexture { index } => index,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
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

impl SceneGraphDraw {
    pub fn shader_texture_slot_mask(&self) -> Result<u32, String> {
        self.shader_texture_slot_mask_with_pass_input(None)
    }

    pub fn shader_texture_slot_mask_with_pass_input(
        &self,
        pass_input: Option<SceneGraphTarget>,
    ) -> Result<u32, String> {
        let mut mask = 0u32;
        for binding in &self.resources {
            if binding.slot >= SCENE_WE_MAX_SHADER_TEXTURE_SLOTS {
                return Err(format!(
                    "scene draw {:?} shader texture slot {} exceeds WE slot mask width {}",
                    self.object, binding.slot, SCENE_WE_MAX_SHADER_TEXTURE_SLOTS
                ));
            }
            let texture_index = binding.role.shader_texture_index();
            if texture_index != binding.slot {
                return Err(format!(
                    "scene draw {:?} resource slot {} does not match WE g_Texture{} role",
                    self.object, binding.slot, texture_index
                ));
            }
            let bit = 1u32 << binding.slot;
            if mask & bit != 0 {
                return Err(format!(
                    "scene draw {:?} has duplicate WE g_Texture{} binding",
                    self.object, binding.slot
                ));
            }
            mask |= bit;
        }
        if let Some(input) = pass_input {
            let bit = 1u32 << SCENE_WE_PASS_INPUT_TEXTURE_SLOT;
            if mask & bit != 0 {
                return Err(format!(
                    "scene draw {:?} pass input {:?} collides with WE g_Texture{} binding",
                    self.object, input, SCENE_WE_PASS_INPUT_TEXTURE_SLOT
                ));
            }
            mask |= bit;
        }
        self.material.shader_texture_slot_mask(mask)
    }
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
