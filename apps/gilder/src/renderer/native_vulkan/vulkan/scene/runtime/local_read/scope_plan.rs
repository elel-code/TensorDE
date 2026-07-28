//! Typed planning for a retained dynamic-rendering local-read scope.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/exe/composelayer-and-effecttarget.md`
//! - Vulkan 1.4 `VK_KHR_dynamic_rendering_local_read`
//!
//! A typed input attachment is executable only when its producer is the
//! immediately preceding authored draw pass in the same graph and both
//! effect targets can remain attached for one rendering scope.  Unsupported
//! combinations are rejected here; callers must not fall back to sampling.

use vulkanalia::vk;

use crate::engine::scene::{
    SceneRenderPassKind, SceneRenderTargetKind, SceneRenderingDeviceGraphPlan,
    SceneRenderingDeviceImageAccess,
    SceneRenderingDevicePassNode, SceneStorage, SceneStringId,
};
use crate::renderer::native_vulkan::scene::native_vulkan_scene_shader_for_key;
use crate::renderer::native_vulkan::scene::BuiltinSceneLocalReadShader;

use super::super::descriptor_layout::{
    ScenePipelineShaderDescriptorAccess, scene_pipeline_shader_descriptor_access,
};
use super::super::effect_target::SceneEffectTargetImagePlan;
use super::{
    SceneLocalReadAttachmentMapping, SceneLocalReadDeviceLimits,
    SceneLocalReadPipelineMetadata, validate_scene_local_read_shader_variant,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum SceneLocalReadScopePassRole {
    Producer,
    Consumer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneLocalReadScopeTarget {
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
    initial_physical_slot: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneLocalReadScopePlan {
    pub(super) graph_index: u32,
    pub(super) producer_pass_node_index: u32,
    pub(super) producer_pass_record_index: u32,
    pub(super) producer_draw_start: u32,
    pub(super) producer_draw_count: u32,
    pub(super) consumer_pass_node_index: u32,
    pub(super) consumer_pass_record_index: u32,
    pub(super) consumer_draw_start: u32,
    pub(super) consumer_draw_count: u32,
    pub(super) source: SceneLocalReadScopeTarget,
    pub(super) destination: SceneLocalReadScopeTarget,
    pub(super) extent: vk::Extent2D,
    pub(super) color_attachment_formats: [vk::Format; 2],
    pub(super) input_slot: u32,
    pub(super) input_attachment_index: u32,
}

impl SceneLocalReadScopePlan {
    pub(in crate::renderer::native_vulkan) fn pass_role(
        &self,
        pass_node_index: u32,
    ) -> Option<SceneLocalReadScopePassRole> {
        if pass_node_index == self.producer_pass_node_index {
            Some(SceneLocalReadScopePassRole::Producer)
        } else if pass_node_index == self.consumer_pass_node_index {
            Some(SceneLocalReadScopePassRole::Consumer)
        } else {
            None
        }
    }

    pub(in crate::renderer::native_vulkan) fn graph_index(&self) -> u32 {
        self.graph_index
    }

    pub(in crate::renderer::native_vulkan) fn producer_pass_record_index(&self) -> u32 {
        self.producer_pass_record_index
    }

    pub(in crate::renderer::native_vulkan) fn producer_pass_node_index(&self) -> u32 {
        self.producer_pass_node_index
    }

    pub(in crate::renderer::native_vulkan) fn producer_draw_range(&self) -> (u32, u32) {
        (self.producer_draw_start, self.producer_draw_count)
    }

    pub(in crate::renderer::native_vulkan) fn consumer_pass_record_index(&self) -> u32 {
        self.consumer_pass_record_index
    }

    pub(in crate::renderer::native_vulkan) fn consumer_pass_node_index(&self) -> u32 {
        self.consumer_pass_node_index
    }

    pub(in crate::renderer::native_vulkan) fn consumer_draw_range(&self) -> (u32, u32) {
        (self.consumer_draw_start, self.consumer_draw_count)
    }

    pub(in crate::renderer::native_vulkan) fn source(&self) -> SceneLocalReadScopeTarget {
        self.source
    }

    pub(in crate::renderer::native_vulkan) fn destination(&self) -> SceneLocalReadScopeTarget {
        self.destination
    }

    pub(in crate::renderer::native_vulkan) fn extent(&self) -> vk::Extent2D {
        self.extent
    }

    pub(in crate::renderer::native_vulkan) fn color_attachment_formats(
        &self,
    ) -> [vk::Format; 2] {
        self.color_attachment_formats
    }

    pub(in crate::renderer::native_vulkan) fn input_slot(&self) -> u32 {
        self.input_slot
    }

    pub(in crate::renderer::native_vulkan) fn pipeline_metadata<'a>(
        &self,
        role: SceneLocalReadScopePassRole,
        descriptor_access: &ScenePipelineShaderDescriptorAccess,
        shader: Option<&'a BuiltinSceneLocalReadShader>,
        limits: SceneLocalReadDeviceLimits,
    ) -> Result<SceneLocalReadPipelineMetadata<'a>, String> {
        let formats = self.color_attachment_formats();
        let locations = self.color_attachment_locations(role);
        match role {
            SceneLocalReadScopePassRole::Producer => {
                SceneLocalReadPipelineMetadata::output_only(
                    descriptor_access,
                    &formats,
                    &locations,
                    limits,
                )
            }
            SceneLocalReadScopePassRole::Consumer => {
                let input_indices = self.color_attachment_input_indices(role);
                SceneLocalReadPipelineMetadata::new(
                    descriptor_access,
                    shader,
                    &formats,
                    &locations,
                    &input_indices,
                    limits,
                )
            }
        }
    }

    pub(in crate::renderer::native_vulkan) fn attachment_mapping(
        &self,
        role: SceneLocalReadScopePassRole,
        limits: SceneLocalReadDeviceLimits,
    ) -> Result<SceneLocalReadAttachmentMapping, String> {
        let locations = self.color_attachment_locations(role);
        let input_indices = self.color_attachment_input_indices(role);
        SceneLocalReadAttachmentMapping::new_for_scope(
            &locations,
            &input_indices,
            limits.max_color_attachments,
            limits.max_per_stage_descriptor_input_attachments,
            role == SceneLocalReadScopePassRole::Consumer,
        )
    }

    fn color_attachment_locations(&self, role: SceneLocalReadScopePassRole) -> [u32; 2] {
        match role {
            SceneLocalReadScopePassRole::Producer => [0, vk::ATTACHMENT_UNUSED],
            SceneLocalReadScopePassRole::Consumer => [vk::ATTACHMENT_UNUSED, 0],
        }
    }

    fn color_attachment_input_indices(&self, role: SceneLocalReadScopePassRole) -> [u32; 2] {
        match role {
            SceneLocalReadScopePassRole::Producer => {
                [vk::ATTACHMENT_UNUSED, vk::ATTACHMENT_UNUSED]
            }
            SceneLocalReadScopePassRole::Consumer => {
                [self.input_attachment_index, vk::ATTACHMENT_UNUSED]
            }
        }
    }
}

impl SceneLocalReadScopeTarget {
    pub(in crate::renderer::native_vulkan) fn graph_index(self) -> u32 {
        self.graph_index
    }

    pub(in crate::renderer::native_vulkan) fn target(self) -> SceneRenderTargetKind {
        self.target
    }

    pub(in crate::renderer::native_vulkan) fn target_name(self) -> SceneStringId {
        self.target_name
    }

    pub(in crate::renderer::native_vulkan) fn initial_physical_slot(self) -> u32 {
        self.initial_physical_slot
    }
}

pub(in crate::renderer::native_vulkan) fn scene_local_read_scope_plans(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    effect_target_plans: &[SceneEffectTargetImagePlan],
) -> Result<Vec<SceneLocalReadScopePlan>, String> {
    let mut plans = Vec::new();
    for (consumer_index, consumer) in graph.pass_nodes.iter().enumerate() {
        let input_bindings = graph
            .sampled_bindings
            .iter()
            .filter(|binding| {
                binding.pass_node_index == consumer_index as u32
                    && binding.access == SceneRenderingDeviceImageAccess::InputAttachment
            })
            .collect::<Vec<_>>();
        if input_bindings.is_empty() {
            continue;
        }
        if input_bindings.len() != 1 {
            return Err(format!(
                "scene graph {} pass {} declares {} input attachments; paired local-read scopes currently require exactly one",
                consumer.graph_index,
                consumer.pass_id,
                input_bindings.len()
            ));
        }
        let producer_index = consumer_index.checked_sub(1).ok_or_else(|| {
            format!(
                "scene graph {} pass {} declares an input attachment without a preceding producer pass",
                consumer.graph_index, consumer.pass_id
            )
        })?;
        let producer = &graph.pass_nodes[producer_index];
        validate_scope_pass_pair(graph, producer_index, producer, consumer_index, consumer)?;

        let binding = input_bindings[0];
        if (binding.mesh_draw_start, binding.mesh_draw_count)
            != (consumer.mesh_draw_start, consumer.mesh_draw_count)
        {
            return Err(format!(
                "scene graph {} pass {} input slot {} draw range {:?} does not match consumer draw range {:?}",
                consumer.graph_index,
                consumer.pass_id,
                binding.slot,
                (binding.mesh_draw_start, binding.mesh_draw_count),
                (consumer.mesh_draw_start, consumer.mesh_draw_count)
            ));
        }
        let source_key = binding.logical_target().ok_or_else(|| {
            format!(
                "scene graph {} pass {} input slot {} has no effect-target source",
                consumer.graph_index, consumer.pass_id, binding.slot
            )
        })?;
        let producer_key = (
            producer.graph_index,
            producer.target,
            producer.target_name,
        );
        if source_key != producer_key {
            return Err(format!(
                "scene graph {} pass {} input slot {} reads {:?}:{:?}, but the immediately preceding pass {} writes {:?}:{:?}",
                consumer.graph_index,
                consumer.pass_id,
                binding.slot,
                binding.target,
                binding.target_name,
                producer.pass_id,
                producer.target,
                producer.target_name
            ));
        }
        if (consumer.target, consumer.target_name) == (producer.target, producer.target_name) {
            return Err(format!(
                "scene graph {} pass {} local-read source and destination are the same logical target {:?}:{:?}",
                consumer.graph_index, consumer.pass_id, consumer.target, consumer.target_name
            ));
        }

        let source = scope_target(graph, producer)?;
        let destination = scope_target(graph, consumer)?;
        validate_scope_attachment_accesses(
            graph,
            producer_index,
            consumer_index,
            source,
            destination,
        )?;
        if source.initial_physical_slot == destination.initial_physical_slot {
            return Err(format!(
                "scene graph {} local-read passes {} and {} alias physical target {} while producer contents are still live",
                consumer.graph_index,
                producer.pass_id,
                consumer.pass_id,
                source.initial_physical_slot
            ));
        }
        let source_plan = effect_target_plan(effect_target_plans, source, "source")?;
        let destination_plan =
            effect_target_plan(effect_target_plans, destination, "destination")?;
        validate_scope_images(source_plan, destination_plan, producer, consumer)?;

        let consumer_record = storage
            .document()
            .render_passes
            .get(consumer.pass_record_index as usize)
            .ok_or_else(|| {
                format!(
                    "scene graph {} pass {} references missing pass record {}",
                    consumer.graph_index, consumer.pass_id, consumer.pass_record_index
                )
            })?;
        let descriptor_access =
            scene_pipeline_shader_descriptor_access(storage, consumer_record.shader_key)?;
        let shader_key = storage.string(consumer_record.shader_key).ok_or_else(|| {
            format!(
                "scene graph {} pass {} has no shader key",
                consumer.graph_index, consumer.pass_id
            )
        })?;
        let shader = native_vulkan_scene_shader_for_key(shader_key).ok_or_else(|| {
            format!("scene local-read consumer shader {shader_key:?} is not built into the catalog")
        })?;
        let local_read_shader = validate_scene_local_read_shader_variant(
            &descriptor_access,
            shader.local_read_shader.as_ref(),
        )?;
        if local_read_shader.input_attachments.len() != 1
            || local_read_shader.color_output_locations != [0]
        {
            return Err(format!(
                "scene local-read consumer shader {shader_key:?} must expose one input attachment and one color output at location 0"
            ));
        }
        let shader_input = local_read_shader.input_attachments[0];
        if shader_input.slot != binding.slot {
            return Err(format!(
                "scene graph {} pass {} input slot {} does not match local-read shader slot {}",
                consumer.graph_index, consumer.pass_id, binding.slot, shader_input.slot
            ));
        }

        plans.push(SceneLocalReadScopePlan {
            graph_index: consumer.graph_index,
            producer_pass_node_index: producer_index as u32,
            producer_pass_record_index: producer.pass_record_index,
            producer_draw_start: producer.mesh_draw_start,
            producer_draw_count: producer.mesh_draw_count,
            consumer_pass_node_index: consumer_index as u32,
            consumer_pass_record_index: consumer.pass_record_index,
            consumer_draw_start: consumer.mesh_draw_start,
            consumer_draw_count: consumer.mesh_draw_count,
            source,
            destination,
            extent: vk::Extent2D {
                width: source_plan.width,
                height: source_plan.height,
            },
            color_attachment_formats: [source_plan.format, destination_plan.format],
            input_slot: binding.slot,
            input_attachment_index: shader_input.input_attachment_index,
        });
    }
    Ok(plans)
}

fn validate_scope_pass_pair(
    graph: &SceneRenderingDeviceGraphPlan,
    producer_index: usize,
    producer: &SceneRenderingDevicePassNode,
    consumer_index: usize,
    consumer: &SceneRenderingDevicePassNode,
) -> Result<(), String> {
    if producer.graph_index != consumer.graph_index {
        return Err(format!(
            "scene graph {} pass {} input attachment crosses graph boundary from graph {}",
            consumer.graph_index, consumer.pass_id, producer.graph_index
        ));
    }
    if producer.mesh_draw_count == 0 || consumer.mesh_draw_count == 0 {
        return Err(format!(
            "scene graph {} local-read pass pair {} -> {} must contain authored draws",
            consumer.graph_index, producer.pass_id, consumer.pass_id
        ));
    }
    if matches!(
        producer.role,
        SceneRenderPassKind::CopyTarget | SceneRenderPassKind::SwapTargetReferences
    ) || matches!(
        consumer.role,
        SceneRenderPassKind::CopyTarget | SceneRenderPassKind::SwapTargetReferences
    ) {
        return Err(format!(
            "scene graph {} local-read pass pair {} -> {} cannot cross copy or swap-reference passes",
            consumer.graph_index, producer.pass_id, consumer.pass_id
        ));
    }
    if graph.sampled_bindings.iter().any(|binding| {
        binding.pass_node_index == producer_index as u32
            && binding.access == SceneRenderingDeviceImageAccess::InputAttachment
    }) {
        return Err(format!(
            "scene graph {} pass {} is already a local-read consumer; chained scopes are not yet proven",
            producer.graph_index, producer.pass_id
        ));
    }
    if !effect_target_is_scope_attachment(producer.target)
        || !effect_target_is_scope_attachment(consumer.target)
    {
        return Err(format!(
            "scene graph {} local-read pass pair {} -> {} must use retained effect targets, got {:?} -> {:?}",
            consumer.graph_index,
            producer.pass_id,
            consumer.pass_id,
            producer.target,
            consumer.target
        ));
    }
    debug_assert_eq!(consumer_index, producer_index + 1);
    Ok(())
}

fn scope_target(
    graph: &SceneRenderingDeviceGraphPlan,
    pass: &SceneRenderingDevicePassNode,
) -> Result<SceneLocalReadScopeTarget, String> {
    let allocation = graph
        .target_allocations
        .iter()
        .find(|allocation| {
            allocation.graph_index == pass.graph_index
                && allocation.target == pass.target
                && allocation.target_name == pass.target_name
        })
        .ok_or_else(|| {
            format!(
                "scene graph {} pass {} local-read target {:?}:{:?} has no allocation",
                pass.graph_index, pass.pass_id, pass.target, pass.target_name
            )
        })?;
    Ok(SceneLocalReadScopeTarget {
        graph_index: pass.graph_index,
        target: pass.target,
        target_name: pass.target_name,
        initial_physical_slot: allocation.physical_slot,
    })
}

fn effect_target_plan<'plans>(
    plans: &'plans [SceneEffectTargetImagePlan],
    target: SceneLocalReadScopeTarget,
    label: &str,
) -> Result<&'plans SceneEffectTargetImagePlan, String> {
    plans
        .iter()
        .find(|plan| plan.physical_slot == target.initial_physical_slot)
        .ok_or_else(|| {
            format!(
                "scene local-read {label} physical slot {} has no retained image plan",
                target.initial_physical_slot
            )
        })
}

