//! Particle compute pipeline backed by global descriptor-heap storage bindings.

use vulkan_renderer::descriptor_heap_element_index;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::engine::scene::SceneRenderingDeviceGraphPlan;
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneDescriptorBindingKind, native_vulkan_particle_compute_shader,
};
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
};

use super::shader_module::create_shader_module;

const PARTICLE_DESCRIPTOR_COUNT: usize = 3;
const PARTICLE_PUSH_BYTES: usize = PARTICLE_DESCRIPTOR_COUNT * 4;

pub(in crate::renderer::native_vulkan) struct SceneParticleComputePipeline {
    pipeline: vk::Pipeline,
    descriptor_push: [u8; PARTICLE_PUSH_BYTES],
}

impl SceneParticleComputePipeline {
    pub(in crate::renderer::native_vulkan) fn handle(&self) -> vk::Pipeline {
        self.pipeline
    }

    pub(in crate::renderer::native_vulkan) fn descriptor_push(&self) -> &[u8] {
        &self.descriptor_push
    }
}

pub(super) fn create_optional_particle_compute_pipeline(
    device: &Device,
    graph: &SceneRenderingDeviceGraphPlan,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    descriptor_base: Option<usize>,
) -> Result<Option<SceneParticleComputePipeline>, String> {
    if graph.particle_gpu_emitters.is_empty() {
        if descriptor_base.is_some() {
            return Err("particle descriptor base exists without GPU emitters".to_owned());
        }
        return Ok(None);
    }
    let descriptor_base = descriptor_base
        .ok_or_else(|| "particle compute requires a retained descriptor base".to_owned())?;
    let descriptor_push = particle_descriptor_push(descriptor_heap_plan, descriptor_base)?;
    create_particle_compute_pipeline(device).map(|pipeline| {
        Some(SceneParticleComputePipeline {
            pipeline,
            descriptor_push,
        })
    })
}

pub(super) fn destroy_optional_particle_compute_pipeline(
    device: &Device,
    pipeline: Option<SceneParticleComputePipeline>,
) {
    if let Some(pipeline) = pipeline {
        unsafe {
            device.destroy_pipeline(pipeline.pipeline, None);
        }
    }
}

fn create_particle_compute_pipeline(device: &Device) -> Result<vk::Pipeline, String> {
    let shader = native_vulkan_particle_compute_shader();
    let module = create_shader_module(device, shader.spirv, "particle compute")?;
    let result = (|| {
        let entry = b"main\0";
        let stage = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(module)
            .name(entry)
            .build();
        let mut flags = vk::PipelineCreateFlags2CreateInfo::builder()
            .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT)
            .build();
        let info = vk::ComputePipelineCreateInfo::builder()
            .stage(stage)
            .layout(vk::PipelineLayout::null())
            .push_next(&mut flags)
            .build();
        let (pipelines, _) =
            unsafe { device.create_compute_pipelines(vk::PipelineCache::null(), &[info], None) }
                .map_err(|err| format!("vkCreateComputePipelines(particle): {err:?}"))?;
        Ok(pipelines[0])
    })();
    unsafe {
        device.destroy_shader_module(module, None);
    }
    result
}

fn particle_descriptor_push(
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    descriptor_base: usize,
) -> Result<[u8; PARTICLE_PUSH_BYTES], String> {
    let shader = native_vulkan_particle_compute_shader();
    if shader.push_constant_bytes as usize != PARTICLE_PUSH_BYTES
        || shader.bindings.len() != PARTICLE_DESCRIPTOR_COUNT
    {
        return Err(format!(
            "particle compute shader requires {} push bytes and {} bindings, found {} and {}",
            PARTICLE_PUSH_BYTES,
            PARTICLE_DESCRIPTOR_COUNT,
            shader.push_constant_bytes,
            shader.bindings.len()
        ));
    }
    if u64::from(shader.push_constant_bytes) > plan.max_push_data_size {
        return Err(format!(
            "particle compute requires {} push-data bytes, device supports {}",
            shader.push_constant_bytes, plan.max_push_data_size
        ));
    }
    let descriptor_base_offset = plan
        .resource_descriptor_offsets
        .get(descriptor_base)
        .copied()
        .ok_or_else(|| "particle compute descriptor base is missing".to_owned())?;
    let slice_base = align_down(descriptor_base_offset, plan.resource_heap_alignment);
    let mut bytes = [0; PARTICLE_PUSH_BYTES];
    for binding in shader.bindings {
        if binding.kind != BuiltinSceneDescriptorBindingKind::StorageBuffer
            || binding.register as usize >= PARTICLE_DESCRIPTOR_COUNT
            || binding.push_offset != binding.register * 4
        {
            return Err(format!(
                "particle compute binding {:?} register {} has invalid native push offset {}",
                binding.kind, binding.register, binding.push_offset
            ));
        }
        let descriptor = descriptor_base + binding.register as usize;
        let kind = plan
            .resource_descriptor_kinds
            .get(descriptor)
            .copied()
            .ok_or_else(|| format!("particle compute descriptor {descriptor} is missing"))?;
        if kind != NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer {
            return Err(format!(
                "particle compute descriptor {descriptor} has kind {kind:?}, expected StorageBuffer"
            ));
        }
        let offset = plan.resource_descriptor_offsets[descriptor]
            .checked_sub(slice_base)
            .ok_or_else(|| "particle descriptor precedes its bound heap slice".to_owned())?;
        let element = descriptor_heap_element_index(offset, plan.buffer_descriptor_size)
            .map_err(|error| format!("resolve particle descriptor heap index: {error}"))?;
        let push_offset = binding.push_offset as usize;
        bytes[push_offset..push_offset + 4].copy_from_slice(&element.to_le_bytes());
    }
    Ok(bytes)
}

fn align_down(value: u64, alignment: u64) -> u64 {
    if alignment <= 1 {
        value
    } else {
        value - value % alignment
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::native_vulkan::{
        NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
        native_vulkan_vulkanalia_descriptor_heap_resource_plan,
    };

    #[test]
    fn native_push_words_address_three_retained_particle_descriptors() {
        let plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer,
                ],
                sampler_count: 0,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 0,
                    buffer_descriptor_size: 16,
                    buffer_descriptor_alignment: 16,
                    max_push_data_size: 128,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );
        assert!(plan.backend_ready);

        let bytes = particle_descriptor_push(&plan, 1).unwrap();
        let words = bytes
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
            .collect::<Vec<_>>();

        assert_eq!(plan.resource_descriptor_offsets, vec![0, 16, 32, 48]);
        assert_eq!(words, vec![1, 2, 3]);
    }
}
