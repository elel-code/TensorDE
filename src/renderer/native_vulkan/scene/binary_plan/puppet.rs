use crate::core::scene::binary::{SceneBinaryError, SceneBinaryLayoutPlan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryPuppetRecord {
    pub(in crate::renderer::native_vulkan::scene) owner_name: u32,
    pub(in crate::renderer::native_vulkan::scene) vertex_count: u32,
    pub(in crate::renderer::native_vulkan::scene) index_count: u32,
    pub(in crate::renderer::native_vulkan::scene) animation_layer_count: u32,
    pub(in crate::renderer::native_vulkan::scene) flags: u32,
    pub(in crate::renderer::native_vulkan::scene) dirty_range_count: u32,
}

pub(super) fn native_vulkan_scene_binary_puppet_records(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<Vec<NativeVulkanSceneBinaryPuppetRecord>, SceneBinaryError> {
    let puppet_records = layout.puppet_records(container)?;
    let mut puppets = Vec::with_capacity(puppet_records.len());
    for puppet in puppet_records {
        let puppet = puppet?;
        puppets.push(NativeVulkanSceneBinaryPuppetRecord {
            owner_name: puppet.owner_name,
            vertex_count: puppet.vertex_count,
            index_count: puppet.index_count,
            animation_layer_count: puppet.animation_layer_count,
            flags: puppet.flags,
            dirty_range_count: puppet.dirty_range_count,
        });
    }
    Ok(puppets)
}
