use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;

#[derive(Debug)]
struct NativeVulkanSceneDrawPassBuild {
    draw_op_count: usize,
    plan_ready: bool,
    fast_clear_color: Option<String>,
    background_clear_color: Option<String>,
    clear_background_op_count: usize,
    color_op_count: usize,
    sampled_image_op_count: usize,
    video_op_count: usize,
    vector_shape_op_count: usize,
    text_op_count: usize,
    path_op_count: usize,
    effect_pass_count: usize,
    effect_pass_non_image_layer_count: usize,
    effect_pass_kind_counts: BTreeMap<&'static str, usize>,
    required_image_resources: Vec<PathBuf>,
    required_video_resources: Vec<PathBuf>,
    recordable_op_count: usize,
    recordable_quads: Vec<NativeVulkanSceneRecordableQuad>,
    quad_recording_payload: NativeVulkanSceneQuadRecordingPayload,
    quad_recording_ready: bool,
    recorded_path_geometry_count: usize,
    recorded_text_geometry_count: usize,
    sampled_image_quads: Vec<NativeVulkanSceneSampledImageQuad>,
    sampled_image_we_graph_plan: NativeVulkanSceneWeImageGraphPlan,
    sampled_image_recording_payload: NativeVulkanSceneSampledImageRecordingPayload,
    sampled_image_recording_ready: bool,
    sampled_image_visible_recording_ready: bool,
    sampled_image_implicit_full_extent_ready: bool,
    video_quads: Vec<NativeVulkanSceneVideoQuad>,
    video_recording_payload: NativeVulkanSceneVideoRecordingPayload,
    video_recording_ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeVulkanSceneDrawPassBackendRoute {
    FastClearColor,
    ClearBackgroundSolidQuadRecording,
    SolidQuadRecording,
    ClearBackgroundMixedQuadSampledImageRecording,
    MixedQuadSampledImageRecording,
    ClearBackgroundMixedQuadSampledImageImplicitFullExtent,
    MixedQuadSampledImageImplicitFullExtent,
    ClearBackgroundSampledImageImplicitFullExtent,
    SampledImageImplicitFullExtent,
    ClearBackgroundSampledImageRecording,
    ClearBackgroundVideoLayerBridge,
    MultiVideoLayerBridge,
    VideoLayerBridge,
    SampledImageRecording,
    BlockedUnsupportedSceneLayers,
    BlockedEmptyDrawPlan,
    PendingVideoLayerBridge,
    PartialSolidQuadRecording,
    PartialSampledImageQuadPayload,
    QuadPayloadRecordingPending,
    DrawRecordingPending,
}

#[derive(Debug, Default)]
struct NativeVulkanSceneDrawPassEffectInventory {
    pass_count: usize,
    non_image_layer_count: usize,
    kind_counts: BTreeMap<&'static str, usize>,
}

pub(in crate::renderer::native_vulkan::scene) fn native_vulkan_scene_draw_pass_plan(
    draw_plan: &NativeVulkanSceneDrawPlan,
) -> NativeVulkanSceneDrawPassPlan {
    NativeVulkanSceneDrawPassBuild::new(draw_plan).into_plan()
}

fn native_vulkan_scene_draw_pass_effect_inventory(
    draw_ops: &[NativeVulkanSceneDrawOp],
) -> NativeVulkanSceneDrawPassEffectInventory {
    let mut inventory = NativeVulkanSceneDrawPassEffectInventory::default();
    for op in draw_ops {
        if op.image_effect_passes.is_empty() {
            continue;
        }
        inventory.pass_count = inventory
            .pass_count
            .saturating_add(op.image_effect_passes.len());
        if op.kind != NativeVulkanSceneDrawOpKind::Image {
            inventory.non_image_layer_count = inventory.non_image_layer_count.saturating_add(1);
        }
        for effect in native_vulkan_scene_effect_passes_from_render_passes(&op.image_effect_passes)
        {
            *inventory
                .kind_counts
                .entry(effect.kind.as_str())
                .or_default() += 1;
        }
    }
    inventory
}

impl NativeVulkanSceneDrawPassBuild {
    fn new(draw_plan: &NativeVulkanSceneDrawPlan) -> Self {
        let op_buckets = native_vulkan_scene_draw_pass_op_buckets(&draw_plan.draw_ops);
        let color_op_count = op_buckets.color_op_count;
        let sampled_image_op_count = op_buckets.sampled_image_op_count;
        let video_op_count = op_buckets.video_op_count;
        let vector_shape_op_count = op_buckets.vector_shape_op_count;
        let text_op_count = op_buckets.text_op_count;
        let path_op_count = op_buckets.path_op_count;
        let effect_inventory = native_vulkan_scene_draw_pass_effect_inventory(&draw_plan.draw_ops);
        let required_image_resources = op_buckets.required_image_resources;
        let required_video_resources = op_buckets.required_video_resources;

        let fast_clear_color = native_vulkan_scene_fast_clear_color(&draw_plan.draw_ops);
        let background_clear_color =
            native_vulkan_scene_background_clear_color(&draw_plan.draw_ops);
        let clear_background_op_count = usize::from(background_clear_color.is_some());
        let recordable_quads = draw_plan
            .draw_ops
            .iter()
            .filter_map(native_vulkan_scene_recordable_quad)
            .collect::<Vec<_>>();
        let recordable_op_count = recordable_quads.len();
        let quad_recording_payload = native_vulkan_scene_quad_recording_payload(&recordable_quads);
        let recorded_path_geometry_count = quad_recording_payload
            .steps
            .iter()
            .filter(|step| step.kind == "path")
            .count();
        let recorded_text_geometry_count = quad_recording_payload
            .steps
            .iter()
            .filter(|step| step.kind == "text")
            .count();
        let quad_recording_ready = !quad_recording_payload.steps.is_empty()
            && quad_recording_payload
                .steps
                .len()
                .saturating_add(clear_background_op_count)
                == draw_plan.draw_ops.len();
        let sampled_image_quads = draw_plan
            .draw_ops
            .iter()
            .filter_map(native_vulkan_scene_sampled_image_quad)
            .collect::<Vec<_>>();
        let sampled_image_we_graph_plan =
            native_vulkan_scene_we_image_graph_plan(&sampled_image_quads);
        let sampled_image_recording_payload = native_vulkan_scene_sampled_image_recording_payload(
            &sampled_image_quads,
            (!draw_plan.dynamic_topology_required)
                .then_some(draw_plan.scene_size)
                .flatten(),
            &sampled_image_we_graph_plan,
        );
        let sampled_image_recording_ready = sampled_image_op_count > 0
            && sampled_image_recording_payload.recordable_quad_count == sampled_image_op_count;
        let sampled_image_visible_recording_ready =
            sampled_image_recording_ready && !sampled_image_recording_payload.steps.is_empty();
        let video_quads = draw_plan
            .draw_ops
            .iter()
            .filter_map(native_vulkan_scene_video_quad)
            .collect::<Vec<_>>();
        let video_recording_payload = native_vulkan_scene_video_recording_payload(&video_quads);
        let video_recording_ready =
            video_op_count > 0 && video_recording_payload.steps.len() == video_op_count;
        let full_extent_sampled_image_op_count =
            native_vulkan_scene_full_extent_sampled_image_op_count(&draw_plan.draw_ops);
        let sampled_image_implicit_full_extent_ready =
            full_extent_sampled_image_op_count == 1 && sampled_image_op_count == 1;

        Self {
            draw_op_count: draw_plan.draw_ops.len(),
            plan_ready: draw_plan.native_draw_ready(),
            fast_clear_color,
            background_clear_color,
            clear_background_op_count,
            color_op_count,
            sampled_image_op_count,
            video_op_count,
            vector_shape_op_count,
            text_op_count,
            path_op_count,
            effect_pass_count: effect_inventory.pass_count,
            effect_pass_non_image_layer_count: effect_inventory.non_image_layer_count,
            effect_pass_kind_counts: effect_inventory.kind_counts,
            required_image_resources,
            required_video_resources,
            recordable_op_count,
            recordable_quads,
            quad_recording_payload,
            quad_recording_ready,
            recorded_path_geometry_count,
            recorded_text_geometry_count,
            sampled_image_quads,
            sampled_image_we_graph_plan,
            sampled_image_recording_payload,
            sampled_image_recording_ready,
            sampled_image_visible_recording_ready,
            sampled_image_implicit_full_extent_ready,
            video_quads,
            video_recording_payload,
            video_recording_ready,
        }
    }

