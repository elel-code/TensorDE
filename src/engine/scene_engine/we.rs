//! Wallpaper Engine semantic layer.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `reverse-engineered/docs/exe/model-and-animation.md`
//! - `reverse-engineered/shaders/effects/waterwaves.frag`
//! - `reverse-engineered/shaders/effects/waterripple.frag`
//! - `reverse-engineered/shaders/effects/waterflow.frag`

pub mod effect;
pub mod image_graph;
pub mod pass;
pub mod shader;
pub mod target;
pub mod vec4;

pub use effect::{WeEffectKind, WeEffectOutputContract};
pub use image_graph::{WeImageGraph, WeImageGraphStep};
pub use pass::{WePassBlendMove, WePassRole};
pub use shader::{
    WeShaderCombo, WeShaderContract, WeShaderInterface, WeShaderStage, WeShaderTextureRequirement,
    WeShaderTextureSlot, WeShaderUniform, WeShaderUniformKind,
};
pub use target::WeTarget;
pub use vec4::{WE_VEC4_BYTES, WE_VEC4_LANES, WeVec4};
