//! Scene Vulkan runtime command order labels used by smoke snapshots.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`

pub(in crate::renderer::native_vulkan) fn scene_command_order(
    no_sampled_slots: bool,
    input_attachment_slots_enabled: bool,
    fallback_texture_enabled: bool,
    scene_textures_enabled: bool,
    skinning_buffer_enabled: bool,
    pipeline_variant_enabled: bool,
    dynamic_effect_uniforms_enabled: bool,
    effect_targets_enabled: bool,
    effect_target_copy_enabled: bool,
    effect_target_swap_enabled: bool,
    effect_target_mesh_draw_enabled: bool,
    effect_target_fullscreen_draw_enabled: bool,
) -> Vec<&'static str> {
    let mut order = vec![
        "create_scene_vertex_buffer",
        "create_scene_index_buffer",
        "create_scene_uniform_buffers",
        "create_descriptor_heap_resource_buffer",
    ];
    if skinning_buffer_enabled {
        order.push("create_scene_skinning_storage_buffer");
    }
    if !no_sampled_slots {
        order.push("create_descriptor_heap_sampler_buffer");
    }
    if fallback_texture_enabled {
        order.push("upload_scene_fallback_texture");
    }
    if scene_textures_enabled {
        order.extend([
            "upload_scene_material_textures",
            "release_scene_texture_staging_after_setup_submit",
        ]);
    }
    if effect_targets_enabled {
        order.push("create_scene_effect_target_images");
    }
    order.push("write_descriptor_heap_uniform_buffer_descriptors");
    if skinning_buffer_enabled {
        order.push("write_descriptor_heap_skinning_storage_buffer_descriptors");
    }
    if !no_sampled_slots {
        order.push("write_descriptor_heap_sampled_image_descriptors");
    }
    if input_attachment_slots_enabled {
        order.push("write_descriptor_heap_input_attachment_descriptors");
    }
    order.push("update_scene_transform_uniforms_per_frame");
    if dynamic_effect_uniforms_enabled {
        order.push("update_scene_effect_uniforms_per_frame");
    }
    if skinning_buffer_enabled {
        order.push("update_scene_skinning_storage_per_frame");
    }
    if effect_targets_enabled {
        order.extend([
            "cmd_effect_target_barriers",
            "cmd_begin_effect_target_rendering",
            "cmd_end_effect_target_rendering",
            "cmd_restore_effect_target_shader_read",
        ]);
    }
    if effect_target_mesh_draw_enabled {
        order.push("cmd_draw_indexed_effect_target_meshes");
    }
    if effect_target_fullscreen_draw_enabled {
        order.push("cmd_draw_effect_target_fullscreen_triangle");
    }
    if effect_target_copy_enabled {
        order.push("cmd_copy_effect_target_image");
    }
    if effect_target_swap_enabled {
        order.push("rewrite_effect_target_logical_references");
    }
    order.extend([
        "cmd_begin_rendering",
        "cmd_bind_resource_heap_ext",
        "cmd_bind_sampler_heap_ext_when_sampled_slots_exist",
        "cmd_bind_scene_pass_pipeline",
        "cmd_bind_scene_mesh_vertex_index_buffers",
        "cmd_draw_indexed_scene_meshes",
        "cmd_end_rendering",
        "queue_submit2",
        "queue_present_khr",
    ]);
    if pipeline_variant_enabled {
        order.push("scene_pipeline_variant_selection_per_draw");
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_order_reports_per_draw_pipeline_variant_selection() {
        let order = scene_command_order(
            false, false, true, true, true, true, true, false, false, false, false, false,
        );

        assert!(order.contains(&"cmd_bind_scene_pass_pipeline"));
        assert!(order.contains(&"scene_pipeline_variant_selection_per_draw"));
        assert!(order.contains(&"update_scene_transform_uniforms_per_frame"));
        assert!(order.contains(&"update_scene_effect_uniforms_per_frame"));
        assert!(order.contains(&"update_scene_skinning_storage_per_frame"));
        assert!(order.contains(&"upload_scene_material_textures"));
        assert!(!order.contains(&"write_descriptor_heap_input_attachment_descriptors"));
    }
}
