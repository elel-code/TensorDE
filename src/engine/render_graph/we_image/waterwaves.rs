//! Typed lowering for consecutive authored-texture waterwaves passes.

use super::{WeEffectPassContract, WeImageGraphContract};
use crate::core::SceneBlendMode;
use crate::engine::render_graph::{
    PassState, PipelineBlendMode, RenderGraph, RenderPassEffectVisibility, RenderPassNode,
    RenderPassRole, RenderTargetRole, RenderTargetSpec, TextureBindingRole,
};

const UV_FIELD_SHADER: &str = "we/waterwaves-uv-field";
const IMAGE_COMPOSITE_SHADER: &str = "we/image-waterwaves-composite";
const IMAGE_MULTIPLY_COMPOSITE_SHADER: &str = "we/image-waterwaves-multiply-composite";
const PUPPET_COMPOSITE_SHADER: &str = "we/puppet-waterwaves-composite";
const UV_TARGET_FORMAT: &str = "rg16f";
const UV_TARGET_NAME: &str = "_gilder_waterwaves_uv_field";
const GROUP_COLOR_TARGET_NAME: &str = "_gilder_puppet_group_color";
const GROUP_COMPOSITE_SHADER: &str = "we/objectcomposite-screen-group";
const UV_TARGET_DIVISOR_MILLI: u32 = 4_000;
const MAX_WATERWAVES_STAGES: usize = 9;

pub(super) fn is_compatible_displacement_chain(contract: &WeImageGraphContract) -> bool {
    contract.effects_in_authored_texture_space
        && contract.framebuffer_snapshot.is_none()
        && contract.base_material_index.is_some()
        && contract.base_texture_slots.iter().all(|slot| *slot == 0)
        && (contract.waterwaves_uv_field_material_index.is_some()
            || contract.waterwaves_direct_material.is_some())
        && are_compatible_effect_passes(&contract.effect_passes)
}

pub(super) fn are_compatible_effect_passes(effect_passes: &[WeEffectPassContract]) -> bool {
    (2..=MAX_WATERWAVES_STAGES).contains(&effect_passes.len())
        && effect_passes.iter().all(compatible_pass)
}

fn compatible_pass(pass: &WeEffectPassContract) -> bool {
    pass.command.is_none()
        && pass.target.is_none()
        && pass.material_index.is_some()
        && pass
            .shader
            .as_deref()
            .and_then(|shader| shader.split("__").next())
            .is_some_and(|shader| shader.eq_ignore_ascii_case("effects/waterwaves"))
        && pass.source.as_deref().is_none_or(is_previous_source)
        && pass
            .binds
            .get(&0)
            .is_some_and(|source| is_previous_source(source))
        && pass.binds.iter().all(|(slot, source)| match *slot {
            0 => is_previous_source(source),
            1 => !is_graph_resource(source),
            _ => false,
        })
        && pass
            .material_blending
            .as_deref()
            .is_none_or(|blend| blend.eq_ignore_ascii_case("normal"))
}

fn is_previous_source(source: &str) -> bool {
    matches!(source, "previous" | "_previous" | "$previous")
}

fn is_graph_resource(source: &str) -> bool {
    is_previous_source(source)
        || source.eq_ignore_ascii_case("source")
        || source.starts_with("fbo_")
        || source.starts_with("_rt_")
        || source.starts_with("_alias_")
}

