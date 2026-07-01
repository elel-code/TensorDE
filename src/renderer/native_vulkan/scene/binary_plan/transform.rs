use crate::core::scene::binary::{SceneBinaryError, SceneBinaryLayoutPlan};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryTransformRecord {
    pub(in crate::renderer::native_vulkan::scene) owner_name: u32,
    pub(in crate::renderer::native_vulkan::scene) timeline_name: u32,
    pub(in crate::renderer::native_vulkan::scene) property: u16,
    pub(in crate::renderer::native_vulkan::scene) flags: u16,
    pub(in crate::renderer::native_vulkan::scene) keyframe_count: u32,
    pub(in crate::renderer::native_vulkan::scene) time_offset_ms: u64,
    pub(in crate::renderer::native_vulkan::scene) first_time_ms: u64,
    pub(in crate::renderer::native_vulkan::scene) last_time_ms: u64,
    pub(in crate::renderer::native_vulkan::scene) values: [f32; 7],
}

pub(super) fn native_vulkan_scene_binary_transform_records(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<Vec<NativeVulkanSceneBinaryTransformRecord>, SceneBinaryError> {
    let transform_records = layout.transform_timeline_records(container)?;
    let mut transforms = Vec::with_capacity(transform_records.len());
    for transform in transform_records {
        let transform = transform?;
        transforms.push(NativeVulkanSceneBinaryTransformRecord {
            owner_name: transform.owner_name,
            timeline_name: transform.timeline_name,
            property: transform.property,
            flags: transform.flags,
            keyframe_count: transform.keyframe_count,
            time_offset_ms: transform.time_offset_ms,
            first_time_ms: transform.first_time_ms,
            last_time_ms: transform.last_time_ms,
            values: [
                transform.value0,
                transform.value1,
                transform.value2,
                transform.value3,
                transform.value4,
                transform.value5,
                transform.value6,
            ],
        });
    }
    Ok(transforms)
}
