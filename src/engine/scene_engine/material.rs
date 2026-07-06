//! WE material and blend contracts.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/blending-modes.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SceneBlendContract {
    NormalReplace,
    TranslucentAlpha,
    Additive,
    ShaderColorBlend(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct SceneMaterialKey {
    pub shader: String,
    pub blend: SceneBlendContract,
    pub writes_depth: bool,
    pub tests_depth: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneMaterialContract {
    pub shader: String,
    pub blend: SceneBlendContract,
    pub writes_depth: bool,
    pub tests_depth: bool,
}

impl SceneMaterialContract {
    pub fn key(&self) -> SceneMaterialKey {
        SceneMaterialKey {
            shader: self.shader.clone(),
            blend: self.blend,
            writes_depth: self.writes_depth,
            tests_depth: self.tests_depth,
        }
    }

    pub fn we_translucent(shader: impl Into<String>) -> Self {
        Self {
            shader: shader.into(),
            blend: SceneBlendContract::TranslucentAlpha,
            writes_depth: false,
            tests_depth: false,
        }
    }
}