    fn into_plan(self) -> NativeVulkanSceneDrawPassPlan {
        let route = self.backend_route();
        let quad_vertex_buffer_bytes = native_vulkan_scene_solid_vertex_buffer_bytes(
            self.quad_recording_payload.vertices.len(),
        );
        let quad_index_buffer_bytes =
            native_vulkan_scene_solid_index_buffer_bytes(self.quad_recording_payload.indices.len());
        let sampled_image_vertex_buffer_bytes =
            native_vulkan_scene_sampled_image_vertex_buffer_bytes(
                self.sampled_image_recording_payload.vertices.len(),
            );
        let sampled_image_index_buffer_bytes = native_vulkan_scene_sampled_image_index_buffer_bytes(
            self.sampled_image_recording_payload.indices.len(),
        );
        let video_vertex_buffer_bytes = native_vulkan_scene_sampled_image_vertex_buffer_bytes(
            self.video_recording_payload.vertices.len(),
        );
        let video_index_buffer_bytes = native_vulkan_scene_sampled_image_index_buffer_bytes(
            self.video_recording_payload.indices.len(),
        );
        let requires_text_geometry = self.text_op_count > self.recorded_text_geometry_count;
        let requires_path_tessellation = self.path_op_count > self.recorded_path_geometry_count;

        NativeVulkanSceneDrawPassPlan {
            plan_ready: self.plan_ready,
            backend_ready: route.ready(),
            backend_status: route.status(),
            blocking_reason: route.blocking_reason(),
            recordable_op_count: self.recordable_op_count,
            recordable_quads: self.recordable_quads,
            quad_recording_ready: self.quad_recording_ready,
            quad_recording_steps: self.quad_recording_payload.steps,
            quad_vertices: self.quad_recording_payload.vertices,
            quad_indices: self.quad_recording_payload.indices,
            quad_vertex_buffer_bytes,
            quad_index_buffer_bytes,
            sampled_image_quads: self.sampled_image_quads,
            sampled_image_we_graph_plan: self.sampled_image_we_graph_plan,
            sampled_image_effect_targets: self.sampled_image_recording_payload.effect_targets,
            sampled_image_sources: self.sampled_image_recording_payload.sources,
            sampled_image_recording_ready: self.sampled_image_recording_ready,
            sampled_image_implicit_full_extent_ready: self.sampled_image_implicit_full_extent_ready,
            sampled_image_recording_steps: self.sampled_image_recording_payload.steps,
            sampled_image_vertices: self.sampled_image_recording_payload.vertices,
            sampled_image_indices: self.sampled_image_recording_payload.indices,
            sampled_image_vertex_buffer_bytes,
            sampled_image_index_buffer_bytes,
            video_quads: self.video_quads,
            video_sources: self.video_recording_payload.sources,
            video_recording_ready: self.video_recording_ready,
            video_recording_steps: self.video_recording_payload.steps,
            video_vertices: self.video_recording_payload.vertices,
            video_indices: self.video_recording_payload.indices,
            video_vertex_buffer_bytes,
            video_index_buffer_bytes,
            clear_background_op_count: self.clear_background_op_count,
            background_clear_color: self.background_clear_color,
            color_op_count: self.color_op_count,
            sampled_image_op_count: self.sampled_image_op_count,
            video_op_count: self.video_op_count,
            vector_shape_op_count: self.vector_shape_op_count,
            text_op_count: self.text_op_count,
            path_op_count: self.path_op_count,
            effect_pass_count: self.effect_pass_count,
            effect_pass_non_image_layer_count: self.effect_pass_non_image_layer_count,
            effect_pass_kind_counts: self.effect_pass_kind_counts,
            required_image_resources: self.required_image_resources,
            required_video_resources: self.required_video_resources,
            requires_text_geometry,
            requires_path_tessellation,
            requires_video_decode: self.video_op_count > 0,
            fast_clear_color: self.fast_clear_color,
        }
    }

