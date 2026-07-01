use crate::core::scene::binary::{
    SCENE_BINARY_NONE_ID, SCENE_BINARY_PARAMETER_ROLE_EFFECT_PROPERTY,
    SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO, SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT,
    SceneBinaryError, SceneBinaryLayoutPlan,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryEffectParameterIngestPlan
{
    pub(in crate::renderer::native_vulkan::scene) record_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_property_count: u32,
    pub(in crate::renderer::native_vulkan::scene) pass_constant_count: u32,
    pub(in crate::renderer::native_vulkan::scene) pass_switch_count: u32,
    pub(in crate::renderer::native_vulkan::scene) named_value_count: u32,
}

pub(super) fn native_vulkan_scene_binary_effect_parameter_ingest_plan(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<NativeVulkanSceneBinaryEffectParameterIngestPlan, SceneBinaryError> {
    let parameter_records = layout.effect_parameter_records(container)?;
    let mut plan = NativeVulkanSceneBinaryEffectParameterIngestPlan {
        record_count: parameter_records.len().min(u32::MAX as usize) as u32,
        ..Default::default()
    };
    for parameter in parameter_records {
        let parameter = parameter?;
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_EFFECT_PROPERTY != 0 {
            plan.effect_property_count = plan.effect_property_count.saturating_add(1);
        }
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT != 0 {
            plan.pass_constant_count = plan.pass_constant_count.saturating_add(1);
        }
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO != 0 {
            plan.pass_switch_count = plan.pass_switch_count.saturating_add(1);
        }
        if parameter.value_name != SCENE_BINARY_NONE_ID {
            plan.named_value_count = plan.named_value_count.saturating_add(1);
        }
    }
    Ok(plan)
}
