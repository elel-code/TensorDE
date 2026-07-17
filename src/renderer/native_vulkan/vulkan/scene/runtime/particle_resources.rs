//! Device-local particle state and indirect draw resources.

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene::{
    SceneParticleGpuEmitterState, SceneParticleIndirectDraw, SceneRenderingDeviceGraphPlan,
    SceneStorage,
};
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaBuffer, NativeVulkanVulkanaliaBufferMemoryPreference,
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaRecordedBufferUpload, VulkanaliaDescriptorHeapResourceResources,
    native_vulkan_vulkanalia_create_buffer,
    native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload,
    native_vulkan_vulkanalia_destroy_buffer, native_vulkan_vulkanalia_read_host_buffer,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_storage_buffer,
};

pub(super) fn append_global_descriptor_plan(
    descriptors: &mut Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    enabled: bool,
) -> Option<usize> {
    if !enabled {
        return None;
    }
    let base = descriptors.len();
    descriptors.extend([
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer,
    ]);
    Some(base)
}

pub(super) struct SceneParticleGpuResources {
    pub state_upload: NativeVulkanVulkanaliaRecordedBufferUpload,
    pub indirect_upload: NativeVulkanVulkanaliaRecordedBufferUpload,
    pub indirect_readback: NativeVulkanVulkanaliaBuffer,
    pub emitter_count: u32,
    pub total_capacity: u64,
}

pub(super) fn create_scene_particle_gpu_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    command_buffer: vk::CommandBuffer,
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
) -> Result<Option<SceneParticleGpuResources>, String> {
    if graph.particle_gpu_emitters.is_empty() {
        return Ok(None);
    }
    let states = graph
        .particle_gpu_emitters
        .iter()
        .map(|plan| {
            let particle = storage
                .particles()
                .get(plan.particle_index as usize)
                .ok_or_else(|| {
                    format!("particle plan {} has no scene record", plan.particle_index)
                })?;
            Ok(SceneParticleGpuEmitterState::from_record(
                particle,
                0.0,
                plan.capacity,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let indirect = graph
        .particle_gpu_emitters
        .iter()
        .map(|_| SceneParticleIndirectDraw::BILLBOARD)
        .collect::<Vec<_>>();
    let state_payload = typed_bytes(&states);
    let indirect_payload = typed_bytes(&indirect);
    let state_upload =
        native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload(
            device,
            memory_properties,
            command_buffer,
            "scene-particle-state-storage-buffer",
            state_payload.len() as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            state_payload,
        )?;
    let indirect_upload =
        match native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload(
            device,
            memory_properties,
            command_buffer,
            "scene-particle-indirect-buffer",
            indirect_payload.len() as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::INDIRECT_BUFFER
                | vk::BufferUsageFlags::TRANSFER_SRC
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            indirect_payload,
        ) {
            Ok(upload) => upload,
            Err(err) => {
                destroy_recorded_buffer_upload(device, state_upload);
                return Err(err);
            }
        };
    let indirect_readback = match native_vulkan_vulkanalia_create_buffer(
        device,
        memory_properties,
        "scene-particle-indirect-readback-buffer",
        indirect_payload.len() as u64,
        vk::BufferUsageFlags::TRANSFER_DST,
        NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
        None,
    ) {
        Ok(buffer) => buffer,
        Err(err) => {
            destroy_recorded_buffer_upload(device, indirect_upload);
            destroy_recorded_buffer_upload(device, state_upload);
            return Err(err);
        }
    };
    Ok(Some(SceneParticleGpuResources {
        state_upload,
        indirect_upload,
        indirect_readback,
        emitter_count: states.len() as u32,
        total_capacity: graph
            .particle_gpu_emitters
            .iter()
            .map(|plan| u64::from(plan.capacity))
            .sum(),
    }))
}

pub(super) fn destroy_scene_particle_gpu_resources(
    device: &Device,
    resources: SceneParticleGpuResources,
) {
    native_vulkan_vulkanalia_destroy_buffer(device, resources.indirect_readback);
    destroy_recorded_buffer_upload(device, resources.indirect_upload);
    destroy_recorded_buffer_upload(device, resources.state_upload);
}

pub(super) fn read_scene_particle_indirect_commands(
    device: &Device,
    resources: &SceneParticleGpuResources,
) -> Result<Vec<SceneParticleIndirectDraw>, String> {
    let bytes = native_vulkan_vulkanalia_read_host_buffer(
        device,
        &resources.indirect_readback,
        resources.indirect_readback.snapshot.requested_bytes,
    )?;
    bytes
        .chunks_exact(16)
        .map(|command| {
            let field = |offset| {
                u32::from_ne_bytes(command[offset..offset + 4].try_into().expect("u32 field"))
            };
            Ok(SceneParticleIndirectDraw {
                vertex_count: field(0),
                instance_count: field(4),
                first_vertex: field(8),
                first_instance: field(12),
            })
        })
        .collect()
}

pub(super) fn validate_scene_particle_indirect_readback(
    device: &Device,
    resources: &SceneParticleGpuResources,
    expected_instance_total: u64,
) -> Result<(bool, u64), String> {
    let commands = read_scene_particle_indirect_commands(device, resources)?;
    let instance_total = commands
        .iter()
        .map(|command| u64::from(command.instance_count))
        .sum();
    let shape_valid = commands.iter().all(|command| {
        command.vertex_count == 4 && command.first_vertex == 0 && command.first_instance == 0
    });
    Ok((
        shape_valid && instance_total == expected_instance_total,
        instance_total,
    ))
}

pub(super) fn write_scene_particle_descriptors(
    device: &Device,
    descriptor_heap: &mut VulkanaliaDescriptorHeapResourceResources,
    descriptor_base: usize,
    resources: &SceneParticleGpuResources,
) -> Result<(), String> {
    native_vulkan_vulkanalia_write_descriptor_heap_resource_storage_buffer(
        device,
        descriptor_heap,
        descriptor_base,
        resources.state_upload.target.device_address,
        resources.state_upload.target.snapshot.requested_bytes,
    )?;
    native_vulkan_vulkanalia_write_descriptor_heap_resource_storage_buffer(
        device,
        descriptor_heap,
        descriptor_base.saturating_add(1),
        resources.indirect_upload.target.device_address,
        resources.indirect_upload.target.snapshot.requested_bytes,
    )
}

pub(super) fn release_scene_particle_staging(
    device: &Device,
    resources: &mut SceneParticleGpuResources,
) {
    if let Some(staging) = resources.state_upload.staging.take() {
        native_vulkan_vulkanalia_destroy_buffer(device, staging);
    }
    if let Some(staging) = resources.indirect_upload.staging.take() {
        native_vulkan_vulkanalia_destroy_buffer(device, staging);
    }
}

fn destroy_recorded_buffer_upload(
    device: &Device,
    upload: NativeVulkanVulkanaliaRecordedBufferUpload,
) {
    if let Some(staging) = upload.staging {
        native_vulkan_vulkanalia_destroy_buffer(device, staging);
    }
    native_vulkan_vulkanalia_destroy_buffer(device, upload.target);
}

fn typed_bytes<T>(values: &[T]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_payload_keeps_indirect_command_bytes() {
        let draws = [SceneParticleIndirectDraw::with_instance_count(17)];
        assert_eq!(typed_bytes(&draws).len(), 16);
    }
}
