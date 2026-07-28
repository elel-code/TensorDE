//! Embedded native text shaders validated by `vulkan-renderer` at module
//! creation. Sources live beside the binaries for reproducible regeneration.

pub(crate) const VERTEX: &[u32] = vulkan_renderer::include_spirv!("shaders/fika_text.vert.spv");
pub(crate) const FRAGMENT: &[u32] = vulkan_renderer::include_spirv!("shaders/fika_text.frag.spv");

#[cfg(test)]
mod tests {
    use vulkan_renderer::ShaderModuleDescriptor;

    #[test]
    fn embedded_text_shaders_are_valid_spirv() {
        for (label, spirv) in [("vertex", super::VERTEX), ("fragment", super::FRAGMENT)] {
            ShaderModuleDescriptor {
                label: Some(format!("fika-text-{label}")),
                spirv: spirv.to_vec(),
            }
            .validate()
            .unwrap();
        }
    }
}
