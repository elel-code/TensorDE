//! Typed cross-graph render textures produced by hidden WE dependency layers.

use std::collections::BTreeSet;

use crate::convert::we_ingest::ir::{
    WeIrImageTarget, WeIrImageTargetExtentDomain, WeIrImageTargetRole,
};
use crate::engine::render_graph::{
    PassState, RenderGraph, RenderPassDrawPrimitive, RenderPassEffectVisibility, RenderPassNode,
    RenderPassRole, RenderTargetExtentDomain, RenderTargetRole, TextureBindingRole,
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
    let producer = graph.passes.iter().rposition(|pass| {
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
                RenderTargetExtentDomain::OwnerAuthored
            }
            RenderTargetRole::SceneColor => RenderTargetExtentDomain::PhysicalSurface,
            _ => unreachable!("image-layer composite producer target is prefiltered"),
        };
        graph.passes[producer].target = RenderTargetRole::FirstClassEffectTarget;
        graph.passes[producer].target_name = Some(target_name.to_owned());
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
}
