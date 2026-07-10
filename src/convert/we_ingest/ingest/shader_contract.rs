//! Shader descriptor requirements derived during WE ingest.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`

use crate::convert::we_ingest::ir::WeIrMaterialTexture;

pub(super) fn declared_texture_slot_mask(
    shader_key: &str,
    textures: &[&WeIrMaterialTexture],
) -> u32 {
    let mut mask = textures
        .iter()
        .filter(|texture| texture.resource.is_some() || !texture.path.is_empty())
        .filter(|texture| texture.slot < 32)
        .fold(0u32, |mask, texture| mask | (1 << texture.slot));
    let key = shader_key.to_ascii_lowercase();
    if mesh_shader_uses_slot_zero(&key) {
        mask |= 1;
    }
    if key.contains("clippingmaskimage4") {
        mask |= 1 << 1;
    }
    if key.contains("clippingtarget") {
        mask |= 1 << 8;
    }
    if let Some(slot_mask) = effect_shader_slot_mask(&key) {
        mask |= slot_mask;
    }
    mask
}

pub(super) fn shader_uniform_buffer_count(shader_key: &str, has_constants: bool) -> u32 {
    let key = shader_key.to_ascii_lowercase();
    if mesh_shader_needs_draw_and_material_uniforms(&key)
        || effect_shader_needs_draw_and_material_uniforms(&key)
    {
        2
    } else {
        1 + u32::from(has_constants)
    }
}

fn mesh_shader_uses_slot_zero(key: &str) -> bool {
    key.contains("genericimage")
        || key.contains("genericparticle")
        || key.contains("clippingmaskimage")
        || key == "minimalalpha"
        || key.starts_with("minimalalpha__")
        || key == "passthrough"
        || key.starts_with("passthrough__")
}

fn effect_shader_slot_mask(key: &str) -> Option<u32> {
    let (_, slots) = key.split_once("__slots_")?;
    let hex = slots
        .chars()
        .take_while(|ch| ch.is_ascii_hexdigit())
        .collect::<String>();
    u32::from_str_radix(&hex, 16).ok()
}

fn effect_shader_needs_draw_and_material_uniforms(key: &str) -> bool {
    [
        "effects/iris",
        "effects/opacity",
        "effects/waterwaves",
        "effects/waterripple",
        "workshop/2790231929/effects/foliagesway",
    ]
    .iter()
    .any(|shader| {
        key == *shader
            || key
                .strip_prefix(shader)
                .is_some_and(|rest| rest.starts_with("__"))
    }) || key.contains("/effects/waterripple__")
}

fn mesh_shader_needs_draw_and_material_uniforms(key: &str) -> bool {
    key.contains("genericimage")
        || key == "color"
        || key.starts_with("color__")
        || key == "we/color"
        || key.starts_with("we/color__")
        || key == "text"
        || key.starts_with("text__")
        || key == "we/text"
        || key.starts_with("we/text__")
        || key.contains("genericparticle")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_effect_slots_are_a_hex_mask_not_a_slot_count() {
        assert_eq!(
            declared_texture_slot_mask("effects/ripple__SLOTS_5", &[]),
            0b0101
        );
        assert_eq!(
            declared_texture_slot_mask("effects/fluid__SLOTS_3d", &[]),
            0x3d
        );
    }

    #[test]
    fn known_effect_uniform_abis_require_draw_and_material_buffers() {
        assert_eq!(
            shader_uniform_buffer_count("effects/iris__SLOTS_3__MASK_1", true),
            2
        );
        assert_eq!(
            shader_uniform_buffer_count("effects/waterwaves__SLOTS_1", false),
            2
        );
        assert_eq!(
            shader_uniform_buffer_count("workshop/2790231929/effects/foliagesway__SLOTS_1", true),
            2
        );
    }
}
