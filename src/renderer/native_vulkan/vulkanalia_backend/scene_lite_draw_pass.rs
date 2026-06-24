use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeVulkanVulkanaliaSceneLiteDrawPassInput {
    pub(crate) plan_ready: bool,
    pub(crate) native_draw_ready: bool,
    pub(crate) draw_op_count: usize,
    pub(crate) backend_status: &'static str,
    pub(crate) blocking_reason: Option<&'static str>,
    pub(crate) fast_clear_color_ready: bool,
    pub(crate) quad_recording_ready: bool,
    pub(crate) quad_recording_step_count: usize,
    pub(crate) quad_vertex_buffer_bytes: u64,
    pub(crate) quad_index_buffer_bytes: u64,
    pub(crate) sampled_image_recording_ready: bool,
    pub(crate) sampled_image_op_count: usize,
    pub(crate) sampled_image_recording_step_count: usize,
    pub(crate) sampled_image_vertex_buffer_bytes: u64,
    pub(crate) sampled_image_index_buffer_bytes: u64,
    pub(crate) color_op_count: usize,
    pub(crate) vector_shape_op_count: usize,
    pub(crate) text_op_count: usize,
    pub(crate) path_op_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneLiteDrawPassSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub backend_ready: bool,
    pub backend_status: &'static str,
    pub blocking_reason: Option<&'static str>,
    pub draw_op_count: usize,
    pub color_op_count: usize,
    pub solid_quad_count: u32,
    pub sampled_image_quad_count: u32,
    pub vector_shape_op_count: usize,
    pub text_op_count: usize,
    pub path_op_count: usize,
    pub pipeline_count: u32,
    pub pipeline_labels: Vec<&'static str>,
    pub descriptor_set_count: u32,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
    pub vertex_stride_bytes: u32,
    pub index_type: &'static str,
    pub draw_indexed_count: u32,
    pub render_pass_compatibility: &'static str,
    pub render_model: &'static str,
    pub command_order: Vec<&'static str>,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub uses_vulkan_1_4_dynamic_rendering_local_read: bool,
    pub vulkan_1_4_dynamic_rendering_local_read_policy: &'static str,
    pub zero_copy_scope: &'static str,
    pub primary_reference: &'static str,
}

