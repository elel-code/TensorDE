//! Vulkan shader module creation for scene pipelines.
//!
//! References:
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/shaders/genericimage4.vert`
//! - `reverse-engineered/shaders/effects/iris.vert`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

pub(in crate::renderer::native_vulkan) fn native_vulkan_create_scene_shader_module(
    device: &Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, String> {
    native_vulkan_validate_scene_spirv(code, label)?;
    let create_info = vk::ShaderModuleCreateInfo::builder()
        .code(code)
        .code_size(std::mem::size_of_val(code));
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("vkCreateShaderModule({label}): {err:?}"))
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_validate_scene_spirv(
    code: &[u32],
    label: &'static str,
) -> Result<(), String> {
    if code.first().copied() != Some(0x0723_0203) {
        return Err(format!("{label} shader is not valid SPIR-V bytecode"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_spirv_validation_rejects_missing_magic() {
        assert!(native_vulkan_validate_scene_spirv(&[0], "bad").is_err());
        native_vulkan_validate_scene_spirv(&[0x0723_0203, 1], "good").expect("valid spirv");
    }
}
