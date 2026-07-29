//! Shared SPIR-V module validation and creation.

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};
use vulkan_renderer::validate_spirv;

pub(super) fn create_shader_module(
    device: &Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, String> {
    validate_spirv(code).map_err(|error| format!("scene {label} shader is invalid: {error}"))?;
    let create_info = vk::ShaderModuleCreateInfo::builder()
        .code(code)
        .code_size(std::mem::size_of_val(code));
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("vkCreateShaderModule(vulkanalia {label}): {err:?}"))
}
