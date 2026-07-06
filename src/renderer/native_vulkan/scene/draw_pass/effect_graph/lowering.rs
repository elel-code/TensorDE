use crate::core::SceneBlendMode;

use super::super::{
    NativeVulkanSceneEffectKind, NativeVulkanSceneFusedEffectKind,
    NativeVulkanSceneFusedEffectPass, NativeVulkanSceneSampledImageQuad,
    NativeVulkanSceneTextureSlot, NativeVulkanSceneWeImagePass, NativeVulkanSceneWeImagePassChain,
    NativeVulkanSceneWeImagePassEndpoint, NativeVulkanSceneWeImagePassLoweringStats,
    NativeVulkanSceneWeImagePassRole,
};
use super::{
    native_vulkan_scene_we_image_graph_endpoint_name,
    native_vulkan_scene_we_image_pass_chain_mesh_is_full_quad,
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

fn native_vulkan_scene_we_image_passes_waterwaves2_ineligible_reasons(
    quad: &NativeVulkanSceneSampledImageQuad,
    first: &NativeVulkanSceneWeImagePass,
    second: &NativeVulkanSceneWeImagePass,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    native_vulkan_scene_we_image_passes_waterwaves2_surface_ineligible_reasons(
        quad,
        first,
        second,
        &mut reasons,
    );
    if first.role != NativeVulkanSceneWeImagePassRole::EffectMaterial
        || second.role != NativeVulkanSceneWeImagePassRole::EffectMaterial
    {
        reasons.push("non-effect-material");
    }
    if first.effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
        || second.effect_kind != Some(NativeVulkanSceneEffectKind::WaterWaves)
    {
        reasons.push("non-waterwaves-kind");
    }
    if first.fused_effect_kind.is_some() || second.fused_effect_kind.is_some() {
        reasons.push("already-fused");
    }
    if first.final_scene_pass {
        reasons.push("first-final-scene-pass");
    }
    if !first.target.is_graph_target() {
        reasons.push("first-target-not-graph");
    }
    if second.input != first.target
        || second.input_name
            != native_vulkan_scene_we_image_graph_endpoint_name(first).map(str::to_owned)
    {
        reasons.push("second-input-not-first-target");
    }
    if first.input.is_graph_target()
        && first.input == second.target
        && first.input_name == second.target_name
    {
        reasons.push("same-input-output-target");
    }
    if first.target_name.is_some() || second.target_name.is_some() {
        reasons.push("explicit-target");
    }
    if first.source.is_some() || second.source.is_some() {
        reasons.push("explicit-source");
    }
    if !first.binds.is_empty() || !second.binds.is_empty() {
        reasons.push("binds");
    }
    if !first.fbos.is_empty() || !second.fbos.is_empty() {
        reasons.push("fbos");
    }
    if !native_vulkan_scene_we_image_passes_have_fusible_waterwaves_texture_slots(first, second) {
        reasons.push("texture-slots");
    }
    if first.effect_uv_transform != second.effect_uv_transform {
        reasons.push("effect-uv-transform");
    }
    if first.scene_blend_mode != SceneBlendMode::Normal {
        reasons.push("first-blend-not-normal");
    }
    if first.depth_test != second.depth_test
        || first.depth_write != second.depth_write
        || first.cull_mode != second.cull_mode
    {
        reasons.push("render-state-mismatch");
    }
    reasons
}

fn native_vulkan_scene_we_image_passes_waterwaves2_surface_ineligible_reasons(
    quad: &NativeVulkanSceneSampledImageQuad,
    first: &NativeVulkanSceneWeImagePass,
    second: &NativeVulkanSceneWeImagePass,
    reasons: &mut Vec<&'static str>,
) {
    if let Some(mesh) = quad.mesh.as_ref()
        && !native_vulkan_scene_we_image_pass_chain_mesh_is_full_quad(quad, mesh)
        && !native_vulkan_scene_we_image_passes_can_fuse_layer_uv_mesh_waterwaves2(first, second)
    {
        reasons.push("mesh-geometry");
    }
    if quad.effect_motion.is_active() {
        reasons.push("dynamic-effect-motion");
    }
    if quad.texture_region.is_some() {
        reasons.push("texture-region");
    }
    if quad.effect_uv_space.is_some() {
        reasons.push("effect-uv-space");
    }
    if quad.effect_target_pass.is_some() {
        reasons.push("first-class-effect-target");
    }
    if quad.material_pass.alpha_texture_slot.is_some() {
        reasons.push("material-alpha-texture");
    }
    if quad.composite_key.is_some() {
        reasons.push("composite-key");
    }
    if !quad.width.is_finite()
        || !quad.height.is_finite()
        || quad.width <= 0.0
        || quad.height <= 0.0
    {
        reasons.push("invalid-extent");
    }
}

fn native_vulkan_scene_we_image_passes_can_fuse_layer_uv_mesh_waterwaves2(
    first: &NativeVulkanSceneWeImagePass,
    second: &NativeVulkanSceneWeImagePass,
) -> bool {
    first.input.is_graph_target()
        && first.target.is_graph_target()
        && second.target == NativeVulkanSceneWeImagePassEndpoint::Scene
        && second.final_scene_pass
}

fn native_vulkan_scene_we_image_passes_have_fusible_waterwaves_texture_slots(
    first: &NativeVulkanSceneWeImagePass,
    second: &NativeVulkanSceneWeImagePass,
) -> bool {
    first
        .texture_slots
        .iter()
        .chain(second.texture_slots.iter())
        .all(|slot| matches!(slot.slot, 1 | 2))
}

fn native_vulkan_scene_we_image_pass_fused_waterwaves2(
    first: &NativeVulkanSceneWeImagePass,
    second: &NativeVulkanSceneWeImagePass,
) -> NativeVulkanSceneWeImagePass {
    let first_texture_slots = native_vulkan_scene_fused_waterwaves_texture_slots(first, 0);
    let second_texture_slots = native_vulkan_scene_fused_waterwaves_texture_slots(second, 2);
    let texture_slots = first_texture_slots
        .iter()
        .chain(second_texture_slots.iter())
        .cloned()
        .collect::<Vec<_>>();
    NativeVulkanSceneWeImagePass {
        pass_index: first.pass_index,
        role: NativeVulkanSceneWeImagePassRole::EffectMaterial,
        effect_kind: Some(NativeVulkanSceneEffectKind::WaterWaves),
        fused_effect_kind: Some(NativeVulkanSceneFusedEffectKind::WaterWaves2),
        fused_effect_passes: vec![
            native_vulkan_scene_fused_effect_pass_from_we_pass(first, first_texture_slots),
            native_vulkan_scene_fused_effect_pass_from_we_pass(second, second_texture_slots),
        ],
        effect_file: first.effect_file.clone(),
        command: None,
        source: None,
        target_name: second.target_name.clone(),
        binds: Default::default(),
        fbos: Default::default(),
        shader: first.shader.clone(),
        blending: second.blending.clone(),
        scene_blend_mode: second.scene_blend_mode,
        render_state: second.render_state.clone(),
        input: first.input,
        input_name: first.input_name.clone(),
        target: second.target,
        final_scene_pass: second.final_scene_pass,
        texture_slot_count: texture_slots.len(),
        texture_slots,
        effect_uv_transform: first.effect_uv_transform,
        parameter_keys: first.parameter_keys.clone(),
        constant_shader_values: first.constant_shader_values.clone(),
        combo_keys: first.combo_keys.clone(),
        combo_values: first.combo_values.clone(),
        depth_test: second.depth_test,
        depth_write: second.depth_write,
        cull_mode: second.cull_mode.clone(),
    }
}

fn native_vulkan_scene_fused_waterwaves_texture_slots(
    pass: &NativeVulkanSceneWeImagePass,
    slot_offset: u32,
) -> Vec<NativeVulkanSceneTextureSlot> {
    pass.texture_slots
        .iter()
        .filter_map(|slot| {
            if !matches!(slot.slot, 1 | 2) {
                return None;
            }
            let mut remapped = slot.clone();
            remapped.slot = remapped.slot.saturating_add(slot_offset);
            Some(remapped)
        })
        .collect()
}

fn native_vulkan_scene_fused_effect_pass_from_we_pass(
    pass: &NativeVulkanSceneWeImagePass,
    texture_slots: Vec<NativeVulkanSceneTextureSlot>,
) -> NativeVulkanSceneFusedEffectPass {
    NativeVulkanSceneFusedEffectPass {
        pass_index: pass.pass_index,
        effect_kind: pass
            .effect_kind
            .expect("fused WE effect pass requires an effect kind"),
        effect_file: pass.effect_file.clone(),
        texture_slots,
        effect_uv_transform: pass.effect_uv_transform,
        constant_shader_values: pass.constant_shader_values.clone(),
        combo_keys: pass.combo_keys.clone(),
        combo_values: pass.combo_values.clone(),
    }
}
