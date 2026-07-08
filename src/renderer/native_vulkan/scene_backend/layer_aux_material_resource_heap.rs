//! Descriptor heap slice planning for WE auxiliary fullscreenlayer material draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/materials/util/fullscreenlayer.json`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

mod store;
mod vk_descriptor;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde::Serialize;
use vulkanalia::vk::{self, Handle};

use crate::engine::scene_engine::{SceneGraphTarget, SceneObjectId};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_resource_plan,
};

use super::layer_aux_material_pipeline::{
    NativeVulkanSceneLayerAuxMaterialPipelineFramePlan,
    NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan, WE_AUX_FULLSCREEN_LAYER_MATERIAL,
    WE_AUX_FULLSCREEN_LAYER_SHADER, WE_AUX_FULLSCREEN_LAYER_TEXTURE_SLOT,
    WE_AUX_FULLSCREEN_LAYER_TEXTURE_SOURCE,
};
use super::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
use super::pipeline::NativeVulkanScenePipelineResourceHeapClass;
pub(in crate::renderer::native_vulkan) use store::{
    NativeVulkanSceneLayerAuxMaterialResourceHeapBindInfo,
    NativeVulkanSceneLayerAuxMaterialResourceHeapStore,
    NativeVulkanSceneLayerAuxMaterialResourceHeapSyncAction,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialResourceHeapFramePlan
{
    pub clear_bind_count: usize,
    pub heap_slice_count: usize,
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub descriptor_model: &'static str,
    pub entries: Vec<NativeVulkanSceneLayerAuxMaterialResourceHeapEntry>,
    pub clear_bindings: Vec<NativeVulkanSceneLayerAuxMaterialResourceHeapClearBinding>,
    pub descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    pub command_order: [&'static str; 5],
    #[serde(skip)]
    pub(super) bindings: Vec<NativeVulkanSceneLayerAuxMaterialResourceHeapDescriptorBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialResourceHeapEntry {
    pub clear_bind_index: usize,
    pub heap_slice_index: usize,
    pub descriptor_index: usize,
    pub descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    pub resource_heap_offset: u64,
    pub sampler_descriptor_index: usize,
    pub sampler_heap_offset: u64,
    pub command_index: usize,
    pub block_index: usize,
    pub object: SceneObjectId,
    pub material: &'static str,
    pub shader: &'static str,
    pub source: &'static str,
    pub source_target: SceneGraphTarget,
    pub target: SceneGraphTarget,
    pub slot: u32,
    pub image_handle: u64,
    pub view_handle: u64,
    pub sampler_handle: u64,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
    pub shader_mapping: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialResourceHeapClearBinding
{
    pub clear_bind_index: usize,
    pub command_index: usize,
    pub block_index: usize,
    pub object: SceneObjectId,
    pub material: &'static str,
    pub shader: &'static str,
    pub source: &'static str,
    pub source_target: SceneGraphTarget,
    pub target: SceneGraphTarget,
    pub texture_slot: u32,
    pub heap_slice_index: usize,
    pub base_resource_descriptor_index: usize,
    pub base_resource_heap_offset: u64,
    pub base_sampler_descriptor_index: usize,
    pub base_sampler_heap_offset: u64,
    pub resource_descriptor_count: usize,
    pub texture_count: usize,
    pub shader_mappings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeVulkanSceneLayerAuxMaterialResourceHeapDescriptorBinding {
    SampledImage {
        descriptor_index: usize,
        sampler_descriptor_index: usize,
        source_target: SceneGraphTarget,
        image: vk::Image,
        view: vk::ImageView,
        sampler: vk::Sampler,
        format: vk::Format,
        width: u32,
        height: u32,
        mip_count: u32,
    },
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanSceneLayerAuxMaterialHeapSliceLayout {
    heap_slice_index: usize,
    base_resource_descriptor_index: usize,
    base_sampler_descriptor_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NativeVulkanSceneLayerAuxMaterialHeapSliceKey {
    shader: &'static str,
    material: &'static str,
    source: &'static str,
    source_target: SceneGraphTarget,
    texture_slot: u32,
}

#[derive(Debug, Clone)]
struct PendingLayerAuxMaterialResourceHeapEntry {
    clear_bind_index: usize,
    heap_slice_index: usize,
    descriptor_index: usize,
    sampler_descriptor_index: usize,
    command_index: usize,
    block_index: usize,
    object: SceneObjectId,
    material: &'static str,
    shader: &'static str,
    source: &'static str,
    source_target: SceneGraphTarget,
    target: SceneGraphTarget,
    slot: u32,
    image: vk::Image,
    view: vk::ImageView,
    sampler: vk::Sampler,
    format: vk::Format,
    width: u32,
    height: u32,
    mip_count: u32,
    shader_mapping: String,
}

impl NativeVulkanSceneLayerAuxMaterialResourceHeapFramePlan {
    pub(in crate::renderer::native_vulkan) fn from_pipeline_plan(
        pipelines: &NativeVulkanSceneLayerAuxMaterialPipelineFramePlan,
        descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        mut source_target: impl FnMut(
            &NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan,
        ) -> Result<SceneGraphTarget, String>,
        mut target_image_binding: impl FnMut(
            SceneGraphTarget,
        )
            -> Result<NativeVulkanSceneOffscreenTargetBinding, String>,
    ) -> Result<Self, String> {
        let mut heap_slice_lookup = BTreeMap::<
            NativeVulkanSceneLayerAuxMaterialHeapSliceKey,
            NativeVulkanSceneLayerAuxMaterialHeapSliceLayout,
        >::new();
        let mut descriptor_kinds = Vec::new();
        let mut pending_entries = Vec::new();
        let mut descriptor_bindings = Vec::new();
        let mut clear_bindings = Vec::with_capacity(pipelines.clear_keys.len());
        let mut sampler_descriptor_count = 0usize;

        for (clear_bind_index, key) in pipelines.clear_keys.iter().enumerate() {
            validate_aux_material_pipeline_key(key)?;
            let source_target = source_target(key)?;
            if source_target == SceneGraphTarget::Swapchain {
                return Err(format!(
                    "scene aux fullscreenlayer material for object {:?} samples _rt_FullFrameBuffer and requires a retained sampleable graph target, got Swapchain",
                    key.object
                ));
            }
            let heap_slice_key = NativeVulkanSceneLayerAuxMaterialHeapSliceKey {
                shader: key.shader,
                material: key.material,
                source: key.source,
                source_target,
                texture_slot: key.texture_slot,
            };
            let slice = if let Some(slice) = heap_slice_lookup.get(&heap_slice_key).copied() {
                slice
            } else {
                let binding = target_image_binding(source_target)?;
                validate_aux_material_source_binding(key, source_target, binding)?;
                let heap_slice_index = heap_slice_lookup.len();
                let base_resource_descriptor_index = descriptor_kinds.len();
                let base_sampler_descriptor_index = sampler_descriptor_count;
                push_aux_material_texture_descriptor(
                    &mut descriptor_kinds,
                    &mut pending_entries,
                    &mut descriptor_bindings,
                    heap_slice_index,
                    clear_bind_index,
                    key,
                    source_target,
                    binding,
                    sampler_descriptor_count,
                );
                sampler_descriptor_count = sampler_descriptor_count.saturating_add(1);
                let slice = NativeVulkanSceneLayerAuxMaterialHeapSliceLayout {
                    heap_slice_index,
                    base_resource_descriptor_index,
                    base_sampler_descriptor_index,
                };
                heap_slice_lookup.insert(heap_slice_key, slice);
                slice
            };

            clear_bindings.push(NativeVulkanSceneLayerAuxMaterialResourceHeapClearBinding {
                clear_bind_index,
                command_index: key.command_index,
                block_index: key.block_index,
                object: key.object,
                material: key.material,
                shader: key.shader,
                source: key.source,
                source_target,
                target: key.target,
                texture_slot: key.texture_slot,
                heap_slice_index: slice.heap_slice_index,
                base_resource_descriptor_index: slice.base_resource_descriptor_index,
                base_resource_heap_offset: 0,
                base_sampler_descriptor_index: slice.base_sampler_descriptor_index,
                base_sampler_heap_offset: 0,
                resource_descriptor_count: 1,
                texture_count: 1,
                shader_mappings: vec![aux_material_shader_mapping(key.texture_slot)],
            });
        }

        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: descriptor_kinds,
                sampler_count: sampler_descriptor_count,
                properties: descriptor_heap_properties,
            },
        );
        if !pending_entries.is_empty() && !descriptor_heap_plan.backend_ready {
            return Err(format!(
                "scene aux material resource heap requires a ready VK_EXT_descriptor_heap sampled-image resource plan: {:?}",
                descriptor_heap_plan.blocking_reason
            ));
        }

        let entries = finalize_aux_material_entries(pending_entries, &descriptor_heap_plan)?;
        for binding in &mut clear_bindings {
            binding.base_resource_heap_offset = *descriptor_heap_plan
                .resource_descriptor_offsets
                .get(binding.base_resource_descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene aux material clear bind {} missing base resource descriptor offset",
                        binding.clear_bind_index
                    )
                })?;
            binding.base_sampler_heap_offset = *descriptor_heap_plan
                .sampler_descriptor_offsets
                .get(binding.base_sampler_descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene aux material clear bind {} missing base sampler descriptor offset",
                        binding.clear_bind_index
                    )
                })?;
        }

        Ok(Self {
            clear_bind_count: clear_bindings.len(),
            heap_slice_count: heap_slice_lookup.len(),
            resource_descriptor_count: descriptor_heap_plan.resource_descriptor_offsets.len(),
            sampler_descriptor_count,
            descriptor_model: "VK_EXT_descriptor_heap",
            entries,
            clear_bindings,
            descriptor_heap_plan,
            command_order: [
                "collect_aux_fullscreenlayer_sampled_texture",
                "dedupe_aux_material_heap_slices",
                "pack_aux_material_descriptor_heap_slices",
                "bind_aux_material_heap_slice",
                "record_aux_0x410_to_aux_0x3f0_draw",
            ],
            bindings: descriptor_bindings,
        })
    }
}

fn validate_aux_material_pipeline_key(
    key: &NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan,
) -> Result<(), String> {
    if key.material != WE_AUX_FULLSCREEN_LAYER_MATERIAL
        || key.shader != WE_AUX_FULLSCREEN_LAYER_SHADER
        || key.source != WE_AUX_FULLSCREEN_LAYER_TEXTURE_SOURCE
        || key.texture_slot != WE_AUX_FULLSCREEN_LAYER_TEXTURE_SLOT
    {
        return Err(format!(
            "scene aux material heap requires fullscreenlayer/_rt_FullFrameBuffer/g_Texture0, got material={} shader={} source={} slot={}",
            key.material, key.shader, key.source, key.texture_slot
        ));
    }
    if key.resource_heap != NativeVulkanScenePipelineResourceHeapClass::LayerAuxMaterial {
        return Err(format!(
            "scene aux material heap requires LayerAuxMaterial resource heap, got {:?}",
            key.resource_heap
        ));
    }
    Ok(())
}

fn validate_aux_material_source_binding(
    key: &NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan,
    source_target: SceneGraphTarget,
    binding: NativeVulkanSceneOffscreenTargetBinding,
) -> Result<(), String> {
    if binding.target != source_target {
        return Err(format!(
            "scene aux material source resolver returned {:?} for requested {:?}",
            binding.target, source_target
        ));
    }
    if binding.image == vk::Image::null()
        || binding.view == vk::ImageView::null()
        || binding.sampler == vk::Sampler::null()
    {
        return Err(format!(
            "scene aux material object {:?} source {:?} requires resident image/view/sampler",
            key.object, source_target
        ));
    }
    if binding.width == 0 || binding.height == 0 {
        return Err(format!(
            "scene aux material object {:?} source {:?} has zero extent",
            key.object, source_target
        ));
    }
    Ok(())
}

fn push_aux_material_texture_descriptor(
    descriptor_kinds: &mut Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    pending_entries: &mut Vec<PendingLayerAuxMaterialResourceHeapEntry>,
    descriptor_bindings: &mut Vec<NativeVulkanSceneLayerAuxMaterialResourceHeapDescriptorBinding>,
    heap_slice_index: usize,
    clear_bind_index: usize,
    key: &NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan,
    source_target: SceneGraphTarget,
    binding: NativeVulkanSceneOffscreenTargetBinding,
    sampler_descriptor_index: usize,
) {
    let descriptor_index = descriptor_kinds.len();
    descriptor_kinds.push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage);
    descriptor_bindings.push(
        NativeVulkanSceneLayerAuxMaterialResourceHeapDescriptorBinding::SampledImage {
            descriptor_index,
            sampler_descriptor_index,
            source_target,
            image: binding.image,
            view: binding.view,
            sampler: binding.sampler,
            format: binding.format,
            width: binding.width,
            height: binding.height,
            mip_count: 1,
        },
    );
    pending_entries.push(PendingLayerAuxMaterialResourceHeapEntry {
        clear_bind_index,
        heap_slice_index,
        descriptor_index,
        sampler_descriptor_index,
        command_index: key.command_index,
        block_index: key.block_index,
        object: key.object,
        material: key.material,
        shader: key.shader,
        source: key.source,
        source_target,
        target: key.target,
        slot: key.texture_slot,
        image: binding.image,
        view: binding.view,
        sampler: binding.sampler,
        format: binding.format,
        width: binding.width,
        height: binding.height,
        mip_count: 1,
        shader_mapping: aux_material_shader_mapping(key.texture_slot),
    });
}

