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
//! - `reverse-engineered/shaders/common_blending.h`
//! - `reverse-engineered/shaders/effects/waterwaves.frag`
//! - `reverse-engineered/shaders/effects/waterripple.frag`
//! - `reverse-engineered/shaders/effects/waterflow.frag`

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
            _ => None,
        }
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
