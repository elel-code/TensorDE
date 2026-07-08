//! WE shader-level contracts.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/shaders/genericimage4.vert`
//! - `reverse-engineered/shaders/genericimage4.frag`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.frag`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.vert`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.frag`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/passthrough.vert`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/passthrough.frag`
//! - `reverse-engineered/shaders/common_blending.h`
//! - `reverse-engineered/shaders/effects/waterwaves.frag`
//! - `reverse-engineered/shaders/effects/waterripple.frag`
//! - `reverse-engineered/shaders/effects/waterflow.frag`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/shaders/effects/iris.vert`
//! - `reverse-engineered/shaders/effects/iris.frag`
//! - `reverse-engineered/docs/particle-format.md`
//! - `reverse-engineered/shaders/genericparticle.vert`
//! - `reverse-engineered/shaders/genericparticle.frag`

use super::vec4::WE_VEC4_BYTES;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WeShaderStage {
    Vertex,
    Fragment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WeShaderTextureRequirement {
    Required,
    ComboDependent,
    RuntimeTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WeShaderTextureSlot {
    pub slot: u32,
    pub name: &'static str,
    pub stage: WeShaderStage,
    pub requirement: WeShaderTextureRequirement,
    pub reference: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WeShaderUniformKind {
    Mat4,
    Mat3,
    Vec4,
    Vec3,
    Vec2,
    Float,
    UintArray,
    Mat4x3Array,
}

impl WeShaderUniformKind {
    pub const fn we_abi_bytes(self) -> Option<u64> {
        match self {
            Self::Vec4 => Some(WE_VEC4_BYTES),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WeShaderUniform {
    pub name: &'static str,
    pub kind: WeShaderUniformKind,
    pub stage: WeShaderStage,
    pub material_key: Option<&'static str>,
    pub reference: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WeShaderCombo {
    pub name: &'static str,
    pub default_value: u32,
    pub material_key: Option<&'static str>,
    pub reference: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WeShaderInterface {
    pub shader: &'static str,
    pub textures: &'static [WeShaderTextureSlot],
    pub uniforms: &'static [WeShaderUniform],
    pub combos: &'static [WeShaderCombo],
}

impl WeShaderInterface {
    pub fn for_shader(shader: &str) -> Option<&'static Self> {
        match shader {
            "we/genericimage4" | "genericimage4" => Some(&GENERICIMAGE4_INTERFACE),
            "we/clippingmaskimage4" | "clippingmaskimage4" => Some(&CLIPPINGMASKIMAGE4_INTERFACE),
            "we/genericparticle" | "genericparticle" => Some(&GENERICPARTICLE_INTERFACE),
            "util/minimalalpha" | "minimalalpha" => Some(&MINIMALALPHA_INTERFACE),
            "util/passthrough" | "passthrough" => Some(&PASSTHROUGH_INTERFACE),
            _ => None,
        }
    }

    pub fn for_effect_shader(shader: &str) -> Option<&'static Self> {
        match shader {
            "effects/iris" => Some(&IRIS_INTERFACE),
            "util/minimalalpha" | "minimalalpha" => Some(&MINIMALALPHA_INTERFACE),
            "util/passthrough" | "passthrough" => Some(&PASSTHROUGH_INTERFACE),
            _ => None,
        }
    }

    pub fn declares_combo(&self, name: &str) -> bool {
        self.combos.iter().any(|combo| combo.name == name)
    }

    pub fn declared_texture_slot_mask(&self) -> u32 {
        texture_slot_mask(self.textures.iter().map(|slot| slot.slot))
    }

    pub fn required_texture_slot_mask(&self) -> u32 {
        texture_slot_mask(self.textures.iter().filter_map(|slot| {
            (slot.requirement == WeShaderTextureRequirement::Required).then_some(slot.slot)
        }))
    }

    pub fn texture_slot_mask_for_material(
        &self,
        shader: &str,
        resource_slot_mask: u32,
    ) -> Result<u32, String> {
        let declared = self.declared_texture_slot_mask();
        let unknown = resource_slot_mask & !declared;
        if unknown != 0 {
            return Err(format!(
                "WE shader '{shader}' received texture slots outside shader interface mask {declared:#010x}: {unknown:#010x}"
            ));
        }

        let required = self.required_texture_slot_mask();
        let missing = required & !resource_slot_mask;
        if missing != 0 {
            return Err(format!(
                "WE shader '{shader}' requires texture slots {required:#010x}, missing {missing:#010x}"
            ));
        }

        Ok(required | resource_slot_mask)
    }
}

const fn slot_bit(slot: u32) -> u32 {
    1u32 << slot
}

fn texture_slot_mask(slots: impl Iterator<Item = u32>) -> u32 {
    slots.fold(0u32, |mask, slot| mask | slot_bit(slot))
}

pub static GENERICIMAGE4_TEXTURES: &[WeShaderTextureSlot] = &[
    WeShaderTextureSlot {
        slot: 0,
        name: "g_Texture0",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::Required,
        reference: "reverse-engineered/shaders/genericimage4.frag:22",
    },
    WeShaderTextureSlot {
        slot: 1,
        name: "g_Texture1",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::ComboDependent,
        reference: "reverse-engineered/shaders/genericimage4.frag:38",
    },
    WeShaderTextureSlot {
        slot: 2,
        name: "g_Texture2",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::ComboDependent,
        reference: "reverse-engineered/shaders/genericimage4.frag:39",
    },
    WeShaderTextureSlot {
        slot: 3,
        name: "g_Texture3",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::RuntimeTarget,
        reference: "reverse-engineered/shaders/genericimage4.frag:68",
    },
    WeShaderTextureSlot {
        slot: 4,
        name: "g_Texture4",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::RuntimeTarget,
        reference: "reverse-engineered/shaders/genericimage4.frag:79",
    },
    WeShaderTextureSlot {
        slot: 5,
        name: "g_Texture5",
        stage: WeShaderStage::Vertex,
        requirement: WeShaderTextureRequirement::ComboDependent,
        reference: "reverse-engineered/shaders/genericimage4.vert:96",
    },
    WeShaderTextureSlot {
        slot: 6,
        name: "g_Texture6",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::RuntimeTarget,
        reference: "reverse-engineered/shaders/genericimage4.frag:9",
    },
    WeShaderTextureSlot {
        slot: 7,
        name: "g_Texture7",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::RuntimeTarget,
        reference: "reverse-engineered/shaders/genericimage4.frag:15",
    },
    WeShaderTextureSlot {
        slot: 8,
        name: "g_Texture8",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::RuntimeTarget,
        reference: "reverse-engineered/shaders/genericimage4.frag:87",
    },
];

pub static GENERICIMAGE4_UNIFORMS: &[WeShaderUniform] = &[
    WeShaderUniform {
        name: "g_ModelViewProjectionMatrix",
        kind: WeShaderUniformKind::Mat4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "reverse-engineered/shaders/genericimage4.vert:5",
    },
    WeShaderUniform {
        name: "g_Texture0Rotation",
        kind: WeShaderUniformKind::Vec4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "reverse-engineered/shaders/genericimage4.vert:6",
    },
    WeShaderUniform {
        name: "g_Texture0Translation",
        kind: WeShaderUniformKind::Vec2,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "reverse-engineered/shaders/genericimage4.vert:7",
    },
    WeShaderUniform {
        name: "g_Texture2Resolution",
        kind: WeShaderUniformKind::Vec4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "reverse-engineered/shaders/genericimage4.vert:39",
    },
    WeShaderUniform {
        name: "g_Color4",
        kind: WeShaderUniformKind::Vec4,
        stage: WeShaderStage::Fragment,
        material_key: None,
        reference: "reverse-engineered/shaders/genericimage4.frag:25",
    },
    WeShaderUniform {
        name: "g_Roughness",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Fragment,
        material_key: Some("roughness"),
        reference: "reverse-engineered/shaders/genericimage4.frag:40",
    },
    WeShaderUniform {
        name: "g_Metallic",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Fragment,
        material_key: Some("metallic"),
        reference: "reverse-engineered/shaders/genericimage4.frag:41",
    },
    WeShaderUniform {
        name: "g_SpecularTint",
        kind: WeShaderUniformKind::Vec3,
        stage: WeShaderStage::Fragment,
        material_key: Some("speculartint"),
        reference: "reverse-engineered/shaders/genericimage4.frag:42",
    },
    WeShaderUniform {
        name: "g_MorphOffsets",
        kind: WeShaderUniformKind::UintArray,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "reverse-engineered/shaders/genericimage4.vert:99",
    },
    WeShaderUniform {
        name: "g_MorphWeights",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "reverse-engineered/shaders/genericimage4.vert:100",
    },
];

pub static GENERICIMAGE4_COMBOS: &[WeShaderCombo] = &[
    WeShaderCombo {
        name: "LIGHTING",
        default_value: 0,
        material_key: Some("ui_editor_properties_lighting"),
        reference: "reverse-engineered/shaders/genericimage4.frag:2",
    },
    WeShaderCombo {
        name: "REFLECTION",
        default_value: 0,
        material_key: Some("ui_editor_properties_reflection"),
        reference: "reverse-engineered/shaders/genericimage4.frag:3",
    },
    WeShaderCombo {
        name: "FOG",
        default_value: 1,
        material_key: Some("ui_editor_properties_fog"),
        reference: "reverse-engineered/shaders/genericimage4.frag:4",
    },
    WeShaderCombo {
        name: "MORPHING",
        default_value: 0,
        material_key: None,
        reference: "reverse-engineered/shaders/genericimage4.vert:95",
    },
    WeShaderCombo {
        name: "SKINNING_ALPHA",
        default_value: 0,
        material_key: None,
        reference: "reverse-engineered/shaders/genericimage4.vert:91",
    },
    WeShaderCombo {
        name: "CLIPPINGUVS",
        default_value: 0,
        material_key: None,
        reference: "reverse-engineered/docs/exe/clipping-pipeline.md:171",
    },
    WeShaderCombo {
        name: "CLIPPINGTARGET",
        default_value: 0,
        material_key: None,
        reference: "reverse-engineered/docs/exe/clipping-pipeline.md:172",
    },
    WeShaderCombo {
        name: "ALPHATOCOVERAGE",
        default_value: 0,
        material_key: Some("blending"),
        reference: "reverse-engineered/shaders/genericimage4.frag:223",
    },
];

pub static GENERICIMAGE4_INTERFACE: WeShaderInterface = WeShaderInterface {
    shader: "we/genericimage4",
    textures: GENERICIMAGE4_TEXTURES,
    uniforms: GENERICIMAGE4_UNIFORMS,
    combos: GENERICIMAGE4_COMBOS,
};

pub static CLIPPINGMASKIMAGE4_TEXTURES: &[WeShaderTextureSlot] = &[
    WeShaderTextureSlot {
        slot: 0,
        name: "g_Texture0",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::Required,
        reference: "reverse-engineered/docs/exe/clipping-pipeline.md:225",
    },
    WeShaderTextureSlot {
        slot: 1,
        name: "g_Texture1",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::Required,
        reference: "reverse-engineered/docs/exe/clipping-pipeline.md:228",
    },
    WeShaderTextureSlot {
        slot: 5,
        name: "g_Texture5",
        stage: WeShaderStage::Vertex,
        requirement: WeShaderTextureRequirement::ComboDependent,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:31",
    },
];

pub static CLIPPINGMASKIMAGE4_UNIFORMS: &[WeShaderUniform] = &[
    WeShaderUniform {
        name: "g_ModelViewProjectionMatrix",
        kind: WeShaderUniformKind::Mat4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:4",
    },
    WeShaderUniform {
        name: "g_Texture0Rotation",
        kind: WeShaderUniformKind::Vec4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:5",
    },
    WeShaderUniform {
        name: "g_Texture0Translation",
        kind: WeShaderUniformKind::Vec2,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:6",
    },
    WeShaderUniform {
        name: "g_Bones",
        kind: WeShaderUniformKind::Mat4x3Array,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:9",
    },
    WeShaderUniform {
        name: "g_BonesAlpha",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:27",
    },
    WeShaderUniform {
        name: "g_Texture5Resolution",
        kind: WeShaderUniformKind::Vec4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:32",
    },
    WeShaderUniform {
        name: "g_MorphOffsets",
        kind: WeShaderUniformKind::UintArray,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:34",
    },
    WeShaderUniform {
        name: "g_MorphWeights",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:35",
    },
    WeShaderUniform {
        name: "g_MorphBoneTransform",
        kind: WeShaderUniformKind::Mat4x3Array,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:38",
    },
    WeShaderUniform {
        name: "g_MorphBoneRules",
        kind: WeShaderUniformKind::Vec3,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:39",
    },
    WeShaderUniform {
        name: "g_RenderVar0",
        kind: WeShaderUniformKind::Vec4,
        stage: WeShaderStage::Fragment,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.frag:5",
    },
];

pub static CLIPPINGMASKIMAGE4_COMBOS: &[WeShaderCombo] = &[
    WeShaderCombo {
        name: "SKINNING",
        default_value: 0,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:8",
    },
    WeShaderCombo {
        name: "SKINNING_ALPHA",
        default_value: 0,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:26",
    },
    WeShaderCombo {
        name: "MORPHING",
        default_value: 0,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:30",
    },
    WeShaderCombo {
        name: "MORPHING_MODIFIERS",
        default_value: 0,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:37",
    },
    WeShaderCombo {
        name: "SPRITESHEET",
        default_value: 0,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.vert:110",
    },
    WeShaderCombo {
        name: "ALPHATOCOVERAGE",
        default_value: 0,
        material_key: Some("blending"),
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/clippingmaskimage4.frag:24",
    },
];

pub static CLIPPINGMASKIMAGE4_INTERFACE: WeShaderInterface = WeShaderInterface {
    shader: "we/clippingmaskimage4",
    textures: CLIPPINGMASKIMAGE4_TEXTURES,
    uniforms: CLIPPINGMASKIMAGE4_UNIFORMS,
    combos: CLIPPINGMASKIMAGE4_COMBOS,
};

pub static MINIMALALPHA_TEXTURES: &[WeShaderTextureSlot] = &[WeShaderTextureSlot {
    slot: 0,
    name: "g_Texture0",
    stage: WeShaderStage::Fragment,
    requirement: WeShaderTextureRequirement::Required,
    reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.frag:4",
}];

pub static MINIMALALPHA_UNIFORMS: &[WeShaderUniform] = &[
    WeShaderUniform {
        name: "g_ModelViewProjectionMatrix",
        kind: WeShaderUniformKind::Mat4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.vert:3",
    },
    WeShaderUniform {
        name: "g_Alpha",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Fragment,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.frag:3",
    },
];

pub static MINIMALALPHA_COMBOS: &[WeShaderCombo] = &[];

pub static MINIMALALPHA_INTERFACE: WeShaderInterface = WeShaderInterface {
    shader: "util/minimalalpha",
    textures: MINIMALALPHA_TEXTURES,
    uniforms: MINIMALALPHA_UNIFORMS,
    combos: MINIMALALPHA_COMBOS,
};

pub static PASSTHROUGH_TEXTURES: &[WeShaderTextureSlot] = &[WeShaderTextureSlot {
    slot: 0,
    name: "g_Texture0",
    stage: WeShaderStage::Fragment,
    requirement: WeShaderTextureRequirement::Required,
    reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/passthrough.frag:4",
}];

pub static PASSTHROUGH_UNIFORMS: &[WeShaderUniform] = &[
    WeShaderUniform {
        name: "g_ModelViewProjectionMatrix",
        kind: WeShaderUniformKind::Mat4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/passthrough.vert:3",
    },
    WeShaderUniform {
        name: "g_Texture0Rotation",
        kind: WeShaderUniformKind::Vec4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/passthrough.vert:4",
    },
    WeShaderUniform {
        name: "g_Texture0Translation",
        kind: WeShaderUniformKind::Vec2,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/passthrough.vert:5",
    },
];

pub static PASSTHROUGH_COMBOS: &[WeShaderCombo] = &[
    WeShaderCombo {
        name: "SPRITESHEET",
        default_value: 0,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/passthrough.vert:18",
    },
    WeShaderCombo {
        name: "TRANSFORM",
        default_value: 0,
        material_key: None,
        reference: "artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/passthrough.vert:25",
    },
];

pub static PASSTHROUGH_INTERFACE: WeShaderInterface = WeShaderInterface {
    shader: "util/passthrough",
    textures: PASSTHROUGH_TEXTURES,
    uniforms: PASSTHROUGH_UNIFORMS,
    combos: PASSTHROUGH_COMBOS,
};

pub static GENERICPARTICLE_TEXTURES: &[WeShaderTextureSlot] = &[
    WeShaderTextureSlot {
        slot: 0,
        name: "g_Texture0",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::Required,
        reference: "reverse-engineered/shaders/genericparticle.frag:10",
    },
    WeShaderTextureSlot {
        slot: 1,
        name: "g_Texture1",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::ComboDependent,
        reference: "reverse-engineered/shaders/genericparticle.frag:16",
    },
    WeShaderTextureSlot {
        slot: 3,
        name: "g_Texture3",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::RuntimeTarget,
        reference: "reverse-engineered/shaders/genericparticle.frag:21",
    },
    WeShaderTextureSlot {
        slot: 4,
        name: "g_Texture4",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::RuntimeTarget,
        reference: "reverse-engineered/shaders/genericparticle.frag:42",
    },
    WeShaderTextureSlot {
        slot: 5,
        name: "g_Texture5",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::RuntimeTarget,
        reference: "reverse-engineered/shaders/genericparticle.frag:47",
    },
];

pub static GENERICPARTICLE_UNIFORMS: &[WeShaderUniform] = &[
    WeShaderUniform {
        name: "g_ModelViewProjectionMatrix",
        kind: WeShaderUniformKind::Mat4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "reverse-engineered/shaders/genericparticle.vert:66",
    },
    WeShaderUniform {
        name: "g_Texture0Resolution",
        kind: WeShaderUniformKind::Vec4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "reverse-engineered/shaders/genericparticle.vert:45",
    },
    WeShaderUniform {
        name: "g_RenderVar1",
        kind: WeShaderUniformKind::Vec4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "reverse-engineered/shaders/genericparticle.vert:43",
    },
    WeShaderUniform {
        name: "g_Overbright",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Fragment,
        material_key: Some("ui_editor_properties_overbright"),
        reference: "reverse-engineered/shaders/genericparticle.frag:11",
    },
    WeShaderUniform {
        name: "g_CutoutStart",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Fragment,
        material_key: Some("ui_editor_properties_cutout_start"),
        reference: "reverse-engineered/shaders/genericparticle.frag:12",
    },
    WeShaderUniform {
        name: "g_CutoutEnd",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Fragment,
        material_key: Some("ui_editor_properties_cutout_end"),
        reference: "reverse-engineered/shaders/genericparticle.frag:13",
    },
    WeShaderUniform {
        name: "g_CutoutOpacity",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Fragment,
        material_key: Some("ui_editor_properties_cutout_opacity"),
        reference: "reverse-engineered/shaders/genericparticle.frag:14",
    },
];

pub static GENERICPARTICLE_COMBOS: &[WeShaderCombo] = &[
    WeShaderCombo {
        name: "LIGHTING",
        default_value: 0,
        material_key: Some("ui_editor_properties_lighting"),
        reference: "reverse-engineered/shaders/genericparticle.frag:2",
    },
    WeShaderCombo {
        name: "DOUBLESIDEDLIGHTING",
        default_value: 0,
        material_key: Some("ui_editor_properties_double_sided_lighting"),
        reference: "reverse-engineered/shaders/genericparticle.frag:3",
    },
    WeShaderCombo {
        name: "FOG",
        default_value: 1,
        material_key: Some("ui_editor_properties_fog"),
        reference: "reverse-engineered/shaders/genericparticle.frag:4",
    },
    WeShaderCombo {
        name: "REFRACT",
        default_value: 0,
        material_key: Some("ui_editor_properties_refract"),
        reference: "reverse-engineered/shaders/genericparticle.frag:5",
    },
    WeShaderCombo {
        name: "CUTOUT",
        default_value: 0,
        material_key: Some("ui_editor_properties_cutout"),
        reference: "reverse-engineered/shaders/genericparticle.frag:6",
    },
    WeShaderCombo {
        name: "SPRITESHEET",
        default_value: 0,
        material_key: None,
        reference: "reverse-engineered/shaders/genericparticle.vert:47",
    },
    WeShaderCombo {
        name: "SPRITESHEETBLEND",
        default_value: 0,
        material_key: None,
        reference: "reverse-engineered/shaders/genericparticle.frag:68",
    },
    WeShaderCombo {
        name: "TRAILRENDERER",
        default_value: 0,
        material_key: None,
        reference: "reverse-engineered/shaders/genericparticle.vert:62",
    },
    WeShaderCombo {
        name: "THICKFORMAT",
        default_value: 0,
        material_key: None,
        reference: "reverse-engineered/shaders/genericparticle.vert:8",
    },
];

pub static GENERICPARTICLE_INTERFACE: WeShaderInterface = WeShaderInterface {
    shader: "we/genericparticle",
    textures: GENERICPARTICLE_TEXTURES,
    uniforms: GENERICPARTICLE_UNIFORMS,
    combos: GENERICPARTICLE_COMBOS,
};

pub static IRIS_TEXTURES: &[WeShaderTextureSlot] = &[
    WeShaderTextureSlot {
        slot: 0,
        name: "g_Texture0",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::Required,
        reference: "reverse-engineered/shaders/effects/iris.frag:6",
    },
    WeShaderTextureSlot {
        slot: 1,
        name: "g_Texture1",
        stage: WeShaderStage::Fragment,
        requirement: WeShaderTextureRequirement::ComboDependent,
        reference: "reverse-engineered/shaders/effects/iris.frag:7",
    },
];

pub static IRIS_UNIFORMS: &[WeShaderUniform] = &[
    WeShaderUniform {
        name: "g_ModelViewProjectionMatrix",
        kind: WeShaderUniformKind::Mat4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "reverse-engineered/shaders/effects/iris.vert:3",
    },
    WeShaderUniform {
        name: "g_Time",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "reverse-engineered/shaders/effects/iris.vert:4",
    },
    WeShaderUniform {
        name: "g_Scale",
        kind: WeShaderUniformKind::Vec2,
        stage: WeShaderStage::Vertex,
        material_key: Some("scale"),
        reference: "reverse-engineered/shaders/effects/iris.vert:6",
    },
    WeShaderUniform {
        name: "g_Speed",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Vertex,
        material_key: Some("speed"),
        reference: "reverse-engineered/shaders/effects/iris.vert:7",
    },
    WeShaderUniform {
        name: "g_Rough",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Vertex,
        material_key: Some("rough"),
        reference: "reverse-engineered/shaders/effects/iris.vert:8",
    },
    WeShaderUniform {
        name: "g_NoiseAmount",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Vertex,
        material_key: Some("noiseamount"),
        reference: "reverse-engineered/shaders/effects/iris.vert:9",
    },
    WeShaderUniform {
        name: "g_PhaseOffset",
        kind: WeShaderUniformKind::Float,
        stage: WeShaderStage::Vertex,
        material_key: Some("phase"),
        reference: "reverse-engineered/shaders/effects/iris.vert:10",
    },
    WeShaderUniform {
        name: "g_Texture1Resolution",
        kind: WeShaderUniformKind::Vec4,
        stage: WeShaderStage::Vertex,
        material_key: None,
        reference: "reverse-engineered/shaders/effects/iris.vert:13",
    },
    WeShaderUniform {
        name: "g_EyeColor",
        kind: WeShaderUniformKind::Vec3,
        stage: WeShaderStage::Fragment,
        material_key: Some("color"),
        reference: "reverse-engineered/shaders/effects/iris.frag:9",
    },
];

pub static IRIS_COMBOS: &[WeShaderCombo] = &[
    WeShaderCombo {
        name: "BACKGROUND",
        default_value: 0,
        material_key: Some("ui_editor_properties_background"),
        reference: "reverse-engineered/shaders/effects/iris.frag:1",
    },
    WeShaderCombo {
        name: "MASK",
        default_value: 0,
        material_key: Some("ui_editor_properties_opacity_mask"),
        reference: "reverse-engineered/shaders/effects/iris.frag:7",
    },
];

pub static IRIS_INTERFACE: WeShaderInterface = WeShaderInterface {
    shader: "effects/iris",
    textures: IRIS_TEXTURES,
    uniforms: IRIS_UNIFORMS,
    combos: IRIS_COMBOS,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genericimage4_interface_tracks_required_and_declared_texture_slots() {
        let interface = WeShaderInterface::for_shader("we/genericimage4").unwrap();

        assert_eq!(interface.required_texture_slot_mask(), 0b1);
        assert_eq!(interface.declared_texture_slot_mask(), 0b1_1111_1111);
        assert_eq!(
            interface
                .texture_slot_mask_for_material("we/genericimage4", 0b1_0001)
                .unwrap(),
            0b1_0001
        );
    }

    #[test]
    fn genericimage4_interface_exposes_generated_clipping_target_combos() {
        let interface = WeShaderInterface::for_shader("we/genericimage4").unwrap();
        let combo_names = interface
            .combos
            .iter()
            .map(|combo| combo.name)
            .collect::<Vec<_>>();

        assert!(combo_names.contains(&"CLIPPINGUVS"));
        assert!(combo_names.contains(&"CLIPPINGTARGET"));
        assert!(combo_names.contains(&"ALPHATOCOVERAGE"));
        assert!(
            interface
                .texture_slot_mask_for_material("we/genericimage4", 0b1_0000_0001)
                .is_ok()
        );
    }

    #[test]
    fn clippingmaskimage4_interface_tracks_mask_generator_slots_and_uniforms() {
        let interface = WeShaderInterface::for_shader("we/clippingmaskimage4").unwrap();

        assert_eq!(interface.required_texture_slot_mask(), 0b11);
        assert_eq!(interface.declared_texture_slot_mask(), 0b10_0011);
        assert_eq!(
            interface
                .texture_slot_mask_for_material("we/clippingmaskimage4", 0b10_0011)
                .unwrap(),
            0b10_0011
        );
        assert!(interface.uniforms.iter().any(|uniform| {
            uniform.name == "g_RenderVar0" && uniform.kind == WeShaderUniformKind::Vec4
        }));
        assert!(
            interface
                .combos
                .iter()
                .any(|combo| combo.name == "ALPHATOCOVERAGE")
        );
    }

    #[test]
    fn minimalalpha_interface_tracks_flattexture_copy_back_slot_and_uniforms() {
        let interface = WeShaderInterface::for_shader("util/minimalalpha").unwrap();

        assert_eq!(interface.required_texture_slot_mask(), 0b1);
        assert_eq!(interface.declared_texture_slot_mask(), 0b1);
        assert_eq!(
            interface
                .texture_slot_mask_for_material("util/minimalalpha", 0b1)
                .unwrap(),
            0b1
        );
        assert!(
            interface
                .uniforms
                .iter()
                .any(|uniform| uniform.name == "g_Alpha")
        );
    }

    #[test]
    fn passthrough_interface_tracks_fullscreenlayer_slot_uniforms_and_combos() {
        let interface = WeShaderInterface::for_shader("util/passthrough").unwrap();

        assert_eq!(interface.required_texture_slot_mask(), 0b1);
        assert_eq!(interface.declared_texture_slot_mask(), 0b1);
        assert_eq!(
            interface
                .texture_slot_mask_for_material("util/passthrough", 0b1)
                .unwrap(),
            0b1
        );
        assert!(interface.declares_combo("SPRITESHEET"));
        assert!(interface.declares_combo("TRANSFORM"));
        assert!(interface.uniforms.iter().any(|uniform| {
            uniform.name == "g_ModelViewProjectionMatrix"
                && uniform.kind == WeShaderUniformKind::Mat4
        }));
    }

    #[test]
    fn genericparticle_interface_tracks_particle_texture_slots_uniforms_and_combos() {
        let interface = WeShaderInterface::for_shader("we/genericparticle").unwrap();

        assert_eq!(interface.required_texture_slot_mask(), 0b1);
        assert_eq!(interface.declared_texture_slot_mask(), 0b11_1011);
        assert_eq!(
            interface
                .texture_slot_mask_for_material("we/genericparticle", 0b1)
                .unwrap(),
            0b1
        );
        assert!(interface.declares_combo("REFRACT"));
        assert!(interface.declares_combo("SPRITESHEET"));
        assert!(interface.uniforms.iter().any(|uniform| {
            uniform.name == "g_Overbright" && uniform.kind == WeShaderUniformKind::Float
        }));
    }

    #[test]
    fn iris_effect_interface_tracks_source_mask_slots_uniforms_and_combos() {
        let interface = WeShaderInterface::for_effect_shader("effects/iris").unwrap();

        assert!(WeShaderInterface::for_shader("effects/iris").is_none());
        assert_eq!(interface.required_texture_slot_mask(), 0b1);
        assert_eq!(interface.declared_texture_slot_mask(), 0b11);
        assert_eq!(
            interface
                .texture_slot_mask_for_material("effects/iris", 0b11)
                .unwrap(),
            0b11
        );
        assert!(interface.declares_combo("MASK"));
        assert!(interface.declares_combo("BACKGROUND"));
        assert!(
            interface.uniforms.iter().any(|uniform| {
                uniform.name == "g_Scale" && uniform.material_key == Some("scale")
            })
        );
        assert!(interface.uniforms.iter().any(|uniform| {
            uniform.name == "g_EyeColor" && uniform.kind == WeShaderUniformKind::Vec3
        }));
    }

    #[test]
    fn iris_effect_interface_rejects_unknown_texture_slots() {
        let interface = WeShaderInterface::for_effect_shader("effects/iris").unwrap();

        let err = interface
            .texture_slot_mask_for_material("effects/iris", 0b101)
            .expect_err("slot 2 is not declared by iris");

        assert!(err.contains("outside shader interface"));
        assert!(err.contains("0x00000004"));
    }

    #[test]
    fn genericimage4_interface_rejects_missing_albedo_slot() {
        let interface = WeShaderInterface::for_shader("we/genericimage4").unwrap();

        let err = interface
            .texture_slot_mask_for_material("we/genericimage4", 0)
            .expect_err("slot 0 is required");

        assert!(err.contains("missing 0x00000001"));
    }

    #[test]
    fn unknown_shader_has_no_contract() {
        assert!(WeShaderInterface::for_shader("we/unknown").is_none());
    }

    #[test]
    fn genericimage4_keeps_we_vec4_uniforms_as_fixed_abi_records() {
        let interface = WeShaderInterface::for_shader("we/genericimage4").unwrap();
        let vec4_uniforms = interface
            .uniforms
            .iter()
            .filter(|uniform| uniform.kind == WeShaderUniformKind::Vec4)
            .map(|uniform| uniform.name)
            .collect::<Vec<_>>();

        assert_eq!(
            WeShaderUniformKind::Vec4.we_abi_bytes(),
            Some(WE_VEC4_BYTES)
        );
        assert!(vec4_uniforms.contains(&"g_Texture0Rotation"));
        assert!(vec4_uniforms.contains(&"g_Texture2Resolution"));
        assert!(vec4_uniforms.contains(&"g_Color4"));
    }
}
