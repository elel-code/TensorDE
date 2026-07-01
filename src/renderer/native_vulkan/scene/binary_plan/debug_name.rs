use crate::core::scene::binary::{
    SCENE_BINARY_DEBUG_NAME_RECORD_SIZE, SceneBinaryChunkKind, SceneBinaryError,
    SceneBinaryLayoutPlan,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryDebugNameSummary {
    pub(in crate::renderer::native_vulkan::scene) record_count: u32,
    pub(in crate::renderer::native_vulkan::scene) string_bytes: u32,
}

pub(super) fn native_vulkan_scene_binary_debug_names(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<NativeVulkanSceneBinaryDebugNameSummary, SceneBinaryError> {
    let debug_names = layout.debug_names(container)?;
    let descriptor =
        layout
            .chunk(SceneBinaryChunkKind::DebugNames)
            .ok_or(SceneBinaryError::MissingChunk {
                kind: SceneBinaryChunkKind::DebugNames,
            })?;
    let record_count = debug_names.len().min(u32::MAX as usize) as u32;
    let record_bytes = debug_names
        .len()
        .saturating_mul(SCENE_BINARY_DEBUG_NAME_RECORD_SIZE);
    let string_bytes = descriptor
        .length
        .saturating_sub(record_bytes.min(u64::MAX as usize) as u64)
        .min(u64::from(u32::MAX)) as u32;

    Ok(NativeVulkanSceneBinaryDebugNameSummary {
        record_count,
        string_bytes,
    })
}