pub(super) fn append_displacement_chain(graph: &mut RenderGraph, contract: &WeImageGraphContract) {
    if let Some(material) = &contract.waterwaves_direct_material {
        append_direct_composite(graph, contract, material);
        return;
    }
    let effect_visibility = super::contiguous_effect_range(&contract.effect_passes)
        .map(|(effect_binding_start, effect_binding_count)| {
            if effect_binding_count == 0 {
                RenderPassEffectVisibility::NONE
            } else {
                RenderPassEffectVisibility::waterwaves_stages(
                    effect_binding_start,
                    effect_binding_count,
                )
            }
        })
        .expect("typed waterwaves chain requires compatible effect visibility");
    let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
    graph.passes.push(RenderPassNode {
        id: pass_id,
        role: RenderPassRole::EffectMaterial,
        object_index: Some(contract.object_index),
        material_index: contract.waterwaves_uv_field_material_index,
        pass_index: pass_id,
        shader: Some(UV_FIELD_SHADER.to_owned()),
        target: RenderTargetRole::Temporary,
        target_name: Some(UV_TARGET_NAME.to_owned()),
        target_extent: None,
        target_format: Some(UV_TARGET_FORMAT.to_owned()),
        bindings: Vec::new(),
        effect_visibility,
        state: PassState {
            pipeline_blend: PipelineBlendMode::Normal,
            scene_blend: contract.final_scene_blend,
            ..PassState::default()
        },
    });
    graph.target_specs.push(RenderTargetSpec {
        role: RenderTargetRole::Temporary,
        name: UV_TARGET_NAME.to_owned(),
        format: UV_TARGET_FORMAT.to_owned(),
        width_divisor_milli: UV_TARGET_DIVISOR_MILLI,
        height_divisor_milli: UV_TARGET_DIVISOR_MILLI,
    });

    let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
    graph.passes.push(RenderPassNode {
        id: pass_id,
        role: RenderPassRole::SceneComposite,
        object_index: Some(contract.object_index),
        material_index: contract.base_material_index,
        pass_index: pass_id,
        shader: Some(composite_shader(contract).to_owned()),
        target: RenderTargetRole::SceneColor,
        target_name: None,
        target_extent: None,
        target_format: None,
        bindings: vec![
            TextureBindingRole::SourceTexture,
            TextureBindingRole::GraphTarget {
                slot: 1,
                role: RenderTargetRole::Temporary,
                name: Some(UV_TARGET_NAME.to_owned()),
            },
        ],
        effect_visibility: RenderPassEffectVisibility::NONE,
        state: PassState {
            pipeline_blend: super::final_pipeline_blend(contract),
            scene_blend: contract.final_scene_blend,
            ..PassState::default()
        },
    });
}

fn append_direct_composite(
    graph: &mut RenderGraph,
    contract: &WeImageGraphContract,
    material: &crate::engine::render_graph::WeWaterWavesDirectMaterial,
) {
    let effect_visibility = super::contiguous_effect_range(&contract.effect_passes)
        .map(|(effect_binding_start, effect_binding_count)| {
            if effect_binding_count == 0 {
                RenderPassEffectVisibility::NONE
            } else {
                RenderPassEffectVisibility::waterwaves_stages(
                    effect_binding_start,
                    effect_binding_count,
                )
            }
        })
        .expect("typed waterwaves chain requires compatible effect visibility");
    if material.group_visual_composite {
        graph.passes.push(RenderPassNode {
            id: 0,
            role: RenderPassRole::BaseMaterial,
            object_index: Some(contract.object_index),
            material_index: Some(material.material_index),
            pass_index: 0,
            shader: Some(material.shader.clone()),
            target: RenderTargetRole::Temporary,
            target_name: Some(GROUP_COLOR_TARGET_NAME.to_owned()),
            target_extent: None,
            target_format: None,
            bindings: Vec::new(),
            effect_visibility,
            state: PassState {
                pipeline_blend: super::base_pipeline_blend(contract),
                scene_blend: SceneBlendMode::Normal,
                ..PassState::default()
            },
        });
        graph.passes.push(RenderPassNode {
            id: 1,
            role: RenderPassRole::SceneComposite,
            object_index: Some(contract.object_index),
            material_index: contract.base_material_index,
            pass_index: 1,
            shader: Some(GROUP_COMPOSITE_SHADER.to_owned()),
            target: RenderTargetRole::SceneColor,
            target_name: None,
            target_extent: None,
            target_format: None,
            bindings: vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }],
            effect_visibility: RenderPassEffectVisibility::NONE,
            state: PassState {
                pipeline_blend: super::final_pipeline_blend(contract),
                scene_blend: contract.final_scene_blend,
                ..PassState::default()
            },
        });
        return;
    }
    graph.passes.push(RenderPassNode {
        id: graph.passes.len().min(u32::MAX as usize) as u32,
        role: RenderPassRole::SceneComposite,
        object_index: Some(contract.object_index),
        material_index: Some(material.material_index),
        pass_index: 0,
        shader: Some(material.shader.clone()),
        target: RenderTargetRole::SceneColor,
        target_name: None,
        target_extent: None,
        target_format: None,
        bindings: Vec::new(),
        effect_visibility,
        state: PassState {
            pipeline_blend: super::final_pipeline_blend(contract),
            scene_blend: contract.final_scene_blend,
            ..PassState::default()
        },
    });
}

