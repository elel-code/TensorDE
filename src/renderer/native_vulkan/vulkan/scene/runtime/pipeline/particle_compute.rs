//! Particle compute pipeline backed by global descriptor-heap storage bindings.

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::engine::scene::SceneRenderingDeviceGraphPlan;
use crate::renderer::native_vulkan::scene::native_vulkan_particle_compute_shader;
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_storage_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info,
};

pub(super) fn create_optional_particle_compute_pipeline(
    device: &Device,
    graph: &SceneRenderingDeviceGraphPlan,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
) -> Result<Option<vk::Pipeline>, String> {
    if graph.particle_gpu_emitters.is_empty() {
        return Ok(None);
    }
    let descriptor_base = descriptor_heap_plan
        .resource_descriptor_count
        .saturating_sub(2);
    create_particle_compute_pipeline(device, descriptor_heap_plan, descriptor_base).map(Some)
}

pub(super) fn destroy_optional_particle_compute_pipeline(
    device: &Device,
    pipeline: Option<vk::Pipeline>,
) {
    if let Some(pipeline) = pipeline {
        unsafe {
            device.destroy_pipeline(pipeline, None);
        }
    }
}

fn create_particle_compute_pipeline(
    device: &Device,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    descriptor_base: usize,
) -> Result<vk::Pipeline, String> {
    let shader = native_vulkan_particle_compute_shader();
    let module = create_shader_module(device, shader.spirv)?;
    let result = (|| {
        let mappings = [
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_storage_buffer_binding_mapping(
                descriptor_heap_plan,
                0,
                descriptor_base,
                descriptor_base,
                false,
            )?,
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_storage_buffer_binding_mapping(
                descriptor_heap_plan,
                1,
                descriptor_base,
                descriptor_base.saturating_add(1),
                true,
            )?,
        ];
        let mut mapping_info =
            native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(&mappings)?;
        let entry = b"main\0";
        let mut stage = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(module)
            .name(entry)
            .build();
        stage.next = &mut mapping_info as *mut _ as *const std::ffi::c_void;
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

fn create_shader_module(device: &Device, code: &[u32]) -> Result<vk::ShaderModule, String> {
    if code.first().copied() != Some(0x0723_0203) {
        return Err("particle compute shader is not valid SPIR-V bytecode".to_owned());
    }
    let info = vk::ShaderModuleCreateInfo::builder()
        .code(code)
        .code_size(std::mem::size_of_val(code));
    unsafe { device.create_shader_module(&info, None) }
        .map_err(|err| format!("vkCreateShaderModule(particle compute): {err:?}"))
}