pub(crate) fn native_vulkan_vulkanalia_scene_lite_draw_pass_snapshot(
    input: NativeVulkanVulkanaliaSceneLiteDrawPassInput,
) -> NativeVulkanVulkanaliaSceneLiteDrawPassSnapshot {
    let solid_quad_ready = input.plan_ready
        && input.native_draw_ready
        && input.quad_recording_ready
        && input.quad_recording_step_count == input.draw_op_count
        && input.sampled_image_op_count == 0
        && input.text_op_count == 0
        && input.path_op_count == 0;
    let sampled_image_pending = input.plan_ready
        && input.native_draw_ready
        && input.sampled_image_recording_ready
        && input.sampled_image_recording_step_count == input.sampled_image_op_count
        && input.sampled_image_op_count == input.draw_op_count;

    let (backend_ready, backend_status, blocking_reason) = if solid_quad_ready {
        (true, "solid-quad-dynamic-rendering-recording-ready", None)
    } else if !input.plan_ready || !input.native_draw_ready {
        (
            false,
            "blocked-by-scene-lite-draw-plan",
            input
                .blocking_reason
                .or(Some("scene-lite-draw-plan-not-ready")),
        )
    } else if input.fast_clear_color_ready {
        (
            false,
            "delegated-to-vulkanalia-clear-present",
            Some("fast-clear-uses-clear-present-not-draw-pass"),
        )
    } else if sampled_image_pending {
        (
            false,
            "sampled-image-dynamic-rendering-recording-pending",
            Some("sampled-image-descriptor-upload-not-yet-wired"),
        )
    } else {
        (
            false,
            input.backend_status,
            input
                .blocking_reason
                .or(Some("vulkanalia-scene-lite-recording-not-ready")),
        )
    };

    let pipeline_labels = if solid_quad_ready {
        vec!["scene-lite-solid-quad-alpha-blend"]
    } else if sampled_image_pending {
        vec!["scene-lite-sampled-image-alpha-blend-pending"]
    } else {
        Vec::new()
    };
    let descriptor_set_count = if sampled_image_pending {
        saturating_u32(input.sampled_image_op_count)
    } else {
        0
    };
    let (vertex_buffer_bytes, index_buffer_bytes, vertex_stride_bytes) = if sampled_image_pending {
        (
            input.sampled_image_vertex_buffer_bytes,
            input.sampled_image_index_buffer_bytes,
            20,
        )
    } else {
        (
            input.quad_vertex_buffer_bytes,
            input.quad_index_buffer_bytes,
            24,
        )
    };

    NativeVulkanVulkanaliaSceneLiteDrawPassSnapshot {
        binding: "vulkanalia",
        route: "scene-lite-dynamic-rendering-draw-pass",
        backend_ready,
        backend_status,
        blocking_reason,
        draw_op_count: input.draw_op_count,
        color_op_count: input.color_op_count,
        solid_quad_count: saturating_u32(input.quad_recording_step_count),
        sampled_image_quad_count: saturating_u32(input.sampled_image_recording_step_count),
        vector_shape_op_count: input.vector_shape_op_count,
        text_op_count: input.text_op_count,
        path_op_count: input.path_op_count,
        pipeline_count: saturating_u32(pipeline_labels.len()),
        pipeline_labels,
        descriptor_set_count,
        vertex_buffer_bytes,
        index_buffer_bytes,
        vertex_stride_bytes,
        index_type: "uint32",
        draw_indexed_count: if solid_quad_ready {
            saturating_u32(input.quad_recording_step_count)
        } else {
            0
        },
        render_pass_compatibility: if solid_quad_ready || sampled_image_pending {
            "dynamic-rendering-no-render-pass"
        } else {
            "not-recordable-yet"
        },
        render_model: if solid_quad_ready {
            "scene-lite solid quad vertices -> Vulkan 1.3/1.4 dynamic rendering indexed draw -> Wayland swapchain"
        } else if sampled_image_pending {
            "scene-lite image quad vertices -> sampled image descriptor upload -> dynamic rendering indexed draw"
        } else {
            "scene-lite draw pass has not reached a vulkanalia-recordable backend"
        },
        command_order: native_vulkan_vulkanalia_scene_lite_draw_pass_command_order(
            solid_quad_ready,
            sampled_image_pending,
            input.fast_clear_color_ready,
        )
        .to_vec(),
        uses_pipeline_rendering_create_info: solid_quad_ready || sampled_image_pending,
        uses_dynamic_rendering: solid_quad_ready || sampled_image_pending,
        uses_synchronization2: solid_quad_ready || sampled_image_pending,
        uses_submit2: solid_quad_ready || sampled_image_pending,
        uses_vulkan_1_4_dynamic_rendering_local_read: false,
        vulkan_1_4_dynamic_rendering_local_read_policy: "not-required-for-single-pass-solid-quad; reserve-for-multipass-scene-local-read",
        zero_copy_scope: "scene-graph-geometry-to-swapchain; no decoded-video frame copy or fallback snapshot upload",
        primary_reference: "Vulkan dynamic rendering; FFmpeg remains first reference for video clock/queue discipline",
    }
}

