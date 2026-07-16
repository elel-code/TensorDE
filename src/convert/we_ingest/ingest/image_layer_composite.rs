//! Typed cross-graph render textures produced by hidden WE dependency layers.

use std::collections::BTreeSet;

use crate::convert::we_ingest::ir::{WeIrImageTarget, WeIrImageTargetRole};
use crate::engine::render_graph::{RenderPassRole, RenderTargetRole, TextureBindingRole};

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
            let Some(graph_index) = self
                .objects
                .iter()
                .find(|object| object.we_id == we_id)
                .and_then(|object| object.render_graph)
                .map(|graph| graph as usize)
            else {
                continue;
            };
            let Some(graph) = self.render_graphs.get_mut(graph_index) else {
                continue;
            };
            let producer = graph.passes.iter().rposition(|pass| {
                pass.role != RenderPassRole::SceneComposite
                    && matches!(
                        pass.target,
                        RenderTargetRole::SceneColor
                            | RenderTargetRole::ImageLocalMain
                            | RenderTargetRole::ImageLocalSub
                    )
            });
            let Some(producer) = producer else {
                continue;
            };
            graph.passes[producer].target = RenderTargetRole::FirstClassEffectTarget;
            graph.passes[producer].target_name = Some(target_name.clone());
            if !self.image_targets.iter().any(|target| {
                target.name == target_name
                    && target.role == WeIrImageTargetRole::FirstClassEffectTarget
            }) {
                self.image_targets.push(WeIrImageTarget {
                    name: target_name,
                    format: "rgba_backbuffer".to_owned(),
                    role: WeIrImageTargetRole::FirstClassEffectTarget,
                    width_divisor_milli: 1_000,
                    height_divisor_milli: 1_000,
                });
            }
        }
    }
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
}
