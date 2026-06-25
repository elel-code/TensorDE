use std::path::PathBuf;

use serde::Serialize;

const SCENE_LITE_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES: u32 = 20;
const SCENE_LITE_SAMPLED_IMAGE_VERTEX_COUNT: usize = 4;
const SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaSceneLiteSampledImagePlanInput {
    pub sampled_image_sources: Vec<PathBuf>,
    pub recording_step_count: usize,
    pub vertex_count: usize,
    pub index_count: usize,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneLiteSampledImagePlanSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub backend_ready: bool,
    pub backend_status: &'static str,
    pub blocking_reason: Option<&'static str>,
    pub sampled_image_count: usize,
    pub resource_count: usize,
    pub sampled_image_sources: Vec<PathBuf>,
    pub recording_step_count: usize,
    pub vertex_count: usize,
    pub index_count: usize,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
    pub vertex_stride_bytes: u32,
    pub descriptor_set_count: u32,
    pub descriptor_type: &'static str,
    pub descriptor_pool_combined_image_sampler_budget: u32,
    pub sampled_image_format: &'static str,
    pub sampled_image_usage: Vec<&'static str>,
    pub staging_buffer_usage: Vec<&'static str>,
    pub image_layout_flow: Vec<&'static str>,
    pub upload_model: &'static str,
    pub descriptor_model: &'static str,
    pub pipeline_label: &'static str,
    pub draw_indexed_count: u32,
    pub command_order: Vec<&'static str>,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub uses_push_descriptor_fast_path: bool,
    pub vulkan_1_4_push_descriptor_policy: &'static str,
    pub zero_copy_scope: &'static str,
    pub primary_reference: &'static str,
}

pub(crate) fn native_vulkan_vulkanalia_scene_lite_sampled_image_plan(
    input: NativeVulkanVulkanaliaSceneLiteSampledImagePlanInput,
) -> NativeVulkanVulkanaliaSceneLiteSampledImagePlanSnapshot {
    let sampled_image_count = input.sampled_image_sources.len();
    let expected_vertex_count =
        sampled_image_count.saturating_mul(SCENE_LITE_SAMPLED_IMAGE_VERTEX_COUNT);
    let expected_index_count =
        sampled_image_count.saturating_mul(SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT);
    let backend_ready = sampled_image_count > 0
        && input.recording_step_count == sampled_image_count
        && input.vertex_count == expected_vertex_count
        && input.index_count == expected_index_count
        && input.vertex_buffer_bytes > 0
        && input.index_buffer_bytes > 0;
    let (backend_status, blocking_reason) = if backend_ready {
        ("sampled-image-dynamic-rendering-recording-ready", None)
    } else if sampled_image_count == 0 {
        ("no-sampled-image-quads", Some("no-sampled-image-quads"))
    } else {
        (
            "sampled-image-geometry-incomplete",
            Some("sampled-image-geometry-payload-incomplete"),
        )
    };
    let descriptor_budget = saturating_nonzero_u32(sampled_image_count);

    NativeVulkanVulkanaliaSceneLiteSampledImagePlanSnapshot {
        binding: "vulkanalia",
        route: "scene-lite-sampled-image-upload-descriptor-plan",
        backend_ready,
        backend_status,
        blocking_reason,
        sampled_image_count,
        resource_count: sampled_image_count,
        sampled_image_sources: input.sampled_image_sources,
        recording_step_count: input.recording_step_count,
        vertex_count: input.vertex_count,
        index_count: input.index_count,
        vertex_buffer_bytes: input.vertex_buffer_bytes,
        index_buffer_bytes: input.index_buffer_bytes,
        vertex_stride_bytes: SCENE_LITE_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES,
        descriptor_set_count: if backend_ready { descriptor_budget } else { 0 },
        descriptor_type: "combined-image-sampler",
        descriptor_pool_combined_image_sampler_budget: descriptor_budget,
        sampled_image_format: "R8G8B8A8_UNORM",
        sampled_image_usage: vec!["transfer-dst", "sampled"],
        staging_buffer_usage: vec!["transfer-src"],
        image_layout_flow: vec![
            "undefined",
            "transfer-dst-optimal",
            "shader-read-only-optimal",
        ],
        upload_model: "decode source image to RGBA once, upload into retained sampled image, reuse descriptor across present frames",
        descriptor_model: "one combined-image-sampler descriptor per sampled image resource; descriptor-set path first, push-descriptor fast path reserved",
        pipeline_label: "scene-lite-sampled-image-alpha-blend",
        draw_indexed_count: if backend_ready { descriptor_budget } else { 0 },
        command_order: scene_lite_sampled_image_command_order(backend_ready).to_vec(),
        uses_pipeline_rendering_create_info: backend_ready,
        uses_dynamic_rendering: backend_ready,
        uses_synchronization2: backend_ready,
        uses_submit2: backend_ready,
        uses_push_descriptor_fast_path: false,
        vulkan_1_4_push_descriptor_policy: "descriptor sets are the stable first path; use Vulkan 1.4 push_descriptor later to reduce descriptor churn when available",
        zero_copy_scope: "source image pixels upload once; present frames sample retained GPU image directly into the swapchain",
        primary_reference: "FFmpeg frame/descriptor lifetime discipline; Vulkan dynamic rendering and sync2 command ordering",
    }
}