    fn backend_route(&self) -> NativeVulkanSceneDrawPassBackendRoute {
        if self.draw_op_count == 0 {
            return NativeVulkanSceneDrawPassBackendRoute::BlockedEmptyDrawPlan;
        }
        if self.fast_clear_color.is_some() {
            return NativeVulkanSceneDrawPassBackendRoute::FastClearColor;
        }
        if self.quad_recording_ready && self.clear_background_op_count > 0 {
            return NativeVulkanSceneDrawPassBackendRoute::ClearBackgroundSolidQuadRecording;
        }
        if self.quad_recording_ready {
            return NativeVulkanSceneDrawPassBackendRoute::SolidQuadRecording;
        }
        if self.mixed_quad_sampled_image_recording_ready() && self.clear_background_op_count > 0 {
            return NativeVulkanSceneDrawPassBackendRoute::ClearBackgroundMixedQuadSampledImageRecording;
        }
        if self.mixed_quad_sampled_image_recording_ready() {
            return NativeVulkanSceneDrawPassBackendRoute::MixedQuadSampledImageRecording;
        }
        if self.mixed_quad_sampled_image_implicit_full_extent_ready()
            && self.clear_background_op_count > 0
        {
            return NativeVulkanSceneDrawPassBackendRoute::ClearBackgroundMixedQuadSampledImageImplicitFullExtent;
        }
        if self.mixed_quad_sampled_image_implicit_full_extent_ready() {
            return NativeVulkanSceneDrawPassBackendRoute::MixedQuadSampledImageImplicitFullExtent;
        }
        if self.sampled_image_implicit_full_extent_backend_ready()
            && self.clear_background_op_count > 0
        {
            return NativeVulkanSceneDrawPassBackendRoute::ClearBackgroundSampledImageImplicitFullExtent;
        }
        if self.sampled_image_implicit_full_extent_backend_ready() {
            return NativeVulkanSceneDrawPassBackendRoute::SampledImageImplicitFullExtent;
        }
        if self.sampled_image_recording_complete() && self.clear_background_op_count > 0 {
            return NativeVulkanSceneDrawPassBackendRoute::ClearBackgroundSampledImageRecording;
        }
        if self.clear_background_video_scene_bridge_ready() {
            return NativeVulkanSceneDrawPassBackendRoute::ClearBackgroundVideoLayerBridge;
        }
        if self.multi_video_scene_bridge_ready()
            || (self.mixed_video_scene_bridge_ready() && self.video_op_count > 1)
        {
            return NativeVulkanSceneDrawPassBackendRoute::MultiVideoLayerBridge;
        }
        if self.single_video_scene_bridge_ready() || self.mixed_video_scene_bridge_ready() {
            return NativeVulkanSceneDrawPassBackendRoute::VideoLayerBridge;
        }
        if self.sampled_image_recording_complete() {
            return NativeVulkanSceneDrawPassBackendRoute::SampledImageRecording;
        }
        if self.video_op_count > 0 {
            return NativeVulkanSceneDrawPassBackendRoute::PendingVideoLayerBridge;
        }
        if !self.plan_ready {
            return NativeVulkanSceneDrawPassBackendRoute::BlockedUnsupportedSceneLayers;
        }
        if !self.quad_recording_payload.steps.is_empty() {
            return NativeVulkanSceneDrawPassBackendRoute::PartialSolidQuadRecording;
        }
        if !self.sampled_image_recording_payload.steps.is_empty() {
            return NativeVulkanSceneDrawPassBackendRoute::PartialSampledImageQuadPayload;
        }
        if !self.recordable_quads.is_empty() {
            return NativeVulkanSceneDrawPassBackendRoute::QuadPayloadRecordingPending;
        }
        NativeVulkanSceneDrawPassBackendRoute::DrawRecordingPending
    }

