use crate::core::scene::binary::{
    SCENE_BINARY_RETAINED_EFFECT_PARAMETER, SCENE_BINARY_RETAINED_EFFECT_PASS,
    SCENE_BINARY_RETAINED_EFFECT_UV_TRANSFORM, SCENE_BINARY_RETAINED_GEOMETRY,
    SCENE_BINARY_RETAINED_MATERIAL_PASS, SCENE_BINARY_RETAINED_PUPPET,
    SCENE_BINARY_RETAINED_RESOURCE, SCENE_BINARY_RETAINED_TEXTURE_SLOT, SceneBinaryError,
    SceneBinaryLayoutPlan,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryRetainedUpdatePlan {
    pub(in crate::renderer::native_vulkan::scene) resource_count: u32,
    pub(in crate::renderer::native_vulkan::scene) texture_slot_count: u32,
    pub(in crate::renderer::native_vulkan::scene) material_pass_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_pass_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_uv_transform_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_parameter_count: u32,
    pub(in crate::renderer::native_vulkan::scene) geometry_count: u32,
    pub(in crate::renderer::native_vulkan::scene) puppet_count: u32,
    pub(in crate::renderer::native_vulkan::scene) dirty_range_count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryRetainedIngestPlan {
    pub(in crate::renderer::native_vulkan::scene) record_count: u32,
    pub(in crate::renderer::native_vulkan::scene) dirty_range_count: u32,
    pub(in crate::renderer::native_vulkan::scene) stable_id_count: u32,
    pub(in crate::renderer::native_vulkan::scene) dirty_record_count: u32,
    pub(in crate::renderer::native_vulkan::scene) update_plan:
        NativeVulkanSceneBinaryRetainedUpdatePlan,
}

pub(super) fn native_vulkan_scene_binary_retained_ingest_plan(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<NativeVulkanSceneBinaryRetainedIngestPlan, SceneBinaryError> {
    let retained_records = layout.retained_gpu_state_records(container)?;
    let mut plan = NativeVulkanSceneBinaryRetainedIngestPlan {
        record_count: retained_records.len().min(u32::MAX as usize) as u32,
        ..Default::default()
    };
    for retained in retained_records {
        let retained = retained?;
        match retained.owner_kind {
            SCENE_BINARY_RETAINED_RESOURCE => {
                plan.update_plan.resource_count = plan.update_plan.resource_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_TEXTURE_SLOT => {
                plan.update_plan.texture_slot_count =
                    plan.update_plan.texture_slot_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_MATERIAL_PASS => {
                plan.update_plan.material_pass_count =
                    plan.update_plan.material_pass_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_EFFECT_PASS => {
                plan.update_plan.effect_pass_count =
                    plan.update_plan.effect_pass_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_EFFECT_UV_TRANSFORM => {
                plan.update_plan.effect_uv_transform_count =
                    plan.update_plan.effect_uv_transform_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_EFFECT_PARAMETER => {
                plan.update_plan.effect_parameter_count =
                    plan.update_plan.effect_parameter_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_GEOMETRY => {
                plan.update_plan.geometry_count = plan.update_plan.geometry_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_PUPPET => {
                plan.update_plan.puppet_count = plan.update_plan.puppet_count.saturating_add(1);
            }
            owner_kind => {
                return Err(SceneBinaryError::UnknownRetainedOwnerKind { owner_kind });
            }
        }
        plan.dirty_range_count = plan
            .dirty_range_count
            .saturating_add(retained.dirty_range_count);
        plan.update_plan.dirty_range_count = plan
            .update_plan
            .dirty_range_count
            .saturating_add(retained.dirty_range_count);
        if retained.stable_id != 0 {
            plan.stable_id_count = plan.stable_id_count.saturating_add(1);
        }
        if retained.dirty_range_count > 0 {
            plan.dirty_record_count = plan.dirty_record_count.saturating_add(1);
        }
    }
    Ok(plan)
}
