//! WE auxiliary layer target facts owned by the scene engine.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/reconstructed/cpp/wallpaper64/layer/resource_update_0x1402065e0.cpp`

use serde::Serialize;

use super::SceneObjectId;

pub const WE_AUX_CLEAR_PREP_VMA: u64 = 0x140207740;
pub const WE_AUX_CLEAR_TARGET_CREATE_VMA: u64 = 0x14020a07b;
pub const WE_AUX_CLEAR_TARGET_STORE_VMA: u64 = 0x14020a083;
pub const WE_AUX_CLEAR_TARGET_RELEASE_ZERO_VMA: u64 = 0x14020a573;

pub const WE_LAYER_AUX_CLEAR_TARGET_OFFSET: u32 = 0x3e8;
pub const WE_LAYER_AUX_MATERIAL_TARGET_OFFSET: u32 = 0x3f0;
pub const WE_LAYER_AUX_EFFECT_TARGET_OFFSET: u32 = 0x3f8;
pub const WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET: u32 = 0x408;
pub const WE_LAYER_AUX_CLEAR_MATERIAL_OFFSET: u32 = 0x410;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneLayerAuxCompositeTargets {
    pub object: SceneObjectId,
    pub clear_target_3e8: bool,
    pub material_target_3f0: bool,
    pub effect_target_3f8: bool,
    pub generated_material_408: bool,
    pub clear_material_410: bool,
}

impl SceneLayerAuxCompositeTargets {
    pub const fn clear_prep_ready(self) -> bool {
        self.clear_target_3e8
            && self.material_target_3f0
            && self.effect_target_3f8
            && self.generated_material_408
            && self.clear_material_410
    }
}