    fn video_resource_ready(&self) -> bool {
        self.video_op_count > 0 && !self.required_video_resources.is_empty()
    }

    fn video_scene_layer_count(&self) -> usize {
        if self.video_op_count <= 1 {
            self.video_op_count
        } else {
            self.video_recording_payload.steps.len()
        }
    }

    fn single_video_scene_bridge_ready(&self) -> bool {
        self.video_op_count == 1 && self.video_resource_ready() && self.draw_op_count == 1
    }

    fn multi_video_scene_bridge_ready(&self) -> bool {
        self.video_op_count > 1
            && self.video_resource_ready()
            && self.video_recording_ready
            && self.draw_op_count == self.video_op_count
    }

    fn clear_background_video_scene_bridge_ready(&self) -> bool {
        self.video_op_count == 1
            && self.video_resource_ready()
            && self.clear_background_op_count == 1
            && self.draw_op_count == 2
    }

    fn sampled_image_recording_complete(&self) -> bool {
        self.sampled_image_visible_recording_ready
            && self
                .sampled_image_op_count
                .saturating_add(self.clear_background_op_count)
                == self.draw_op_count
    }

    fn sampled_image_implicit_full_extent_backend_ready(&self) -> bool {
        self.sampled_image_implicit_full_extent_ready
            && self
                .sampled_image_op_count
                .saturating_add(self.clear_background_op_count)
                == self.draw_op_count
    }

    fn mixed_quad_sampled_image_recording_ready(&self) -> bool {
        !self.quad_recording_payload.steps.is_empty()
            && self.sampled_image_visible_recording_ready
            && self
                .quad_recording_payload
                .steps
                .len()
                .saturating_add(self.sampled_image_recording_payload.recordable_quad_count)
                .saturating_add(self.clear_background_op_count)
                == self.draw_op_count
    }

    fn mixed_quad_sampled_image_implicit_full_extent_ready(&self) -> bool {
        !self.quad_recording_payload.steps.is_empty()
            && self.sampled_image_implicit_full_extent_ready
            && self
                .sampled_image_op_count
                .saturating_add(self.clear_background_op_count)
                .saturating_add(self.quad_recording_payload.steps.len())
                == self.draw_op_count
    }

