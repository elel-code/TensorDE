//! Engine-owned WE effect program facts.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/index.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/texture-and-format.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use std::collections::BTreeMap;

use serde::Serialize;

use super::{
    SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneGraphTarget, SceneObjectId,
    SceneResourceId, we::WeEffectKind,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneObjectEffectProgram {
    pub object: SceneObjectId,
    pub program: SceneEffectProgram,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneEffectProgram {
    pub effect_file: String,
    pub effect: WeEffectKind,
    pub fbos: Vec<SceneEffectFboBinding>,
    pub commands: Vec<SceneEffectCommand>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SceneEffectCommand {
    MaterialPass(SceneEffectMaterialPass),
    Copy(SceneEffectCopyCommand),
    Swap(SceneEffectSwapCommand),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneEffectMaterialPass {
    pub pass_index: usize,
    pub shader: Option<String>,
    pub source: Option<SceneEffectImageRef>,
    pub target: Option<SceneEffectImageRef>,
    pub blend: SceneEffectPassBlend,
    pub depth_test: SceneDepthTest,
    pub depth_write: bool,
    pub cull_mode: SceneCullMode,
    pub alpha_write: SceneAlphaWriteMode,
    pub texture_resources: Vec<SceneEffectTextureResourceBinding>,
    pub binds: BTreeMap<u32, SceneEffectImageRef>,
    pub combos: BTreeMap<String, i64>,
    pub constants: BTreeMap<String, SceneEffectConstantValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneEffectSwapCommand {
    pub pass_index: usize,
    pub a: SceneEffectImageRef,
    pub b: SceneEffectImageRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneEffectCopyCommand {
    pub pass_index: usize,
    pub source: SceneEffectImageRef,
    pub target: SceneEffectImageRef,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneEffectFboBinding {
    pub name: String,
    pub target: SceneGraphTarget,
    pub format: Option<SceneEffectFboFormat>,
    pub scale: f32,
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneEffectTextureResourceBinding {
    pub slot: u32,
    pub resource: SceneResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SceneEffectImageRef {
    PreviousFramebuffer,
    SourceTexture,
    Scene,
    NamedFbo(String),
    GraphTarget(SceneGraphTarget),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SceneEffectFboFormat {
    Rgba16Float,
    Rg16Float,
    R16Float,
    R8Unorm,
    Rgba8Unorm,
    RgbaBackbuffer,
    RgbBackbuffer,
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SceneEffectPassBlend {
    NormalReplace,
    TranslucentAlpha,
    Additive,
    AlphaToCoverage,
    Disabled,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SceneEffectConstantValue {
    Bool(bool),
    Float(f32),
    Integer(i64),
    String(String),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
}

impl SceneEffectProgram {
    pub fn material_pass_count(&self) -> usize {
        self.commands
            .iter()
            .filter(|command| matches!(command, SceneEffectCommand::MaterialPass(_)))
            .count()
    }

    pub fn swap_command_count(&self) -> usize {
        self.commands
            .iter()
            .filter(|command| matches!(command, SceneEffectCommand::Swap(_)))
            .count()
    }

    pub fn copy_command_count(&self) -> usize {
        self.commands
            .iter()
            .filter(|command| matches!(command, SceneEffectCommand::Copy(_)))
            .count()
    }
}

impl SceneEffectImageRef {
    pub fn from_we_name(name: &str) -> Self {
        let normalized = name.trim().replace('\\', "/").to_ascii_lowercase();
        match normalized.as_str() {
            "previous" | "previousframe" | "previous_frame" | "backbuffer" => {
                Self::PreviousFramebuffer
            }
            "source" | "sourceimage" | "source_image" | "g_texture0" => Self::SourceTexture,
            "scene" | "swapchain" => Self::Scene,
            _ => Self::NamedFbo(name.to_owned()),
        }
    }
}

impl SceneEffectFboFormat {
    pub fn from_we_name(format: &str) -> Self {
        let normalized = format.trim().replace('-', "_").to_ascii_lowercase();
        match normalized.as_str() {
            "rgba16f" | "rgba16161616f" | "rgba16_float" => Self::Rgba16Float,
            "rg1616f" | "rg16f" | "rg16_float" => Self::Rg16Float,
            "r16f" | "r16_float" => Self::R16Float,
            "r8" | "r8_unorm" | "r8unorm" => Self::R8Unorm,
            "rgba8888" | "rgba8" | "rgba8_unorm" => Self::Rgba8Unorm,
            "rgba_backbuffer" | "backbuffer" => Self::RgbaBackbuffer,
            "rgb_backbuffer" => Self::RgbBackbuffer,
            _ => Self::Other(format.to_owned()),
        }
    }
}

impl SceneEffectPassBlend {
    pub fn from_we_name(name: Option<&str>) -> Self {
        let Some(name) = name else {
            return Self::NormalReplace;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "normal" | "replace" => Self::NormalReplace,
            "alpha" | "translucent" | "alphablend" => Self::TranslucentAlpha,
            "add" | "additive" => Self::Additive,
            "alphatocoverage" | "alpha-to-coverage" => Self::AlphaToCoverage,
            "disabled" | "none" | "off" => Self::Disabled,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_image_ref_maps_we_framebuffer_names() {
        assert_eq!(
            SceneEffectImageRef::from_we_name("previous"),
            SceneEffectImageRef::PreviousFramebuffer
        );
        assert_eq!(
            SceneEffectImageRef::from_we_name("g_Texture0"),
            SceneEffectImageRef::SourceTexture
        );
        assert_eq!(
            SceneEffectImageRef::from_we_name("_rt_SmokeVelocity1"),
            SceneEffectImageRef::NamedFbo("_rt_SmokeVelocity1".to_owned())
        );
        assert_eq!(
            SceneEffectImageRef::GraphTarget(SceneGraphTarget::ObjectFinal(SceneObjectId(7))),
            SceneEffectImageRef::GraphTarget(SceneGraphTarget::ObjectFinal(SceneObjectId(7)))
        );
    }

    #[test]
    fn effect_fbo_format_maps_we_formats_without_guessing_unknowns() {
        assert_eq!(
            SceneEffectFboFormat::from_we_name("rgba16f"),
            SceneEffectFboFormat::Rgba16Float
        );
        assert_eq!(
            SceneEffectFboFormat::from_we_name("rg1616f"),
            SceneEffectFboFormat::Rg16Float
        );
        assert_eq!(
            SceneEffectFboFormat::from_we_name("r8_unorm"),
            SceneEffectFboFormat::R8Unorm
        );
        assert_eq!(
            SceneEffectFboFormat::from_we_name("rgb_backbuffer"),
            SceneEffectFboFormat::RgbBackbuffer
        );
        assert_eq!(
            SceneEffectFboFormat::from_we_name("vendor-custom"),
            SceneEffectFboFormat::Other("vendor-custom".to_owned())
        );
    }

    #[test]
    fn effect_pass_blend_maps_we_low_modes() {
        assert_eq!(
            SceneEffectPassBlend::from_we_name(Some("normal")),
            SceneEffectPassBlend::NormalReplace
        );
        assert_eq!(
            SceneEffectPassBlend::from_we_name(Some("translucent")),
            SceneEffectPassBlend::TranslucentAlpha
        );
        assert_eq!(
            SceneEffectPassBlend::from_we_name(Some("additive")),
            SceneEffectPassBlend::Additive
        );
        assert_eq!(
            SceneEffectPassBlend::from_we_name(Some("alphatocoverage")),
            SceneEffectPassBlend::AlphaToCoverage
        );
    }

    #[test]
    fn effect_program_counts_material_copy_and_swap_commands() {
        let program = SceneEffectProgram {
            effect_file: "effects/fluidsimulation/effect.json".to_owned(),
            effect: WeEffectKind::Unknown,
            fbos: Vec::new(),
            commands: vec![
                SceneEffectCommand::MaterialPass(SceneEffectMaterialPass {
                    pass_index: 0,
                    shader: Some("effects/fluidsimulation_curl".to_owned()),
                    source: Some(SceneEffectImageRef::NamedFbo(
                        "_rt_SmokeVelocity1".to_owned(),
                    )),
                    target: Some(SceneEffectImageRef::NamedFbo("_rt_SmokeCurl".to_owned())),
                    blend: SceneEffectPassBlend::NormalReplace,
                    depth_test: SceneDepthTest::Disabled,
                    depth_write: false,
                    cull_mode: SceneCullMode::None,
                    alpha_write: SceneAlphaWriteMode::Default,
                    texture_resources: Vec::new(),
                    binds: BTreeMap::new(),
                    combos: BTreeMap::new(),
                    constants: BTreeMap::new(),
                }),
                SceneEffectCommand::Copy(SceneEffectCopyCommand {
                    pass_index: 1,
                    source: SceneEffectImageRef::NamedFbo("_rt_FullCompoBuffer2".to_owned()),
                    target: SceneEffectImageRef::NamedFbo("_rt_FullCompoBuffer1".to_owned()),
                }),
                SceneEffectCommand::Swap(SceneEffectSwapCommand {
                    pass_index: 2,
                    a: SceneEffectImageRef::NamedFbo("_rt_SmokeVelocity2".to_owned()),
                    b: SceneEffectImageRef::NamedFbo("_rt_SmokeVelocity1".to_owned()),
                }),
            ],
        };

        assert_eq!(program.material_pass_count(), 1);
        assert_eq!(program.copy_command_count(), 1);
        assert_eq!(program.swap_command_count(), 1);
    }
}
