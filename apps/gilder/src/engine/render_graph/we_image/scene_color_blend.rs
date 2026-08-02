//! Typed WE object color-blend preservation through a scene snapshot.

use super::WeImageGraphContract;
use crate::core::SceneBlendMode;
use crate::engine::render_graph::{
    ColorWriteMask, PassState, PipelineBlendMode, RenderGraph, RenderPassDrawPrimitive,
    RenderPassEffectVisibility, RenderPassNode, RenderPassRole, RenderTargetRole,
    TextureBindingRole,
};

pub(super) const SCENE_SNAPSHOT_SLOT: u32 = 4;
const SHADER: &str = "we/genericimage4-scene-color-blend";

pub(super) fn is_compatible(contract: &WeImageGraphContract) -> bool {
    (1..=30).contains(&contract.color_blend_mode)
        && contract.effect_passes.is_empty()
        && contract.base_material_index.is_some()
        && contract
            .base_shader
            .as_deref()
            .is_some_and(is_unskinned_generic_image)
        && contract.base_texture_slots.as_slice() == [0]
}

pub(super) fn append_authored_source_and_composite(
    graph: &mut RenderGraph,
    contract: &WeImageGraphContract,
) {
    let snapshot = contract
        .framebuffer_snapshot
        .as_ref()
        .expect("shader object blend requires a typed scene snapshot");
    assert_eq!(
        snapshot.texture_slot, SCENE_SNAPSHOT_SLOT,
        "shader object blend snapshot must retain WE slot 4"
    );
    graph.passes.push(RenderPassNode {
        id: 0,
        role: RenderPassRole::CopyTarget,
        draw_primitive: RenderPassDrawPrimitive::None,
        object_index: Some(contract.object_index),
        material_index: None,
        pass_index: 0,
        shader: None,
        target: RenderTargetRole::FirstClassEffectTarget,
        target_name: Some(snapshot.target_name.clone()),
        target_extent: None,
        target_format: Some("rgba_backbuffer".to_owned()),
        bindings: vec![TextureBindingRole::GraphTarget {
            slot: snapshot.texture_slot,
            role: RenderTargetRole::SceneColor,
            name: None,
        }],
        effect_visibility: RenderPassEffectVisibility::NONE,
        state: PassState::default(),
    });
    graph.passes.push(RenderPassNode {
        id: 1,
        role: RenderPassRole::ObjectLocalSource,
        draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
        object_index: Some(contract.object_index),
        material_index: contract.base_material_index,
        pass_index: 0,
        shader: contract.base_shader.clone(),
        target: RenderTargetRole::ImageLocalMain,
        target_name: None,
        target_extent: None,
        target_format: Some("rgba8".to_owned()),
        bindings: std::iter::once(TextureBindingRole::SourceTexture)
            .chain(
                contract
                    .base_texture_slots
                    .iter()
                    .copied()
                    .filter(|slot| *slot != 0)
                    .map(|slot| TextureBindingRole::TextureSlot { slot }),
            )
            .chain(
                contract
                    .base_pass_constants
                    .iter()
                    .cloned()
                    .map(|name| TextureBindingRole::PassConstant { name }),
            )
            .collect(),
        effect_visibility: RenderPassEffectVisibility::NONE,
        state: PassState {
            pipeline_blend: PipelineBlendMode::Normal,
            scene_blend: SceneBlendMode::Normal,
            ..PassState::default()
        },
    });
    graph.passes.push(RenderPassNode {
        id: 2,
        role: RenderPassRole::SceneComposite,
        draw_primitive: RenderPassDrawPrimitive::ObjectCompositeMesh,
        object_index: Some(contract.object_index),
        material_index: contract.base_material_index,
        pass_index: 0,
        shader: Some(SHADER.to_owned()),
        target: RenderTargetRole::SceneColor,
        target_name: None,
        target_extent: None,
        target_format: None,
        bindings: vec![
            TextureBindingRole::PreviousGraphTarget { slot: 0 },
            TextureBindingRole::EffectTarget {
                slot: snapshot.texture_slot,
                name: snapshot.target_name.clone(),
            },
        ],
        effect_visibility: RenderPassEffectVisibility::NONE,
        state: PassState {
            pipeline_blend: PipelineBlendMode::Translucent,
            scene_blend: SceneBlendMode::Alpha,
            color_write_mask: ColorWriteMask::Rgb,
            ..PassState::default()
        },
    });
}

fn is_unskinned_generic_image(shader: &str) -> bool {
    !shader.to_ascii_lowercase().contains("puppetskinning")
        && matches!(
            shader
                .split("__")
                .next()
                .unwrap_or_default()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "genericimage2" | "genericimage4"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_preserves_snapshot_local_source_and_mesh_composite() {
        let contract = WeImageGraphContract {
            object_index: 1,
            base_material_index: Some(5),
            base_shader: Some("we/genericimage4".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            color_blend_mode: 11,
            framebuffer_snapshot: Some(super::super::WeFramebufferSnapshotContract {
                target_name: "_rt_FullFrameBuffer".to_owned(),
                texture_slot: SCENE_SNAPSHOT_SLOT,
                composite_to_object_mesh: false,
                usage: super::super::WeFramebufferSnapshotUsage::ObjectSource,
            }),
            final_scene_blend: SceneBlendMode::Alpha,
            static_black_output: false,
            effects_in_authored_texture_space: true,
            puppet_skinning_after_effects: false,
            waterwaves_uv_field_material_index: None,
            waterwaves_direct_material: None,
            foliage_ripple_material: None,
            final_effect_material: None,
            effect_passes: Vec::new(),
        };

        let graph = super::super::we_image_graph(&contract);

        assert_eq!(graph.passes.len(), 3);
        assert_eq!(graph.passes[0].role, RenderPassRole::CopyTarget);
        assert_eq!(graph.passes[1].role, RenderPassRole::ObjectLocalSource);
        assert_eq!(graph.passes[1].target, RenderTargetRole::ImageLocalMain);
        assert_eq!(
            graph.passes[1].state.pipeline_blend,
            PipelineBlendMode::Normal
        );
        assert_eq!(graph.passes[2].role, RenderPassRole::SceneComposite);
        assert_eq!(
            graph.passes[2].draw_primitive,
            RenderPassDrawPrimitive::ObjectCompositeMesh
        );
        assert_eq!(graph.passes[2].shader.as_deref(), Some(SHADER));
        assert_eq!(graph.passes[2].state.color_write_mask, ColorWriteMask::Rgb);
        assert_eq!(
            graph.passes[2].state.pipeline_blend,
            PipelineBlendMode::Translucent
        );
        assert_eq!(graph.passes[2].bindings.len(), 2);
    }

    #[test]
    fn normal_and_fixed_function_object_blends_do_not_enter_shader_snapshot_path() {
        for mode in [0, 31, 32] {
            let contract = WeImageGraphContract {
                object_index: 1,
                base_material_index: Some(5),
                base_shader: Some("we/genericimage4".to_owned()),
                base_material_blending: Some("translucent".to_owned()),
                base_texture_slots: vec![0],
                base_pass_constants: Vec::new(),
                color_blend_mode: mode,
                framebuffer_snapshot: None,
                final_scene_blend: SceneBlendMode::Alpha,
                static_black_output: false,
                effects_in_authored_texture_space: true,
                puppet_skinning_after_effects: false,
                waterwaves_uv_field_material_index: None,
                waterwaves_direct_material: None,
                foliage_ripple_material: None,
                final_effect_material: None,
                effect_passes: Vec::new(),
            };
            assert!(!is_compatible(&contract));
        }
    }
}
