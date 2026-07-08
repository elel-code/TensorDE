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
pub const WE_AUX_CLEAR_SOURCE_DIMENSION_REGION: &str = "0x14020a2f1..0x14020a33c";
pub const WE_AUX_CLEAR_UV_FLIP_FLAG_SOURCE: &str = "[[layer+0xc8]+0x118] bit 0";

pub const WE_LAYER_AUX_CLEAR_TARGET_OFFSET: u32 = 0x3e8;
pub const WE_LAYER_AUX_MATERIAL_TARGET_OFFSET: u32 = 0x3f0;
pub const WE_LAYER_AUX_EFFECT_TARGET_OFFSET: u32 = 0x3f8;
pub const WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET: u32 = 0x408;
pub const WE_LAYER_AUX_CLEAR_MATERIAL_OFFSET: u32 = 0x410;

pub const WE_LAYER_AUX_CLEAR_TARGET_DEFAULT_COLOR_FORMAT: u32 = 0;
pub const WE_LAYER_AUX_CLEAR_TARGET_HDR_COLOR_FORMAT: u32 = 0x0e;
pub const WE_LAYER_AUX_CLEAR_TARGET_AUX_FORMAT: u32 = 0x1b;
pub const WE_LAYER_AUX_CLEAR_TARGET_R9_SELECTOR: u32 = 1;
pub const WE_LAYER_AUX_CLEAR_TARGET_RESOURCE_SELECTOR: u32 = 2;
pub const WE_LAYER_AUX_CLEAR_TARGET_CACHE_SELECTOR: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneLayerAuxCompositeTargets {
    pub object: SceneObjectId,
    pub clear_target_3e8: bool,
    pub material_target_3f0: bool,
    pub effect_target_3f8: bool,
    pub generated_material_408: bool,
    pub clear_material_410: bool,
    pub clear_source_width: u32,
    pub clear_source_height: u32,
    pub clear_target_width: u32,
    pub clear_target_height: u32,
    pub clear_uv_y_flipped: bool,
    pub clear_target_color_format: u32,
    pub clear_target_aux_format: u32,
    pub clear_target_r9_selector: u32,
    pub clear_target_resource_selector: u32,
    pub clear_target_cache_selector: u32,
}

impl SceneLayerAuxCompositeTargets {
    pub const fn clear_prep_ready(self) -> bool {
        self.clear_target_3e8
            && self.material_target_3f0
            && self.effect_target_3f8
            && self.generated_material_408
            && self.clear_material_410
            && self.clear_source_width != 0
            && self.clear_source_height != 0
            && self.clear_target_width != 0
            && self.clear_target_height != 0
            && (self.clear_target_color_format == WE_LAYER_AUX_CLEAR_TARGET_DEFAULT_COLOR_FORMAT
                || self.clear_target_color_format == WE_LAYER_AUX_CLEAR_TARGET_HDR_COLOR_FORMAT)
            && self.clear_target_aux_format == WE_LAYER_AUX_CLEAR_TARGET_AUX_FORMAT
            && self.clear_target_r9_selector == WE_LAYER_AUX_CLEAR_TARGET_R9_SELECTOR
            && self.clear_target_resource_selector == WE_LAYER_AUX_CLEAR_TARGET_RESOURCE_SELECTOR
            && self.clear_target_cache_selector == WE_LAYER_AUX_CLEAR_TARGET_CACHE_SELECTOR
    }
}
