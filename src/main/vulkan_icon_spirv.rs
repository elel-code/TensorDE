//! Checked-in SPIR-V for the native resident-icon pipeline.
//!
//! Sources live beside these artifacts so shader changes remain reviewable.

pub(crate) const VERTEX: &[u32] = vulkan_renderer::include_spirv!("shaders/fika_icon.vert.spv");
pub(crate) const FRAGMENT: &[u32] = vulkan_renderer::include_spirv!("shaders/fika_icon.frag.spv");

#[cfg(test)]
mod tests {
    use vulkan_renderer::ShaderModuleDescriptor;

    #[test]
    fn embedded_icon_shaders_are_valid_spirv() {
        for (label, spirv) in [("vertex", super::VERTEX), ("fragment", super::FRAGMENT)] {
            ShaderModuleDescriptor {
                label: Some(format!("fika-icon-{label}")),
                spirv: spirv.to_vec(),
            }
            .validate()
            .unwrap();
        }
    }
}
