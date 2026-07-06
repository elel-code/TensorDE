//! WE shader-level contracts.
//!
//! References:
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/shaders/common_blending.h`
//! - `reverse-engineered/shaders/effects/waterwaves.frag`
//! - `reverse-engineered/shaders/effects/waterripple.frag`
//! - `reverse-engineered/shaders/effects/waterflow.frag`

use super::{WeEffectKind, WeEffectOutputContract};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WeShaderContract {
    pub effect: WeEffectKind,
    pub output: WeEffectOutputContract,
    pub applies_vertex_tint_in_effect_shader: bool,
}

impl WeShaderContract {
    pub fn from_effect(effect: WeEffectKind) -> Self {
        let output = effect.output_contract();
        Self {
            effect,
            output,
            applies_vertex_tint_in_effect_shader: !matches!(
                output,
                WeEffectOutputContract::SourcePreserving
            ),
        }
    }
}