    fn mixed_video_scene_bridge_ready(&self) -> bool {
        self.video_op_count > 0
            && self.video_resource_ready()
            && self.draw_op_count > 1
            && self
                .video_scene_layer_count()
                .saturating_add(self.clear_background_op_count)
                .saturating_add(self.quad_recording_payload.steps.len())
                .saturating_add(self.sampled_image_recording_payload.recordable_quad_count)
                == self.draw_op_count
    }
}

impl NativeVulkanSceneDrawPassBackendRoute {
    fn ready(self) -> bool {
        matches!(
            self,
            Self::FastClearColor
                | Self::ClearBackgroundSolidQuadRecording
                | Self::SolidQuadRecording
                | Self::ClearBackgroundMixedQuadSampledImageRecording
                | Self::MixedQuadSampledImageRecording
                | Self::ClearBackgroundMixedQuadSampledImageImplicitFullExtent
                | Self::MixedQuadSampledImageImplicitFullExtent
                | Self::ClearBackgroundSampledImageImplicitFullExtent
                | Self::SampledImageImplicitFullExtent
                | Self::ClearBackgroundSampledImageRecording
                | Self::ClearBackgroundVideoLayerBridge
                | Self::MultiVideoLayerBridge
                | Self::VideoLayerBridge
                | Self::SampledImageRecording
        )
    }

    fn status(self) -> &'static str {
        match self {
            Self::FastClearColor => "fast-clear-color-ready",
            Self::ClearBackgroundSolidQuadRecording => {
                "clear-background-solid-quad-recording-ready"
            }
            Self::SolidQuadRecording => "solid-quad-recording-ready",
            Self::ClearBackgroundMixedQuadSampledImageRecording => {
                "clear-background-mixed-quad-sampled-image-recording-ready"
            }
            Self::MixedQuadSampledImageRecording => "mixed-quad-sampled-image-recording-ready",
            Self::ClearBackgroundMixedQuadSampledImageImplicitFullExtent => {
                "clear-background-mixed-quad-sampled-image-implicit-full-extent-ready"
            }
            Self::MixedQuadSampledImageImplicitFullExtent => {
                "mixed-quad-sampled-image-implicit-full-extent-ready"
            }
            Self::ClearBackgroundSampledImageImplicitFullExtent => {
                "clear-background-sampled-image-implicit-full-extent-ready"
            }
            Self::SampledImageImplicitFullExtent => "sampled-image-implicit-full-extent-ready",
            Self::ClearBackgroundSampledImageRecording => {
                "clear-background-sampled-image-recording-ready"
            }
            Self::ClearBackgroundVideoLayerBridge => {
                "clear-background-video-layer-vulkan-video-scene-bridge-ready"
            }
            Self::MultiVideoLayerBridge => "multi-video-layer-vulkan-video-scene-bridge-ready",
            Self::VideoLayerBridge => "video-layer-vulkan-video-scene-bridge-ready",
            Self::SampledImageRecording => "sampled-image-recording-ready",
            Self::BlockedUnsupportedSceneLayers => "blocked-by-unsupported-scene-layers",
            Self::BlockedEmptyDrawPlan => "blocked-empty-scene-draw-plan",
            Self::PendingVideoLayerBridge => "video-layer-vulkan-video-scene-bridge-pending",
            Self::PartialSolidQuadRecording => "partial-solid-quad-recording-ready",
            Self::PartialSampledImageQuadPayload => "partial-sampled-image-quad-payload-ready",
            Self::QuadPayloadRecordingPending => "quad-payload-ready-recording-pending",
            Self::DrawRecordingPending => "draw-pass-plan-ready-recording-pending",
        }
    }

    fn blocking_reason(self) -> Option<&'static str> {
        match self {
            Self::BlockedUnsupportedSceneLayers => Some("unsupported-scene-layers"),
            Self::BlockedEmptyDrawPlan => Some("empty-draw-plan"),
            Self::PendingVideoLayerBridge => Some("video-layer-needs-vulkan-video-scene-bridge"),
            Self::PartialSolidQuadRecording => Some("non-quad-draw-ops-need-recording-backend"),
            Self::PartialSampledImageQuadPayload => {
                Some("non-image-quad-draw-ops-need-recording-backend")
            }
            Self::QuadPayloadRecordingPending => Some("vulkan-quad-recording-not-implemented"),
            Self::DrawRecordingPending => Some("vulkan-draw-recording-not-implemented"),
            _ => None,
        }
    }
}
