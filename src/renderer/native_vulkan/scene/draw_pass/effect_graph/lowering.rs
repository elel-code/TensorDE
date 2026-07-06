use super::super::{
    NativeVulkanSceneEffectKind, NativeVulkanSceneSampledImageQuad, NativeVulkanSceneWeImagePass,
    NativeVulkanSceneWeImagePassChain, NativeVulkanSceneWeImagePassEndpoint,
    NativeVulkanSceneWeImagePassLoweringStats,
};
use super::{
    native_vulkan_scene_we_image_graph_endpoint_name,
    native_vulkan_scene_we_image_pass_chain_mesh_is_full_quad,
};

mod residency;
mod waterwaves;

use self::waterwaves::{
    native_vulkan_scene_we_image_pass_fused_waterwaves2,
    native_vulkan_scene_we_image_passes_waterwaves2_ineligible_reasons,
    native_vulkan_scene_we_image_passes_waterwaves3_ineligible_reasons,
};

pub(super) fn native_vulkan_scene_we_image_pass_chain_lower(
    quad: &NativeVulkanSceneSampledImageQuad,
    passes: Vec<NativeVulkanSceneWeImagePass>,
) -> (
    Vec<NativeVulkanSceneWeImagePass>,
    NativeVulkanSceneWeImagePassLoweringStats,
) {
    NativeVulkanSceneWeImagePassLowering::new(quad, passes).run()
}

struct NativeVulkanSceneWeImagePassLowering<'a> {
    quad: &'a NativeVulkanSceneSampledImageQuad,
    passes: Vec<NativeVulkanSceneWeImagePass>,
    lowered: Vec<NativeVulkanSceneWeImagePass>,
    stats: NativeVulkanSceneWeImagePassLoweringStats,
    index: usize,
}

impl<'a> NativeVulkanSceneWeImagePassLowering<'a> {
    fn new(
        quad: &'a NativeVulkanSceneSampledImageQuad,
        passes: Vec<NativeVulkanSceneWeImagePass>,
    ) -> Self {
        Self {
            quad,
            lowered: Vec::with_capacity(passes.len()),
            passes,
            stats: NativeVulkanSceneWeImagePassLoweringStats::default(),
            index: 0,
        }
    }

    fn run(
        mut self,
    ) -> (
        Vec<NativeVulkanSceneWeImagePass>,
        NativeVulkanSceneWeImagePassLoweringStats,
    ) {
        while self.index < self.passes.len() {
            self.lower_next_window();
        }
        (self.lowered, self.stats)
    }

    fn lower_next_window(&mut self) {
        if let Some(reasons) = self.waterwaves3_candidate_reasons() {
            self.stats.waterwaves3_candidate_triple_count = self
                .stats
                .waterwaves3_candidate_triple_count
                .saturating_add(1);
            self.stats.waterwaves3_blocked_triple_count = self
                .stats
                .waterwaves3_blocked_triple_count
                .saturating_add(1);
            for reason in reasons {
                *self
                    .stats
                    .waterwaves3_blocked_reason_counts
                    .entry(reason)
                    .or_default() += 1;
            }
        }

        let waterwaves2_candidate_reasons = self.waterwaves2_candidate_reasons();
        if waterwaves2_candidate_reasons.is_some() {
            self.stats.waterwaves2_candidate_pair_count = self
                .stats
                .waterwaves2_candidate_pair_count
                .saturating_add(1);
        }
        let Some(window) = self.waterwaves2_window() else {
            if let Some(reasons) = waterwaves2_candidate_reasons {
                self.stats.waterwaves2_blocked_pair_count =
                    self.stats.waterwaves2_blocked_pair_count.saturating_add(1);
                for reason in reasons {
                    *self
                        .stats
                        .waterwaves2_blocked_reason_counts
                        .entry(reason)
                        .or_default() += 1;
                }
            }
            self.lowered.push(self.passes[self.index].clone());
            self.index += 1;
            return;
        };
        match window {
            NativeVulkanSceneWeImagePassLoweringWindow::DirectWaterWaves2 => {
                self.stats.waterwaves2_direct_pair_count =
                    self.stats.waterwaves2_direct_pair_count.saturating_add(1);
                self.lowered
                    .push(native_vulkan_scene_we_image_pass_fused_waterwaves2(
                        &self.passes[self.index],
                        &self.passes[self.index + 1],
                    ));
                self.index += 2;
            }
            NativeVulkanSceneWeImagePassLoweringWindow::RedirectedWaterWaves2 {
                original_output,
                redirected_output,
            } => {
                self.stats.waterwaves2_redirected_pair_count = self
                    .stats
                    .waterwaves2_redirected_pair_count
                    .saturating_add(1);
                let mut fused_pass = native_vulkan_scene_we_image_pass_fused_waterwaves2(
                    &self.passes[self.index],
                    &self.passes[self.index + 1],
                );
                fused_pass.target = redirected_output.endpoint;
                fused_pass.target_name = redirected_output.name.clone();
                fused_pass.final_scene_pass = false;
                native_vulkan_scene_we_image_pass_chain_swap_following_endpoints(
                    &mut self.passes[self.index + 2..],
                    original_output,
                    redirected_output,
                );
                self.lowered.push(fused_pass);
                self.index += 2;
            }
        }
    }

