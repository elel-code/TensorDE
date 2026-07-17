//! Device-local particle state and indirect draw resources.

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene::{
    SceneParticleGpuEmitterState, SceneParticleIndirectDraw, SceneRenderingDeviceGraphPlan,
    SceneStorage,
};
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaRecordedBufferUpload,
    native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload,
    native_vulkan_vulkanalia_destroy_buffer,
};

pub(super) struct SceneParticleGpuResources {
    pub state_upload: NativeVulkanVulkanaliaRecordedBufferUpload,
    pub indirect_upload: NativeVulkanVulkanaliaRecordedBufferUpload,
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
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            indirect_payload,
        ) {
            Ok(upload) => upload,
            Err(err) => {
                destroy_recorded_buffer_upload(device, state_upload);
                return Err(err);
            }
        };
    Ok(Some(SceneParticleGpuResources {
        state_upload,
        indirect_upload,
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
    destroy_recorded_buffer_upload(device, resources.indirect_upload);
    destroy_recorded_buffer_upload(device, resources.state_upload);
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
