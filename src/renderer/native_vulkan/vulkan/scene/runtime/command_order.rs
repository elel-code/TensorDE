//! Scene Vulkan runtime command order labels used by smoke snapshots.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`

pub(in crate::renderer::native_vulkan) fn scene_command_order(
    no_sampled_slots: bool,
    skinning_buffer_enabled: bool,
    effect_targets_enabled: bool,
    effect_target_copy_enabled: bool,
    effect_target_swap_enabled: bool,
    effect_target_mesh_draw_enabled: bool,
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
        order.extend([
            "create_descriptor_heap_sampler_buffer",
            "upload_scene_fallback_texture",
        ]);
    }
    if effect_targets_enabled {
        order.push("create_scene_effect_target_images");
    }
    order.push("write_descriptor_heap_uniform_buffer_descriptors");
    if skinning_buffer_enabled {
        order.push("write_descriptor_heap_skinning_storage_buffer_descriptors");
    }
    order.push("write_descriptor_heap_sampled_image_descriptors");
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
        "cmd_bind_scene_mesh_pipeline",
        "cmd_bind_scene_mesh_vertex_index_buffers",
        "cmd_draw_indexed_scene_meshes",
        "cmd_end_rendering",
        "queue_submit2",
        "queue_present_khr",
    ]);
    order
}
