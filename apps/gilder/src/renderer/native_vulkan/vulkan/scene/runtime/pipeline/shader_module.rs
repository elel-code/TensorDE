//! Shared SPIR-V module validation and creation.

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

pub(super) fn create_shader_module(
    device: &Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, String> {
    if code.first().copied() != Some(0x0723_0203) {
        return Err(format!("scene {label} shader is not valid SPIR-V bytecode"));
    }
    let create_info = vk::ShaderModuleCreateInfo::builder()
        .code(code)
        .code_size(std::mem::size_of_val(code));
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("vkCreateShaderModule(vulkanalia {label}): {err:?}"))
}
