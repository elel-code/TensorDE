//! Precompiled Vulkan 1.4 shaders for Tensor Files's instanced analytic rectangles.
//!
//! The binary assets are embedded through `vulkan-renderer`'s standard SPIR-V
//! inclusion macro. This preserves Vulkanalia's compile-time alignment and
//! byte-length validation without giving Tensor Files a direct Vulkanalia dependency.

pub(super) const VERTEX: &[u32] =
    vulkan_renderer::include_spirv!("shaders/tensor_files_analytic_rect.vert.spv");
pub(super) const FRAGMENT: &[u32] =
    vulkan_renderer::include_spirv!("shaders/tensor_files_analytic_rect.frag.spv");

#[cfg(test)]
mod tests {
    use vulkan_renderer::ShaderModuleDescriptor;

    use super::{FRAGMENT, VERTEX};

    #[test]
    fn embedded_analytic_rect_shaders_are_valid_spirv() {
        for spirv in [VERTEX, FRAGMENT] {
            ShaderModuleDescriptor {
                label: None,
                spirv: spirv.to_vec(),
            }
            .validate()
            .unwrap();
        }
    }
}
