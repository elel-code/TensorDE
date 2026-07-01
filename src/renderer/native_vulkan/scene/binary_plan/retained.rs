use crate::core::scene::binary::{
    SCENE_BINARY_RETAINED_EFFECT_PARAMETER, SCENE_BINARY_RETAINED_EFFECT_PASS,
    SCENE_BINARY_RETAINED_GEOMETRY, SCENE_BINARY_RETAINED_MATERIAL_PASS,
    SCENE_BINARY_RETAINED_RESOURCE, SCENE_BINARY_RETAINED_TEXTURE_SLOT, SceneBinaryError,
    SceneBinaryLayoutPlan,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryRetainedGpuRecord {
    pub(in crate::renderer::native_vulkan::scene) owner_kind: u16,
    pub(in crate::renderer::native_vulkan::scene) flags: u16,
    pub(in crate::renderer::native_vulkan::scene) owner_name: u32,
    pub(in crate::renderer::native_vulkan::scene) stable_id: u64,
    pub(in crate::renderer::native_vulkan::scene) record_index: u32,
    pub(in crate::renderer::native_vulkan::scene) dirty_range_count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryRetainedUpdatePlan {
    pub(in crate::renderer::native_vulkan::scene) resource_count: u32,
    pub(in crate::renderer::native_vulkan::scene) texture_slot_count: u32,
    pub(in crate::renderer::native_vulkan::scene) material_pass_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_pass_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_parameter_count: u32,
    pub(in crate::renderer::native_vulkan::scene) geometry_count: u32,
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

pub(super) fn native_vulkan_scene_binary_retained_update_plan(
    records: &[NativeVulkanSceneBinaryRetainedGpuRecord],
) -> Result<NativeVulkanSceneBinaryRetainedUpdatePlan, SceneBinaryError> {
    let mut plan = NativeVulkanSceneBinaryRetainedUpdatePlan::default();
    for record in records {
        match record.owner_kind {
            SCENE_BINARY_RETAINED_RESOURCE => {
                plan.resource_count = plan.resource_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_TEXTURE_SLOT => {
                plan.texture_slot_count = plan.texture_slot_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_MATERIAL_PASS => {
                plan.material_pass_count = plan.material_pass_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_EFFECT_PASS => {
                plan.effect_pass_count = plan.effect_pass_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_EFFECT_PARAMETER => {
                plan.effect_parameter_count = plan.effect_parameter_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_GEOMETRY => {
                plan.geometry_count = plan.geometry_count.saturating_add(1);
            }
            owner_kind => {
                return Err(SceneBinaryError::UnknownRetainedOwnerKind { owner_kind });
            }
        }
        plan.dirty_range_count = plan
            .dirty_range_count
            .saturating_add(record.dirty_range_count);
    }
    Ok(plan)
}
