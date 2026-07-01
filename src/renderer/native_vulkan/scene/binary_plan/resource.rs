use crate::core::scene::binary::{SceneBinaryError, SceneBinaryLayoutPlan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryResourceRecord {
    pub(in crate::renderer::native_vulkan::scene) id_name: u32,
    pub(in crate::renderer::native_vulkan::scene) source_name: u32,
    pub(in crate::renderer::native_vulkan::scene) original_source_name: u32,
    pub(in crate::renderer::native_vulkan::scene) role_name: u32,
    pub(in crate::renderer::native_vulkan::scene) kind: u16,
    pub(in crate::renderer::native_vulkan::scene) flags: u16,
    pub(in crate::renderer::native_vulkan::scene) width: u32,
    pub(in crate::renderer::native_vulkan::scene) height: u32,
    pub(in crate::renderer::native_vulkan::scene) upload_hints: u32,
}

pub(super) fn native_vulkan_scene_binary_resource_records(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<Vec<NativeVulkanSceneBinaryResourceRecord>, SceneBinaryError> {
    let resource_records = layout.resource_records(container)?;
    let mut resources = Vec::with_capacity(resource_records.len());
    for resource in resource_records {
        let resource = resource?;
        resources.push(NativeVulkanSceneBinaryResourceRecord {
            id_name: resource.id_name,
            source_name: resource.source_name,
            original_source_name: resource.original_source_name,
            role_name: resource.role_name,
            kind: resource.kind,
            flags: resource.flags,
            width: resource.width,
            height: resource.height,
            upload_hints: resource.upload_hints,
        });
    }
    Ok(resources)
}
