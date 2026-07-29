//! Shader descriptor requirements derived during WE ingest.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/shader-conventions.md`
//! - `reverse-engineered/gilder/docs/exe/global-uniforms.md`

use std::collections::BTreeSet;

use crate::convert::we_ingest::ir::{
    WeIrMaterialConstant, WeIrMaterialPass, WeIrMaterialTexture, WeIrShaderContract,
    WeIrShaderOrigin,
};
use crate::engine::render_graph::{RenderGraph, TextureBindingRole};

use super::super::shader_key::canonical_scene_shader_key;
use super::super::shader_origin::scene_shader_origin;

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
        let origin = pass.shader_origin;
        let shader_key = material_shader_program_key(pass);
        let texture_slot_mask = declared_texture_slot_mask(&shader_key, &textures);
        let pipeline_key = format!(
            "{}|blend={:?}|depth={:?}|depthwrite={}|cull={:?}",
            shader_key, pass.pipeline_blend, pass.depth_test, pass.depth_write, pass.cull_mode
        );
        if seen_pipeline_keys.insert(pipeline_key.clone()) {
            contracts.push(shader_contract(
                shader_key,
                pass.shader_source_key.clone(),
                origin,
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
        let origin = scene_shader_origin(shader_key);
        let shader_key = canonical_scene_shader_key(shader_key);
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
        }) | declared_texture_slot_mask(&shader_key, &[]);
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
            shader_key.clone(),
            origin,
            pipeline_key,
            texture_slot_mask,
            constants,
        ));
    }
    contracts
}

fn material_shader_program_key(pass: &WeIrMaterialPass) -> String {
    if pass.shader_origin == WeIrShaderOrigin::EngineBuiltIn {
        return canonical_scene_shader_key(&pass.shader_key);
    }
    let variant = pass
        .shader_key
        .split_once("__")
        .map_or("", |(_, variant)| variant);
    let source = pass
        .shader_source_key
        .strip_prefix("shaders/")
        .unwrap_or(&pass.shader_source_key);
    if variant.is_empty() {
        source.to_owned()
    } else {
        format!("{source}__{variant}")
    }
}

