//! Shader descriptor requirements derived during WE ingest.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`

use std::collections::BTreeSet;

use crate::convert::we_ingest::ir::{
    WeIrMaterialConstant, WeIrMaterialPass, WeIrMaterialTexture, WeIrShaderContract,
};
use crate::engine::render_graph::{RenderGraph, TextureBindingRole};

pub(super) fn build_shader_contract_records(
    render_graphs: &[RenderGraph],
    material_passes: &[WeIrMaterialPass],
    material_textures: &[WeIrMaterialTexture],
    material_constants: &[WeIrMaterialConstant],
) -> Vec<WeIrShaderContract> {
    let used_materials = render_graphs
        .iter()
        .flat_map(|graph| graph.passes.iter().filter_map(|pass| pass.material_index))
        .collect::<BTreeSet<_>>();
    let mut contracts = Vec::new();
    let mut seen_pipeline_keys = BTreeSet::new();
    for pass in material_passes {
        if pass.shader_key.is_empty() || !used_materials.contains(&(pass.material as usize)) {
            continue;
        }
        let textures = material_textures
            .iter()
            .skip(pass.texture_start as usize)
            .take(pass.texture_count as usize)
            .collect::<Vec<_>>();
        let constants = material_constants
            .iter()
            .skip(pass.constant_start as usize)
            .take(pass.constant_count as usize)
            .map(|constant| constant.name.clone())
            .collect::<Vec<_>>();
        let texture_slot_mask = declared_texture_slot_mask(&pass.shader_key, &textures);
        let pipeline_key = format!(
            "{}|blend={:?}|depth={:?}|depthwrite={}|cull={:?}",
            pass.shader_key, pass.pipeline_blend, pass.depth_test, pass.depth_write, pass.cull_mode
        );
        if seen_pipeline_keys.insert(pipeline_key.clone()) {
            contracts.push(shader_contract(
                pass.shader_key.clone(),
                pipeline_key,
                texture_slot_mask,
                constants,
            ));
        }
    }

    let mut represented_shaders = contracts
        .iter()
        .map(|contract| contract.shader_key.clone())
        .collect::<BTreeSet<_>>();
    for pass in render_graphs.iter().flat_map(|graph| &graph.passes) {
        let Some(shader_key) = pass.shader.as_ref().filter(|shader| !shader.is_empty()) else {
            continue;
        };
        if !represented_shaders.insert(shader_key.clone()) {
            continue;
        }
        let constants = pass
            .bindings
            .iter()
            .filter_map(|binding| match binding {
                TextureBindingRole::PassConstant { name } => Some(name.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let texture_slot_mask = pass.bindings.iter().fold(0u32, |mask, binding| {
            binding_texture_slot(binding)
                .filter(|slot| *slot < 32)
                .map_or(mask, |slot| mask | (1 << slot))
        }) | graph_shader_texture_slot_mask(shader_key);
        let pipeline_key = format!(
            "{}|blend={:?}|depth={:?}|depthwrite={}|cull={:?}",
            shader_key,
            pass.state.pipeline_blend,
            pass.state.depth_test,
            pass.state.depth_write,
            pass.state.cull_mode
        );
        contracts.push(shader_contract(
            shader_key.clone(),
            pipeline_key,
            texture_slot_mask,
            constants,
        ));
    }
    contracts
}

fn shader_contract(
    shader_key: String,
    pipeline_key: String,
    texture_slot_mask: u32,
    constants: Vec<String>,
) -> WeIrShaderContract {
    let texture_count = texture_slot_mask.count_ones();
    let uniform_count = shader_uniform_buffer_count(&shader_key, !constants.is_empty());
    WeIrShaderContract {
        shader_key,
        pipeline_key,
        texture_slot_mask,
        constants,
        resource_heap_count: texture_count + uniform_count,
        sampler_heap_count: texture_count,
    }
}

fn binding_texture_slot(binding: &TextureBindingRole) -> Option<u32> {
    match binding {
        TextureBindingRole::SourceTexture => Some(0),
        TextureBindingRole::TextureSlot { slot }
        | TextureBindingRole::AlphaTextureSlot { slot }
        | TextureBindingRole::PreviousGraphTarget { slot }
        | TextureBindingRole::GraphTarget { slot, .. }
        | TextureBindingRole::NamedFboBind { slot, .. }
        | TextureBindingRole::EffectTarget { slot, .. } => Some(*slot),
        TextureBindingRole::VideoFrame { media_instance } => Some(*media_instance),
        TextureBindingRole::AudioUniform
        | TextureBindingRole::SystemUniform
        | TextureBindingRole::PassConstant { .. } => None,
    }
}

fn graph_shader_texture_slot_mask(shader_key: &str) -> u32 {
    u32::from(is_foliage_ripple_shader(
        shader_key.to_ascii_lowercase().as_str(),
    )) * 0x0b
}

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
    if key == "we/waterwaves-uv-field" || is_waterwaves_direct_shader(&key) {
        mask |= 0xfe;
    }
    if is_foliage_ripple_shader(&key) {
        mask |= 0x0b;
    }
    if key == "we/image-ripple-source" {
        mask |= 0x05;
    }
    if matches!(
        key.as_str(),
        "we/image-ripple-flow-composite" | "we/image-ripple-flow-multiply-composite"
    ) {
        mask |= 0x07;
    }
    match key.as_str() {
        "we/image-waterwaves-final" => mask |= 0x03,
        "we/image-waterripple-final" => mask |= 0x07,
        "we/image-waterripple-modulate-final" => mask |= 0x07,
        "we/image-scroll-final" | "we/image-colorkey-scroll-final" => mask |= 0x01,
        "we/image-cloudmotion-final" => mask |= 0x05,
        "we/puppet-opacity-final" => mask |= 0x03,
        "we/puppet-opacity-clipping-final" => mask |= 0x103,
        "we/puppet-iris-waterripple-final" => mask |= 0x0f,
        "we/puppet-iris-waterripple-clipping-final" => mask |= 0x10f,
        "we/audio-bars-final" => mask |= 0x01,
        "we/framebuffer-water-final" => mask |= 0x3f,
        "we/framebuffer-water-post-final" => mask |= 0x03,
        "we/framebuffer-lut16-final" | "we/framebuffer-lut64-final" => mask |= 0x03,
        "we/framebuffer-lightning-screen-final" | "we/framebuffer-lightning-add-final" => {
            mask |= 0x01
        }
        _ => {}
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
        || key == "composelayer"
        || key.starts_with("composelayer__")
        || key == "we/objectcomposite"
        || key == "we/image-effect-source"
        || key == "we/image-effect-composite"
        || key == "we/image-effect-modulate-composite"
        || key == "we/flat-rounded-hsl-source"
        || key == "we/image-waterwaves-composite"
        || key == "we/image-waterwaves-multiply-composite"
        || is_foliage_ripple_shader(key)
        || key == "we/image-ripple-flow-composite"
        || key == "we/image-ripple-flow-multiply-composite"
        || key == "we/image-waterwaves-final"
        || key == "we/image-waterripple-final"
        || key == "we/image-waterripple-modulate-final"
        || key == "we/image-scroll-final"
        || key == "we/image-colorkey-scroll-final"
        || key == "we/image-cloudmotion-final"
        || key == "we/puppet-opacity-final"
        || key == "we/puppet-opacity-clipping-final"
        || key == "we/puppet-iris-waterripple-final"
        || key == "we/puppet-iris-waterripple-clipping-final"
        || key == "we/framebuffer-water-final"
        || key == "we/framebuffer-water-post-final"
        || key == "we/framebuffer-lut16-final"
        || key == "we/framebuffer-lut64-final"
        || key == "we/framebuffer-lightning-screen-final"
        || key == "we/framebuffer-lightning-add-final"
        || key == "we/puppet-effect-source"
        || key == "we/puppet-effect-composite"
        || key == "we/puppet-waterwaves-composite"
        || is_waterwaves_direct_shader(key)
        || key == "utilitycomposite"
        || key.starts_with("utilitycomposite__")
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
        "effects/caustics",
        "effects/111",
        "effects/blend",
        "effects/cloudmotion",
        "effects/iris",
        "effects/opacity",
        "effects/scroll",
        "effects/shake",
        "effects/shimmer",
        "effects/swing",
        "effects/foliagesway",
        "effects/skew",
        "effects/waterwaves",
        "effects/waterflow",
        "effects/waterripple",
        "workshop/2790231929/effects/foliagesway",
        "workshop/2962751255/effects/blend",
        "workshop/2962751255/effects/blendgradient",
        "workshop/2123274886/effects/tech_circle",
        "workshop/3082978660/effects/simple_audio_bars",
        "workshop/3083593512/effects/rounded_mask",
        "workshop/3165346237/effects/lut_loader",
        "workshop/3706822104/effects/raindrop_on_glass",
        "workshop/2799421411/effects/audio_responsive_oscilloscope",
    ]
    .iter()
    .any(|shader| {
        key == *shader
            || key
                .strip_prefix(shader)
                .is_some_and(|rest| rest.starts_with("__"))
    }) || key.contains("/effects/waterripple__")
        || key == "we/waterwaves-uv-field"
        || key == "we/image-ripple-source"
}

fn mesh_shader_needs_draw_and_material_uniforms(key: &str) -> bool {
    key.contains("genericimage")
        || key == "we/image-waterwaves-final"
        || key == "we/image-waterripple-final"
        || key == "we/image-waterripple-modulate-final"
        || key == "we/image-scroll-final"
        || key == "we/image-colorkey-scroll-final"
        || key == "we/image-cloudmotion-final"
        || key == "we/puppet-opacity-final"
        || key == "we/puppet-opacity-clipping-final"
        || key == "we/puppet-iris-waterripple-final"
        || key == "we/puppet-iris-waterripple-clipping-final"
        || key == "we/flat-rounded-opacity-final"
        || key == "we/flat-rounded-hsl-source"
        || key == "we/tech-circle-final"
        || key == "we/audio-bars-final"
        || key == "we/framebuffer-water-final"
        || key == "we/framebuffer-water-post-final"
        || key == "we/framebuffer-lut16-final"
        || key == "we/framebuffer-lut64-final"
        || key == "we/framebuffer-lightning-screen-final"
        || key == "we/framebuffer-lightning-add-final"
        || key == "color"
        || key.starts_with("color__")
        || key == "we/color"
        || key.starts_with("we/color__")
        || key == "text"
        || key.starts_with("text__")
        || key == "we/text"
        || key.starts_with("we/text__")
        || key == "flat"
        || key.starts_with("flat__")
        || key == "we/flat"
        || key.starts_with("we/flat__")
        || key == "we/objectcomposite"
        || key == "we/image-effect-composite"
        || key == "we/image-effect-modulate-composite"
        || key == "we/puppet-effect-composite"
        || key == "we/image-waterwaves-composite"
        || key == "we/image-waterwaves-multiply-composite"
        || is_foliage_ripple_shader(key)
        || key == "we/image-ripple-flow-composite"
        || key == "we/image-ripple-flow-multiply-composite"
        || key == "we/puppet-waterwaves-composite"
        || is_waterwaves_direct_shader(key)
        || key.contains("genericparticle")
}

fn is_waterwaves_direct_shader(key: &str) -> bool {
    [
        "we/image-waterwaves-direct",
        "we/image-waterwaves-multiply-direct",
        "we/puppet-waterwaves-direct",
        "we/effect-waterwaves-direct",
    ]
    .iter()
    .any(|base| {
        key == *base
            || key
                .strip_prefix(base)
                .is_some_and(|suffix| suffix.starts_with("__STAGES_"))
    })
}

fn is_foliage_ripple_shader(key: &str) -> bool {
    [
        "we/image-foliage-ripple-composite",
        "we/image-foliage-ripple-screen-composite",
    ]
    .iter()
    .any(|base| {
        key == *base
            || key
                .strip_prefix(base)
                .is_some_and(|suffix| suffix.starts_with("__"))
    })
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