fn finalize_aux_material_entries(
    pending_entries: Vec<PendingLayerAuxMaterialResourceHeapEntry>,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
) -> Result<Vec<NativeVulkanSceneLayerAuxMaterialResourceHeapEntry>, String> {
    pending_entries
        .into_iter()
        .map(|entry| {
            let resource_heap_offset = *descriptor_heap_plan
                .resource_descriptor_offsets
                .get(entry.descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene aux material descriptor {} missing resource offset",
                        entry.descriptor_index
                    )
                })?;
            let sampler_heap_offset = *descriptor_heap_plan
                .sampler_descriptor_offsets
                .get(entry.sampler_descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene aux material sampler descriptor {} missing sampler offset",
                        entry.sampler_descriptor_index
                    )
                })?;
            Ok(NativeVulkanSceneLayerAuxMaterialResourceHeapEntry {
                clear_bind_index: entry.clear_bind_index,
                heap_slice_index: entry.heap_slice_index,
                descriptor_index: entry.descriptor_index,
                descriptor_kind:
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                resource_heap_offset,
                sampler_descriptor_index: entry.sampler_descriptor_index,
                sampler_heap_offset,
                command_index: entry.command_index,
                block_index: entry.block_index,
                object: entry.object,
                material: entry.material,
                shader: entry.shader,
                source: entry.source,
                source_target: entry.source_target,
                target: entry.target,
                slot: entry.slot,
                image_handle: entry.image.as_raw(),
                view_handle: entry.view.as_raw(),
                sampler_handle: entry.sampler.as_raw(),
                format: format!("{:?}", entry.format),
                width: entry.width,
                height: entry.height,
                mip_count: entry.mip_count,
                shader_mapping: entry.shader_mapping,
            })
        })
        .collect()
}

fn aux_material_shader_mapping(slot: u32) -> String {
    format!("we.texture_slot{slot}.g_Texture{slot} -> aux-material-heap-slice-offset0")
}
