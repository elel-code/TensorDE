//! Scene Vulkan runtime command order labels used by smoke snapshots.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `references/gilder/godot/servers/rendering/rendering_device_graph.*`

#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct SceneCommandOrderFacts {
    pub no_sampled_slots: bool,
    pub input_attachment_slots_enabled: bool,
    pub fallback_texture_enabled: bool,
    pub scene_textures_enabled: bool,
    pub skinning_buffer_enabled: bool,
    pub pipeline_variant_enabled: bool,
    pub dynamic_effect_uniforms_enabled: bool,
    pub effect_targets_enabled: bool,
    pub effect_target_copy_enabled: bool,
    pub effect_target_swap_enabled: bool,
    pub effect_target_mesh_draw_enabled: bool,
    pub effect_target_fullscreen_draw_enabled: bool,
}

pub(in crate::renderer::native_vulkan) fn scene_command_order(
    facts: SceneCommandOrderFacts,
) -> Vec<&'static str> {
    let mut order = vec![
        "create_live_physical_offscreen_scene_color_per_frame_slot",
        "create_terminal_present_descriptor_heaps_per_frame_slot",
        "create_native_slang_o2_terminal_present_pipeline",
        "create_scene_vertex_buffer",
        "create_scene_index_buffer",
        "create_scene_uniform_buffers",
        "create_descriptor_heap_resource_buffer",
    ];
    if facts.skinning_buffer_enabled {
        order.push("create_scene_skinning_storage_buffer");
    }
    if !facts.no_sampled_slots {
        order.push("create_descriptor_heap_sampler_buffer");
    }
    if facts.fallback_texture_enabled {
        order.push("upload_scene_fallback_texture");
    }
    if facts.scene_textures_enabled {
        order.extend([
            "upload_scene_material_textures",
            "release_scene_texture_staging_after_setup_submit",
        ]);
    }
    if facts.effect_targets_enabled {
        order.push("create_scene_effect_target_images");
    }
    order.push("write_descriptor_heap_uniform_buffer_descriptors");
    if facts.skinning_buffer_enabled {
        order.push("write_descriptor_heap_skinning_storage_buffer_descriptors");
    }
    if !facts.no_sampled_slots {
        order.push("write_descriptor_heap_sampled_image_descriptors");
    }
    if facts.input_attachment_slots_enabled {
        order.push("write_descriptor_heap_input_attachment_descriptors");
    }
    order.push("update_scene_transform_uniforms_per_frame");
    if facts.dynamic_effect_uniforms_enabled {
        order.push("update_scene_effect_uniforms_per_frame");
    }
    if facts.skinning_buffer_enabled {
        order.push("update_scene_skinning_storage_per_frame");
    }
    if facts.effect_targets_enabled {
        order.extend([
            "cmd_effect_target_barriers",
            "cmd_begin_effect_target_rendering",
            "cmd_end_effect_target_rendering",
            "cmd_restore_effect_target_shader_read",
        ]);
    }
    if facts.effect_target_mesh_draw_enabled {
        order.push("cmd_draw_indexed_effect_target_meshes");
    }
    if facts.effect_target_fullscreen_draw_enabled {
        order.push("cmd_draw_effect_target_fullscreen_triangle");
    }
    if facts.effect_target_copy_enabled {
        order.push("cmd_copy_effect_target_image");
    }
    if facts.effect_target_swap_enabled {
        order.push("rewrite_effect_target_logical_references");
    }
    order.extend([
        "cmd_begin_offscreen_scene_color_rendering",
        "cmd_bind_resource_heap_ext",
        "cmd_bind_sampler_heap_ext_when_sampled_slots_exist",
        "cmd_bind_scene_pass_pipeline",
        "cmd_bind_scene_mesh_vertex_index_buffers",
        "cmd_draw_indexed_scene_meshes",
        "cmd_end_offscreen_scene_color_rendering",
        "cmd_transition_scene_color_to_sampled",
        "cmd_transition_swapchain_to_color_attachment",
        "cmd_begin_terminal_present_rendering",
        "cmd_bind_terminal_present_descriptor_heaps_ext",
        "cmd_bind_native_slang_o2_terminal_present_pipeline",
        "cmd_draw_terminal_present_fullscreen_triangle",
        "cmd_end_terminal_present_rendering",
        "cmd_transition_swapchain_to_present",
        "queue_submit2",
        "queue_present_khr",
    ]);
    if facts.pipeline_variant_enabled {
        order.push("scene_pipeline_variant_selection_per_draw");
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_order_reports_per_draw_pipeline_variant_selection() {
        let order = scene_command_order(SceneCommandOrderFacts {
            no_sampled_slots: false,
            input_attachment_slots_enabled: false,
            fallback_texture_enabled: true,
            scene_textures_enabled: true,
            skinning_buffer_enabled: true,
            pipeline_variant_enabled: true,
            dynamic_effect_uniforms_enabled: true,
            effect_targets_enabled: false,
            effect_target_copy_enabled: false,
            effect_target_swap_enabled: false,
            effect_target_mesh_draw_enabled: false,
            effect_target_fullscreen_draw_enabled: false,
        });

        assert!(order.contains(&"cmd_bind_scene_pass_pipeline"));
        assert!(order.contains(&"scene_pipeline_variant_selection_per_draw"));
        assert!(order.contains(&"update_scene_transform_uniforms_per_frame"));
        assert!(order.contains(&"update_scene_effect_uniforms_per_frame"));
        assert!(order.contains(&"update_scene_skinning_storage_per_frame"));
        assert!(order.contains(&"upload_scene_material_textures"));
        assert!(!order.contains(&"write_descriptor_heap_input_attachment_descriptors"));
    }

    #[test]
    fn command_order_keeps_scene_color_offscreen_until_terminal_present() {
        let order = scene_command_order(SceneCommandOrderFacts {
            no_sampled_slots: false,
            input_attachment_slots_enabled: false,
            fallback_texture_enabled: false,
            scene_textures_enabled: true,
            skinning_buffer_enabled: false,
            pipeline_variant_enabled: false,
            dynamic_effect_uniforms_enabled: false,
            effect_targets_enabled: false,
            effect_target_copy_enabled: false,
            effect_target_swap_enabled: false,
            effect_target_mesh_draw_enabled: false,
            effect_target_fullscreen_draw_enabled: false,
        });
        let position = |label| {
            order
                .iter()
                .position(|entry| *entry == label)
                .unwrap_or_else(|| panic!("missing command label {label}"))
        };

        assert!(
            position("create_live_physical_offscreen_scene_color_per_frame_slot")
                < position("create_native_slang_o2_terminal_present_pipeline")
        );
        assert!(
            position("cmd_draw_indexed_scene_meshes")
                < position("cmd_transition_scene_color_to_sampled")
        );
        assert!(
            position("cmd_transition_scene_color_to_sampled")
                < position("cmd_draw_terminal_present_fullscreen_triangle")
        );
        assert!(
            position("cmd_draw_terminal_present_fullscreen_triangle")
                < position("cmd_transition_swapchain_to_present")
        );
    }
}