fn scene_lite_sampled_image_command_order(backend_ready: bool) -> &'static [&'static str] {
    if backend_ready {
        &[
            "decode_source_image_rgba",
            "create_sampled_image_transfer_dst_sampled",
            "create_rgba_upload_staging_buffer",
            "cmd_pipeline_barrier2_transfer_dst",
            "cmd_copy_buffer_to_image",
            "cmd_pipeline_barrier2_shader_read",
            "create_combined_image_sampler_descriptor",
            "cmd_begin_rendering",
            "cmd_bind_scene_lite_sampled_image_pipeline",
            "cmd_bind_sampled_image_vertex_buffer",
            "cmd_bind_sampled_image_index_buffer",
            "cmd_bind_sampled_image_descriptor_set",
            "cmd_draw_indexed_per_image_quad",
            "cmd_end_rendering",
            "queue_submit2_present",
        ]
    } else {
        &["wait_for_scene_lite_sampled_image_geometry"]
    }
}

fn saturating_nonzero_u32(value: usize) -> u32 {
    u32::try_from(value.max(1)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampled_image_plan_marks_descriptor_upload_shape_ready() {
        let snapshot = native_vulkan_vulkanalia_scene_lite_sampled_image_plan(
            NativeVulkanVulkanaliaSceneLiteSampledImagePlanInput {
                sampled_image_sources: vec![PathBuf::from("/tmp/hero.png")],
                recording_step_count: 1,
                vertex_count: 4,
                index_count: 6,
                vertex_buffer_bytes: 80,
                index_buffer_bytes: 24,
            },
        );

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "sampled-image-dynamic-rendering-recording-ready"
        );
        assert_eq!(snapshot.blocking_reason, None);
        assert_eq!(snapshot.descriptor_set_count, 1);
        assert_eq!(snapshot.descriptor_type, "combined-image-sampler");
        assert_eq!(snapshot.sampled_image_format, "R8G8B8A8_UNORM");
        assert_eq!(
            snapshot.sampled_image_usage,
            vec!["transfer-dst", "sampled"]
        );
        assert_eq!(
            snapshot.image_layout_flow,
            vec![
                "undefined",
                "transfer-dst-optimal",
                "shader-read-only-optimal"
            ]
        );
        assert!(snapshot.command_order.contains(&"cmd_copy_buffer_to_image"));
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_bind_sampled_image_descriptor_set")
        );
        assert_eq!(snapshot.draw_indexed_count, 1);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert!(!snapshot.uses_push_descriptor_fast_path);
    }

    #[test]
    fn sampled_image_plan_rejects_incomplete_geometry() {
        let snapshot = native_vulkan_vulkanalia_scene_lite_sampled_image_plan(
            NativeVulkanVulkanaliaSceneLiteSampledImagePlanInput {
                sampled_image_sources: vec![PathBuf::from("/tmp/hero.png")],
                recording_step_count: 1,
                vertex_count: 3,
                index_count: 6,
                vertex_buffer_bytes: 60,
                index_buffer_bytes: 24,
            },
        );

        assert!(!snapshot.backend_ready);
        assert_eq!(snapshot.backend_status, "sampled-image-geometry-incomplete");
        assert_eq!(
            snapshot.blocking_reason,
            Some("sampled-image-geometry-payload-incomplete")
        );
        assert_eq!(snapshot.descriptor_set_count, 0);
        assert_eq!(
            snapshot.command_order,
            vec!["wait_for_scene_lite_sampled_image_geometry"]
        );
    }
}
