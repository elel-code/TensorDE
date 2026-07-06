//! WE pass roles and first-pass/final-pass blend movement.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WePassRole {
    BaseMaterial,
    EffectMaterial,
    ColorBlendPassthrough,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WePassBlendMove {
    KeepOnFirstPass,
    MoveToFinalScenePass,
}
