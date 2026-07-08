//! Engine-owned WE layer compositing plan.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/renderer_canvas_render_rd.h`

use std::collections::BTreeSet;

use serde::Serialize;

use super::{
    SceneEffectPassGraphOutput, SceneEffectPassGraphPlan, SceneFinalCompositorPlan,
    SceneGraphTarget, SceneImageLayerTargetPlan, SceneObject, SceneObjectGeometry, SceneObjectId,
    SceneResource,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneLayerCompositorPlan {
    pub layer_count: usize,
    pub command_count: usize,
    pub object_final_layer_count: usize,
    pub tokenized_layer_count: usize,
    pub layers: Vec<SceneLayerCompositorLayer>,
    pub command_order: [&'static str; 9],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneLayerCompositorLayer {
    pub object: SceneObjectId,
    pub route: SceneLayerCompositorRoute,
    pub uses_tokenized_subdraw: bool,
    pub has_active_aux_clear_target: bool,
    pub commands: Vec<SceneLayerCompositorCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SceneLayerCompositorRoute {
    DirectSwapchain,
    ObjectFinalMeshComposite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneLayerCompositorCommand {
    pub entry: SceneLayerCompositorEntry,
    pub operation: SceneLayerCompositorOperation,
    pub condition: SceneLayerCompositorCondition,
    pub source: Option<SceneLayerCompositorTarget>,
    pub target: SceneLayerCompositorTarget,
    pub blend_key: SceneLayerCompositorBlendKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SceneLayerCompositorEntry {
    NormalRenderEntry32,
    ClearPrepEntry50,
    FullLayerCompositeEntry51,
    TokenizedCompositeEntry52,
    TokenizedCompositeWithMaterialEntry53,
    AlphaMaskHelper20d6a0,
    FlatTextureCopyBack20d9ed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SceneLayerCompositorOperation {
    NormalRender,
    ClearPrep,
    FullLayerComposite,
    TokenProgramDispatch,
    DrawClippingMask,
    CopyIntermediateToFullAlphaMask,
    DrawGeneratedClippingTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SceneLayerCompositorCondition {
    Always,
    Token1OrToken2FirstPair,
    Token2IntermediatePairOrFinalMask,
    Token2AfterIntermediateMask,
    TokenizedGeneratedMaterial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SceneLayerCompositorBlendKey {
    Inherit,
    WrapperPushBlendEnumAndAlphaWriteBits0x2000x8,
    LowBlendNormalViaWrapper128,
    SubdrawBlendByteToGeneratedMaterial1f0,
    DestColorCopyBackBit0x100,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SceneLayerCompositorTarget {
    Swapchain,
    ObjectFinal(SceneObjectId),
    LayerTarget490,
    EffectTarget3f8,
    FallbackImage400,
    DirectTarget2d8,
    ImageLayerCompositeA(SceneObjectId),
    ImageLayerSource(SceneObjectId),
    FullAlphaMask,
    FullAlphaMaskIntermediate,
}

impl SceneLayerCompositorPlan {
    pub fn empty() -> Self {
        Self {
            layer_count: 0,
            command_count: 0,
            object_final_layer_count: 0,
            tokenized_layer_count: 0,
            layers: Vec::new(),
            command_order: layer_compositor_command_order(),
        }
    }

    pub fn from_scene(
        resources: &[SceneResource],
        objects: &[SceneObject],
        effect_graph: &SceneEffectPassGraphPlan,
        final_compositor: &SceneFinalCompositorPlan,
    ) -> Self {
        let mut plan = Self::empty();
        let tokenized_targets = tokenized_clipping_targets(effect_graph);
        for object in objects {
            let uses_object_final = final_compositor.contains_object(object.id);
            let image_layer_target = image_layer_target_for_object(effect_graph, object.id);
            let final_input = final_compositor.input_for_object(object.id);
            let uses_tokenized_subdraw = object_has_puppet_clipping(resources, object)
                || effect_graph.passes.iter().any(|pass| {
                    pass.object == object.id
                        && pass_output_uses_tokenized_target(&pass.output, &tokenized_targets)
                });
            let has_active_aux_clear_target = object_has_active_aux_clear_target(resources, object);
            let layer = SceneLayerCompositorLayer {
                object: object.id,
                route: if uses_object_final {
                    SceneLayerCompositorRoute::ObjectFinalMeshComposite
                } else {
                    SceneLayerCompositorRoute::DirectSwapchain
                },
                uses_tokenized_subdraw,
                has_active_aux_clear_target,
                commands: layer_commands(
                    object.id,
                    uses_object_final,
                    uses_tokenized_subdraw,
                    image_layer_target,
                    final_input,
                ),
            };
            plan.command_count = plan.command_count.saturating_add(layer.commands.len());
            plan.object_final_layer_count = plan
                .object_final_layer_count
                .saturating_add(usize::from(uses_object_final));
            plan.tokenized_layer_count = plan
                .tokenized_layer_count
                .saturating_add(usize::from(uses_tokenized_subdraw));
            plan.layers.push(layer);
        }
        plan.layer_count = plan.layers.len();
        plan
    }

    pub fn layer_for_object(&self, object: SceneObjectId) -> Option<&SceneLayerCompositorLayer> {
        self.layers.iter().find(|layer| layer.object == object)
    }
}

impl SceneLayerCompositorLayer {
    pub fn routes_object_final(&self) -> bool {
        self.route == SceneLayerCompositorRoute::ObjectFinalMeshComposite
    }
}

fn tokenized_clipping_targets(
    effect_graph: &SceneEffectPassGraphPlan,
) -> BTreeSet<super::SceneGraphTarget> {
    effect_graph
        .targets
        .iter()
        .filter(|target| tokenized_target_name(&target.name))
        .map(|target| target.target)
        .collect()
}

fn pass_output_uses_tokenized_target(
    output: &SceneEffectPassGraphOutput,
    tokenized_targets: &BTreeSet<super::SceneGraphTarget>,
) -> bool {
    match output {
        SceneEffectPassGraphOutput::ObjectFinal(_) => false,
        SceneEffectPassGraphOutput::GraphTarget(target) => tokenized_targets.contains(target),
    }
}

fn tokenized_target_name(name: &str) -> bool {
    let normalized = name.replace('\\', "/").to_ascii_lowercase();
    normalized.contains("fullalphamask")
        || normalized.contains("alpha_mask")
        || normalized.contains("alphamask")
        || normalized.contains("clipping")
}

fn object_has_puppet_clipping(resources: &[SceneResource], object: &SceneObject) -> bool {
    let SceneObjectGeometry::Puppet { puppet, .. } = object.geometry else {
        return false;
    };
    resources.iter().any(|resource| {
        matches!(
            resource,
            SceneResource::PuppetRig { id, clipping, .. }
                if *id == puppet && !clipping.is_empty()
        )
    })
}

fn object_has_active_aux_clear_target(resources: &[SceneResource], object: &SceneObject) -> bool {
    resources.iter().any(|resource| {
        matches!(
            resource,
            SceneResource::LayerAuxCompositeTargets { targets }
                if targets.object == object.id && targets.clear_prep_ready()
        )
    })
}

fn image_layer_target_for_object(
    effect_graph: &SceneEffectPassGraphPlan,
    object: SceneObjectId,
) -> Option<&SceneImageLayerTargetPlan> {
    effect_graph
        .image_layer_targets
        .iter()
        .find(|target| target.object == object)
}

fn layer_commands(
    object: SceneObjectId,
    uses_object_final: bool,
    uses_tokenized_subdraw: bool,
    image_layer_target: Option<&SceneImageLayerTargetPlan>,
    final_input: Option<SceneGraphTarget>,
) -> Vec<SceneLayerCompositorCommand> {
    let mut commands = Vec::new();
    commands.push(SceneLayerCompositorCommand {
        entry: SceneLayerCompositorEntry::NormalRenderEntry32,
        operation: SceneLayerCompositorOperation::NormalRender,
        condition: SceneLayerCompositorCondition::Always,
        source: None,
        target: if uses_object_final {
            image_layer_target
                .map(|target| layer_target_from_graph_target(target.prefill_target))
                .unwrap_or(SceneLayerCompositorTarget::ObjectFinal(object))
        } else {
            SceneLayerCompositorTarget::Swapchain
        },
        blend_key: SceneLayerCompositorBlendKey::WrapperPushBlendEnumAndAlphaWriteBits0x2000x8,
    });

    if uses_object_final {
        commands.push(SceneLayerCompositorCommand {
            entry: SceneLayerCompositorEntry::ClearPrepEntry50,
            operation: SceneLayerCompositorOperation::ClearPrep,
            condition: SceneLayerCompositorCondition::Always,
            source: None,
            target: SceneLayerCompositorTarget::LayerTarget490,
            blend_key: SceneLayerCompositorBlendKey::Inherit,
        });
    }

    if uses_tokenized_subdraw {
        commands.push(SceneLayerCompositorCommand {
            entry: SceneLayerCompositorEntry::TokenizedCompositeEntry52,
            operation: SceneLayerCompositorOperation::TokenProgramDispatch,
            condition: SceneLayerCompositorCondition::Always,
            target: SceneLayerCompositorTarget::LayerTarget490,
            source: None,
            blend_key: SceneLayerCompositorBlendKey::Inherit,
        });
        commands.push(SceneLayerCompositorCommand {
            entry: SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
            operation: SceneLayerCompositorOperation::DrawClippingMask,
            condition: SceneLayerCompositorCondition::Token1OrToken2FirstPair,
            source: None,
            target: SceneLayerCompositorTarget::FullAlphaMask,
            blend_key: SceneLayerCompositorBlendKey::Inherit,
        });
        commands.push(SceneLayerCompositorCommand {
            entry: SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
            operation: SceneLayerCompositorOperation::DrawClippingMask,
            condition: SceneLayerCompositorCondition::Token2IntermediatePairOrFinalMask,
            source: None,
            target: SceneLayerCompositorTarget::FullAlphaMaskIntermediate,
            blend_key: SceneLayerCompositorBlendKey::Inherit,
        });
        commands.push(SceneLayerCompositorCommand {
            entry: SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed,
            operation: SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
            condition: SceneLayerCompositorCondition::Token2AfterIntermediateMask,
            source: Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
            target: SceneLayerCompositorTarget::FullAlphaMask,
            blend_key: SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
        });
        commands.push(SceneLayerCompositorCommand {
            entry: SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53,
            operation: SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
            condition: SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
            source: Some(SceneLayerCompositorTarget::FullAlphaMask),
            target: SceneLayerCompositorTarget::LayerTarget490,
            blend_key: SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0,
        });
    }

    if uses_object_final {
        commands.push(SceneLayerCompositorCommand {
            entry: SceneLayerCompositorEntry::FullLayerCompositeEntry51,
            operation: SceneLayerCompositorOperation::FullLayerComposite,
            condition: SceneLayerCompositorCondition::Always,
            source: final_input
                .map(layer_target_from_graph_target)
                .or(Some(SceneLayerCompositorTarget::ObjectFinal(object))),
            target: SceneLayerCompositorTarget::Swapchain,
            blend_key: SceneLayerCompositorBlendKey::LowBlendNormalViaWrapper128,
        });
    }

    commands
}

fn layer_target_from_graph_target(target: SceneGraphTarget) -> SceneLayerCompositorTarget {
    match target {
        SceneGraphTarget::Swapchain => SceneLayerCompositorTarget::Swapchain,
        SceneGraphTarget::ObjectFinal(object) => SceneLayerCompositorTarget::ObjectFinal(object),
        SceneGraphTarget::ImageLayerCompositeA(object) => {
            SceneLayerCompositorTarget::ImageLayerCompositeA(object)
        }
        SceneGraphTarget::ImageLayerSource(object) => {
            SceneLayerCompositorTarget::ImageLayerSource(object)
        }
        SceneGraphTarget::FullAlphaMask => SceneLayerCompositorTarget::FullAlphaMask,
        SceneGraphTarget::FullAlphaMaskIntermediate => {
            SceneLayerCompositorTarget::FullAlphaMaskIntermediate
        }
        SceneGraphTarget::ImageLocalMain(_)
        | SceneGraphTarget::ImageLocalSub(_)
        | SceneGraphTarget::NamedFbo(_)
        | SceneGraphTarget::EffectTarget(_)
        | SceneGraphTarget::LayerAuxClear(_) => {
            panic!("scene layer compositor cannot consume unrelated graph target {target:?}")
        }
    }
}

fn layer_compositor_command_order() -> [&'static str; 9] {
    [
        "preserve_scene_object_order",
        "classify_object_final_routes",
        "model_vtable_32_normal_render_entry",
        "model_vtable_50_clear_prep_entry",
        "model_vtable_52_53_tokenized_subdraw_entries",
        "model_wrapper_0xd8_0x128_state_keys",
        "model_flattexture_intermediate_copy_back_bit_0x100",
        "model_vtable_51_full_layer_composite_entry",
        "lower_layer_routes_to_scene_graph_passes",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneEffectFboFormat, SceneEffectPassBlend, SceneEffectPassGraphMaterialPass,
        SceneEffectPassGraphOutput, SceneEffectPassGraphTarget, SceneFinalCompositorObjectInput,
        SceneFinalCompositorPlan, SceneGeometryId, SceneGraphTarget, SceneMaterialContract,
        SceneObjectGeometry, SceneResourceId, we::WeEffectKind,
    };

    #[test]
    fn layer_compositor_preserves_object_order_and_routes_object_final() {
        let objects = vec![
            object(SceneObjectId(1)),
            object(SceneObjectId(2)),
            object(SceneObjectId(3)),
        ];
        let effect_graph = SceneEffectPassGraphPlan {
            material_pass_count: 1,
            passes: vec![effect_output_pass(SceneObjectId(2))],
            ..SceneEffectPassGraphPlan::empty()
        };
        let final_compositor =
            SceneFinalCompositorPlan::from_effect_pass_graph(&objects, &effect_graph);

        let plan =
            SceneLayerCompositorPlan::from_scene(&[], &objects, &effect_graph, &final_compositor);

        assert_eq!(plan.layer_count, 3);
        assert_eq!(plan.object_final_layer_count, 1);
        assert_eq!(plan.tokenized_layer_count, 0);
        assert_eq!(plan.layers[0].object, SceneObjectId(1));
        assert_eq!(plan.layers[1].object, SceneObjectId(2));
        assert_eq!(plan.layers[2].object, SceneObjectId(3));
        assert_eq!(
            plan.layers[1].route,
            SceneLayerCompositorRoute::ObjectFinalMeshComposite
        );
        assert!(plan.layers[1].routes_object_final());
        assert_eq!(
            plan.layers[1].commands,
            vec![
                command(
                    SceneLayerCompositorEntry::NormalRenderEntry32,
                    SceneLayerCompositorOperation::NormalRender,
                    SceneLayerCompositorCondition::Always,
                    None,
                    SceneLayerCompositorTarget::ObjectFinal(SceneObjectId(2)),
                    SceneLayerCompositorBlendKey::WrapperPushBlendEnumAndAlphaWriteBits0x2000x8,
                ),
                command(
                    SceneLayerCompositorEntry::ClearPrepEntry50,
                    SceneLayerCompositorOperation::ClearPrep,
                    SceneLayerCompositorCondition::Always,
                    None,
                    SceneLayerCompositorTarget::LayerTarget490,
                    SceneLayerCompositorBlendKey::Inherit,
                ),
                command(
                    SceneLayerCompositorEntry::FullLayerCompositeEntry51,
                    SceneLayerCompositorOperation::FullLayerComposite,
                    SceneLayerCompositorCondition::Always,
                    Some(SceneLayerCompositorTarget::ObjectFinal(SceneObjectId(2))),
                    SceneLayerCompositorTarget::Swapchain,
                    SceneLayerCompositorBlendKey::LowBlendNormalViaWrapper128,
                ),
            ]
        );
    }

    #[test]
    fn layer_compositor_routes_scene_output_effects_through_image_layer_targets() {
        let object_id = SceneObjectId(1530);
        let objects = vec![object(object_id)];
        let image_layer_target =
            crate::engine::scene_engine::SceneImageLayerTargetPlan::for_object(
                object_id,
                Some(SceneResourceId(77)),
                1,
            )
            .expect("image layer target plan");
        let effect_graph = SceneEffectPassGraphPlan {
            material_pass_count: 1,
            image_layer_target_count: 1,
            image_layer_scene_output_pass_count: 1,
            image_layer_targets: vec![image_layer_target],
            passes: vec![SceneEffectPassGraphMaterialPass {
                output: SceneEffectPassGraphOutput::GraphTarget(
                    SceneGraphTarget::ImageLayerCompositeA(object_id),
                ),
                ..effect_output_pass(object_id)
            }],
            ..SceneEffectPassGraphPlan::empty()
        };
        let final_compositor =
            SceneFinalCompositorPlan::from_effect_pass_graph(&objects, &effect_graph);

        let plan =
            SceneLayerCompositorPlan::from_scene(&[], &objects, &effect_graph, &final_compositor);

        assert_eq!(plan.object_final_layer_count, 1);
        assert_eq!(
            plan.layers[0].commands[0].target,
            SceneLayerCompositorTarget::ImageLayerSource(object_id)
        );
        assert_eq!(
            plan.layers[0].commands.last().unwrap().source,
            Some(SceneLayerCompositorTarget::ImageLayerCompositeA(object_id))
        );
        assert_eq!(
            plan.layers[0].commands.last().unwrap().target,
            SceneLayerCompositorTarget::Swapchain
        );
    }

    #[test]
    fn layer_compositor_keeps_direct_layers_single_normal_entry() {
        let objects = vec![object(SceneObjectId(9))];
        let effect_graph = SceneEffectPassGraphPlan::empty();
        let final_compositor = SceneFinalCompositorPlan::empty();

        let plan =
            SceneLayerCompositorPlan::from_scene(&[], &objects, &effect_graph, &final_compositor);

        assert_eq!(plan.command_count, 1);
        assert_eq!(
            plan.layers[0].route,
            SceneLayerCompositorRoute::DirectSwapchain
        );
        assert_eq!(
            plan.layers[0].commands[0],
            command(
                SceneLayerCompositorEntry::NormalRenderEntry32,
                SceneLayerCompositorOperation::NormalRender,
                SceneLayerCompositorCondition::Always,
                None,
                SceneLayerCompositorTarget::Swapchain,
                SceneLayerCompositorBlendKey::WrapperPushBlendEnumAndAlphaWriteBits0x2000x8,
            )
        );
    }

    #[test]
    fn layer_compositor_marks_tokenized_entries_only_for_puppet_clipping() {
        let object = SceneObject {
            id: SceneObjectId(42),
            geometry: SceneObjectGeometry::Puppet {
                geometry: SceneGeometryId(3),
                puppet: crate::engine::scene_engine::ScenePuppetId(5),
                vertex_count: 16,
                index_count: 24,
            },
            material: SceneMaterialContract::we_translucent("we/genericimage4"),
            source: Some(SceneResourceId(8)),
        };
        let resources = vec![SceneResource::PuppetRig {
            id: crate::engine::scene_engine::ScenePuppetId(5),
            source_record: 0,
            skin: None,
            clips: Vec::new(),
            layers: Vec::new(),
            clipping: crate::engine::scene_engine::ScenePuppetClippingProgram::from_source_records(
                vec![crate::core::scene::SceneMeshPuppetClippingRecord {
                    source_name: Some("eye-right".to_owned()),
                    mask: "masks/clipping_mask_eye".to_owned(),
                    mask_resource: Some("assets/clipping-mask.gtex".to_owned()),
                    duration_frames: 1680,
                    flags: 1,
                    bones: vec![42, 43],
                    frame_keys: vec![0, 1, 2],
                }],
                Vec::new(),
            ),
        }];
        let effect_graph = SceneEffectPassGraphPlan::empty();
        let final_compositor = SceneFinalCompositorPlan::empty();

        let plan = SceneLayerCompositorPlan::from_scene(
            &resources,
            &[object],
            &effect_graph,
            &final_compositor,
        );

        assert_eq!(plan.tokenized_layer_count, 1);
        assert!(plan.layers[0].uses_tokenized_subdraw);
        assert!(!plan.layers[0].has_active_aux_clear_target);
        assert!(plan.layers[0].commands.iter().any(|command| {
            command.entry == SceneLayerCompositorEntry::TokenizedCompositeEntry52
        }));
        assert!(plan.layers[0].commands.iter().any(|command| {
            command.entry == SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53
        }));
        assert!(plan.layers[0].commands.iter().any(|command| {
            command.operation == SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
                && command.source == Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate)
                && command.target == SceneLayerCompositorTarget::FullAlphaMask
                && command.blend_key == SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100
        }));
        assert!(plan.layers[0].commands.iter().any(|command| {
            command.operation == SceneLayerCompositorOperation::DrawGeneratedClippingTarget
                && command.source == Some(SceneLayerCompositorTarget::FullAlphaMask)
                && command.target == SceneLayerCompositorTarget::LayerTarget490
                && command.blend_key
                    == SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0
        }));
    }

    #[test]
    fn layer_compositor_marks_active_aux_clear_prep_only_from_complete_aux_fact() {
        let object = object(SceneObjectId(77));
        let final_compositor = SceneFinalCompositorPlan {
            object_final_count: 1,
            pass_count: 0,
            object_finals: vec![object.id],
            object_inputs: vec![SceneFinalCompositorObjectInput {
                object: object.id,
                input: SceneGraphTarget::ObjectFinal(object.id),
            }],
            passes: Vec::new(),
            command_order: SceneFinalCompositorPlan::empty().command_order,
        };
        let incomplete = SceneResource::LayerAuxCompositeTargets {
            targets: crate::engine::scene_engine::SceneLayerAuxCompositeTargets {
                object: object.id,
                clear_target_3e8: true,
                material_target_3f0: true,
                effect_target_3f8: false,
                generated_material_408: true,
                clear_material_410: true,
                clear_source_width: 3840,
                clear_source_height: 2160,
                clear_target_width: 3840,
                clear_target_height: 2160,
                clear_uv_y_flipped: false,
                clear_target_color_format:
                    crate::engine::scene_engine::WE_LAYER_AUX_CLEAR_TARGET_DEFAULT_COLOR_FORMAT,
                clear_target_aux_format:
                    crate::engine::scene_engine::WE_LAYER_AUX_CLEAR_TARGET_AUX_FORMAT,
                clear_target_r9_selector:
                    crate::engine::scene_engine::WE_LAYER_AUX_CLEAR_TARGET_R9_SELECTOR,
                clear_target_resource_selector:
                    crate::engine::scene_engine::WE_LAYER_AUX_CLEAR_TARGET_RESOURCE_SELECTOR,
                clear_target_cache_selector:
                    crate::engine::scene_engine::WE_LAYER_AUX_CLEAR_TARGET_CACHE_SELECTOR,
            },
        };

        let incomplete_plan = SceneLayerCompositorPlan::from_scene(
            &[incomplete],
            std::slice::from_ref(&object),
            &SceneEffectPassGraphPlan::empty(),
            &final_compositor,
        );
        assert!(!incomplete_plan.layers[0].has_active_aux_clear_target);

        let complete = SceneResource::LayerAuxCompositeTargets {
            targets: crate::engine::scene_engine::SceneLayerAuxCompositeTargets {
                object: object.id,
                clear_target_3e8: true,
                material_target_3f0: true,
                effect_target_3f8: true,
                generated_material_408: true,
                clear_material_410: true,
                clear_source_width: 3840,
                clear_source_height: 2160,
                clear_target_width: 3840,
                clear_target_height: 2160,
                clear_uv_y_flipped: false,
                clear_target_color_format:
                    crate::engine::scene_engine::WE_LAYER_AUX_CLEAR_TARGET_DEFAULT_COLOR_FORMAT,
                clear_target_aux_format:
                    crate::engine::scene_engine::WE_LAYER_AUX_CLEAR_TARGET_AUX_FORMAT,
                clear_target_r9_selector:
                    crate::engine::scene_engine::WE_LAYER_AUX_CLEAR_TARGET_R9_SELECTOR,
                clear_target_resource_selector:
                    crate::engine::scene_engine::WE_LAYER_AUX_CLEAR_TARGET_RESOURCE_SELECTOR,
                clear_target_cache_selector:
                    crate::engine::scene_engine::WE_LAYER_AUX_CLEAR_TARGET_CACHE_SELECTOR,
            },
        };
        let complete_plan = SceneLayerCompositorPlan::from_scene(
            &[complete],
            &[object],
            &SceneEffectPassGraphPlan::empty(),
            &final_compositor,
        );

        assert!(complete_plan.layers[0].has_active_aux_clear_target);
        assert!(complete_plan.layers[0].commands.iter().any(|command| {
            command.entry == SceneLayerCompositorEntry::ClearPrepEntry50
                && command.operation == SceneLayerCompositorOperation::ClearPrep
        }));
    }

    #[test]
    fn layer_compositor_marks_named_alpha_mask_targets_as_tokenized() {
        let objects = vec![object(SceneObjectId(12))];
        let alpha_mask_target = SceneGraphTarget::NamedFbo(44);
        let effect_graph = SceneEffectPassGraphPlan {
            target_count: 1,
            material_pass_count: 1,
            targets: vec![SceneEffectPassGraphTarget {
                target: alpha_mask_target,
                object: SceneObjectId(12),
                program_index: 0,
                name: "_rt_FullAlphaMaskIntermediate".to_owned(),
                format: Some(SceneEffectFboFormat::R8Unorm),
                scale: 2.0,
                unique: false,
            }],
            passes: vec![SceneEffectPassGraphMaterialPass {
                output: SceneEffectPassGraphOutput::GraphTarget(alpha_mask_target),
                ..effect_output_pass(SceneObjectId(12))
            }],
            ..SceneEffectPassGraphPlan::empty()
        };
        let final_compositor = SceneFinalCompositorPlan::empty();

        let plan =
            SceneLayerCompositorPlan::from_scene(&[], &objects, &effect_graph, &final_compositor);

        assert_eq!(plan.tokenized_layer_count, 1);
        assert!(plan.layers[0].uses_tokenized_subdraw);
        assert!(!plan.layers[0].has_active_aux_clear_target);
        assert!(plan.layers[0].commands.iter().any(|command| {
            command.condition == SceneLayerCompositorCondition::Token2AfterIntermediateMask
                && command.entry == SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed
        }));
    }

    fn command(
        entry: SceneLayerCompositorEntry,
        operation: SceneLayerCompositorOperation,
        condition: SceneLayerCompositorCondition,
        source: Option<SceneLayerCompositorTarget>,
        target: SceneLayerCompositorTarget,
        blend_key: SceneLayerCompositorBlendKey,
    ) -> SceneLayerCompositorCommand {
        SceneLayerCompositorCommand {
            entry,
            operation,
            condition,
            source,
            target,
            blend_key,
        }
    }

    fn effect_output_pass(object: SceneObjectId) -> SceneEffectPassGraphMaterialPass {
        SceneEffectPassGraphMaterialPass {
            graph_command_index: 0,
            graph_pass_index: 0,
            object,
            program_index: 0,
            pass_index: 0,
            effect_file: "effects/opacity/effect.json".to_owned(),
            effect: WeEffectKind::Opacity,
            shader: Some("effects/opacity".to_owned()),
            source: None,
            input_bindings: Vec::new(),
            output: SceneEffectPassGraphOutput::ObjectFinal(object),
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: crate::engine::scene_engine::SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: crate::engine::scene_engine::SceneCullMode::None,
            alpha_write: crate::engine::scene_engine::SceneAlphaWriteMode::Default,
            texture_resources: Vec::new(),
            combos: Default::default(),
            constants: Default::default(),
        }
    }

    fn object(id: SceneObjectId) -> SceneObject {
        SceneObject {
            id,
            geometry: SceneObjectGeometry::Mesh {
                geometry: SceneGeometryId(id.0),
                vertex_count: 4,
                index_count: 6,
            },
            material: SceneMaterialContract::we_translucent("we/genericimage4"),
            source: Some(SceneResourceId(id.0)),
        }
    }
}
