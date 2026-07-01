use crate::core::scene::binary::{SceneBinaryError, SceneBinaryLayoutPlan};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryNodeRecord {
    pub(in crate::renderer::native_vulkan::scene) id_name: u32,
    pub(in crate::renderer::native_vulkan::scene) display_name: u32,
    pub(in crate::renderer::native_vulkan::scene) parent_index: u32,
    pub(in crate::renderer::native_vulkan::scene) resource_name: u32,
    pub(in crate::renderer::native_vulkan::scene) kind: u16,
    pub(in crate::renderer::native_vulkan::scene) flags: u16,
    pub(in crate::renderer::native_vulkan::scene) draw_order: u32,
    pub(in crate::renderer::native_vulkan::scene) child_count: u32,
    pub(in crate::renderer::native_vulkan::scene) first_child_index: u32,
    pub(in crate::renderer::native_vulkan::scene) subtree_node_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_count: u32,
    pub(in crate::renderer::native_vulkan::scene) audio_count: u32,
    pub(in crate::renderer::native_vulkan::scene) property_count: u32,
    pub(in crate::renderer::native_vulkan::scene) material_index: u32,
    pub(in crate::renderer::native_vulkan::scene) geometry_index: u32,
    pub(in crate::renderer::native_vulkan::scene) first_transform: u32,
    pub(in crate::renderer::native_vulkan::scene) transform_count: u32,
    pub(in crate::renderer::native_vulkan::scene) puppet_index: u32,
    pub(in crate::renderer::native_vulkan::scene) opacity: f32,
    pub(in crate::renderer::native_vulkan::scene) color_rgba: u32,
    pub(in crate::renderer::native_vulkan::scene) stroke_color_rgba: u32,
    pub(in crate::renderer::native_vulkan::scene) stroke_width: f32,
    pub(in crate::renderer::native_vulkan::scene) corner_radius: f32,
    pub(in crate::renderer::native_vulkan::scene) fit: u16,
}

pub(super) fn native_vulkan_scene_binary_node_records(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<Vec<NativeVulkanSceneBinaryNodeRecord>, SceneBinaryError> {
    let node_records = layout.node_records(container)?;
    let mut nodes = Vec::with_capacity(node_records.len());
    for node in node_records {
        let node = node?;
        nodes.push(NativeVulkanSceneBinaryNodeRecord {
            id_name: node.id_name,
            display_name: node.display_name,
            parent_index: node.parent_index,
            resource_name: node.resource_name,
            kind: node.kind,
            flags: node.flags,
            draw_order: node.draw_order,
            child_count: node.child_count,
            first_child_index: node.first_child_index,
            subtree_node_count: node.subtree_node_count,
            effect_count: node.effect_count,
            audio_count: node.audio_count,
            property_count: node.property_count,
            material_index: node.material_index,
            geometry_index: node.geometry_index,
            first_transform: node.first_transform,
            transform_count: node.transform_count,
            puppet_index: node.puppet_index,
            opacity: node.opacity,
            color_rgba: node.color_rgba,
            stroke_color_rgba: node.stroke_color_rgba,
            stroke_width: node.stroke_width,
            corner_radius: node.corner_radius,
            fit: node.fit,
        });
    }
    Ok(nodes)
}
