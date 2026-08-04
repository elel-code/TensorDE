//! Checked-in SPIR-V for the native resident-icon pipeline.
//!
//! Sources live beside these artifacts so shader changes remain reviewable.

pub(crate) const VERTEX: &[u32] =
    vulkan_renderer::include_spirv!("shaders/tensor_files_icon.vert.spv");
pub(crate) const FRAGMENT: &[u32] =
    vulkan_renderer::include_spirv!("shaders/tensor_files_icon.frag.spv");
pub(crate) const SVG_VERTEX: &[u32] =
    vulkan_renderer::include_spirv!("shaders/tensor_files_svg_icon.vert.spv");
pub(crate) const SVG_FRAGMENT: &[u32] =
    vulkan_renderer::include_spirv!("shaders/tensor_files_svg_icon.frag.spv");
pub(crate) const PREVIEW_COMPOSITE_VERTEX: &[u32] =
    vulkan_renderer::include_spirv!("shaders/tensor_files_preview_composite.vert.spv");
pub(crate) const PREVIEW_COMPOSITE_FRAGMENT: &[u32] =
    vulkan_renderer::include_spirv!("shaders/tensor_files_preview_composite.frag.spv");

#[cfg(test)]
mod tests {
    use vulkan_renderer::ShaderModuleDescriptor;

    #[test]
    fn embedded_icon_shaders_are_valid_spirv() {
        for (label, spirv) in [
            ("vertex", super::VERTEX),
            ("fragment", super::FRAGMENT),
            ("svg-vertex", super::SVG_VERTEX),
            ("svg-fragment", super::SVG_FRAGMENT),
            ("preview-composite-vertex", super::PREVIEW_COMPOSITE_VERTEX),
            (
                "preview-composite-fragment",
                super::PREVIEW_COMPOSITE_FRAGMENT,
            ),
        ] {
            ShaderModuleDescriptor {
                label: Some(format!("tensor-files-icon-{label}")),
                spirv: spirv.to_vec(),
            }
            .validate()
            .unwrap();
        }
    }
}
