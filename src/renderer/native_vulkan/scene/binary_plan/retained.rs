use crate::core::scene::binary::{SceneBinaryError, SceneBinaryLayoutPlan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryRetainedGpuRecord {
    pub(in crate::renderer::native_vulkan::scene) owner_kind: u16,
    pub(in crate::renderer::native_vulkan::scene) flags: u16,
    pub(in crate::renderer::native_vulkan::scene) owner_name: u32,
    pub(in crate::renderer::native_vulkan::scene) stable_id: u64,
    pub(in crate::renderer::native_vulkan::scene) record_index: u32,
    pub(in crate::renderer::native_vulkan::scene) dirty_range_count: u32,
}

pub(super) fn native_vulkan_scene_binary_retained_gpu_records(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<Vec<NativeVulkanSceneBinaryRetainedGpuRecord>, SceneBinaryError> {
    let retained_records = layout.retained_gpu_state_records(container)?;
    let mut records = Vec::with_capacity(retained_records.len());
    for retained in retained_records {
        let retained = retained?;
        records.push(NativeVulkanSceneBinaryRetainedGpuRecord {
            owner_kind: retained.owner_kind,
            flags: retained.flags,
            owner_name: retained.owner_name,
            stable_id: retained.stable_id,
            record_index: retained.record_index,
            dirty_range_count: retained.dirty_range_count,
        });
    }
    Ok(records)
}

pub(super) fn native_vulkan_scene_binary_retained_dirty_range_count(
    records: &[NativeVulkanSceneBinaryRetainedGpuRecord],
) -> u32 {
    records
        .iter()
        .map(|record| record.dirty_range_count)
        .fold(0u32, u32::saturating_add)
}