fn validate_scope_images(
    source: &SceneEffectTargetImagePlan,
    destination: &SceneEffectTargetImagePlan,
    producer: &SceneRenderingDevicePassNode,
    consumer: &SceneRenderingDevicePassNode,
) -> Result<(), String> {
    if source.batch_field_count != 1 || destination.batch_field_count != 1 {
        return Err(format!(
            "scene graph {} local-read pass pair {} -> {} cannot use effect-batch atlas targets",
            consumer.graph_index, producer.pass_id, consumer.pass_id
        ));
    }
    if !source.input_attachment_required {
        return Err(format!(
            "scene graph {} pass {} local-read source physical slot {} lacks input-attachment image usage",
            consumer.graph_index, consumer.pass_id, source.physical_slot
        ));
    }
    if !destination.input_attachment_required {
        return Err(format!(
            "scene graph {} pass {} local-read destination physical slot {} lacks input-attachment image usage",
            consumer.graph_index, consumer.pass_id, destination.physical_slot
        ));
    }
    if source.width != destination.width || source.height != destination.height {
        return Err(format!(
            "scene graph {} local-read pass pair {} -> {} has mismatched extents {}x{} and {}x{}",
            consumer.graph_index,
            producer.pass_id,
            consumer.pass_id,
            source.width,
            source.height,
            destination.width,
            destination.height
        ));
    }
    if source.format == vk::Format::UNDEFINED || destination.format == vk::Format::UNDEFINED {
        return Err(format!(
            "scene graph {} local-read pass pair {} -> {} has an undefined attachment format",
            consumer.graph_index, producer.pass_id, consumer.pass_id
        ));
    }
    Ok(())
}