fn native_vulkan_vulkanalia_scene_lite_draw_pass_command_order(
    solid_quad_ready: bool,
    sampled_image_pending: bool,
    fast_clear_color_ready: bool,
) -> &'static [&'static str] {
    if solid_quad_ready {
        &[
            "cmd_pipeline_barrier2_swapchain_attachment",
            "cmd_begin_rendering",
            "cmd_bind_scene_lite_solid_quad_pipeline",
            "cmd_bind_scene_lite_vertex_buffer",
            "cmd_bind_scene_lite_index_buffer",
            "cmd_draw_indexed_per_quad",
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
        ]
    } else if sampled_image_pending {
        &[
            "create_sampled_image_descriptors_pending",
            "cmd_begin_rendering_pending",
            "cmd_draw_indexed_sampled_image_quad_pending",
        ]
    } else if fast_clear_color_ready {
        &["delegate_to_vulkanalia_clear_present"]
    } else {
        &["wait_for_scene_lite_recordable_draw_ops"]
    }
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> NativeVulkanVulkanaliaSceneLiteDrawPassInput {
        NativeVulkanVulkanaliaSceneLiteDrawPassInput {
            plan_ready: true,
            native_draw_ready: true,
            draw_op_count: 1,
            backend_status: "solid-quad-recording-ready",
            blocking_reason: None,
            fast_clear_color_ready: false,
            quad_recording_ready: true,
            quad_recording_step_count: 1,
            quad_vertex_buffer_bytes: 96,
            quad_index_buffer_bytes: 24,
            sampled_image_recording_ready: false,
            sampled_image_op_count: 0,
            sampled_image_recording_step_count: 0,
            sampled_image_vertex_buffer_bytes: 0,
            sampled_image_index_buffer_bytes: 0,
            color_op_count: 0,
            vector_shape_op_count: 1,
            text_op_count: 0,
            path_op_count: 0,
        }
    }

    #[test]
    fn solid_quad_scene_lite_path_is_dynamic_rendering_recordable() {
        let snapshot = native_vulkan_vulkanalia_scene_lite_draw_pass_snapshot(input());

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "solid-quad-dynamic-rendering-recording-ready"
        );
        assert_eq!(
            snapshot.pipeline_labels,
            vec!["scene-lite-solid-quad-alpha-blend"]
        );
        assert_eq!(snapshot.vertex_buffer_bytes, 96);
        assert_eq!(snapshot.index_buffer_bytes, 24);
        assert_eq!(snapshot.vertex_stride_bytes, 24);
        assert_eq!(snapshot.draw_indexed_count, 1);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_pipeline_rendering_create_info);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert!(
            !snapshot.uses_vulkan_1_4_dynamic_rendering_local_read,
            "single-pass solid quads should not require local read"
        );
        assert!(snapshot.command_order.contains(&"cmd_begin_rendering"));
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_draw_indexed_per_quad")
        );
    }

    #[test]
    fn sampled_image_scene_lite_path_stays_explicitly_pending() {
        let mut input = input();
        input.draw_op_count = 1;
        input.backend_status = "sampled-image-quad-payload-ready-recording-pending";
        input.quad_recording_ready = false;
        input.quad_recording_step_count = 0;
        input.quad_vertex_buffer_bytes = 0;
        input.quad_index_buffer_bytes = 0;
        input.sampled_image_recording_ready = true;
        input.sampled_image_op_count = 1;
        input.sampled_image_recording_step_count = 1;
        input.sampled_image_vertex_buffer_bytes = 80;
        input.sampled_image_index_buffer_bytes = 24;
        input.vector_shape_op_count = 0;

        let snapshot = native_vulkan_vulkanalia_scene_lite_draw_pass_snapshot(input);

        assert!(!snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "sampled-image-dynamic-rendering-recording-pending"
        );
        assert_eq!(
            snapshot.blocking_reason,
            Some("sampled-image-descriptor-upload-not-yet-wired")
        );
        assert_eq!(
            snapshot.pipeline_labels,
            vec!["scene-lite-sampled-image-alpha-blend-pending"]
        );
        assert_eq!(snapshot.descriptor_set_count, 1);
        assert_eq!(snapshot.vertex_stride_bytes, 20);
        assert_eq!(snapshot.draw_indexed_count, 0);
    }
}