    fn waterwaves2_window(&self) -> Option<NativeVulkanSceneWeImagePassLoweringWindow> {
        if self.index + 1 >= self.passes.len() {
            return None;
        }
        let first = &self.passes[self.index];
        let second = &self.passes[self.index + 1];
        if native_vulkan_scene_we_image_passes_can_fuse_waterwaves2(self.quad, first, second) {
            return Some(NativeVulkanSceneWeImagePassLoweringWindow::DirectWaterWaves2);
        }
        if !native_vulkan_scene_we_image_passes_can_fuse_redirected_waterwaves2(
            self.quad, first, second,
        ) {
            return None;
        }
        Some(
            NativeVulkanSceneWeImagePassLoweringWindow::RedirectedWaterWaves2 {
                original_output: NativeVulkanSceneWeImagePassEndpointRef {
                    endpoint: second.target,
                    name: native_vulkan_scene_we_image_graph_endpoint_name(second)
                        .map(str::to_owned),
                },
                redirected_output: NativeVulkanSceneWeImagePassEndpointRef {
                    endpoint: first.target,
                    name: native_vulkan_scene_we_image_graph_endpoint_name(first)
                        .map(str::to_owned),
                },
            },
        )
    }

    fn waterwaves3_candidate_reasons(&self) -> Option<Vec<&'static str>> {
        if self.index + 2 >= self.passes.len() {
            return None;
        }
        let first = &self.passes[self.index];
        let second = &self.passes[self.index + 1];
        let third = &self.passes[self.index + 2];
        if first.effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
            || second.effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
            || third.effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
        {
            return None;
        }
        Some(
            native_vulkan_scene_we_image_passes_waterwaves3_ineligible_reasons(
                self.quad, first, second, third,
            ),
        )
    }

    fn waterwaves2_candidate_reasons(&self) -> Option<Vec<&'static str>> {
        if self.index + 1 >= self.passes.len() {
            return None;
        }
        let first = &self.passes[self.index];
        let second = &self.passes[self.index + 1];
        if first.effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
            || second.effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
        {
            return None;
        }
        Some(
            native_vulkan_scene_we_image_passes_waterwaves2_ineligible_reasons(
                self.quad, first, second,
            ),
        )
    }
}

enum NativeVulkanSceneWeImagePassLoweringWindow {
    DirectWaterWaves2,
    RedirectedWaterWaves2 {
        original_output: NativeVulkanSceneWeImagePassEndpointRef,
        redirected_output: NativeVulkanSceneWeImagePassEndpointRef,
    },
}

#[derive(Clone)]
struct NativeVulkanSceneWeImagePassEndpointRef {
    endpoint: NativeVulkanSceneWeImagePassEndpoint,
    name: Option<String>,
}

fn native_vulkan_scene_we_image_passes_can_fuse_waterwaves2(
    quad: &NativeVulkanSceneSampledImageQuad,
    first: &NativeVulkanSceneWeImagePass,
    second: &NativeVulkanSceneWeImagePass,
) -> bool {
    native_vulkan_scene_we_image_passes_waterwaves2_ineligible_reasons(quad, first, second)
        .is_empty()
}

fn native_vulkan_scene_we_image_passes_can_fuse_redirected_waterwaves2(
    quad: &NativeVulkanSceneSampledImageQuad,
    first: &NativeVulkanSceneWeImagePass,
    second: &NativeVulkanSceneWeImagePass,
) -> bool {
    if first.input != second.target
        || first.input_name
            != native_vulkan_scene_we_image_graph_endpoint_name(second).map(str::to_owned)
        || !first.input.is_graph_target()
        || !first.target.is_graph_target()
        || !second.target.is_graph_target()
    {
        return false;
    }
    let layer_uv_mesh_pair = quad
        .mesh
        .as_ref()
        .is_some_and(|mesh| !native_vulkan_scene_we_image_pass_chain_mesh_is_full_quad(quad, mesh));
    native_vulkan_scene_we_image_passes_waterwaves2_ineligible_reasons(quad, first, second)
        .into_iter()
        .all(|reason| {
            reason == "same-input-output-target"
                || (layer_uv_mesh_pair && reason == "mesh-geometry")
        })
}

fn native_vulkan_scene_we_image_pass_chain_swap_following_endpoints(
    passes: &mut [NativeVulkanSceneWeImagePass],
    left: NativeVulkanSceneWeImagePassEndpointRef,
    right: NativeVulkanSceneWeImagePassEndpointRef,
) {
    for pass in passes {
        native_vulkan_scene_we_image_pass_swap_endpoint_reference(
            &mut pass.input,
            &mut pass.input_name,
            &left,
            &right,
        );
        native_vulkan_scene_we_image_pass_swap_endpoint_reference(
            &mut pass.target,
            &mut pass.target_name,
            &left,
            &right,
        );
    }
}

fn native_vulkan_scene_we_image_pass_swap_endpoint_reference(
    endpoint: &mut NativeVulkanSceneWeImagePassEndpoint,
    name: &mut Option<String>,
    left: &NativeVulkanSceneWeImagePassEndpointRef,
    right: &NativeVulkanSceneWeImagePassEndpointRef,
) {
    if *endpoint == left.endpoint && name.as_deref() == left.name.as_deref() {
        *endpoint = right.endpoint;
        *name = right.name.clone();
    } else if *endpoint == right.endpoint && name.as_deref() == right.name.as_deref() {
        *endpoint = left.endpoint;
        *name = left.name.clone();
    }
}

pub(super) fn native_vulkan_scene_we_image_pass_chain_waterwaves2_ineligible_reasons(
    quad: &NativeVulkanSceneSampledImageQuad,
    chain: &NativeVulkanSceneWeImagePassChain,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    for pair in chain.passes.windows(2) {
        if pair[0].effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
            || pair[1].effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
        {
            continue;
        }
        reasons.extend(
            native_vulkan_scene_we_image_passes_waterwaves2_ineligible_reasons(
                quad, &pair[0], &pair[1],
            ),
        );
    }
    reasons
}