fn validate_scope_attachment_accesses(
    graph: &SceneRenderingDeviceGraphPlan,
    producer_index: usize,
    consumer_index: usize,
    source: SceneLocalReadScopeTarget,
    destination: SceneLocalReadScopeTarget,
) -> Result<(), String> {
    for binding in graph.sampled_bindings.iter().filter(|binding| {
        (binding.pass_node_index == producer_index as u32
            || binding.pass_node_index == consumer_index as u32)
            && binding.access == SceneRenderingDeviceImageAccess::SampledImage
    }) {
        let Some(key) = binding.logical_target() else {
            continue;
        };
        let source_key = (source.graph_index, source.target, source.target_name);
        let destination_key = (
            destination.graph_index,
            destination.target,
            destination.target_name,
        );
        if key == source_key || key == destination_key {
            return Err(format!(
                "scene local-read scope pass {} slot {} samples an attached target {:?}:{:?}; attached targets cannot retain sampled-image layout",
                binding.pass_node_index, binding.slot, binding.target, binding.target_name
            ));
        }
    }
    Ok(())
}

fn effect_target_is_scope_attachment(target: SceneRenderTargetKind) -> bool {
    matches!(
        target,
        SceneRenderTargetKind::ImageLocalMain
            | SceneRenderTargetKind::ImageLocalSub
            | SceneRenderTargetKind::NamedFbo
            | SceneRenderTargetKind::FirstClassEffectTarget
            | SceneRenderTargetKind::Temporary
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        SceneBinaryDocument, SceneColorWriteMask, SceneCompositeBlend, SceneCullMode,
        SceneDepthTest, SceneMaterialHandle, SceneObjectHandle, ScenePipelineBlend,
        SceneRenderEffectVisibilityPolicy, SceneRenderPassKind, SceneRenderPassRecord,
        SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceMeshDraw,
        SceneRenderingDeviceSampledBinding, SceneRenderingDeviceTargetAllocation,
        SceneShaderContractRecord, SceneVec3, INVALID_MATERIAL_ID, INVALID_OBJECT_ID,
    };

    #[test]
    fn planner_keeps_adjacent_exact_pixel_passes_in_one_typed_scope() {
        let storage = local_read_storage("we/passthrough", 1);
        let graph = local_read_graph();
        let plans = effect_target_plans(true, [64, 64], [64, 64]);

        let scopes = scene_local_read_scope_plans(&storage, &graph, &plans)
            .expect("typed local-read scope");

        assert_eq!(scopes.len(), 1);
        let scope = &scopes[0];
        assert_eq!(scope.graph_index, 0);
        assert_eq!(scope.pass_role(0), Some(SceneLocalReadScopePassRole::Producer));
        assert_eq!(scope.pass_role(1), Some(SceneLocalReadScopePassRole::Consumer));
        assert_eq!(scope.extent.width, 64);
        assert_eq!(scope.color_attachment_formats, [vk::Format::R8G8B8A8_UNORM; 2]);
        assert_eq!(scope.input_slot, 0);
        assert_eq!(scope.input_attachment_index, 0);

        let producer_access =
            scene_pipeline_shader_descriptor_access(&storage, SceneStringId(0))
                .expect("producer descriptor access");
        let producer_metadata = scope
            .pipeline_metadata(
                SceneLocalReadScopePassRole::Producer,
                &producer_access,
                None,
                SceneLocalReadDeviceLimits::new(8, 8),
            )
            .expect("producer pipeline metadata");
        assert_eq!(producer_metadata.local_read_fragment_spirv(), None);

        let consumer_access =
            scene_pipeline_shader_descriptor_access(&storage, SceneStringId(1))
                .expect("consumer descriptor access");
        let consumer_shader = native_vulkan_scene_shader_for_key("we/passthrough")
            .expect("passthrough")
            .local_read_shader
            .expect("local-read variant");
        let consumer_metadata = scope
            .pipeline_metadata(
                SceneLocalReadScopePassRole::Consumer,
                &consumer_access,
                Some(&consumer_shader),
                SceneLocalReadDeviceLimits::new(8, 8),
            )
            .expect("consumer pipeline metadata");
        assert_eq!(
            consumer_metadata.local_read_fragment_spirv(),
            Some(consumer_shader.fragment_spirv)
        );
    }

    #[test]
    fn planner_rejects_scope_without_usage_extent_or_explicit_shader_contract() {
        let graph = local_read_graph();
        let storage = local_read_storage("we/passthrough", 1);
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(false, false, [64, 64], [64, 64]),
        )
        .expect_err("input usage is required");
        assert!(error.contains("lacks input-attachment image usage"));

        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [32, 64]),
        )
        .expect_err("matching extents are required");
        assert!(error.contains("mismatched extents"));

        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, false, [64, 64], [64, 64]),
        )
        .expect_err("both attached images require input usage");
        assert!(error.contains("destination physical slot"));

        let storage = local_read_storage("we/genericimage4", 1);
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [64, 64]),
        )
        .expect_err("explicit subpassInput shader is required");
        assert!(error.contains("no explicit subpassInput variant"));
    }

    #[test]
    fn planner_rejects_non_adjacent_or_aliased_producer_instead_of_sampling() {
        let storage = local_read_storage("we/passthrough", 1);
        let mut graph = local_read_graph();
        graph.pass_nodes.insert(1, SceneRenderingDevicePassNode {
            graph_index: 0,
            graph_activation_policy: crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
            pass_record_index: 0,
            pass_id: 7,
            role: SceneRenderPassKind::CopyTarget,
            target: SceneRenderTargetKind::Temporary,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            mesh_draw_start: 1,
            mesh_draw_count: 0,
        });
        graph.sampled_bindings[0].pass_node_index = 2;
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [64, 64]),
        )
        .expect_err("copy boundary cannot be crossed");
        assert!(error.contains("must contain authored draws"));

        let mut graph = local_read_graph();
        graph.target_allocations[1].physical_slot = 0;
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [64, 64]),
        )
        .expect_err("live targets cannot alias");
        assert!(error.contains("alias physical target"));

        let mut graph = local_read_graph();
        graph.pass_nodes[0].role = SceneRenderPassKind::CopyTarget;
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [64, 64]),
        )
        .expect_err("copy pass cannot become a local-read producer");
        assert!(error.contains("cannot cross copy or swap-reference passes"));

        let mut graph = local_read_graph();
        graph.sampled_bindings.push(SceneRenderingDeviceSampledBinding {
            pass_node_index: 1,
            graph_index: 0,
            mesh_draw_start: 1,
            mesh_draw_count: 1,
            kind: crate::engine::scene::SceneRenderBindingKind::EffectTarget,
            slot: 2,
            target: SceneRenderTargetKind::ImageLocalMain,
            target_name: SceneStringId::NONE,
            access: SceneRenderingDeviceImageAccess::SampledImage,
        });
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [64, 64]),
        )
        .expect_err("attached target cannot use sampled access");
        assert!(error.contains("attached targets cannot retain sampled-image layout"));

        let mut graph = local_read_graph();
        graph.sampled_bindings[0].mesh_draw_start = 0;
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [64, 64]),
        )
        .expect_err("input binding draw range must match consumer");
        assert!(error.contains("does not match consumer draw range"));
    }

    fn local_read_storage(consumer_shader: &str, consumer_input_mask: u32) -> SceneStorage {
        SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["we/genericimage4".to_owned(), consumer_shader.to_owned()],
            shader_contracts: vec![
                SceneShaderContractRecord {
                    shader_key: SceneStringId(0),
                    pipeline_key: SceneStringId(0),
                    texture_slot_mask: 0,
                    input_attachment_slot_mask: 0,
                    constant_start: 0,
                    constant_count: 0,
                    resource_heap_count: 2,
                    sampler_heap_count: 0,
                },
                SceneShaderContractRecord {
                    shader_key: SceneStringId(1),
                    pipeline_key: SceneStringId(1),
                    texture_slot_mask: 0,
                    input_attachment_slot_mask: consumer_input_mask,
                    constant_start: 0,
                    constant_count: 0,
                    resource_heap_count: 1,
                    sampler_heap_count: 0,
                },
            ],
            render_passes: vec![
                render_pass(0, SceneStringId(0), SceneRenderTargetKind::ImageLocalMain),
                render_pass(1, SceneStringId(1), SceneRenderTargetKind::ImageLocalSub),
            ],
            ..SceneBinaryDocument::default()
        })
        .expect("local-read storage")
    }

    fn render_pass(
        id: u32,
        shader_key: SceneStringId,
        target: SceneRenderTargetKind,
    ) -> SceneRenderPassRecord {
        SceneRenderPassRecord {
            id,
            role: if id == 0 {
                SceneRenderPassKind::BaseMaterial
            } else {
                SceneRenderPassKind::EffectMaterial
            },
            draw_primitive: if id == 0 {
                crate::engine::scene::SceneRenderPassDrawPrimitive::ObjectMesh
            } else {
                crate::engine::scene::SceneRenderPassDrawPrimitive::FullscreenTriangle
            },
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            pass_index: id,
            shader_key,
            target,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            pipeline_blend: ScenePipelineBlend::Normal,
            scene_blend: SceneCompositeBlend::Alpha,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            color_write_mask: SceneColorWriteMask::Rgba,
            clear_target: false,
        }
    }

    fn local_read_graph() -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            pass_nodes: vec![
                pass_node(0, 0, SceneRenderTargetKind::ImageLocalMain),
                pass_node(1, 1, SceneRenderTargetKind::ImageLocalSub),
            ],
            target_allocations: vec![
                target_allocation(SceneRenderTargetKind::ImageLocalMain, 0),
                target_allocation(SceneRenderTargetKind::ImageLocalSub, 1),
            ],
            sampled_bindings: vec![SceneRenderingDeviceSampledBinding {
                pass_node_index: 1,
                graph_index: 0,
                mesh_draw_start: 1,
                mesh_draw_count: 1,
                kind: crate::engine::scene::SceneRenderBindingKind::PreviousGraphTarget,
                slot: 0,
                target: SceneRenderTargetKind::ImageLocalMain,
                target_name: SceneStringId::NONE,
                access: SceneRenderingDeviceImageAccess::InputAttachment,
            }],
            mesh_draws: vec![draw(SceneStringId(0)), draw(SceneStringId(1))],
            graph_physical_target_count: 2,
            descriptor_heap_required: true,
            fifo_latest_ready_present_required: true,
            ..SceneRenderingDeviceGraphPlan {
                pass_nodes: Vec::new(),
                target_allocations: Vec::new(),
                effect_batches: Vec::new(),
                effect_batch_instances: Vec::new(),
                sampled_bindings: Vec::new(),
                material_sampled_bindings: Vec::new(),
                mesh_draws: Vec::new(),
                puppet_bone_palettes: Vec::new(),
                puppet_bone_matrices: Vec::new(),
                particle_gpu_emitters: Vec::new(),
                resolved_object_count: 0,
                resolved_visible_object_count: 0,
                resolved_attachment_link_count: 0,
                resolved_visible_effect_instance_count: 0,
                resolved_visible_effect_pass_count: 0,
                resolved_visible_effect_fbo_count: 0,
                descriptor_heap_required: false,
                descriptor_heap_resource_count: 0,
                descriptor_heap_sampled_image_count: 0,
                descriptor_heap_uniform_buffer_count: 0,
                descriptor_heap_storage_buffer_count: 0,
                descriptor_heap_sampler_count: 0,
                graph_physical_target_count: 0,
                graph_aliased_target_count: 0,
                fifo_latest_ready_present_required: false,
            }
        }
    }

    fn pass_node(
        pass_id: u32,
        draw_start: u32,
        target: SceneRenderTargetKind,
    ) -> SceneRenderingDevicePassNode {
        SceneRenderingDevicePassNode {
            graph_index: 0,
            graph_activation_policy: crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
            pass_record_index: pass_id,
            pass_id,
            role: if pass_id == 0 {
                SceneRenderPassKind::BaseMaterial
            } else {
                SceneRenderPassKind::EffectMaterial
            },
            target,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            mesh_draw_start: draw_start,
            mesh_draw_count: 1,
        }
    }

    fn target_allocation(
        target: SceneRenderTargetKind,
        physical_slot: u32,
    ) -> SceneRenderingDeviceTargetAllocation {
        SceneRenderingDeviceTargetAllocation {
            graph_index: 0,
            target,
            target_name: SceneStringId::NONE,
            first_write_pass_id: physical_slot,
            last_use_pass_id: physical_slot + 1,
            physical_slot,
            width: 64,
            height: 64,
        }
    }

    fn draw(shader_key: SceneStringId) -> SceneRenderingDeviceMeshDraw {
        SceneRenderingDeviceMeshDraw {
            primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            shader_key,
            mesh_index: INVALID_OBJECT_ID,
            resolved_object_index: INVALID_OBJECT_ID,
            clip_transform: [[0.0; 4]; 4],
            authored_source_extent: [64.0, 64.0],
            skinning_palette_start: 0,
            skinning_palette_count: 0,
            resolved_color: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            resolved_alpha: 1.0,
            apply_resolved_visual: false,
            effect_batch_atlas_tile: INVALID_OBJECT_ID,
            effect_batch_atlas_grid: [0; 2],
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            resolved_effect_visibility_mask: 0,
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 3,
            index_start: 0,
            index_count: 3,
            instance_count: 1,
        }
    }

    fn effect_target_plans(
        source_input_usage: bool,
        source_extent: [u32; 2],
        destination_extent: [u32; 2],
    ) -> Vec<SceneEffectTargetImagePlan> {
        effect_target_plans_with_usage(
            source_input_usage,
            source_input_usage,
            source_extent,
            destination_extent,
        )
    }

    fn effect_target_plans_with_usage(
        source_input_usage: bool,
        destination_input_usage: bool,
        source_extent: [u32; 2],
        destination_extent: [u32; 2],
    ) -> Vec<SceneEffectTargetImagePlan> {
        vec![
            effect_target_plan(0, SceneRenderTargetKind::ImageLocalMain, source_extent, source_input_usage),
            effect_target_plan(1, SceneRenderTargetKind::ImageLocalSub, destination_extent, destination_input_usage),
        ]
    }

    fn effect_target_plan(
        physical_slot: u32,
        target: SceneRenderTargetKind,
        extent: [u32; 2],
        input_attachment_required: bool,
    ) -> SceneEffectTargetImagePlan {
        SceneEffectTargetImagePlan {
            physical_slot,
            graph_index: 0,
            target,
            target_name: SceneStringId::NONE,
            format: vk::Format::R8G8B8A8_UNORM,
            width: extent[0],
            height: extent[1],
            batch_field_count: 1,
            batch_atlas_columns: 1,
            batch_atlas_rows: 1,
            persistent_across_frames: false,
            aliased_logical_target_count: 1,
            input_attachment_required,
        }
    }
}
