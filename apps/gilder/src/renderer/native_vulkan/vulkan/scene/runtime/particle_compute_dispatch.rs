//! Particle compute dispatch and compute-to-indirect synchronization.

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder};

use crate::renderer::native_vulkan::native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor;

use super::SceneGpuResources;

pub(super) fn record_particle_compute_dispatch(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    scene: &SceneGpuResources,
) -> Result<bool, String> {
    let (Some(pipeline), Some(resources)) = (
        scene.pipelines.particle_compute.as_ref(),
        scene.particle_resources.as_ref(),
    ) else {
        return Ok(false);
    };
    let frame = scene.active_frame();
    let resource_bind =
        native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor(
            &frame.descriptor_heap,
            scene
                .particle_global_descriptor_base
                .ok_or_else(|| "particle compute descriptor base is missing".to_owned())?,
        )?;
    let group_count = resources.emitter_count.div_ceil(64);
    if group_count == 0 {
        return Ok(false);
    }
    unsafe {
        let time_bytes = scene.particle_scene_time_seconds.to_ne_bytes();
        device.cmd_update_buffer(
            command_buffer,
            resources.frame_time.target.buffer,
            0,
            &time_bytes,
        );
        let state_barrier = vk::BufferMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_STORAGE_READ)
            .buffer(resources.frame_time.target.buffer)
            .offset(0)
            .size(vk::WHOLE_SIZE)
            .build();
        let state_dependency = vk::DependencyInfo::builder()
            .buffer_memory_barriers(std::slice::from_ref(&state_barrier))
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &state_dependency);
        device.cmd_bind_resource_heap_ext(command_buffer, &resource_bind);
        device.cmd_bind_pipeline(
            command_buffer,
            vk::PipelineBindPoint::COMPUTE,
            pipeline.handle(),
        );
        let push_bytes = pipeline.descriptor_push();
        let push_range = vk::HostAddressRangeConstEXT::builder().address(push_bytes);
        let push = vk::PushDataInfoEXT::builder().offset(0).data(push_range);
        device.cmd_push_data_ext(command_buffer, &push);
        device.cmd_dispatch(command_buffer, group_count, 1, 1);
        let barrier = vk::BufferMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::DRAW_INDIRECT)
            .dst_access_mask(vk::AccessFlags2::INDIRECT_COMMAND_READ)
            .buffer(resources.indirect_upload.target.buffer)
            .offset(0)
            .size(vk::WHOLE_SIZE)
            .build();
        let dependency = vk::DependencyInfo::builder()
            .buffer_memory_barriers(std::slice::from_ref(&barrier))
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &dependency);
    }
    Ok(true)
}
