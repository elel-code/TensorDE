//! Wallpaper Engine semantic layer.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/shaders/effects/waterwaves.frag`
//! - `reverse-engineered/shaders/effects/waterripple.frag`
//! - `reverse-engineered/shaders/effects/waterflow.frag`

pub mod effect;
pub mod image_graph;
pub mod pass;
pub mod shader;
pub mod target;

pub use effect::{WeEffectKind, WeEffectOutputContract};
pub use image_graph::{WeImageGraph, WeImageGraphStep};
pub use pass::{WePassBlendMove, WePassRole};
pub use shader::WeShaderContract;
pub use target::WeTarget;
