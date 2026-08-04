//! Typed cross-graph render textures produced by hidden WE dependency layers.

use std::collections::BTreeSet;

use crate::convert::we_ingest::ir::{
    WeIrImageTarget, WeIrImageTargetExtentDomain, WeIrImageTargetRole,
};
use crate::engine::render_graph::{
    ColorWriteMask, PassState, RenderGraph, RenderPassDrawPrimitive, RenderPassEffectVisibility,
    RenderPassNode, RenderPassRole, RenderTargetExtentDomain, RenderTargetRole, TextureBindingRole,
};

use super::WeIrBuilder;

const IMAGE_LAYER_COMPOSITE_PREFIX: &str = "_rt_imageLayerComposite_";

impl WeIrBuilder {
    pub(super) fn materialize_image_layer_composite_targets(&mut self) {
        let target_names = self
            .render_graphs
            .iter()
            .flat_map(|graph| &graph.passes)
            .flat_map(|pass| &pass.bindings)
            .filter_map(|binding| match binding {
                TextureBindingRole::EffectTarget { name, .. }
                    if image_layer_composite_we_id(name.as_str()).is_some() =>
                {
                    Some(name.clone())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();

        for target_name in target_names {
            let Some(we_id) = image_layer_composite_we_id(&target_name) else {
                continue;
            };
            let Some((object_index, graph_index)) = self
                .objects
                .iter()
                .enumerate()
                .find(|(_, object)| object.we_id == we_id)
                .and_then(|(object_index, object)| {
                    object
                        .render_graph
                        .map(|graph_index| (object_index, graph_index as usize))
                })
            else {
                continue;
            };
            let Some(graph) = self.render_graphs.get_mut(graph_index) else {
                continue;
            };
            let extent_domain = materialize_graph_target(graph, object_index, &target_name);
            if !self.image_targets.iter().any(|target| {
                target.name == target_name
                    && target.role == WeIrImageTargetRole::FirstClassEffectTarget
            }) {
                self.image_targets.push(WeIrImageTarget {
                    name: target_name,
                    format: "rgba_backbuffer".to_owned(),
                    role: WeIrImageTargetRole::FirstClassEffectTarget,
                    extent_domain: match extent_domain {
                        RenderTargetExtentDomain::PhysicalSurface => {
                            WeIrImageTargetExtentDomain::PhysicalSurface
                        }
                        RenderTargetExtentDomain::OwnerAuthored => {
                            WeIrImageTargetExtentDomain::OwnerAuthored
                        }
                        RenderTargetExtentDomain::GraphSource => {
                            WeIrImageTargetExtentDomain::GraphSource
                        }
                    },
                    width_divisor_milli: 1_000,
                    height_divisor_milli: 1_000,
                });
            }
        }
    }
}

fn materialize_graph_target(
    graph: &mut RenderGraph,
    object_index: usize,
    target_name: &str,
) -> RenderTargetExtentDomain {
    let self_consumer = graph.passes.iter().position(|pass| {
        pass.bindings.iter().any(|binding| {
            matches!(
                binding,
                TextureBindingRole::EffectTarget { name, .. } if name == target_name
            )
        })
    });
    let producer_limit = self_consumer.unwrap_or(graph.passes.len());
    let producer = graph.passes[..producer_limit].iter().rposition(|pass| {
        pass.role != RenderPassRole::SceneComposite
            && matches!(
                pass.target,
                RenderTargetRole::SceneColor
                    | RenderTargetRole::ImageLocalMain
                    | RenderTargetRole::ImageLocalSub
            )
    });
    if let Some(producer) = producer {
        let extent_domain = match graph.passes[producer].target {
            RenderTargetRole::ImageLocalMain | RenderTargetRole::ImageLocalSub => {
                RenderTargetExtentDomain::GraphSource
            }
            // `_rt_imageLayerComposite_*` is the layer-owned source target rebuilt from the
            // producer's authored image extent. It is not a physical SceneColor snapshot.
            RenderTargetRole::SceneColor => RenderTargetExtentDomain::GraphSource,
            _ => unreachable!("image-layer composite producer target is prefiltered"),
        };
        graph.passes[producer].target = RenderTargetRole::FirstClassEffectTarget;
        graph.passes[producer].target_name = Some(target_name.to_owned());
        if let Some(consumer) = self_consumer {
            promote_terminal_self_consumer(graph, consumer);
        }
        return extent_domain;
    }
    for pass in &mut graph.passes {
        pass.id = pass.id.saturating_add(1);
    }
    graph.passes.insert(
        0,
        RenderPassNode {
            id: 0,
            role: RenderPassRole::CopyTarget,
            draw_primitive: RenderPassDrawPrimitive::None,
            object_index: Some(object_index),
            material_index: None,
            pass_index: 0,
            shader: None,
            target: RenderTargetRole::FirstClassEffectTarget,
            target_name: Some(target_name.to_owned()),
            target_extent: None,
            target_format: Some("rgba_backbuffer".to_owned()),
            bindings: vec![TextureBindingRole::GraphTarget {
                slot: 0,
                role: RenderTargetRole::SceneColor,
                name: None,
            }],
            effect_visibility: RenderPassEffectVisibility::NONE,
            state: PassState::default(),
        },
    );
    RenderTargetExtentDomain::PhysicalSurface
}

/// A layer may bind its own `_rt_imageLayerComposite_*` source from its terminal effect. WE first
/// renders the base layer into that target, then executes the terminal effect directly on the
/// scene-color object quad. Selecting the consumer itself as the target producer creates an
/// impossible read/write cycle and an extra composite.
fn promote_terminal_self_consumer(graph: &mut RenderGraph, consumer: usize) {
    if consumer + 2 != graph.passes.len() {
        return;
    }
    let terminal = &graph.passes[consumer + 1];
    if terminal.role != RenderPassRole::SceneComposite
        || terminal.target != RenderTargetRole::SceneColor
        || terminal.bindings != [TextureBindingRole::PreviousGraphTarget { slot: 0 }]
        || !matches!(
            graph.passes[consumer].role,
            RenderPassRole::EffectMaterial | RenderPassRole::ColorBlendPassthrough
        )
        || !matches!(
            graph.passes[consumer].target,
            RenderTargetRole::ImageLocalMain | RenderTargetRole::ImageLocalSub
        )
        || graph.passes[consumer].target_name.is_some()
    {
        return;
    }

    let terminal_pipeline_blend = terminal.state.pipeline_blend;
    let terminal_scene_blend = terminal.state.scene_blend;
    let pass = &mut graph.passes[consumer];
    pass.role = RenderPassRole::SceneComposite;
    pass.draw_primitive = RenderPassDrawPrimitive::ObjectMesh;
    pass.target = RenderTargetRole::SceneColor;
    pass.state.pipeline_blend = terminal_pipeline_blend;
    pass.state.scene_blend = terminal_scene_blend;
    pass.state.color_write_mask = ColorWriteMask::Rgb;
    graph.passes.pop();
}

fn image_layer_composite_we_id(name: &str) -> Option<u32> {
    name.strip_prefix(IMAGE_LAYER_COMPOSITE_PREFIX)?
        .split('_')
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_target_recovers_source_object_id() {
        assert_eq!(
            image_layer_composite_we_id("_rt_imageLayerComposite_398_a"),
            Some(398)
        );
        assert_eq!(image_layer_composite_we_id("_rt_FullFrameBuffer"), None);
    }

    #[test]
    fn empty_dependency_graph_snapshots_scene_color_into_its_first_class_target() {
        let mut graph = RenderGraph::default();

        materialize_graph_target(&mut graph, 7, "_rt_imageLayerComposite_461_a");

        assert_eq!(graph.passes.len(), 1);
        let pass = &graph.passes[0];
        assert_eq!(pass.role, RenderPassRole::CopyTarget);
        assert_eq!(pass.object_index, Some(7));
        assert_eq!(pass.target, RenderTargetRole::FirstClassEffectTarget);
        assert_eq!(
            pass.target_name.as_deref(),
            Some("_rt_imageLayerComposite_461_a")
        );
        assert_eq!(
            pass.bindings,
            [TextureBindingRole::GraphTarget {
                slot: 0,
                role: RenderTargetRole::SceneColor,
                name: None,
            }]
        );
    }

    #[test]
    fn direct_scene_producer_moves_into_authored_texture_space() {
        let mut graph = RenderGraph {
            passes: vec![RenderPassNode {
                id: 0,
                role: RenderPassRole::BaseMaterial,
                draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
                object_index: Some(7),
                material_index: Some(3),
                pass_index: 0,
                shader: Some("genericimage2".to_owned()),
                target: RenderTargetRole::SceneColor,
                target_name: None,
                target_extent: None,
                target_format: None,
                bindings: vec![TextureBindingRole::SourceTexture],
                effect_visibility: RenderPassEffectVisibility::NONE,
                state: PassState::default(),
            }],
            ..RenderGraph::default()
        };

        let extent_domain =
            materialize_graph_target(&mut graph, 7, "_rt_imageLayerComposite_461_a");

        assert_eq!(extent_domain, RenderTargetExtentDomain::GraphSource);
        assert_eq!(graph.passes.len(), 1);
        let pass = &graph.passes[0];
        assert_eq!(pass.role, RenderPassRole::BaseMaterial);
        assert_eq!(pass.target, RenderTargetRole::FirstClassEffectTarget);
        assert_eq!(
            pass.target_name.as_deref(),
            Some("_rt_imageLayerComposite_461_a")
        );
        assert_eq!(pass.draw_primitive, RenderPassDrawPrimitive::ObjectMesh);
        assert_eq!(pass.bindings, [TextureBindingRole::SourceTexture]);
    }

    #[test]
    fn self_consuming_terminal_effect_uses_the_base_target_then_composites_directly() {
        let target = "_rt_imageLayerComposite_461_a";
        let mut graph = RenderGraph {
            passes: vec![
                RenderPassNode {
                    id: 0,
                    role: RenderPassRole::BaseMaterial,
                    draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
                    object_index: Some(7),
                    material_index: Some(3),
                    pass_index: 0,
                    shader: Some("we/flat".to_owned()),
                    target: RenderTargetRole::ImageLocalMain,
                    target_name: None,
                    target_extent: None,
                    target_format: None,
                    bindings: vec![TextureBindingRole::SourceTexture],
                    effect_visibility: RenderPassEffectVisibility::NONE,
                    state: PassState::default(),
                },
                RenderPassNode {
                    id: 1,
                    role: RenderPassRole::EffectMaterial,
                    draw_primitive: RenderPassDrawPrimitive::FullscreenTriangle,
                    object_index: Some(7),
                    material_index: Some(4),
                    pass_index: 0,
                    shader: Some("package/effects/procedural".to_owned()),
                    target: RenderTargetRole::ImageLocalSub,
                    target_name: None,
                    target_extent: None,
                    target_format: None,
                    bindings: vec![TextureBindingRole::EffectTarget {
                        slot: 0,
                        name: target.to_owned(),
                    }],
                    effect_visibility: RenderPassEffectVisibility::passthrough(2, 1),
                    state: PassState::default(),
                },
                RenderPassNode {
                    id: 2,
                    role: RenderPassRole::SceneComposite,
                    draw_primitive: RenderPassDrawPrimitive::FullscreenTriangle,
                    object_index: Some(7),
                    material_index: Some(3),
                    pass_index: 2,
                    shader: Some("we/objectcomposite".to_owned()),
                    target: RenderTargetRole::SceneColor,
                    target_name: None,
                    target_extent: None,
                    target_format: None,
                    bindings: vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }],
                    effect_visibility: RenderPassEffectVisibility::NONE,
                    state: PassState {
                        pipeline_blend: crate::engine::render_graph::PipelineBlendMode::Translucent,
                        ..PassState::default()
                    },
                },
            ],
            ..RenderGraph::default()
        };

        let extent_domain = materialize_graph_target(&mut graph, 7, target);

        assert_eq!(extent_domain, RenderTargetExtentDomain::GraphSource);
        assert_eq!(graph.passes.len(), 2);
        assert_eq!(
            graph.passes[0].target,
            RenderTargetRole::FirstClassEffectTarget
        );
        assert_eq!(graph.passes[0].target_name.as_deref(), Some(target));
        let terminal = &graph.passes[1];
        assert_eq!(terminal.role, RenderPassRole::SceneComposite);
        assert_eq!(terminal.draw_primitive, RenderPassDrawPrimitive::ObjectMesh);
        assert_eq!(terminal.target, RenderTargetRole::SceneColor);
        assert_eq!(
            terminal.state.pipeline_blend,
            crate::engine::render_graph::PipelineBlendMode::Translucent
        );
        assert_eq!(terminal.state.color_write_mask, ColorWriteMask::Rgb);
        assert_eq!(
            terminal.bindings,
            [TextureBindingRole::EffectTarget {
                slot: 0,
                name: target.to_owned(),
            }]
        );
    }
}
