//! Precompiled shaders for Fika's untextured UI triangle stream.

pub(super) const VERTEX: &[u32] = vulkan_renderer::include_spirv!("shaders/fika_color.vert.spv");
pub(super) const FRAGMENT: &[u32] = vulkan_renderer::include_spirv!("shaders/fika_color.frag.spv");

#[cfg(test)]
mod tests {
    use vulkan_renderer::ShaderModuleDescriptor;

    use super::{FRAGMENT, VERTEX};

    #[test]
    fn embedded_color_shaders_are_valid_spirv() {
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
