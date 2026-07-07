//! WE material and blend contracts.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/blending-modes.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`

use serde::Serialize;

use super::we::WeShaderInterface;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SceneBlendContract {
    NormalReplace,
    TranslucentAlpha,
    Additive,
    AlphaToCoverage,
    DestColorCopyBackBit0x100,
    ShaderColorBlend(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SceneDepthTest {
    Disabled,
    Less,
    LessEqual,
    Equal,
    NotEqual,
    Greater,
    Never,
}

impl SceneDepthTest {
    pub const fn enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SceneCullMode {
    None,
    Front,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SceneAlphaWriteMode {
    Default,
    Enabled,
    Disabled,
}

impl SceneAlphaWriteMode {
    pub const fn writes_alpha(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct SceneMaterialRenderState {
    pub depth_test: SceneDepthTest,
    pub depth_write: bool,
    pub cull_mode: SceneCullMode,
    pub alpha_write: SceneAlphaWriteMode,
}

impl SceneMaterialRenderState {
    pub const fn translucent_2d() -> Self {
        Self {
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
        }
    }
}

impl Default for SceneMaterialRenderState {
    fn default() -> Self {
        Self::translucent_2d()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct SceneMaterialKey {
    pub shader: String,
    pub blend: SceneBlendContract,
    pub render_state: SceneMaterialRenderState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneMaterialContract {
    pub shader: String,
    pub blend: SceneBlendContract,
    pub render_state: SceneMaterialRenderState,
}

impl SceneMaterialContract {
    pub fn key(&self) -> SceneMaterialKey {
        SceneMaterialKey {
            shader: self.shader.clone(),
            blend: self.blend,
            render_state: self.render_state,
        }
    }

    pub fn we_translucent(shader: impl Into<String>) -> Self {
        Self {
            shader: shader.into(),
            blend: SceneBlendContract::TranslucentAlpha,
            render_state: SceneMaterialRenderState::translucent_2d(),
        }
    }
}

impl SceneMaterialKey {
    pub fn shader_texture_slot_mask(&self, resource_slot_mask: u32) -> Result<u32, String> {
        let interface = WeShaderInterface::for_shader(&self.shader).ok_or_else(|| {
            format!(
                "scene material references unknown WE shader '{}'",
                self.shader
            )
        })?;
        interface.texture_slot_mask_for_material(&self.shader, resource_slot_mask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_write_default_inherits_rgba_wrapper_state() {
        assert!(SceneAlphaWriteMode::Default.writes_alpha());
        assert!(SceneAlphaWriteMode::Enabled.writes_alpha());
        assert!(!SceneAlphaWriteMode::Disabled.writes_alpha());
    }
}
