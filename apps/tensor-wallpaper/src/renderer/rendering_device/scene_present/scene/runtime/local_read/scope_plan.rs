//! Typed planning for a retained dynamic-rendering local-read scope.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/exe/composelayer-and-effecttarget.md`
//! - Vulkan 1.4 `VK_KHR_dynamic_rendering_local_read`
//!
//! A typed input attachment is executable only when its producer is the
//! immediately preceding authored draw pass in the same graph and both
//! effect targets can remain attached for one rendering scope.  Unsupported
//! combinations are rejected here; callers must not fall back to sampling.

use vulkan_renderer::{
    Backend, Extent2D, RenderingLocalReadMapping, RenderingLocalReadMappingDescriptor,
    RenderingLocalReadMappingKind, TextureFormat,
};

use crate::engine::scene::{
    SceneRenderPassKind, SceneRenderTargetKind, SceneRenderingDeviceGraphPlan,
    SceneRenderingDeviceImageAccess, SceneRenderingDevicePassNode, SceneStorage, SceneStringId,
};
use crate::renderer::rendering_device::scene::BuiltinSceneLocalReadShader;
use crate::renderer::rendering_device::scene::rendering_device_scene_shader_for_key;

use super::super::descriptor_layout::{
    ScenePipelineShaderDescriptorAccess, scene_pipeline_shader_descriptor_access,
};
use super::super::effect_target::SceneEffectTargetImagePlan;
use super::{SceneLocalReadPipelineMetadata, validate_scene_local_read_shader_variant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::rendering_device) enum SceneLocalReadScopePassRole {
    Producer,
    Consumer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::rendering_device) struct SceneLocalReadScopeTarget {
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
    initial_physical_slot: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::rendering_device) struct SceneLocalReadScopePlan {
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
    pub(super) extent: Extent2D,
    pub(super) color_attachment_formats: [TextureFormat; 2],
    pub(super) input_slot: u32,
    pub(super) input_attachment_index: u32,
}

impl SceneLocalReadScopePlan {
    pub(in crate::renderer::rendering_device) fn pass_role(
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

    pub(in crate::renderer::rendering_device) fn graph_index(&self) -> u32 {
        self.graph_index
    }

    pub(in crate::renderer::rendering_device) fn producer_pass_record_index(&self) -> u32 {
        self.producer_pass_record_index
    }

    pub(in crate::renderer::rendering_device) fn producer_pass_node_index(&self) -> u32 {
        self.producer_pass_node_index
    }

    pub(in crate::renderer::rendering_device) fn producer_draw_range(&self) -> (u32, u32) {
        (self.producer_draw_start, self.producer_draw_count)
    }

    pub(in crate::renderer::rendering_device) fn consumer_pass_record_index(&self) -> u32 {
        self.consumer_pass_record_index
    }

    pub(in crate::renderer::rendering_device) fn consumer_pass_node_index(&self) -> u32 {
        self.consumer_pass_node_index
    }

    pub(in crate::renderer::rendering_device) fn consumer_draw_range(&self) -> (u32, u32) {
        (self.consumer_draw_start, self.consumer_draw_count)
    }

    pub(in crate::renderer::rendering_device) fn source(&self) -> SceneLocalReadScopeTarget {
        self.source
    }

    pub(in crate::renderer::rendering_device) fn destination(&self) -> SceneLocalReadScopeTarget {
        self.destination
    }

    pub(in crate::renderer::rendering_device) fn extent(&self) -> Extent2D {
        self.extent
    }

    pub(in crate::renderer::rendering_device) fn color_attachment_formats(
        &self,
    ) -> [TextureFormat; 2] {
        self.color_attachment_formats
    }

    pub(in crate::renderer::rendering_device) fn input_slot(&self) -> u32 {
        self.input_slot
    }

    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn shared_mapping(
        &self,
        device: &Backend,
        role: SceneLocalReadScopePassRole,
    ) -> Result<RenderingLocalReadMapping, String> {
        let locations = self.color_attachment_locations(role);
        let input_indices = self.color_attachment_input_indices(role);
        device
            .create_rendering_local_read_mapping(RenderingLocalReadMappingDescriptor {
                color_attachment_locations: &locations,
                color_attachment_input_indices: &input_indices,
                kind: match role {
                    SceneLocalReadScopePassRole::Producer => {
                        RenderingLocalReadMappingKind::OutputOnly
                    }
                    SceneLocalReadScopePassRole::Consumer => {
                        RenderingLocalReadMappingKind::InputAttachment
                    }
                },
            })
            .map_err(|error| format!("create scene local-read {role:?} mapping: {error}"))
    }

    pub(in crate::renderer::rendering_device) fn pipeline_metadata<'a>(
        &self,
        role: SceneLocalReadScopePassRole,
        descriptor_access: &ScenePipelineShaderDescriptorAccess,
        shader: Option<&'a BuiltinSceneLocalReadShader>,
    ) -> Result<SceneLocalReadPipelineMetadata<'a>, String> {
        let formats = self.color_attachment_formats();
        let locations = self.color_attachment_locations(role);
        match role {
            SceneLocalReadScopePassRole::Producer => {
                SceneLocalReadPipelineMetadata::output_only(descriptor_access, &formats, &locations)
            }
            SceneLocalReadScopePassRole::Consumer => {
                let input_indices = self.color_attachment_input_indices(role);
                SceneLocalReadPipelineMetadata::new(
                    descriptor_access,
                    shader,
                    &formats,
                    &locations,
                    &input_indices,
                )
            }
        }
    }

    fn color_attachment_locations(&self, role: SceneLocalReadScopePassRole) -> [Option<u32>; 2] {
        match role {
            SceneLocalReadScopePassRole::Producer => [Some(0), None],
            SceneLocalReadScopePassRole::Consumer => [None, Some(0)],
        }
    }

    fn color_attachment_input_indices(
        &self,
        role: SceneLocalReadScopePassRole,
    ) -> [Option<u32>; 2] {
        match role {
            SceneLocalReadScopePassRole::Producer => [None, None],
            SceneLocalReadScopePassRole::Consumer => [Some(self.input_attachment_index), None],
        }
    }
}

impl SceneLocalReadScopeTarget {
    pub(in crate::renderer::rendering_device) fn graph_index(self) -> u32 {
        self.graph_index
    }

    pub(in crate::renderer::rendering_device) fn target(self) -> SceneRenderTargetKind {
        self.target
    }

    pub(in crate::renderer::rendering_device) fn target_name(self) -> SceneStringId {
        self.target_name
    }

    pub(in crate::renderer::rendering_device) fn initial_physical_slot(self) -> u32 {
        self.initial_physical_slot
    }
}

pub(in crate::renderer::rendering_device) fn scene_local_read_scope_plans(
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
        let producer_key = (producer.graph_index, producer.target, producer.target_name);
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
        let destination_plan = effect_target_plan(effect_target_plans, destination, "destination")?;
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
        let shader = rendering_device_scene_shader_for_key(shader_key).ok_or_else(|| {
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
            extent: source_plan.extent,
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
    if source.extent != destination.extent {
        return Err(format!(
            "scene graph {} local-read pass pair {} -> {} has mismatched extents {}x{} and {}x{}",
            consumer.graph_index,
            producer.pass_id,
            consumer.pass_id,
            source.extent.width,
            source.extent.height,
            destination.extent.width,
            destination.extent.height
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
#[path = "scope_plan/tests.rs"]
mod tests;