fn composite_shader(contract: &WeImageGraphContract) -> &'static str {
    if contract.puppet_skinning_after_effects {
        PUPPET_COMPOSITE_SHADER
    } else if contract.final_scene_blend == SceneBlendMode::Multiply {
        IMAGE_MULTIPLY_COMPOSITE_SHADER
    } else {
        IMAGE_COMPOSITE_SHADER
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::core::SceneBlendMode;

    #[test]
    fn compatible_chain_lowers_to_one_quarter_scale_typed_uv_field() {
        let contract = WeImageGraphContract {
            object_index: 7,
            base_material_index: Some(3),
            base_shader: Some("genericimage4".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Alpha,
            static_black_output: false,
            effects_in_authored_texture_space: true,
            puppet_skinning_after_effects: true,
            waterwaves_uv_field_material_index: Some(9),
            waterwaves_direct_material: None,
            foliage_ripple_material: None,
            ripple_flow_material_indices: None,
            final_effect_material: None,
            effect_passes: vec![effect(4, false), effect(5, true)],
        };

        let graph = super::super::we_image_graph(&contract);

        assert_eq!(graph.passes.len(), 2);
        assert_eq!(graph.target_specs.len(), 1);
        let target = &graph.target_specs[0];
        assert_eq!(target.role, RenderTargetRole::Temporary);
        assert_eq!(target.format, UV_TARGET_FORMAT);
        assert_eq!(target.width_divisor_milli, UV_TARGET_DIVISOR_MILLI);
        assert_eq!(target.height_divisor_milli, UV_TARGET_DIVISOR_MILLI);
        assert_eq!(graph.passes[0].shader.as_deref(), Some(UV_FIELD_SHADER));
        assert_eq!(graph.passes[0].material_index, Some(9));
        assert_eq!(
            graph.passes[1].shader.as_deref(),
            Some("we/puppet-waterwaves-composite")
        );
        assert_eq!(graph.passes[1].target, RenderTargetRole::SceneColor);
    }

    #[test]
    fn singleton_waterwaves_pass_keeps_exact_color_pass_path() {
        let contract = WeImageGraphContract {
            object_index: 7,
            base_material_index: Some(3),
            base_shader: Some("genericimage4".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Alpha,
            static_black_output: false,
            effects_in_authored_texture_space: true,
            puppet_skinning_after_effects: true,
            waterwaves_uv_field_material_index: None,
            waterwaves_direct_material: None,
            foliage_ripple_material: None,
            ripple_flow_material_indices: None,
            final_effect_material: None,
            effect_passes: vec![effect(4, true)],
        };

        let graph = super::super::we_image_graph(&contract);

        assert!(graph.target_specs.is_empty());
        assert_eq!(
            graph.passes[1].shader.as_deref(),
            Some("effects/waterwaves__SLOTS_3")
        );
    }

    #[test]
    fn image_multiply_chain_selects_the_premultiplied_fixed_blend_shader() {
        let contract = WeImageGraphContract {
            object_index: 7,
            base_material_index: Some(3),
            base_shader: Some("genericimage4".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Multiply,
            static_black_output: false,
            effects_in_authored_texture_space: true,
            puppet_skinning_after_effects: false,
            waterwaves_uv_field_material_index: Some(9),
            waterwaves_direct_material: None,
            foliage_ripple_material: None,
            ripple_flow_material_indices: None,
            final_effect_material: None,
            effect_passes: vec![effect(4, false), effect(5, true)],
        };

        let graph = super::super::we_image_graph(&contract);
        assert_eq!(
            graph.passes[1].shader.as_deref(),
            Some(IMAGE_MULTIPLY_COMPOSITE_SHADER)
        );
    }

    #[test]
    fn typed_direct_chain_has_one_mesh_composite_and_no_temporary_target() {
        let mut contract = WeImageGraphContract {
            object_index: 7,
            base_material_index: Some(3),
            base_shader: Some("genericimage4".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Alpha,
            static_black_output: false,
            effects_in_authored_texture_space: true,
            puppet_skinning_after_effects: true,
            waterwaves_uv_field_material_index: None,
            waterwaves_direct_material: Some(
                crate::engine::render_graph::WeWaterWavesDirectMaterial {
                    material_index: 12,
                    shader: "we/puppet-waterwaves-direct__STAGES_2".to_owned(),
                    group_visual_composite: false,
                },
            ),
            foliage_ripple_material: None,
            ripple_flow_material_indices: None,
            final_effect_material: None,
            effect_passes: vec![effect(4, false), effect(5, true)],
        };

        let graph = super::super::we_image_graph(&contract);

        assert!(graph.target_specs.is_empty());
        assert_eq!(graph.passes.len(), 1);
        assert_eq!(graph.passes[0].material_index, Some(12));
        assert_eq!(
            graph.passes[0].shader.as_deref(),
            Some("we/puppet-waterwaves-direct__STAGES_2")
        );
        assert_eq!(graph.passes[0].target, RenderTargetRole::SceneColor);
        assert!(graph.passes[0].bindings.is_empty());

        contract
            .waterwaves_direct_material
            .as_mut()
            .expect("direct material")
            .group_visual_composite = true;
        let grouped = super::super::we_image_graph(&contract);
        assert_eq!(grouped.passes.len(), 2);
        assert_eq!(grouped.passes[0].role, RenderPassRole::BaseMaterial);
        assert_eq!(grouped.passes[0].target, RenderTargetRole::Temporary);
        assert_eq!(
            grouped.passes[0].state.pipeline_blend,
            PipelineBlendMode::Translucent
        );
        assert_eq!(grouped.passes[1].role, RenderPassRole::SceneComposite);
        assert_eq!(grouped.passes[1].target, RenderTargetRole::SceneColor);
        assert_eq!(
            grouped.passes[1].shader.as_deref(),
            Some(GROUP_COMPOSITE_SHADER)
        );
        assert_eq!(
            grouped.passes[1].bindings,
            [TextureBindingRole::PreviousGraphTarget { slot: 0 }]
        );
    }

    fn effect(material_index: usize, masked: bool) -> WeEffectPassContract {
        let mut binds = BTreeMap::from([(0, "previous".to_owned())]);
        if masked {
            binds.insert(1, "masks/waterwaves".to_owned());
        }
        WeEffectPassContract {
            object_index: 7,
            effect_binding_start: material_index as u32,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(material_index),
            effect_file: "effects/waterwaves/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some(if masked {
                "effects/waterwaves__SLOTS_3".to_owned()
            } else {
                "effects/waterwaves__SLOTS_1".to_owned()
            }),
            source: None,
            target: None,
            binds,
            pass_constants: vec!["strength".to_owned()],
            material_blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            combos: BTreeMap::new(),
        }
    }
}
