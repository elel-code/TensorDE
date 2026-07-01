use crate::core::scene::binary::{SceneBinaryError, SceneBinaryLayoutPlan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryFlutterRecord {
    pub(in crate::renderer::native_vulkan::scene) owner_name: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_name: u32,
    pub(in crate::renderer::native_vulkan::scene) first_parameter: u32,
    pub(in crate::renderer::native_vulkan::scene) parameter_count: u32,
    pub(in crate::renderer::native_vulkan::scene) pass_count: u32,
    pub(in crate::renderer::native_vulkan::scene) motion_family_mask: u32,
    pub(in crate::renderer::native_vulkan::scene) anchor_name: u32,
    pub(in crate::renderer::native_vulkan::scene) dirty_range_count: u32,
}

pub(super) fn native_vulkan_scene_binary_flutter_records(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<Vec<NativeVulkanSceneBinaryFlutterRecord>, SceneBinaryError> {
    let flutter_states = layout.flutter_state_records(container)?;
    let mut records = Vec::with_capacity(flutter_states.len());
    for flutter in flutter_states {
        let flutter = flutter?;
        let parameter_count = layout
            .flutter_parameter_records(container, flutter)?
            .len()
            .min(u32::MAX as usize) as u32;
        records.push(NativeVulkanSceneBinaryFlutterRecord {
            owner_name: flutter.owner_name,
            effect_name: flutter.effect_name,
            first_parameter: flutter.first_parameter,
            parameter_count,
            pass_count: flutter.pass_count,
            motion_family_mask: flutter.motion_family_mask,
            anchor_name: flutter.anchor_name,
            dirty_range_count: flutter.dirty_range_count,
        });
    }
    Ok(records)
}