fn shader_contract(
    shader_key: String,
    shader_source_key: String,
    origin: WeIrShaderOrigin,
    pipeline_key: String,
    texture_slot_mask: u32,
    constants: Vec<String>,
) -> WeIrShaderContract {
    let texture_count = texture_slot_mask.count_ones();
    let uniform_count = shader_uniform_buffer_count(&shader_key, !constants.is_empty());
    WeIrShaderContract {
        shader_key,
        shader_source_key,
        origin,
        pipeline_key,
        texture_slot_mask,
        // No input attachment is inferred from a shader name or a sampler
        // slot.  The converter will populate this only from an explicit,
        // verified exact-pixel contract.
        input_attachment_slot_mask: 0,
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

pub(super) fn declared_texture_slot_mask(
    shader_key: &str,
    textures: &[&WeIrMaterialTexture],
) -> u32 {
    let mut mask = textures
        .iter()
        .filter(|texture| texture.resource.is_some() || !texture.path.is_empty())
        .filter(|texture| texture.slot < 32)
        .fold(0u32, |mask, texture| mask | (1 << texture.slot));
    let key = shader_key;
    if mesh_shader_uses_slot_zero(key) {
        mask |= 1;
    }
    if shader_program(key) == "we/clippingmaskimage4" {
        mask |= 1 << 1;
    }
    if shader_variant_enabled(key, "CLIPPINGTARGET") {
        mask |= 1 << 8;
    }
    if let Some(slot_mask) = effect_shader_slot_mask(key) {
        mask |= slot_mask;
    }
    if key == "we/waterwaves-uv-field" || is_waterwaves_direct_shader(key) {
        mask |= 0x3fe;
    }
    if is_foliage_ripple_shader(key) {
        mask |= 0x0b;
    }
    if key == "we/image-ripple-source" {
        mask |= 0x05;
    }
    if matches!(
        key,
        "we/image-ripple-flow-composite" | "we/image-ripple-flow-multiply-composite"
    ) {
        mask |= 0x07;
    }
    match key {
        "we/image-waterwaves-final" => mask |= 0x03,
        "we/image-waterripple-final" => mask |= 0x07,
        "we/image-waterripple-modulate-final" => mask |= 0x07,
        "we/image-scroll-final" | "we/image-colorkey-scroll-final" => mask |= 0x01,
        "we/image-cloudmotion-final" => mask |= 0x05,
        "we/framebuffer-water-quantized-water-opacity" => mask |= 0x01,
        "we/framebuffer-water-quantized-shake-final" => mask |= 0x03,
        "we/puppet-opacity-final" => mask |= 0x03,
        "we/puppet-opacity-clipping-final" => mask |= 0x103,
        "we/puppet-iris-waterripple-final" => mask |= 0x0f,
        "we/puppet-iris-waterripple-clipping-final" => mask |= 0x10f,
        "we/audio-bars-final" => mask |= 0x01,
        _ => {}
    }
    mask
}

pub(super) fn shader_uniform_buffer_count(shader_key: &str, has_constants: bool) -> u32 {
    if mesh_shader_needs_draw_and_material_uniforms(shader_key)
        || effect_shader_needs_draw_and_material_uniforms(shader_key)
    {
        2
    } else {
        1 + u32::from(has_constants)
    }
}

fn mesh_shader_uses_slot_zero(key: &str) -> bool {
    matches!(
        shader_program(key),
        "we/genericimage2"
            | "we/genericimage4"
            | "we/genericparticle"
            | "we/clippingmaskimage4"
            | "we/minimalalpha"
            | "we/passthrough"
            | "we/composelayer"
    ) || key == "we/objectcomposite"
        || key == "gilder/dynamic-text"
        || key == "we/objectcomposite-screen-group"
        || key == "we/image-effect-source"
        || key == "we/image-effect-composite"
        || key == "we/image-effect-composite__STATIC_BLACK_1"
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
        || key == "we/framebuffer-water-quantized-water-opacity"
        || key == "we/framebuffer-water-quantized-shake-final"
        || key == "we/puppet-opacity-final"
        || key == "we/puppet-opacity-clipping-final"
        || key == "we/puppet-iris-waterripple-final"
        || key == "we/puppet-iris-waterripple-clipping-final"
        || key == "we/puppet-effect-source"
        || key == "we/puppet-effect-composite"
        || key == "we/puppet-waterwaves-composite"
        || is_waterwaves_direct_shader(key)
        || key == "we/utilitycomposite"
}

fn shader_program(key: &str) -> &str {
    key.split("__").next().unwrap_or(key)
}

fn shader_variant_enabled(key: &str, name: &str) -> bool {
    key.split("__")
        .skip(1)
        .any(|variant| variant == format!("{name}_1"))
}

fn effect_shader_slot_mask(key: &str) -> Option<u32> {
    let (_, slots) = key.split_once("__SLOTS_")?;
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
        "effects/blendgradient",
        "effects/tech_circle",
        "effects/simple_audio_bars",
        "effects/rounded_mask",
        "effects/lut_loader",
        "effects/raindrop_on_glass",
        "effects/audio_responsive_oscilloscope",
    ]
    .iter()
    .any(|shader| {
        key == *shader
            || key
                .strip_prefix(shader)
                .is_some_and(|rest| rest.starts_with("__"))
    }) || key == "we/waterwaves-uv-field"
        || key == "we/image-ripple-source"
}

fn mesh_shader_needs_draw_and_material_uniforms(key: &str) -> bool {
    matches!(shader_program(key), "we/genericimage2" | "we/genericimage4")
        || key == "we/image-waterwaves-final"
        || key == "we/image-waterripple-final"
        || key == "we/image-waterripple-modulate-final"
        || key == "we/image-scroll-final"
        || key == "we/image-colorkey-scroll-final"
        || key == "we/image-cloudmotion-final"
        || key == "we/framebuffer-water-quantized-water-opacity"
        || key == "we/framebuffer-water-quantized-shake-final"
        || key == "we/puppet-opacity-final"
        || key == "we/puppet-opacity-clipping-final"
        || key == "we/puppet-iris-waterripple-final"
        || key == "we/puppet-iris-waterripple-clipping-final"
        || key == "we/flat-rounded-opacity-final"
        || key == "we/flat-rounded-hsl-source"
        || key == "we/tech-circle-final"
        || key == "we/audio-bars-final"
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
        || key == "we/objectcomposite-screen-group"
        || key == "we/image-effect-composite"
        || key == "we/image-effect-composite__STATIC_BLACK_1"
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
    use crate::engine::render_graph::{
        PassState, RenderPassDrawPrimitive, RenderPassEffectVisibility, RenderPassNode,
        RenderPassRole, RenderTargetRole,
    };
    use crate::engine::scene::{SceneCullMode, SceneDepthTest, ScenePipelineBlend};

    #[test]
    fn authored_contract_key_uses_source_identity_and_specialization() {
        let pass = WeIrMaterialPass {
            material: 0,
            shader_key: "effects/simple_audio_bars__SLOTS_1__SHAPE_7".to_owned(),
            shader_source_key: "workshop/test/effects/Simple_Audio_Bars".to_owned(),
            shader_origin: WeIrShaderOrigin::AuthoredPackage,
            target: String::new(),
            texture_start: 0,
            texture_count: 0,
            constant_start: 0,
            constant_count: 0,
            pipeline_blend: ScenePipelineBlend::Normal,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_writing: String::new(),
            clear_target: false,
        };

        assert_eq!(
            material_shader_program_key(&pass),
            "workshop/test/effects/Simple_Audio_Bars__SLOTS_1__SHAPE_7"
        );
    }

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
            shader_uniform_buffer_count("effects/foliagesway__SLOTS_1", true),
            2
        );
    }

    #[test]
    fn generated_clipping_final_programs_declare_every_sampled_slot() {
        assert_eq!(
            declared_texture_slot_mask("we/puppet-opacity-clipping-final", &[]),
            0x103
        );
        assert_eq!(
            declared_texture_slot_mask("we/puppet-iris-waterripple-clipping-final", &[]),
            0x10f
        );
    }

    #[test]
    fn graph_generated_clipping_contracts_inherit_the_final_program_interface() {
        let pass = |id, shader: &str| RenderPassNode {
            id,
            role: RenderPassRole::MeshClippedTarget,
            draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
            object_index: Some(0),
            material_index: None,
            pass_index: id,
            shader: Some(shader.to_owned()),
            target: RenderTargetRole::SceneColor,
            target_name: None,
            target_extent: None,
            target_format: None,
            bindings: vec![
                TextureBindingRole::SourceTexture,
                TextureBindingRole::EffectTarget {
                    slot: 8,
                    name: "_rt_FullAlphaMask".to_owned(),
                },
            ],
            effect_visibility: RenderPassEffectVisibility::NONE,
            state: PassState::default(),
        };
        let graphs = [RenderGraph {
            activation_policy: Default::default(),
            passes: vec![
                pass(0, "we/puppet-opacity-clipping-final"),
                pass(1, "we/puppet-iris-waterripple-clipping-final"),
            ],
            target_specs: Vec::new(),
            unsupported: Vec::new(),
        }];

        let contracts = build_shader_contract_records(&graphs, &[], &[], &[]);
        let opacity = contracts
            .iter()
            .find(|contract| contract.shader_key == "we/puppet-opacity-clipping-final")
            .expect("graph-generated opacity clipping contract");
        assert_eq!(opacity.texture_slot_mask, 0x103);
        assert_eq!(opacity.resource_heap_count, 5);
        assert_eq!(opacity.sampler_heap_count, 3);
        let iris = contracts
            .iter()
            .find(|contract| contract.shader_key == "we/puppet-iris-waterripple-clipping-final")
            .expect("graph-generated iris/waterripple clipping contract");
        assert_eq!(iris.texture_slot_mask, 0x10f);
        assert_eq!(iris.resource_heap_count, 7);
        assert_eq!(iris.sampler_heap_count, 5);
    }

    #[test]
    fn framebuffer_water_contracts_declare_every_texture_and_both_uniforms() {
        let prepass_shader =
            "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1";
        let intermediate_shader = "we/framebuffer-water-quantized-water-opacity";
        let final_shader = "we/framebuffer-water-quantized-shake-final";
        let material_pass =
            |material, shader_key: &str, texture_start, texture_count| WeIrMaterialPass {
                material,
                shader_key: shader_key.to_owned(),
                shader_source_key: shader_key
                    .split_once("__")
                    .map_or(shader_key, |(program, _)| program)
                    .to_owned(),
                shader_origin: scene_shader_origin(shader_key),
                target: String::new(),
                texture_start,
                texture_count,
                constant_start: 0,
                constant_count: 0,
                pipeline_blend: ScenePipelineBlend::Normal,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
                alpha_writing: String::new(),
                clear_target: false,
            };
        let graph_pass = |id, material_index, shader: &str| RenderPassNode {
            id,
            role: RenderPassRole::EffectMaterial,
            draw_primitive: RenderPassDrawPrimitive::FullscreenTriangle,
            object_index: Some(0),
            material_index: Some(material_index),
            pass_index: id,
            shader: Some(shader.to_owned()),
            target: RenderTargetRole::SceneColor,
            target_name: None,
            target_extent: None,
            target_format: None,
            bindings: vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }],
            effect_visibility: RenderPassEffectVisibility::NONE,
            state: PassState::default(),
        };
        let graph = RenderGraph {
            activation_policy: Default::default(),
            passes: vec![
                graph_pass(0, 0, prepass_shader),
                graph_pass(1, 1, intermediate_shader),
                graph_pass(2, 2, final_shader),
            ],
            target_specs: Vec::new(),
            unsupported: Vec::new(),
        };
        let material_passes = [
            material_pass(0, prepass_shader, 0, 4),
            material_pass(1, intermediate_shader, 4, 0),
            material_pass(2, final_shader, 4, 1),
        ];
        let material_textures = [
            WeIrMaterialTexture {
                slot: 2,
                resource: Some(2),
                path: String::new(),
            },
            WeIrMaterialTexture {
                slot: 3,
                resource: Some(3),
                path: String::new(),
            },
            WeIrMaterialTexture {
                slot: 4,
                resource: Some(4),
                path: String::new(),
            },
            WeIrMaterialTexture {
                slot: 5,
                resource: Some(5),
                path: String::new(),
            },
            WeIrMaterialTexture {
                slot: 1,
                resource: Some(1),
                path: String::new(),
            },
        ];

        let contracts =
            build_shader_contract_records(&[graph], &material_passes, &material_textures, &[]);
        let prepass = contracts
            .iter()
            .find(|contract| contract.shader_key == prepass_shader)
            .expect("quantized caustics prepass contract");
        assert_eq!(prepass.texture_slot_mask, 0x3d);
        assert_eq!(prepass.resource_heap_count, 7);
        assert_eq!(prepass.sampler_heap_count, 5);
        let intermediate = contracts
            .iter()
            .find(|contract| contract.shader_key == intermediate_shader)
            .expect("quantized framebuffer-water intermediate contract");
        assert_eq!(intermediate.texture_slot_mask, 0x01);
        assert_eq!(intermediate.resource_heap_count, 3);
        assert_eq!(intermediate.sampler_heap_count, 1);
        let final_program = contracts
            .iter()
            .find(|contract| contract.shader_key == final_shader)
            .expect("quantized framebuffer-water final contract");
        assert_eq!(final_program.texture_slot_mask, 0x03);
        assert_eq!(final_program.resource_heap_count, 4);
        assert_eq!(final_program.sampler_heap_count, 2);
    }
}
