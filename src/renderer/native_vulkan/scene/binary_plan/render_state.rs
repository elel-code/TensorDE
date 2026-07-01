use crate::core::scene::binary::{SceneBinaryError, SceneBinaryLayoutPlan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryRenderStateRecord {
    pub(in crate::renderer::native_vulkan::scene) width: u32,
    pub(in crate::renderer::native_vulkan::scene) height: u32,
    pub(in crate::renderer::native_vulkan::scene) resource_count: u32,
    pub(in crate::renderer::native_vulkan::scene) node_count: u32,
    pub(in crate::renderer::native_vulkan::scene) material_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_count: u32,
    pub(in crate::renderer::native_vulkan::scene) flags: u32,
    pub(in crate::renderer::native_vulkan::scene) texture_slot_count: u32,
}

pub(super) fn native_vulkan_scene_binary_render_state_records(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<Vec<NativeVulkanSceneBinaryRenderStateRecord>, SceneBinaryError> {
    let render_state_records = layout.render_state_records(container)?;
    let mut render_states = Vec::with_capacity(render_state_records.len());
    for render_state in render_state_records {
        let render_state = render_state?;
        render_states.push(NativeVulkanSceneBinaryRenderStateRecord {
            width: render_state.width,
            height: render_state.height,
            resource_count: render_state.resource_count,
            node_count: render_state.node_count,
            material_count: render_state.material_count,
            effect_count: render_state.effect_count,
            flags: render_state.flags,
            texture_slot_count: render_state.texture_slot_count,
        });
    }
    Ok(render_states)
}
