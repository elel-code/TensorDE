use serde::Serialize;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::AtomicUsize;

use crate::core::scene::{
    SceneEffectFbo, SceneLayerCompositeKey, SceneMesh, ScenePuppetAnimationFrameDebug,
    SceneSnapshotLayer, SceneSnapshotSampledImageLayer, SceneTextureSlot,
};
use crate::core::{
    FitMode, SceneBlendMode, SceneNodeKind, ScenePathFillRule, SceneSize, SceneSystemStatus,
    SceneTextAlign, SceneTextureRegion, SceneTransform,
};
use crate::renderer::native_vulkan::effect_debug::{
    NativeVulkanEffectDebugR8UvGroup, NativeVulkanEffectDebugRgbaUvGroup,
    native_vulkan_effect_debug_bc7_mode6_gtex_group_report, native_vulkan_effect_debug_enabled,
    native_vulkan_effect_debug_log, native_vulkan_effect_debug_log_limited,
    native_vulkan_effect_debug_r8_gtex_group_report,
    native_vulkan_effect_debug_read_bc7_mode6_gtex_cached,
    native_vulkan_effect_debug_read_r8_gtex_cached,
};
use crate::renderer::{SceneRenderAlphaTextureMode, SceneRenderLayer};

use super::super::present::render_item::NativeVulkanRenderItem;
use super::super::present::render_plan::{
    NativeVulkanSceneDrawPlan, NativeVulkanSceneEffectUvMapping, NativeVulkanSceneEffectUvSpace,
    native_vulkan_scene_draw_plan, native_vulkan_scene_effect_uv_space_from_transform,
    native_vulkan_scene_effect_uv_transform_for_scene_passes,
};
use super::super::vulkan::{
    NativeVulkanVulkanaliaSceneBlendEquation, NativeVulkanVulkanaliaSceneBlendState,
    NativeVulkanVulkanaliaSceneCullMode, NativeVulkanVulkanaliaSceneDrawPassInput,
    NativeVulkanVulkanaliaSceneDrawPassSnapshot, NativeVulkanVulkanaliaSceneEffectKind,
    NativeVulkanVulkanaliaSceneMaterialFlag, NativeVulkanVulkanaliaSceneRenderState,
    NativeVulkanVulkanaliaSceneSampledImageDrawStep,
    NativeVulkanVulkanaliaSceneSampledImageEffectTarget,
    NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
    NativeVulkanVulkanaliaSceneSampledImageMaterial,
    NativeVulkanVulkanaliaSceneSampledImageMaterialKind,
    NativeVulkanVulkanaliaSceneSampledImagePlanInput,
    NativeVulkanVulkanaliaSceneSampledImagePlanSnapshot,
    NativeVulkanVulkanaliaSceneSampledImageRenderTarget,
    NativeVulkanVulkanaliaSceneSampledImageVertex, NativeVulkanVulkanaliaSceneSolidQuadDrawStep,
    NativeVulkanVulkanaliaSceneSolidQuadGeometryInput, NativeVulkanVulkanaliaSceneSolidQuadVertex,
    NativeVulkanVulkanaliaSceneTextureSlotResourceBinding,
    NativeVulkanVulkanaliaSceneVideoLayerDrawStep,
    NativeVulkanVulkanaliaSceneVideoLayerGeometryInput,
    NativeVulkanVulkanaliaSceneWeImageGraphResource,
    native_vulkan_vulkanalia_scene_draw_pass_snapshot,
    native_vulkan_vulkanalia_scene_sampled_image_plan,
    native_vulkan_vulkanalia_take_scene_sampled_image_vertex_vec,
};
use super::binary_ingest::{
    NativeVulkanSceneBinaryIngestSummary, native_vulkan_scene_binary_ingest_from_reader,
};
use super::draw_pass::{
    NativeVulkanSceneBlendState, NativeVulkanSceneEffectRecord, NativeVulkanSceneMaterialPass,
    NativeVulkanSceneRenderState, NativeVulkanSceneSampledImageEffectTarget,
    NativeVulkanSceneSampledImageRenderTarget, NativeVulkanSceneSampledImageVertex,
    NativeVulkanSceneTextureSlot, NativeVulkanSceneTextureSlotResourceBinding,
    NativeVulkanSceneWeImageGraphPlan, NativeVulkanSceneWeImageGraphTarget,
    NativeVulkanSceneWeImageGraphTextureBinding, NativeVulkanSceneWeImagePassChain,
    native_vulkan_scene_append_sampled_image_geometry_from_render_layer,
    native_vulkan_scene_append_sampled_image_geometry_from_snapshot_layer,
    native_vulkan_scene_append_sampled_image_vertices_from_sampled_layer_with_effect_chain,
    native_vulkan_scene_append_sampled_image_vertices_from_sampled_layer_with_effect_uv_space,
    native_vulkan_scene_append_sampled_image_vertices_from_snapshot_layer,
    native_vulkan_scene_draw_pass_plan, native_vulkan_scene_effect_passes_from_render_passes,
    native_vulkan_scene_render_layer_is_clear,
    native_vulkan_scene_solid_geometry_from_render_layer, native_vulkan_scene_we_image_pass_chain,
};

const SCENE_RUNTIME_SAMPLED_VERTEX_POOL_MAX_RETAINED: usize = 3;
const SCENE_RUNTIME_SAMPLED_VERTEX_POOL_MAX_CAPACITY: usize = 128 * 1024;
const SCENE_RUNTIME_EYE_DARK_LUMA_THRESHOLD: f64 = 48.0;
const SCENE_RUNTIME_EYE_VISIBLE_ALPHA_THRESHOLD: f64 = 16.0;
const SCENE_RUNTIME_EYE_ALIGNMENT_EPSILON_PX: f64 = 1.0;
static SCENE_RUNTIME_EFFECT_DEBUG_LOG_COUNT: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static SCENE_RUNTIME_SAMPLED_VERTEX_POOL:
        RefCell<Vec<Vec<NativeVulkanSceneSampledImageVertex>>> = RefCell::new(Vec::new());
}

fn native_vulkan_scene_take_sampled_vertex_vec(
    capacity: usize,
) -> Vec<NativeVulkanSceneSampledImageVertex> {
    SCENE_RUNTIME_SAMPLED_VERTEX_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        let mut vertices = pool
            .iter()
            .position(|vertices| vertices.capacity() >= capacity)
            .map(|index| pool.swap_remove(index))
            .unwrap_or_else(|| Vec::with_capacity(capacity));
        vertices.clear();
        vertices
    })
}

fn native_vulkan_scene_recycle_sampled_vertex_vec(
    mut vertices: Vec<NativeVulkanSceneSampledImageVertex>,
) {
    if vertices.capacity() > SCENE_RUNTIME_SAMPLED_VERTEX_POOL_MAX_CAPACITY {
        return;
    }
    vertices.clear();
    SCENE_RUNTIME_SAMPLED_VERTEX_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < SCENE_RUNTIME_SAMPLED_VERTEX_POOL_MAX_RETAINED {
            pool.push(vertices);
        }
    });
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneRuntimeSnapshot {
    pub snapshot_time_ms: u64,
    pub scene_size: Option<SceneSize>,
    pub scene_fit: FitMode,
    pub full_scene: NativeVulkanFullSceneRuntimeSnapshot,
    pub scene_input_model: &'static str,
    pub scene_resource_model: &'static str,
    pub scene_binary_ingest: Option<NativeVulkanSceneBinaryIngestRuntimeSnapshot>,
    pub native_draw_ready: bool,
    pub runtime_display_available: bool,
    pub draw_pass_plan_ready: bool,
    pub draw_pass_backend_ready: bool,
    pub draw_pass_backend_status: &'static str,
    pub draw_pass_blocking_reason: Option<&'static str>,
    pub draw_pass_recordable_op_count: usize,
    pub draw_pass_recordable_quads: Vec<NativeVulkanSceneRecordableQuadSnapshot>,
    pub draw_pass_quad_recording_ready: bool,
    pub draw_pass_quad_recording_step_count: usize,
    pub draw_pass_quad_recording_steps: Vec<NativeVulkanSceneQuadRecordingStepSnapshot>,
    pub draw_pass_quad_vertices: Vec<NativeVulkanSceneQuadVertexSnapshot>,
    pub draw_pass_quad_indices: Vec<u32>,
    pub draw_pass_quad_vertex_buffer_bytes: u64,
    pub draw_pass_quad_index_buffer_bytes: u64,
    pub draw_pass_sampled_image_quads: Vec<NativeVulkanSceneSampledImageQuadSnapshot>,
    pub draw_pass_sampled_image_we_graph_chain_count: usize,
    pub draw_pass_sampled_image_we_graph_step_count: usize,
    pub draw_pass_sampled_image_we_graph_first_class_target_chain_count: usize,
    pub draw_pass_sampled_image_we_graph_temporary_raw_fallback_chain_count: usize,
    pub draw_pass_sampled_image_we_graph_suppressed_chain_count: usize,
    pub draw_pass_sampled_image_we_graph_target_count: usize,
    pub draw_pass_sampled_image_we_graph_final_scene_step_count: usize,
    pub draw_pass_sampled_image_we_graph_effect_kind_counts: BTreeMap<String, usize>,
    pub draw_pass_sampled_image_we_graph_resource_count: usize,
    pub draw_pass_sampled_image_we_graph_texture_resource_count: usize,
    pub draw_pass_sampled_image_we_graph_target_resource_count: usize,
    pub draw_pass_sampled_image_we_graph_resources:
        Vec<NativeVulkanSceneWeImageGraphResourceSnapshot>,
    pub draw_pass_sampled_image_we_graph_targets: Vec<NativeVulkanSceneWeImageGraphTargetSnapshot>,
    pub draw_pass_sampled_image_we_graph_steps: Vec<NativeVulkanSceneWeImageGraphStepSnapshot>,
    pub draw_pass_sampled_image_effect_targets:
        Vec<NativeVulkanSceneSampledImageEffectTargetSnapshot>,
    pub draw_pass_sampled_image_sources: Vec<PathBuf>,
    pub draw_pass_sampled_image_recording_ready: bool,
    pub draw_pass_sampled_image_implicit_full_extent_ready: bool,
    pub draw_pass_sampled_image_recording_step_count: usize,
    pub draw_pass_sampled_image_recording_steps:
        Vec<NativeVulkanSceneSampledImageRecordingStepSnapshot>,
    pub draw_pass_sampled_image_vertices: Vec<NativeVulkanSceneSampledImageVertexSnapshot>,
    pub draw_pass_sampled_image_indices: Vec<u32>,
    pub draw_pass_sampled_image_vertex_buffer_bytes: u64,
    pub draw_pass_sampled_image_index_buffer_bytes: u64,
    pub draw_pass_video_quads: Vec<NativeVulkanSceneVideoQuadSnapshot>,
    pub draw_pass_video_sources: Vec<PathBuf>,
    pub draw_pass_video_recording_ready: bool,
    pub draw_pass_video_recording_step_count: usize,
    pub draw_pass_video_recording_steps: Vec<NativeVulkanSceneVideoRecordingStepSnapshot>,
    pub draw_pass_video_vertices: Vec<NativeVulkanSceneSampledImageVertexSnapshot>,
    pub draw_pass_video_indices: Vec<u32>,
    pub draw_pass_video_vertex_buffer_bytes: u64,
    pub draw_pass_video_index_buffer_bytes: u64,
    pub draw_pass_clear_background_op_count: usize,
    pub draw_pass_background_clear_color: Option<String>,
    pub draw_pass_color_op_count: usize,
    pub draw_pass_sampled_image_op_count: usize,
    pub scene_solid_quad_draw_count: usize,
    pub scene_sampled_image_resource_count: usize,
    pub scene_sampled_image_descriptor_heap_required: bool,
    pub draw_pass_video_op_count: usize,
    pub scene_video_layer_resource_count: usize,
    pub scene_video_native_layer_count: usize,
    pub draw_pass_vector_shape_op_count: usize,
    pub draw_pass_text_op_count: usize,
    pub draw_pass_path_op_count: usize,
    pub draw_pass_effect_pass_count: usize,
    pub draw_pass_effect_pass_non_image_layer_count: usize,
    pub draw_pass_effect_pass_kind_counts: BTreeMap<String, usize>,
    pub draw_pass_required_image_resources: Vec<PathBuf>,
    pub draw_pass_required_video_resources: Vec<PathBuf>,
    pub draw_pass_requires_text_geometry: bool,
    pub draw_pass_requires_path_tessellation: bool,
    pub draw_pass_requires_video_decode: bool,
    pub draw_pass_fast_clear_color: Option<String>,
    pub vulkanalia_draw_pass: NativeVulkanVulkanaliaSceneDrawPassSnapshot,
    pub vulkanalia_sampled_image: NativeVulkanVulkanaliaSceneSampledImagePlanSnapshot,
    pub draw_op_count: usize,
    pub unsupported_layer_count: usize,
    pub draw_ops: Vec<NativeVulkanSceneDrawOpSnapshot>,
    pub unsupported_layers: Vec<NativeVulkanSceneUnsupportedLayerSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneBinaryRetainedIngestRuntimeSnapshot {
    pub record_count: u32,
    pub resource_count: u32,
    pub texture_slot_count: u32,
    pub material_pass_count: u32,
    pub effect_pass_count: u32,
    pub effect_uv_transform_count: u32,
    pub effect_parameter_count: u32,
    pub geometry_count: u32,
    pub puppet_count: u32,
    pub particle_emitter_count: u32,
    pub dirty_range_count: u32,
    pub stable_id_count: u32,
    pub dirty_record_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneBinaryIngestRuntimeSnapshot {
    pub boundary: &'static str,
    pub input_model: &'static str,
    pub payload_retention_model: &'static str,
    pub feature_flags: u32,
    pub chunk_count: u32,
    pub resource_count: u32,
    pub node_count: u32,
    pub draw_record_count: u32,
    pub transform_timeline_count: u32,
    pub geometry_record_count: u32,
    pub generated_vertex_count: u32,
    pub generated_index_count: u32,
    pub mesh_vertex_count: u32,
    pub mesh_index_count: u32,
    pub mesh_vertex_stream_bytes: u64,
    pub mesh_index_stream_bytes: u64,
    pub texture_slot_count: u32,
    pub material_pass_count: u32,
    pub effect_pass_count: u32,
    pub effect_parameter_count: u32,
    pub effect_property_count: u32,
    pub effect_pass_constant_count: u32,
    pub effect_pass_switch_count: u32,
    pub flutter_state_count: u32,
    pub puppet_count: u32,
    pub particle_emitter_count: u32,
    pub puppet_vertex_count: u32,
    pub puppet_index_count: u32,
    pub puppet_animation_layer_count: u32,
    pub render_state_count: u32,
    pub retained: NativeVulkanSceneBinaryRetainedIngestRuntimeSnapshot,
    pub debug_name_count: u32,
    pub debug_name_string_bytes: u32,
}

impl NativeVulkanSceneBinaryIngestRuntimeSnapshot {
    fn from_summary(summary: NativeVulkanSceneBinaryIngestSummary) -> Self {
        Self {
            boundary: "native-vulkan-scene-binary-read-upload-drop",
            input_model: "gscn-versioned-binary-chunks",
            payload_retention_model: "read-header-table-stream-records-drop-source-bytes",
            feature_flags: summary.feature_flags,
            chunk_count: summary.chunk_count,
            resource_count: summary.resource_count,
            node_count: summary.node_count,
            draw_record_count: summary.draw_record_count,
            transform_timeline_count: summary.transform_timeline_count,
            geometry_record_count: summary.geometry_record_count,
            generated_vertex_count: summary.generated_vertex_count,
            generated_index_count: summary.generated_index_count,
            mesh_vertex_count: summary.mesh_vertex_count,
            mesh_index_count: summary.mesh_index_count,
            mesh_vertex_stream_bytes: summary.mesh_vertex_stream_bytes,
            mesh_index_stream_bytes: summary.mesh_index_stream_bytes,
            texture_slot_count: summary.texture_slot_count,
            material_pass_count: summary.material_pass_count,
            effect_pass_count: summary.effect_pass_count,
            effect_parameter_count: summary.effect_parameter_count,
            effect_property_count: summary.effect_property_count,
            effect_pass_constant_count: summary.effect_pass_constant_count,
            effect_pass_switch_count: summary.effect_pass_switch_count,
            flutter_state_count: summary.flutter_state_count,
            puppet_count: summary.puppet_count,
            particle_emitter_count: summary.particle_emitter_count,
            puppet_vertex_count: summary.puppet_vertex_count,
            puppet_index_count: summary.puppet_index_count,
            puppet_animation_layer_count: summary.puppet_animation_layer_count,
            render_state_count: summary.render_state_count,
            retained: NativeVulkanSceneBinaryRetainedIngestRuntimeSnapshot {
                record_count: summary.retained.record_count,
                resource_count: summary.retained.resource_count,
                texture_slot_count: summary.retained.texture_slot_count,
                material_pass_count: summary.retained.material_pass_count,
                effect_pass_count: summary.retained.effect_pass_count,
                effect_uv_transform_count: summary.retained.effect_uv_transform_count,
                effect_parameter_count: summary.retained.effect_parameter_count,
                geometry_count: summary.retained.geometry_count,
                puppet_count: summary.retained.puppet_count,
                particle_emitter_count: summary.retained.particle_emitter_count,
                dirty_range_count: summary.retained.dirty_range_count,
                stable_id_count: summary.retained.stable_id_count,
                dirty_record_count: summary.retained.dirty_record_count,
            },
            debug_name_count: summary.debug_name_count,
            debug_name_string_bytes: summary.debug_name_string_bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanFullSceneRuntimeSnapshot {
    pub target_runtime: &'static str,
    pub current_runtime: &'static str,
    pub progress_estimate_percent: u8,
    pub full_scene_complete: bool,
    pub execution_model: &'static str,
    pub native_scene_graph_lowering_ready: bool,
    pub native_present_route_ready: bool,
    pub retained_resource_model_ready: bool,
    pub scene_binary_ingest_ready: bool,
    pub timeline_snapshot_runtime_ready: bool,
    pub timeline_snapshot_time_ms: u64,
    pub timeline_animation_runtime_ready: bool,
    pub timeline_animation_count: usize,
    pub timeline_animated_layer_count: usize,
    pub puppet_animation_layer_count: usize,
    pub source_layer_count: usize,
    pub active_scene_layer_count: usize,
    pub flattened_draw_layer_count: usize,
    pub unsupported_layer_count: usize,
    pub native_runtime_layer_count: usize,
    pub native_runtime_pending_layer_count: usize,
    pub native_runtime_coverage_percent: u8,
    pub clear_background_layer_count: usize,
    pub solid_geometry_layer_count: usize,
    pub rounded_rectangle_layer_count: usize,
    pub sampled_image_native_layer_count: usize,
    pub video_native_layer_count: usize,
    pub tessellated_path_layer_count: usize,
    pub curve_path_layer_count: usize,
    pub arc_path_layer_count: usize,
    pub compound_path_layer_count: usize,
    pub text_geometry_layer_count: usize,
    pub stroke_geometry_layer_count: usize,
    pub color_layer_count: usize,
    pub sampled_image_layer_count: usize,
    pub video_layer_count: usize,
    pub vector_shape_layer_count: usize,
    pub text_layer_count: usize,
    pub path_layer_count: usize,
    pub property_update_runtime_ready: bool,
    pub property_binding_count: usize,
    pub pause_resume_policy_ready: bool,
    pub package_state_persistence_ready: bool,
    pub scene_state_persistence_model: &'static str,
    pub scene_audio_cue_count: usize,
    pub scene_audio_cue_resource_model_ready: bool,
    pub scene_scenescript_detected: bool,
    pub scene_scenescript_ready: bool,
    pub scene_scenescript_binding_count: usize,
    pub scene_shader_material_graph_detected: bool,
    pub scene_shader_material_graph_ready: bool,
    pub scene_material_graph_count: usize,
    pub scene_material_graph_resource_count: usize,
    pub scene_effect_graph_count: usize,
    pub scene_audio_response_detected: bool,
    pub scene_audio_response_ready: bool,
    pub scene_audio_response_binding_count: usize,
    pub scene_particle_system_detected: bool,
    pub scene_particle_system_ready: bool,
    pub particle_runtime_layer_count: usize,
    pub cursor_parallax_input_ready: bool,
    pub scene_video_composition_required: bool,
    pub scene_video_composition_ready: bool,
    pub scene_text_geometry_required: bool,
    pub scene_text_geometry_ready: bool,
    pub scene_path_tessellation_required: bool,
    pub scene_path_tessellation_ready: bool,
    pub unsupported_scene_feature_count: usize,
    pub unsupported_scene_features: Vec<String>,
    pub completed_boundaries: Vec<&'static str>,
    pub pending_boundaries: Vec<&'static str>,
    pub unsupported_boundaries: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneSampledGeometryInputs {
    pub solid_geometry: Option<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput>,
    pub sampled_geometry: NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
}

impl NativeVulkanSceneRuntimeSnapshot {
    pub(in crate::renderer::native_vulkan) fn release_cpu_draw_payloads_for_present(&mut self) {
        self.draw_pass_recordable_quads = Vec::new();
        self.draw_pass_quad_recording_steps = Vec::new();
        self.draw_pass_quad_vertices = Vec::new();
        self.draw_pass_quad_indices = Vec::new();
        self.draw_pass_sampled_image_quads = Vec::new();
        self.draw_pass_sampled_image_effect_targets = Vec::new();
        self.draw_pass_sampled_image_sources = Vec::new();
        self.draw_pass_sampled_image_recording_steps = Vec::new();
        self.draw_pass_sampled_image_vertices = Vec::new();
        self.draw_pass_sampled_image_indices = Vec::new();
        self.draw_pass_video_quads = Vec::new();
        self.draw_pass_video_sources = Vec::new();
        self.draw_pass_video_recording_steps = Vec::new();
        self.draw_pass_video_vertices = Vec::new();
        self.draw_pass_video_indices = Vec::new();
        self.draw_pass_required_image_resources = Vec::new();
        self.draw_pass_required_video_resources = Vec::new();
        self.draw_ops = Vec::new();
        self.unsupported_layers = Vec::new();
    }

    pub fn take_vulkanalia_solid_quad_geometry_input(
        &mut self,
    ) -> Option<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput> {
        if self.draw_pass_quad_recording_step_count == 0
            || self.draw_pass_quad_vertices.is_empty()
            || self.draw_pass_quad_indices.is_empty()
        {
            return None;
        }

        let draw_steps = std::mem::take(&mut self.draw_pass_quad_recording_steps)
            .into_iter()
            .map(|step| NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index: step.layer_index,
                first_index: step.first_index,
                index_count: step.index_count,
                blend: native_vulkan_scene_vulkanalia_blend_state(step.blend),
            })
            .collect::<Vec<_>>();

        Some(
            NativeVulkanVulkanaliaSceneSolidQuadGeometryInput::new_batched(
                std::mem::take(&mut self.draw_pass_quad_vertices)
                    .into_iter()
                    .map(|vertex| {
                        NativeVulkanVulkanaliaSceneSolidQuadVertex::new(
                            vertex.position,
                            vertex.rgba,
                        )
                    })
                    .collect(),
                std::mem::take(&mut self.draw_pass_quad_indices),
                draw_steps,
                "scene-runtime-draw-plan",
            ),
        )
    }

    pub fn take_vulkanalia_sampled_image_geometry_input(
        &mut self,
    ) -> Option<(
        PathBuf,
        NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
    )> {
        if !self.draw_pass_sampled_image_recording_ready
            || self.draw_pass_sampled_image_quads.is_empty()
            || self.draw_pass_sampled_image_vertices.is_empty()
            || self.draw_pass_sampled_image_indices.is_empty()
        {
            return None;
        }

        let sources = std::mem::take(&mut self.draw_pass_sampled_image_sources);
        self.draw_pass_sampled_image_quads = Vec::new();
        let source = sources.first().cloned()?;
        let draw_steps = std::mem::take(&mut self.draw_pass_sampled_image_recording_steps)
            .into_iter()
            .map(|step| NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                layer_index: step.layer_index,
                texture_slot_bindings: step
                    .texture_slot_bindings
                    .into_iter()
                    .map(native_vulkan_scene_vulkanalia_texture_slot_resource_binding)
                    .collect(),
                material: native_vulkan_scene_vulkanalia_sampled_image_material(step.material_pass),
                first_index: step.first_index,
                index_count: step.index_count,
                fit: Some(step.fit),
                texture_region: step.texture_region,
                render_target: native_vulkan_scene_vulkanalia_sampled_image_render_target(
                    step.render_target,
                ),
            })
            .collect::<Vec<_>>();
        let effect_targets = std::mem::take(&mut self.draw_pass_sampled_image_effect_targets)
            .into_iter()
            .map(
                |target| NativeVulkanVulkanaliaSceneSampledImageEffectTarget {
                    effect_target_index: target.effect_target_index,
                    layer_index: target.layer_index,
                    width: target.width,
                    height: target.height,
                    we_graph_chain_index: target.we_graph_chain_index,
                    we_graph_target_index: target.we_graph_target_index,
                    we_graph_endpoint: target.we_graph_endpoint,
                },
            )
            .collect::<Vec<_>>();
        let we_graph_resources =
            std::mem::take(&mut self.draw_pass_sampled_image_we_graph_resources)
                .into_iter()
                .map(native_vulkan_scene_vulkanalia_we_image_graph_resource)
                .collect::<Vec<_>>();

        Some((
            source,
            NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new_batched_with_effect_targets_and_we_graph_resources(
                std::mem::take(&mut self.draw_pass_sampled_image_vertices)
                    .into_iter()
                    .map(|vertex| {
                        NativeVulkanVulkanaliaSceneSampledImageVertex::new_with_effect_uv(
                            vertex.position,
                            vertex.uv,
                            vertex.effect_uv,
                            vertex.opacity,
                            vertex.tint,
                        )
                    })
                    .collect(),
                std::mem::take(&mut self.draw_pass_sampled_image_indices),
                sources,
                effect_targets,
                we_graph_resources,
                draw_steps,
                "scene-runtime-sampled-image-draw-plan",
            ),
        ))
    }

    pub fn take_vulkanalia_sampled_image_implicit_full_extent_input(
        &mut self,
    ) -> Option<(PathBuf, FitMode)> {
        if !self.draw_pass_sampled_image_implicit_full_extent_ready {
            return None;
        }
        let op = self.draw_ops.iter_mut().find(|op| op.kind == "image")?;
        Some((op.source.take()?, op.fit))
    }

    pub fn take_vulkanalia_video_layer_geometry_input(
        &mut self,
    ) -> Option<NativeVulkanVulkanaliaSceneVideoLayerGeometryInput> {
        if !self.draw_pass_video_recording_ready
            || self.draw_pass_video_sources.is_empty()
            || self.draw_pass_video_vertices.is_empty()
            || self.draw_pass_video_indices.is_empty()
        {
            return None;
        }

        let sources = std::mem::take(&mut self.draw_pass_video_sources);
        self.draw_pass_video_quads = Vec::new();
        let draw_steps = std::mem::take(&mut self.draw_pass_video_recording_steps)
            .into_iter()
            .map(|step| NativeVulkanVulkanaliaSceneVideoLayerDrawStep {
                layer_index: step.layer_index,
                resource_index: step.resource_index,
                first_index: step.first_index,
                index_count: step.index_count,
                fit: Some(step.fit),
            })
            .collect::<Vec<_>>();

        Some(
            NativeVulkanVulkanaliaSceneVideoLayerGeometryInput::new_batched(
                std::mem::take(&mut self.draw_pass_video_vertices)
                    .into_iter()
                    .map(|vertex| {
                        NativeVulkanVulkanaliaSceneSampledImageVertex::new(
                            vertex.position,
                            vertex.uv,
                            vertex.opacity,
                        )
                    })
                    .collect(),
                std::mem::take(&mut self.draw_pass_video_indices),
                sources,
                draw_steps,
                "scene-runtime-video-layer-draw-plan",
            ),
        )
    }

    pub fn take_vulkanalia_mixed_solid_quad_geometry_input(
        &mut self,
    ) -> Option<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput> {
        if self.draw_pass_quad_recording_step_count == 0
            || self.draw_pass_quad_vertices.is_empty()
            || self.draw_pass_quad_indices.is_empty()
        {
            return None;
        }

        let draw_steps = std::mem::take(&mut self.draw_pass_quad_recording_steps)
            .into_iter()
            .map(|step| NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index: step.layer_index,
                first_index: step.first_index,
                index_count: step.index_count,
                blend: native_vulkan_scene_vulkanalia_blend_state(step.blend),
            })
            .collect::<Vec<_>>();

        Some(
            NativeVulkanVulkanaliaSceneSolidQuadGeometryInput::new_batched(
                std::mem::take(&mut self.draw_pass_quad_vertices)
                    .into_iter()
                    .map(|vertex| {
                        NativeVulkanVulkanaliaSceneSolidQuadVertex::new(
                            vertex.position,
                            vertex.rgba,
                        )
                    })
                    .collect(),
                std::mem::take(&mut self.draw_pass_quad_indices),
                draw_steps,
                "scene-runtime-mixed-solid-quad-draw-plan",
            ),
        )
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_solid_quad_geometry_input_from_layers(
    snapshot_time_ms: u64,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
    layers: &[SceneRenderLayer],
) -> Result<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput, String> {
    let _ = (snapshot_time_ms, scene_size, scene_fit);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut draw_steps = Vec::new();
    let mut recordable_layer_count = 0usize;

    for (layer_index, layer) in layers.iter().enumerate() {
        if native_vulkan_scene_render_layer_has_no_visual_geometry(layer) {
            continue;
        }
        recordable_layer_count = recordable_layer_count.saturating_add(1);
        let Some((solid_vertices, solid_indices)) =
            native_vulkan_scene_solid_geometry_from_render_layer(layer_index, layer).map_err(
                |reason| format!("dynamic scene is not solid-quad recordable: {reason}"),
            )?
        else {
            continue;
        };
        let first_vertex = vertices.len().min(u32::MAX as usize) as u32;
        let first_index = indices.len().min(u32::MAX as usize) as u32;
        let index_count = solid_indices.len().min(u32::MAX as usize) as u32;
        draw_steps.push(NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
            layer_index,
            first_index,
            index_count,
            blend: NativeVulkanVulkanaliaSceneBlendState::from_mode(layer.blend_mode),
        });
        vertices.extend(solid_vertices.into_iter().map(|vertex| {
            NativeVulkanVulkanaliaSceneSolidQuadVertex::new(vertex.position, vertex.rgba)
        }));
        indices.extend(
            solid_indices
                .into_iter()
                .map(|index| first_vertex.saturating_add(index)),
        );
    }

    if draw_steps.is_empty() || vertices.is_empty() || indices.is_empty() {
        return Err("dynamic solid scene produced no quad geometry".to_owned());
    }
    if draw_steps.len() != recordable_layer_count {
        return Err(
            "dynamic scene is not solid-quad recordable: partial-solid-quad-recording-ready"
                .to_owned(),
        );
    }
    Ok(
        NativeVulkanVulkanaliaSceneSolidQuadGeometryInput::new_batched(
            vertices,
            indices,
            draw_steps,
            "scene-runtime-direct-solid-draw-plan",
        ),
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_mixed_solid_quad_geometry_input_from_layers(
    snapshot_time_ms: u64,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
    layers: &[SceneRenderLayer],
) -> Result<Option<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput>, String> {
    let _ = (snapshot_time_ms, scene_size, scene_fit);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut draw_steps = Vec::new();

    for (layer_index, layer) in layers.iter().enumerate() {
        if native_vulkan_scene_render_layer_has_no_visual_geometry(layer)
            || layer.kind == crate::core::SceneNodeKind::Video
            || layer.kind == crate::core::SceneNodeKind::Image
        {
            continue;
        }
        let Some((solid_vertices, solid_indices)) =
            native_vulkan_scene_solid_geometry_from_render_layer(layer_index, layer).map_err(
                |reason| format!("dynamic mixed scene is not solid-quad recordable: {reason}"),
            )?
        else {
            continue;
        };
        let first_vertex = vertices.len().min(u32::MAX as usize) as u32;
        let first_index = indices.len().min(u32::MAX as usize) as u32;
        let index_count = solid_indices.len().min(u32::MAX as usize) as u32;
        draw_steps.push(NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
            layer_index,
            first_index,
            index_count,
            blend: NativeVulkanVulkanaliaSceneBlendState::from_mode(layer.blend_mode),
        });
        vertices.extend(solid_vertices.into_iter().map(|vertex| {
            NativeVulkanVulkanaliaSceneSolidQuadVertex::new(vertex.position, vertex.rgba)
        }));
        indices.extend(
            solid_indices
                .into_iter()
                .map(|index| first_vertex.saturating_add(index)),
        );
    }

    if draw_steps.is_empty() || vertices.is_empty() || indices.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        NativeVulkanVulkanaliaSceneSolidQuadGeometryInput::new_batched(
            vertices,
            indices,
            draw_steps,
            "scene-runtime-direct-mixed-solid-quad-vertex-update",
        ),
    ))
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_solid_quad_geometry_input_from_snapshot_layers(
    layers: &[SceneSnapshotLayer],
) -> Result<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput, String> {
    let mut vertices = Vec::new();

    for (layer_index, layer) in layers.iter().enumerate() {
        if native_vulkan_scene_snapshot_layer_has_no_visual_geometry(layer) {
            continue;
        }
        let render_layer = native_vulkan_scene_render_layer_from_snapshot_for_geometry(layer);
        let Some((solid_vertices, solid_indices)) =
            native_vulkan_scene_solid_geometry_from_render_layer(layer_index, &render_layer)
                .map_err(|reason| {
                    format!("dynamic scene is not solid-quad recordable: {reason}")
                })?
        else {
            continue;
        };
        let _ = solid_indices;
        vertices.extend(solid_vertices.into_iter().map(|vertex| {
            NativeVulkanVulkanaliaSceneSolidQuadVertex::new(vertex.position, vertex.rgba)
        }));
    }

    if vertices.is_empty() {
        return Err("dynamic solid scene produced no quad geometry".to_owned());
    }
    Ok(
        NativeVulkanVulkanaliaSceneSolidQuadGeometryInput::new_batched(
            vertices,
            Vec::new(),
            Vec::new(),
            "scene-runtime-direct-snapshot-solid-draw-plan",
        ),
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_sampled_geometry_inputs_from_layers(
    snapshot_time_ms: u64,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
    layers: &[SceneRenderLayer],
) -> Result<NativeVulkanSceneSampledGeometryInputs, String> {
    native_vulkan_scene_sampled_geometry_inputs_from_layers_with_source_indices(
        snapshot_time_ms,
        scene_size,
        scene_fit,
        layers,
        None,
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_sampled_geometry_inputs_from_layers_with_source_indices(
    snapshot_time_ms: u64,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
    layers: &[SceneRenderLayer],
    sampled_source_indices: Option<&BTreeMap<PathBuf, u32>>,
) -> Result<NativeVulkanSceneSampledGeometryInputs, String> {
    let _ = (snapshot_time_ms, scene_size, scene_fit);
    let mut solid_vertices = Vec::new();
    let mut solid_indices = Vec::new();
    let mut solid_draw_steps = Vec::new();
    let mut sampled_scene_vertices = Vec::with_capacity(layers.len().saturating_mul(4));
    let mut sampled_indices = Vec::with_capacity(layers.len().saturating_mul(6));
    let mut sampled_sources = Vec::new();
    let mut sampled_draw_steps = Vec::with_capacity(layers.len());

    for (layer_index, layer) in layers.iter().enumerate() {
        if native_vulkan_scene_render_layer_has_no_visual_geometry(layer) {
            continue;
        }
        if layer.kind == crate::core::SceneNodeKind::Video {
            continue;
        }
        if layer.kind == crate::core::SceneNodeKind::Image {
            if native_vulkan_scene_render_layer_suppresses_unimplemented_we_effect_chain(layer) {
                continue;
            }
            let Some((fit, texture_region, range)) =
                native_vulkan_scene_append_sampled_image_geometry_from_render_layer(
                    layer_index,
                    layer,
                    &mut sampled_scene_vertices,
                    &mut sampled_indices,
                )
                .map_err(|reason| {
                    format!("dynamic scene is not sampled-image recordable: {reason}")
                })?
            else {
                continue;
            };
            let Some(source) = layer.source.as_ref() else {
                return Err(
                    "dynamic scene is not sampled-image recordable: image-layer-missing-source"
                        .to_owned(),
                );
            };
            let resource_index = if let Some(source_indices) = sampled_source_indices {
                *source_indices.get(source).ok_or_else(|| {
                    format!(
                        "dynamic scene sampled source {} is absent from retained sampled image topology",
                        source.display()
                    )
                })?
            } else {
                native_vulkan_scene_sampled_source_index(&mut sampled_sources, source.clone())
            };
            let texture_slot_bindings = native_vulkan_scene_render_layer_texture_slot_bindings(
                layer,
                resource_index,
                sampled_source_indices,
                &mut sampled_sources,
            )?;
            sampled_draw_steps.push(NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                layer_index,
                material: NativeVulkanVulkanaliaSceneSampledImageMaterial::sampled_image(
                    layer.blend_mode,
                    layer.alpha_texture_slot,
                    layer.alpha_texture_mode,
                    texture_slot_bindings.len(),
                ),
                texture_slot_bindings,
                first_index: range.first_index,
                index_count: range.index_count,
                fit: Some(fit),
                texture_region,
                render_target: NativeVulkanVulkanaliaSceneSampledImageRenderTarget::Swapchain,
            });
        } else {
            let Some((vertices, indices)) = native_vulkan_scene_solid_geometry_from_render_layer(
                layer_index,
                layer,
            )
            .map_err(|reason| {
                format!("dynamic mixed sampled scene is not solid-quad recordable: {reason}")
            })?
            else {
                continue;
            };
            let first_vertex = solid_vertices.len().min(u32::MAX as usize) as u32;
            let first_index = solid_indices.len().min(u32::MAX as usize) as u32;
            let index_count = indices.len().min(u32::MAX as usize) as u32;
            solid_draw_steps.push(NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index,
                first_index,
                index_count,
                blend: NativeVulkanVulkanaliaSceneBlendState::from_mode(layer.blend_mode),
            });
            solid_vertices.extend(vertices.into_iter().map(|vertex| {
                NativeVulkanVulkanaliaSceneSolidQuadVertex::new(vertex.position, vertex.rgba)
            }));
            solid_indices.extend(
                indices
                    .into_iter()
                    .map(|index| first_vertex.saturating_add(index)),
            );
        }
    }

    if sampled_source_indices.is_some() {
        sampled_sources.clear();
    }
    if sampled_draw_steps.is_empty()
        || sampled_scene_vertices.is_empty()
        || sampled_indices.is_empty()
    {
        return Err("dynamic sampled-image scene produced no sampled geometry".to_owned());
    }
    let sampled_vertices = sampled_scene_vertices
        .into_iter()
        .map(|vertex: NativeVulkanSceneSampledImageVertex| {
            NativeVulkanVulkanaliaSceneSampledImageVertex::new_with_effect_uv(
                vertex.position,
                vertex.uv,
                vertex.effect_uv,
                vertex.opacity,
                vertex.tint,
            )
        })
        .collect();
    let sampled_geometry = NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new_batched(
        sampled_vertices,
        sampled_indices,
        sampled_sources,
        sampled_draw_steps,
        "scene-runtime-direct-sampled-image-draw-plan",
    );
    let solid_geometry =
        if solid_draw_steps.is_empty() || solid_vertices.is_empty() || solid_indices.is_empty() {
            None
        } else {
            Some(
                NativeVulkanVulkanaliaSceneSolidQuadGeometryInput::new_batched(
                    solid_vertices,
                    solid_indices,
                    solid_draw_steps,
                    "scene-runtime-direct-mixed-solid-quad-draw-plan",
                ),
            )
        };
    Ok(NativeVulkanSceneSampledGeometryInputs {
        solid_geometry,
        sampled_geometry,
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_sampled_geometry_input_from_snapshot_layers_with_package_source_indices(
    layers: &[SceneSnapshotLayer],
    sampled_source_indices: &BTreeMap<String, u32>,
) -> Result<NativeVulkanVulkanaliaSceneSampledImageGeometryInput, String> {
    native_vulkan_scene_sampled_geometry_inputs_from_snapshot_layers_with_package_source_indices(
        layers,
        sampled_source_indices,
        false,
    )
    .map(|geometry| geometry.sampled_geometry)
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_sampled_geometry_inputs_from_snapshot_layers_with_package_source_indices(
    layers: &[SceneSnapshotLayer],
    sampled_source_indices: &BTreeMap<String, u32>,
    include_solid_geometry: bool,
) -> Result<NativeVulkanSceneSampledGeometryInputs, String> {
    let mut solid_vertices = Vec::new();
    let mut solid_indices = Vec::new();
    let mut solid_draw_steps = Vec::new();
    let mut sampled_scene_vertices = Vec::with_capacity(layers.len().saturating_mul(4));
    let mut sampled_indices = Vec::with_capacity(layers.len().saturating_mul(6));
    let mut sampled_draw_steps = Vec::with_capacity(layers.len());

    for (layer_index, layer) in layers.iter().enumerate() {
        if native_vulkan_scene_snapshot_layer_has_no_visual_geometry(layer) {
            continue;
        }
        if layer.kind == crate::core::SceneNodeKind::Video {
            continue;
        }
        if layer.kind == crate::core::SceneNodeKind::Image {
            if native_vulkan_scene_snapshot_layer_suppresses_unimplemented_we_effect_chain(layer) {
                continue;
            }
            let Some((fit, texture_region, range)) =
                native_vulkan_scene_append_sampled_image_geometry_from_snapshot_layer(
                    layer_index,
                    layer,
                    &mut sampled_scene_vertices,
                    &mut sampled_indices,
                )
                .map_err(|reason| {
                    format!("dynamic scene is not sampled-image recordable: {reason}")
                })?
            else {
                continue;
            };
            let Some(source) = layer.source.as_ref() else {
                return Err(
                    "dynamic scene is not sampled-image recordable: image-layer-missing-source"
                        .to_owned(),
                );
            };
            let resource_index = *sampled_source_indices.get(source.as_str()).ok_or_else(|| {
                format!(
                    "dynamic scene sampled package source {} is absent from retained sampled image topology",
                    source.as_str()
                )
            })?;
            let texture_slot_bindings = native_vulkan_scene_snapshot_layer_texture_slot_bindings(
                layer,
                resource_index,
                sampled_source_indices,
            )?;
            sampled_draw_steps.push(NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                layer_index,
                material: NativeVulkanVulkanaliaSceneSampledImageMaterial::sampled_image(
                    layer.blend_mode,
                    layer.alpha_texture_slot,
                    layer.alpha_texture_mode.into(),
                    texture_slot_bindings.len(),
                ),
                texture_slot_bindings,
                first_index: range.first_index,
                index_count: range.index_count,
                fit: Some(fit),
                texture_region,
                render_target: NativeVulkanVulkanaliaSceneSampledImageRenderTarget::Swapchain,
            });
        } else if include_solid_geometry {
            let render_layer = native_vulkan_scene_render_layer_from_snapshot_for_geometry(layer);
            let Some((vertices, indices)) = native_vulkan_scene_solid_geometry_from_render_layer(
                layer_index,
                &render_layer,
            )
            .map_err(|reason| {
                format!(
                    "dynamic mixed sampled snapshot scene is not solid-quad recordable: {reason}"
                )
            })?
            else {
                continue;
            };
            let first_vertex = solid_vertices.len().min(u32::MAX as usize) as u32;
            let first_index = solid_indices.len().min(u32::MAX as usize) as u32;
            let index_count = indices.len().min(u32::MAX as usize) as u32;
            solid_draw_steps.push(NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index,
                first_index,
                index_count,
                blend: NativeVulkanVulkanaliaSceneBlendState::from_mode(layer.blend_mode),
            });
            solid_vertices.extend(vertices.into_iter().map(|vertex| {
                NativeVulkanVulkanaliaSceneSolidQuadVertex::new(vertex.position, vertex.rgba)
            }));
            solid_indices.extend(
                indices
                    .into_iter()
                    .map(|index| first_vertex.saturating_add(index)),
            );
        }
    }

    if sampled_draw_steps.is_empty()
        || sampled_scene_vertices.is_empty()
        || sampled_indices.is_empty()
    {
        return Err("dynamic sampled-image scene produced no sampled geometry".to_owned());
    }
    let sampled_vertices = sampled_scene_vertices
        .into_iter()
        .map(|vertex: NativeVulkanSceneSampledImageVertex| {
            NativeVulkanVulkanaliaSceneSampledImageVertex::new_with_effect_uv(
                vertex.position,
                vertex.uv,
                vertex.effect_uv,
                vertex.opacity,
                vertex.tint,
            )
        })
        .collect();
    let sampled_geometry = NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new_batched(
        sampled_vertices,
        sampled_indices,
        Vec::new(),
        sampled_draw_steps,
        "scene-runtime-direct-snapshot-sampled-image-draw-plan",
    );
    let solid_geometry =
        if solid_draw_steps.is_empty() || solid_vertices.is_empty() || solid_indices.is_empty() {
            None
        } else {
            Some(
                NativeVulkanVulkanaliaSceneSolidQuadGeometryInput::new_batched(
                    solid_vertices,
                    solid_indices,
                    solid_draw_steps,
                    "scene-runtime-direct-snapshot-mixed-solid-quad-draw-plan",
                ),
            )
        };
    Ok(NativeVulkanSceneSampledGeometryInputs {
        solid_geometry,
        sampled_geometry,
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_sampled_vertex_inputs_from_snapshot_layers(
    layers: &[SceneSnapshotLayer],
    include_solid_geometry: bool,
) -> Result<NativeVulkanSceneSampledGeometryInputs, String> {
    let mut solid_vertices = Vec::new();
    let mut solid_indices = Vec::new();
    let mut solid_draw_steps = Vec::new();
    let mut sampled_scene_vertices = Vec::with_capacity(layers.len().saturating_mul(4));

    for (layer_index, layer) in layers.iter().enumerate() {
        if native_vulkan_scene_snapshot_layer_has_no_visual_geometry(layer) {
            continue;
        }
        if layer.kind == crate::core::SceneNodeKind::Video {
            continue;
        }
        if layer.kind == crate::core::SceneNodeKind::Image {
            if native_vulkan_scene_snapshot_layer_suppresses_unimplemented_we_effect_chain(layer) {
                continue;
            }
            native_vulkan_scene_append_sampled_image_vertices_from_snapshot_layer(
                layer_index,
                layer,
                &mut sampled_scene_vertices,
            )
            .map_err(|reason| format!("dynamic scene is not sampled-image recordable: {reason}"))?;
        } else if include_solid_geometry {
            let render_layer = native_vulkan_scene_render_layer_from_snapshot_for_geometry(layer);
            let Some((vertices, indices)) = native_vulkan_scene_solid_geometry_from_render_layer(
                layer_index,
                &render_layer,
            )
            .map_err(|reason| {
                format!(
                    "dynamic mixed sampled snapshot scene is not solid-quad recordable: {reason}"
                )
            })?
            else {
                continue;
            };
            let first_vertex = solid_vertices.len().min(u32::MAX as usize) as u32;
            let first_index = solid_indices.len().min(u32::MAX as usize) as u32;
            let index_count = indices.len().min(u32::MAX as usize) as u32;
            solid_draw_steps.push(NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index,
                first_index,
                index_count,
                blend: NativeVulkanVulkanaliaSceneBlendState::from_mode(layer.blend_mode),
            });
            solid_vertices.extend(vertices.into_iter().map(|vertex| {
                NativeVulkanVulkanaliaSceneSolidQuadVertex::new(vertex.position, vertex.rgba)
            }));
            solid_indices.extend(
                indices
                    .into_iter()
                    .map(|index| first_vertex.saturating_add(index)),
            );
        }
    }

    if sampled_scene_vertices.is_empty() {
        return Err("dynamic sampled-image scene produced no sampled vertices".to_owned());
    }
    let sampled_vertices = sampled_scene_vertices
        .into_iter()
        .map(|vertex: NativeVulkanSceneSampledImageVertex| {
            NativeVulkanVulkanaliaSceneSampledImageVertex::new_with_effect_uv(
                vertex.position,
                vertex.uv,
                vertex.effect_uv,
                vertex.opacity,
                vertex.tint,
            )
        })
        .collect();
    let sampled_geometry = NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new_batched(
        sampled_vertices,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        "scene-runtime-direct-snapshot-sampled-image-retained-topology-vertices",
    );
    let solid_geometry =
        if solid_draw_steps.is_empty() || solid_vertices.is_empty() || solid_indices.is_empty() {
            None
        } else {
            Some(
                NativeVulkanVulkanaliaSceneSolidQuadGeometryInput::new_batched(
                    solid_vertices,
                    solid_indices,
                    solid_draw_steps,
                    "scene-runtime-direct-snapshot-mixed-solid-quad-draw-plan",
                ),
            )
        };
    Ok(NativeVulkanSceneSampledGeometryInputs {
        solid_geometry,
        sampled_geometry,
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_sampled_vertex_input_from_sampled_layers(
    layers: &[SceneSnapshotSampledImageLayer],
) -> Result<NativeVulkanVulkanaliaSceneSampledImageGeometryInput, String> {
    native_vulkan_scene_sampled_vertex_input_from_sampled_layers_at(None, layers)
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_sampled_vertex_input_from_sampled_layers_at(
    snapshot_time_ms: Option<u64>,
    layers: &[SceneSnapshotSampledImageLayer],
) -> Result<NativeVulkanVulkanaliaSceneSampledImageGeometryInput, String> {
    native_vulkan_scene_sampled_vertex_input_from_sampled_layers_at_with_package_root(
        snapshot_time_ms,
        layers,
        None,
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_sampled_vertex_input_from_sampled_layers_at_with_package_root(
    snapshot_time_ms: Option<u64>,
    layers: &[SceneSnapshotSampledImageLayer],
    package_root: Option<&Path>,
) -> Result<NativeVulkanVulkanaliaSceneSampledImageGeometryInput, String> {
    let folded_layers =
        native_vulkan_scene_prepare_sampled_image_alpha_effect_layers(snapshot_time_ms, layers);
    let independent_alpha_layer_count = folded_layers
        .iter()
        .filter(|folded| folded.layer.alpha_texture_slot.is_some())
        .count();
    let effect_uv_target_count = folded_layers
        .iter()
        .filter(|folded| folded.effect_uv_space.is_some())
        .count();
    let mut sampled_scene_vertices =
        native_vulkan_scene_take_sampled_vertex_vec(folded_layers.len().saturating_mul(4));
    let mut eye_debug_layers = Vec::new();

    for folded_index in 0..folded_layers.len() {
        let folded = &folded_layers[folded_index];
        let before_vertices = sampled_scene_vertices.len();
        let adjusted_layer;
        let layer = if (folded.opacity - folded.layer.opacity).abs() > f64::EPSILON {
            adjusted_layer = {
                let mut layer = folded.layer.clone();
                layer.opacity = folded.opacity;
                layer
            };
            &adjusted_layer
        } else {
            folded.layer
        };
        if native_vulkan_scene_sampled_layer_suppresses_unimplemented_we_effect_chain(layer) {
            continue;
        }
        if native_vulkan_scene_sampled_layer_uses_first_class_effect_target(layer) {
            native_vulkan_scene_append_sampled_image_vertices_from_sampled_layer_with_effect_chain(
                folded.layer_index,
                layer,
                folded.effect_uv_space,
                &mut sampled_scene_vertices,
            )
        } else {
            native_vulkan_scene_append_sampled_image_vertices_from_sampled_layer_with_effect_uv_space(
                folded.layer_index,
                layer,
                folded.effect_uv_space,
                &mut sampled_scene_vertices,
            )
        }
        .map_err(|reason| format!("dynamic scene is not sampled-image recordable: {reason}"))?;
        if native_vulkan_effect_debug_enabled()
            && native_vulkan_scene_sampled_layer_is_eye_debug_target(layer)
        {
            let after_vertices = sampled_scene_vertices.len();
            native_vulkan_scene_runtime_effect_debug_log(format_args!(
                "time_ms={} eye layer built index={} alpha_slot={:?} vertices_added={} effect_uv_range={} slots={} geometry={} effect_uv_space={}",
                snapshot_time_ms
                    .map(|time| time.to_string())
                    .unwrap_or_else(|| "<unknown>".to_owned()),
                folded.layer_index,
                folded.layer.alpha_texture_slot,
                after_vertices.saturating_sub(before_vertices),
                native_vulkan_scene_sampled_vertices_effect_uv_range_label(
                    &sampled_scene_vertices[before_vertices..after_vertices]
                ),
                native_vulkan_scene_sampled_texture_slots_label(&folded.layer.texture_slots),
                native_vulkan_scene_sampled_layer_geometry_label(folded.layer),
                native_vulkan_scene_runtime_effect_uv_space_label(folded.effect_uv_space)
            ));
            native_vulkan_scene_runtime_eye_contribution_debug(
                snapshot_time_ms,
                folded.layer_index,
                layer,
                &sampled_scene_vertices[before_vertices..after_vertices],
                package_root,
            );
            if native_vulkan_scene_runtime_should_log_eye_frame(snapshot_time_ms) {
                eye_debug_layers.push(NativeVulkanSceneRuntimeEyeLayerRecord {
                    layer_index: folded.layer_index,
                    layer: folded.layer,
                    vertices: sampled_scene_vertices[before_vertices..after_vertices].to_vec(),
                });
            }
        }
    }
    if native_vulkan_effect_debug_enabled()
        && native_vulkan_scene_runtime_should_log_eye_frame(snapshot_time_ms)
    {
        native_vulkan_scene_runtime_eye_overlap_debug(
            snapshot_time_ms,
            &eye_debug_layers,
            package_root,
        );
    }

    if sampled_scene_vertices.is_empty() {
        native_vulkan_scene_recycle_sampled_vertex_vec(sampled_scene_vertices);
        return Err("dynamic sampled-image scene produced no sampled vertices".to_owned());
    }
    if native_vulkan_effect_debug_enabled() {
        native_vulkan_scene_runtime_effect_debug_log(format_args!(
            "time_ms={} sampled vertices built layers={} independent_alpha_layers={} effect_uv_targets={} vertices={} effect_uv_range={}",
            snapshot_time_ms
                .map(|time| time.to_string())
                .unwrap_or_else(|| "<unknown>".to_owned()),
            layers.len(),
            independent_alpha_layer_count,
            effect_uv_target_count,
            sampled_scene_vertices.len(),
            native_vulkan_scene_sampled_vertices_effect_uv_range_label(&sampled_scene_vertices)
        ));
    }
    let mut sampled_vertices =
        native_vulkan_vulkanalia_take_scene_sampled_image_vertex_vec(sampled_scene_vertices.len());
    sampled_vertices.extend(sampled_scene_vertices.iter().map(
        |vertex: &NativeVulkanSceneSampledImageVertex| {
            NativeVulkanVulkanaliaSceneSampledImageVertex::new_with_effect_uv(
                vertex.position,
                vertex.uv,
                vertex.effect_uv,
                vertex.opacity,
                vertex.tint,
            )
        },
    ));
    native_vulkan_scene_recycle_sampled_vertex_vec(sampled_scene_vertices);
    Ok(
        NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new_batched(
            sampled_vertices,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            "scene-runtime-direct-sampled-image-retained-topology-vertices",
        ),
    )
}

struct NativeVulkanSceneFoldedSampledImageLayer<'a> {
    layer_index: usize,
    layer: &'a SceneSnapshotSampledImageLayer,
    opacity: f64,
    effect_uv_space: Option<NativeVulkanSceneEffectUvSpace>,
}

struct NativeVulkanSceneRuntimeEyeLayerRecord<'a> {
    layer_index: usize,
    layer: &'a SceneSnapshotSampledImageLayer,
    vertices: Vec<NativeVulkanSceneSampledImageVertex>,
}

fn native_vulkan_scene_prepare_sampled_image_alpha_effect_layers(
    snapshot_time_ms: Option<u64>,
    layers: &[SceneSnapshotSampledImageLayer],
) -> Vec<NativeVulkanSceneFoldedSampledImageLayer<'_>> {
    layers
        .iter()
        .enumerate()
        .map(
            |(layer_index, layer)| {
                let effect_uv_space =
                    native_vulkan_scene_opacity_effect_uv_space_from_sampled_layer(layer);
                if native_vulkan_effect_debug_enabled() && layer.alpha_texture_slot.is_some() {
                    native_vulkan_scene_runtime_effect_debug_log(format_args!(
                        "time_ms={} alpha texture layer index={} alpha_slot={:?} slots={} geometry={} key={:?} effect_uv_space={}",
                        snapshot_time_ms
                            .map(|time| time.to_string())
                            .unwrap_or_else(|| "<unknown>".to_owned()),
                        layer_index,
                        layer.alpha_texture_slot,
                        native_vulkan_scene_sampled_texture_slots_label(&layer.texture_slots),
                        native_vulkan_scene_sampled_layer_geometry_label(layer),
                        layer.composite_key,
                        native_vulkan_scene_runtime_effect_uv_space_label(effect_uv_space)
                    ));
                }
                NativeVulkanSceneFoldedSampledImageLayer {
                    layer_index,
                    layer,
                    opacity: layer.opacity,
                    effect_uv_space,
                }
            }
        )
        .collect::<Vec<_>>()
}

fn native_vulkan_scene_opacity_effect_uv_space_from_sampled_layers(
    _target: &SceneSnapshotSampledImageLayer,
    carrier: &SceneSnapshotSampledImageLayer,
) -> NativeVulkanSceneEffectUvSpace {
    native_vulkan_scene_effect_uv_space_from_transform(
        native_vulkan_scene_effect_uv_transform_for_scene_passes(
            &carrier.image_effect_passes,
            carrier.alpha_texture_slot,
        ),
        carrier.width.unwrap_or(0.0),
        carrier.height.unwrap_or(0.0),
        carrier.texture_region,
        carrier.transform,
    )
}

fn native_vulkan_scene_opacity_effect_uv_space_from_sampled_layer(
    layer: &SceneSnapshotSampledImageLayer,
) -> Option<NativeVulkanSceneEffectUvSpace> {
    layer.alpha_texture_slot?;
    Some(native_vulkan_scene_effect_uv_space_from_transform(
        native_vulkan_scene_effect_uv_transform_for_scene_passes(
            &layer.image_effect_passes,
            layer.alpha_texture_slot,
        ),
        layer.width.unwrap_or(0.0),
        layer.height.unwrap_or(0.0),
        layer.texture_region,
        layer.transform,
    ))
}

fn native_vulkan_scene_runtime_effect_debug_log(args: std::fmt::Arguments<'_>) {
    native_vulkan_effect_debug_log_limited(
        &SCENE_RUNTIME_EFFECT_DEBUG_LOG_COUNT,
        48,
        "runtime.composite",
        args,
    );
}

fn native_vulkan_scene_sampled_layer_uses_first_class_effect_target(
    layer: &SceneSnapshotSampledImageLayer,
) -> bool {
    layer.image_effect_passes.iter().any(|pass| {
        native_vulkan_scene_effect_pass_uses_first_class_target(
            pass.runtime.as_deref(),
            &pass.effect_file,
        )
    })
}

fn native_vulkan_scene_render_layer_suppresses_unimplemented_we_effect_chain(
    layer: &SceneRenderLayer,
) -> bool {
    !layer.image_effect_passes.iter().any(|pass| {
        native_vulkan_scene_effect_pass_uses_first_class_target(
            pass.runtime.as_deref(),
            &pass.effect_file,
        )
    }) && layer
        .image_effect_passes
        .iter()
        .any(|pass| native_vulkan_scene_effect_pass_is_unimplemented_water_chain(&pass.effect_file))
}

fn native_vulkan_scene_snapshot_layer_suppresses_unimplemented_we_effect_chain(
    layer: &SceneSnapshotLayer,
) -> bool {
    !layer.image_effect_passes.iter().any(|pass| {
        native_vulkan_scene_effect_pass_uses_first_class_target(
            pass.runtime.as_deref(),
            &pass.effect_file,
        )
    }) && layer
        .image_effect_passes
        .iter()
        .any(|pass| native_vulkan_scene_effect_pass_is_unimplemented_water_chain(&pass.effect_file))
}

fn native_vulkan_scene_sampled_layer_suppresses_unimplemented_we_effect_chain(
    layer: &SceneSnapshotSampledImageLayer,
) -> bool {
    !native_vulkan_scene_sampled_layer_uses_first_class_effect_target(layer)
        && layer.image_effect_passes.iter().any(|pass| {
            native_vulkan_scene_effect_pass_is_unimplemented_water_chain(&pass.effect_file)
        })
}

fn native_vulkan_scene_effect_pass_uses_first_class_target(
    runtime: Option<&str>,
    effect_file: &str,
) -> bool {
    if runtime == Some("native-iris-mask") || runtime == Some("native-opacity-mask") {
        return true;
    }
    let file = effect_file.replace('\\', "/").to_ascii_lowercase();
    file == "effects/iris/effect.json"
        || file.ends_with("/effects/iris/effect.json")
        || file == "effects/opacity/effect.json"
        || file.ends_with("/effects/opacity/effect.json")
}

fn native_vulkan_scene_effect_pass_is_unimplemented_water_chain(effect_file: &str) -> bool {
    let file = effect_file.replace('\\', "/").to_ascii_lowercase();
    file.contains("waterripple")
        || file.contains("water_ripple")
        || file.contains("waterflow")
        || file.contains("water_flow")
        || file.contains("watercaustics")
        || file.contains("water_caustics")
}

fn native_vulkan_scene_runtime_eye_contribution_debug(
    snapshot_time_ms: Option<u64>,
    layer_index: usize,
    layer: &SceneSnapshotSampledImageLayer,
    vertices: &[NativeVulkanSceneSampledImageVertex],
    package_root: Option<&Path>,
) {
    let Some(effect_label) = native_vulkan_scene_sampled_layer_eye_effect_label(layer) else {
        return;
    };
    if !native_vulkan_scene_runtime_should_log_eye_frame(snapshot_time_ms) {
        return;
    }
    let mask_slot = native_vulkan_scene_sampled_layer_effect_mask_slot(layer);
    let mode = native_vulkan_scene_sampled_layer_effect_mode(layer);
    let split = if native_vulkan_scene_sampled_layer_uses_first_class_effect_target(layer)
        && vertices.len() > 4
    {
        vertices.len().saturating_sub(4)
    } else {
        vertices.len()
    };
    let (base_vertices, final_vertices) = vertices.split_at(split);
    let effect_uv_space = native_vulkan_scene_runtime_effect_uv_space_label(
        native_vulkan_scene_opacity_effect_uv_space_from_sampled_layer(layer),
    );
    let mask_report = mask_slot
        .map(|mask_slot| {
            native_vulkan_scene_runtime_eye_mask_report(
                package_root,
                mask_slot,
                base_vertices,
                final_vertices,
            )
        })
        .unwrap_or_else(|| "<none>".to_owned());
    let base_texture_report =
        native_vulkan_scene_runtime_eye_base_texture_report(package_root, layer, base_vertices);
    native_vulkan_effect_debug_log(
        "runtime.eye-contribution",
        format_args!(
            "time_ms={} layer_index={} layer_id={} effect={} mode={} opacity={:.3} alpha_slot={:?} base_slot={} mask_source={} mask_extent={} effect_uv_space={} puppet_frame={} passes={} vertices={} geometry_semantics={} base_pos_range={} base_uv_range={} base_effect_uv_range={} base_opacity_range={} final_pos_range={} final_uv_range={} final_effect_uv_range={} final_opacity_range={} alpha_zero_reason={} alpha_semantics={} base_texture_report={} mask_report={}",
            snapshot_time_ms
                .map(|time| time.to_string())
                .unwrap_or_else(|| "<unknown>".to_owned()),
            layer_index,
            layer.id,
            effect_label,
            mode.as_str(),
            layer.opacity,
            mask_slot.map(|slot| slot.slot),
            native_vulkan_scene_sampled_layer_base_slot_label(layer),
            mask_slot
                .map(|slot| slot.source.as_str())
                .unwrap_or("<none>"),
            mask_slot
                .map(|slot| native_vulkan_scene_texture_slot_extent_label(slot.width, slot.height))
                .unwrap_or_else(|| "<none>".to_owned()),
            effect_uv_space,
            native_vulkan_scene_puppet_animation_frames_label(&layer.puppet_animation_frames),
            native_vulkan_scene_sampled_image_effect_passes_label(&layer.image_effect_passes),
            vertices.len(),
            native_vulkan_scene_sampled_layer_effect_geometry_semantics_label(layer, vertices),
            native_vulkan_scene_sampled_vertices_position_range_label(base_vertices),
            native_vulkan_scene_sampled_vertices_uv_range_label(base_vertices),
            native_vulkan_scene_sampled_vertices_effect_uv_range_label(base_vertices),
            native_vulkan_scene_sampled_vertices_opacity_range_label(base_vertices),
            native_vulkan_scene_sampled_vertices_position_range_label(final_vertices),
            native_vulkan_scene_sampled_vertices_uv_range_label(final_vertices),
            native_vulkan_scene_sampled_vertices_effect_uv_range_label(final_vertices),
            native_vulkan_scene_sampled_vertices_opacity_range_label(final_vertices),
            native_vulkan_scene_eye_alpha_zero_reason_label(mask_slot.map(|slot| slot.slot), mode),
            native_vulkan_scene_eye_alpha_semantics_label(mode),
            base_texture_report,
            mask_report,
        ),
    );
}

fn native_vulkan_scene_runtime_eye_overlap_debug(
    snapshot_time_ms: Option<u64>,
    records: &[NativeVulkanSceneRuntimeEyeLayerRecord<'_>],
    package_root: Option<&Path>,
) {
    let Some(base) = records.iter().find(|record| {
        native_vulkan_scene_sampled_layer_eye_effect_label(record.layer) == Some("iris-base-eye")
    }) else {
        return;
    };
    let Some(overlay) = records.iter().find(|record| {
        native_vulkan_scene_sampled_layer_eye_effect_label(record.layer)
            == Some("opacity-duplicate-overlay")
    }) else {
        return;
    };
    let report = native_vulkan_scene_runtime_eye_overlap_report(base, overlay, package_root)
        .unwrap_or_else(|err| format!("error={err}"));
    native_vulkan_effect_debug_log(
        "runtime.eye-overlap",
        format_args!(
            "time_ms={} base_layer_index={} base_layer_id={} overlay_layer_index={} overlay_layer_id={} {}",
            snapshot_time_ms
                .map(|time| time.to_string())
                .unwrap_or_else(|| "<unknown>".to_owned()),
            base.layer_index,
            base.layer.id,
            overlay.layer_index,
            overlay.layer.id,
            report,
        ),
    );
}

fn native_vulkan_scene_runtime_eye_overlap_report(
    base: &NativeVulkanSceneRuntimeEyeLayerRecord<'_>,
    overlay: &NativeVulkanSceneRuntimeEyeLayerRecord<'_>,
    package_root: Option<&Path>,
) -> Result<String, String> {
    if native_vulkan_scene_sampled_layer_uses_first_class_effect_target(base.layer) {
        let base_vertices =
            native_vulkan_scene_runtime_eye_base_vertices(base.layer, &base.vertices);
        let final_vertex_count = base.vertices.len().saturating_sub(base_vertices.len());
        return Ok(format!(
            "direct_base_swapchain=false base_layer_rendering=effect-target-local-pass-then-final-quad base_target_vertices={} final_quad_vertices={} overlay_vertices={} note=base pupil mesh is no longer emitted directly to swapchain; opacity duplicate remains an independent later draw",
            base_vertices.len(),
            final_vertex_count,
            overlay.vertices.len()
        ));
    }
    let base_slot = base
        .layer
        .texture_slots
        .iter()
        .find(|slot| slot.slot == 0)
        .ok_or_else(|| "base eye has no texture slot 0".to_owned())?;
    let mask_slot = native_vulkan_scene_sampled_layer_alpha_slot(overlay.layer)
        .ok_or_else(|| "opacity overlay has no alpha texture slot".to_owned())?;
    let base_path =
        native_vulkan_scene_runtime_texture_slot_path(package_root, base_slot.source.as_str());
    let mask_path =
        native_vulkan_scene_runtime_texture_slot_path(package_root, mask_slot.source.as_str());
    let base_texture = native_vulkan_effect_debug_read_bc7_mode6_gtex_cached(&base_path)?;
    let mask_texture = native_vulkan_effect_debug_read_r8_gtex_cached(&mask_path)?;
    let base_mesh = base
        .layer
        .mesh
        .as_ref()
        .ok_or_else(|| "base eye has no mesh".to_owned())?;
    let overlay_mesh = overlay
        .layer
        .mesh
        .as_ref()
        .ok_or_else(|| "opacity overlay has no mesh".to_owned())?;
    let base_vertices = native_vulkan_scene_runtime_eye_base_vertices(base.layer, &base.vertices);
    let overlay_vertices =
        native_vulkan_scene_runtime_eye_base_vertices(overlay.layer, &overlay.vertices);
    let paired_triangles = (base_mesh.indices.len().min(overlay_mesh.indices.len())) / 3;
    if paired_triangles == 0 {
        return Ok(format!(
            "paired_triangles=0 base_vertices={} overlay_vertices={} base_indices={} overlay_indices={}",
            base_vertices.len(),
            overlay_vertices.len(),
            base_mesh.indices.len(),
            overlay_mesh.indices.len()
        ));
    }

    let mut same_index_triangles = 0usize;
    let mut valid_pairs = 0usize;
    let mut aligned_pairs = 0usize;
    let mut delta_sum = 0.0;
    let mut delta_max = 0.0_f64;
    let mut dark = NativeVulkanSceneRuntimeEyeOverlapDarkStats::default();
    let mut dark_uv_range = NativeVulkanSceneRuntimeEyeOverlapRange2::default();
    let mut dark_pos_range = NativeVulkanSceneRuntimeEyeOverlapRange2::default();
    let mut dark_mask_uv_range = NativeVulkanSceneRuntimeEyeOverlapRange2::default();

    for triangle_index in 0..paired_triangles {
        let base_triangle = &base_mesh.indices[triangle_index * 3..triangle_index * 3 + 3];
        let overlay_triangle = &overlay_mesh.indices[triangle_index * 3..triangle_index * 3 + 3];
        if base_triangle == overlay_triangle {
            same_index_triangles += 1;
        }
        let Some(base_centroid) =
            native_vulkan_scene_runtime_eye_triangle_centroid(base_vertices, base_triangle)
        else {
            continue;
        };
        let Some(overlay_centroid) =
            native_vulkan_scene_runtime_eye_triangle_centroid(overlay_vertices, overlay_triangle)
        else {
            continue;
        };
        valid_pairs += 1;
        let delta = native_vulkan_scene_runtime_eye_position_delta(
            base_centroid.position,
            overlay_centroid.position,
        );
        delta_sum += delta;
        delta_max = delta_max.max(delta);
        let aligned = delta <= SCENE_RUNTIME_EYE_ALIGNMENT_EPSILON_PX;
        if aligned {
            aligned_pairs += 1;
        }

        let base_color = base_texture.sample_linear(base_centroid.uv);
        let base_luma = native_vulkan_scene_runtime_eye_luma(base_color);
        let base_output_alpha =
            base_color[3] * f64::from(base_centroid.opacity) * f64::from(base_centroid.tint_alpha);
        if base_output_alpha <= SCENE_RUNTIME_EYE_VISIBLE_ALPHA_THRESHOLD
            || base_luma >= SCENE_RUNTIME_EYE_DARK_LUMA_THRESHOLD
        {
            continue;
        }

        let mask_value = mask_texture.sample_linear(overlay_centroid.effect_uv);
        let overlay_color = base_texture.sample_linear(overlay_centroid.uv);
        let overlay_luma = native_vulkan_scene_runtime_eye_luma(overlay_color);
        let overlay_output_alpha = overlay_color[3]
            * f64::from(overlay_centroid.opacity)
            * f64::from(overlay_centroid.tint_alpha)
            * (mask_value / 255.0);
        dark.include(
            aligned,
            base_centroid.opacity,
            mask_value,
            overlay_output_alpha,
            overlay_luma,
        );
        dark_uv_range.include(base_centroid.uv);
        dark_pos_range.include(base_centroid.position);
        dark_mask_uv_range.include(overlay_centroid.effect_uv);
    }

    let delta_mean = if valid_pairs > 0 {
        delta_sum / valid_pairs as f64
    } else {
        0.0
    };
    Ok(format!(
        "paired_triangles={} valid_pairs={} same_index_triangles={}/{} vertex_counts={}/{} index_counts={}/{} screen_delta_px_mean={:.4} screen_delta_px_max={:.4} aligned_le{:.1}px={}/{} base_dark_output_alpha_gt{:.0}_luma_lt{:.0}={} dark_base_opacity_below_one={} dark_aligned={}/{} dark_mask_zero={} dark_mask_gt0={} dark_mask_gt127={} dark_mask_full={} dark_overlay_alpha_gt{:.0}={} dark_aligned_overlay_alpha_gt{:.0}={} dark_overlay_still_dark_alpha_gt{:.0}={} dark_uncovered_by_aligned_overlay_alpha={} dark_mask_value={} dark_base_uv={} dark_mask_uv={} dark_pos={}",
        paired_triangles,
        valid_pairs,
        same_index_triangles,
        paired_triangles,
        base_vertices.len(),
        overlay_vertices.len(),
        base_mesh.indices.len(),
        overlay_mesh.indices.len(),
        delta_mean,
        delta_max,
        SCENE_RUNTIME_EYE_ALIGNMENT_EPSILON_PX,
        aligned_pairs,
        valid_pairs,
        SCENE_RUNTIME_EYE_VISIBLE_ALPHA_THRESHOLD,
        SCENE_RUNTIME_EYE_DARK_LUMA_THRESHOLD,
        dark.total,
        dark.base_opacity_below_one,
        dark.aligned,
        dark.total,
        dark.mask_zero,
        dark.mask_gt0,
        dark.mask_gt127,
        dark.mask_full,
        SCENE_RUNTIME_EYE_VISIBLE_ALPHA_THRESHOLD,
        dark.overlay_alpha_gt_visible,
        SCENE_RUNTIME_EYE_VISIBLE_ALPHA_THRESHOLD,
        dark.aligned_overlay_alpha_gt_visible,
        SCENE_RUNTIME_EYE_VISIBLE_ALPHA_THRESHOLD,
        dark.overlay_dark_alpha_gt_visible,
        dark.total
            .saturating_sub(dark.aligned_overlay_alpha_gt_visible),
        dark.mask_value_label(),
        dark_uv_range.label(),
        dark_mask_uv_range.label(),
        dark_pos_range.label(),
    ))
}

fn native_vulkan_scene_runtime_eye_base_vertices<'a>(
    layer: &SceneSnapshotSampledImageLayer,
    vertices: &'a [NativeVulkanSceneSampledImageVertex],
) -> &'a [NativeVulkanSceneSampledImageVertex] {
    if native_vulkan_scene_sampled_layer_uses_first_class_effect_target(layer) && vertices.len() > 4
    {
        &vertices[..vertices.len().saturating_sub(4)]
    } else {
        vertices
    }
}

#[derive(Clone, Copy)]
struct NativeVulkanSceneRuntimeEyeTriangleCentroid {
    position: [f32; 2],
    uv: [f32; 2],
    effect_uv: [f32; 2],
    opacity: f32,
    tint_alpha: f32,
}

fn native_vulkan_scene_runtime_eye_triangle_centroid(
    vertices: &[NativeVulkanSceneSampledImageVertex],
    triangle: &[u32],
) -> Option<NativeVulkanSceneRuntimeEyeTriangleCentroid> {
    if triangle.len() != 3 {
        return None;
    }
    let a = vertices.get(usize::try_from(triangle[0]).ok()?)?;
    let b = vertices.get(usize::try_from(triangle[1]).ok()?)?;
    let c = vertices.get(usize::try_from(triangle[2]).ok()?)?;
    Some(NativeVulkanSceneRuntimeEyeTriangleCentroid {
        position: [
            (a.position[0] + b.position[0] + c.position[0]) / 3.0,
            (a.position[1] + b.position[1] + c.position[1]) / 3.0,
        ],
        uv: [
            (a.uv[0] + b.uv[0] + c.uv[0]) / 3.0,
            (a.uv[1] + b.uv[1] + c.uv[1]) / 3.0,
        ],
        effect_uv: [
            (a.effect_uv[0] + b.effect_uv[0] + c.effect_uv[0]) / 3.0,
            (a.effect_uv[1] + b.effect_uv[1] + c.effect_uv[1]) / 3.0,
        ],
        opacity: (a.opacity + b.opacity + c.opacity) / 3.0,
        tint_alpha: (a.tint[3] + b.tint[3] + c.tint[3]) / 3.0,
    })
}

fn native_vulkan_scene_runtime_eye_position_delta(a: [f32; 2], b: [f32; 2]) -> f64 {
    let dx = f64::from(a[0]) - f64::from(b[0]);
    let dy = f64::from(a[1]) - f64::from(b[1]);
    dx.hypot(dy)
}

fn native_vulkan_scene_runtime_eye_luma(color: [f64; 4]) -> f64 {
    color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722
}

#[derive(Default)]
struct NativeVulkanSceneRuntimeEyeOverlapDarkStats {
    total: usize,
    aligned: usize,
    base_opacity_below_one: usize,
    mask_zero: usize,
    mask_gt0: usize,
    mask_gt127: usize,
    mask_full: usize,
    overlay_alpha_gt_visible: usize,
    aligned_overlay_alpha_gt_visible: usize,
    overlay_dark_alpha_gt_visible: usize,
    mask_min: f64,
    mask_max: f64,
    mask_sum: f64,
    has_mask_value: bool,
}

impl NativeVulkanSceneRuntimeEyeOverlapDarkStats {
    fn include(
        &mut self,
        aligned: bool,
        base_opacity: f32,
        mask_value: f64,
        overlay_output_alpha: f64,
        overlay_luma: f64,
    ) {
        self.total += 1;
        if aligned {
            self.aligned += 1;
        }
        if base_opacity < 0.999 {
            self.base_opacity_below_one += 1;
        }
        if mask_value <= 0.5 {
            self.mask_zero += 1;
        }
        if mask_value > 0.0 {
            self.mask_gt0 += 1;
        }
        if mask_value > 127.0 {
            self.mask_gt127 += 1;
        }
        if mask_value >= 254.5 {
            self.mask_full += 1;
        }
        if overlay_output_alpha > SCENE_RUNTIME_EYE_VISIBLE_ALPHA_THRESHOLD {
            self.overlay_alpha_gt_visible += 1;
            if aligned {
                self.aligned_overlay_alpha_gt_visible += 1;
            }
            if overlay_luma < SCENE_RUNTIME_EYE_DARK_LUMA_THRESHOLD {
                self.overlay_dark_alpha_gt_visible += 1;
            }
        }
        if self.has_mask_value {
            self.mask_min = self.mask_min.min(mask_value);
            self.mask_max = self.mask_max.max(mask_value);
        } else {
            self.mask_min = mask_value;
            self.mask_max = mask_value;
            self.has_mask_value = true;
        }
        self.mask_sum += mask_value;
    }

    fn mask_value_label(&self) -> String {
        if self.total == 0 || !self.has_mask_value {
            return "<none>".to_owned();
        }
        format!(
            "{:.1}..{:.1}/mean={:.1}",
            self.mask_min,
            self.mask_max,
            self.mask_sum / self.total as f64
        )
    }
}

#[derive(Default)]
struct NativeVulkanSceneRuntimeEyeOverlapRange2 {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    initialized: bool,
}

impl NativeVulkanSceneRuntimeEyeOverlapRange2 {
    fn include(&mut self, value: [f32; 2]) {
        if !value[0].is_finite() || !value[1].is_finite() {
            return;
        }
        if self.initialized {
            self.min_x = self.min_x.min(value[0]);
            self.min_y = self.min_y.min(value[1]);
            self.max_x = self.max_x.max(value[0]);
            self.max_y = self.max_y.max(value[1]);
        } else {
            self.min_x = value[0];
            self.min_y = value[1];
            self.max_x = value[0];
            self.max_y = value[1];
            self.initialized = true;
        }
    }

    fn label(&self) -> String {
        if !self.initialized {
            return "<none>".to_owned();
        }
        format!(
            "x={:.3}..{:.3} y={:.3}..{:.3}",
            self.min_x, self.max_x, self.min_y, self.max_y
        )
    }
}

fn native_vulkan_scene_runtime_should_log_eye_frame(snapshot_time_ms: Option<u64>) -> bool {
    static LOG_EVERY_FRAME: OnceLock<bool> = OnceLock::new();
    let every_frame = *LOG_EVERY_FRAME.get_or_init(|| {
        std::env::var("GILDER_NATIVE_VULKAN_EFFECT_DEBUG_FRAMES")
            .map(|value| {
                let value = value.trim().to_ascii_lowercase();
                !value.is_empty()
                    && value != "0"
                    && value != "false"
                    && value != "off"
                    && value != "no"
            })
            .unwrap_or(false)
    });
    if every_frame {
        return true;
    }
    let Some(time_ms) = snapshot_time_ms else {
        return true;
    };
    time_ms % 500 <= 20
}

fn native_vulkan_scene_runtime_eye_mask_report(
    package_root: Option<&Path>,
    mask_slot: &SceneTextureSlot,
    base_vertices: &[NativeVulkanSceneSampledImageVertex],
    final_vertices: &[NativeVulkanSceneSampledImageVertex],
) -> String {
    let mask_path =
        native_vulkan_scene_runtime_texture_slot_path(package_root, mask_slot.source.as_str());
    let base_effect_uvs = native_vulkan_scene_sampled_vertices_effect_uvs(base_vertices, false);
    let final_effect_uvs = native_vulkan_scene_sampled_vertices_effect_uvs(final_vertices, false);
    let sparse_final_effect_uvs =
        native_vulkan_scene_sampled_vertices_effect_uvs(final_vertices, true);
    let canonical_samples = [
        [0.0, 0.0],
        [0.25, 0.25],
        [0.5, 0.5],
        [0.75, 0.75],
        [1.0, 1.0],
    ];
    let groups = [
        NativeVulkanEffectDebugR8UvGroup {
            label: "canonical",
            sample_uvs: &canonical_samples,
            coverage_uvs: &canonical_samples,
        },
        NativeVulkanEffectDebugR8UvGroup {
            label: "base_effect",
            sample_uvs: &[],
            coverage_uvs: &base_effect_uvs,
        },
        NativeVulkanEffectDebugR8UvGroup {
            label: "final_effect",
            sample_uvs: &sparse_final_effect_uvs,
            coverage_uvs: &final_effect_uvs,
        },
    ];
    match native_vulkan_effect_debug_r8_gtex_group_report(&mask_path, &groups) {
        Ok(report) => format!("path={} {}", mask_path.display(), report),
        Err(err) => format!("path={} error={err}", mask_path.display()),
    }
}

fn native_vulkan_scene_runtime_eye_base_texture_report(
    package_root: Option<&Path>,
    layer: &SceneSnapshotSampledImageLayer,
    base_vertices: &[NativeVulkanSceneSampledImageVertex],
) -> String {
    let Some(base_slot) = layer.texture_slots.iter().find(|slot| slot.slot == 0) else {
        return "path=<missing-base-slot>".to_owned();
    };
    let base_path =
        native_vulkan_scene_runtime_texture_slot_path(package_root, base_slot.source.as_str());
    let sparse_base_uvs = native_vulkan_scene_sampled_vertices_uvs(base_vertices, true);
    let base_vertex_uvs = native_vulkan_scene_sampled_vertices_uvs(base_vertices, false);
    let triangle_centroid_uvs =
        native_vulkan_scene_sampled_layer_triangle_centroid_uvs(layer, base_vertices);
    let canonical_samples = [
        [0.0, 0.0],
        [0.25, 0.25],
        [0.5, 0.5],
        [0.75, 0.75],
        [1.0, 1.0],
    ];
    let groups = [
        NativeVulkanEffectDebugRgbaUvGroup {
            label: "canonical",
            sample_uvs: &canonical_samples,
            coverage_uvs: &canonical_samples,
        },
        NativeVulkanEffectDebugRgbaUvGroup {
            label: "base_vertex_uv",
            sample_uvs: &sparse_base_uvs,
            coverage_uvs: &base_vertex_uvs,
        },
        NativeVulkanEffectDebugRgbaUvGroup {
            label: "base_tri_centroid_uv",
            sample_uvs: &[],
            coverage_uvs: &triangle_centroid_uvs,
        },
    ];
    match native_vulkan_effect_debug_bc7_mode6_gtex_group_report(&base_path, &groups) {
        Ok(report) => format!(
            "path={}{} {}",
            base_path.display(),
            native_vulkan_scene_texture_slot_extent_label(base_slot.width, base_slot.height),
            report
        ),
        Err(err) => format!("path={} error={err}", base_path.display()),
    }
}

fn native_vulkan_scene_runtime_texture_slot_path(
    package_root: Option<&Path>,
    source: &str,
) -> PathBuf {
    let source_path = Path::new(source);
    if source_path.is_absolute() {
        return source_path.to_path_buf();
    }
    package_root
        .map(|root| root.join(source))
        .unwrap_or_else(|| PathBuf::from(source))
}

fn native_vulkan_scene_sampled_vertices_effect_uvs(
    vertices: &[NativeVulkanSceneSampledImageVertex],
    sparse: bool,
) -> Vec<[f32; 2]> {
    if vertices.is_empty() {
        return Vec::new();
    }
    if !sparse {
        return vertices.iter().map(|vertex| vertex.effect_uv).collect();
    }
    let mut uvs = Vec::with_capacity(6);
    let offsets = [
        0usize,
        vertices.len() / 8,
        vertices.len() / 4,
        vertices.len() / 2,
        vertices.len().saturating_mul(3) / 4,
        vertices.len().saturating_sub(1),
    ];
    for offset in offsets {
        uvs.push(vertices[offset.min(vertices.len() - 1)].effect_uv);
    }
    uvs
}

fn native_vulkan_scene_sampled_vertices_uvs(
    vertices: &[NativeVulkanSceneSampledImageVertex],
    sparse: bool,
) -> Vec<[f32; 2]> {
    if vertices.is_empty() {
        return Vec::new();
    }
    if !sparse {
        return vertices.iter().map(|vertex| vertex.uv).collect();
    }
    let mut uvs = Vec::with_capacity(6);
    let offsets = [
        0usize,
        vertices.len() / 8,
        vertices.len() / 4,
        vertices.len() / 2,
        vertices.len().saturating_mul(3) / 4,
        vertices.len().saturating_sub(1),
    ];
    for offset in offsets {
        uvs.push(vertices[offset.min(vertices.len() - 1)].uv);
    }
    uvs
}

fn native_vulkan_scene_sampled_layer_triangle_centroid_uvs(
    layer: &SceneSnapshotSampledImageLayer,
    base_vertices: &[NativeVulkanSceneSampledImageVertex],
) -> Vec<[f32; 2]> {
    let Some(mesh) = layer.mesh.as_ref() else {
        return Vec::new();
    };
    if mesh.vertices.len() != base_vertices.len() {
        return Vec::new();
    }
    let mut uvs = Vec::with_capacity(mesh.indices.len() / 3);
    for triangle in mesh.indices.chunks_exact(3) {
        let Some(a) = usize::try_from(triangle[0])
            .ok()
            .and_then(|index| base_vertices.get(index))
        else {
            continue;
        };
        let Some(b) = usize::try_from(triangle[1])
            .ok()
            .and_then(|index| base_vertices.get(index))
        else {
            continue;
        };
        let Some(c) = usize::try_from(triangle[2])
            .ok()
            .and_then(|index| base_vertices.get(index))
        else {
            continue;
        };
        uvs.push([
            (a.uv[0] + b.uv[0] + c.uv[0]) / 3.0,
            (a.uv[1] + b.uv[1] + c.uv[1]) / 3.0,
        ]);
    }
    uvs
}

fn native_vulkan_scene_sampled_layer_alpha_slot(
    layer: &SceneSnapshotSampledImageLayer,
) -> Option<&SceneTextureSlot> {
    let alpha_slot = layer.alpha_texture_slot?;
    layer
        .texture_slots
        .iter()
        .find(|slot| slot.slot == alpha_slot)
}

fn native_vulkan_scene_sampled_layer_effect_mask_slot(
    layer: &SceneSnapshotSampledImageLayer,
) -> Option<&SceneTextureSlot> {
    native_vulkan_scene_sampled_layer_alpha_slot(layer).or_else(|| {
        layer
            .image_effect_passes
            .iter()
            .filter(|pass| {
                pass.runtime.as_deref() == Some("native-iris-mask")
                    || pass.runtime.as_deref() == Some("native-opacity-mask")
                    || native_vulkan_scene_effect_file_is_iris_mask(&pass.effect_file)
                    || native_vulkan_scene_effect_file_is_opacity_mask(&pass.effect_file)
            })
            .flat_map(|pass| pass.texture_slots.iter())
            .filter(|slot| slot.slot > 0)
            .min_by_key(|slot| slot.slot)
    })
}

fn native_vulkan_scene_sampled_layer_effect_mode(
    layer: &SceneSnapshotSampledImageLayer,
) -> SceneRenderAlphaTextureMode {
    if layer.image_effect_passes.iter().any(|pass| {
        pass.runtime.as_deref() == Some("native-iris-mask")
            || native_vulkan_scene_effect_file_is_iris_mask(&pass.effect_file)
    }) {
        return SceneRenderAlphaTextureMode::Iris;
    }
    if layer.image_effect_passes.iter().any(|pass| {
        pass.runtime.as_deref() == Some("native-opacity-mask")
            || native_vulkan_scene_effect_file_is_opacity_mask(&pass.effect_file)
    }) {
        return SceneRenderAlphaTextureMode::Coverage;
    }
    layer.alpha_texture_mode.into()
}

fn native_vulkan_scene_sampled_layer_is_eye_debug_target(
    layer: &SceneSnapshotSampledImageLayer,
) -> bool {
    native_vulkan_scene_sampled_layer_eye_effect_label(layer).is_some()
}

fn native_vulkan_scene_sampled_layer_eye_effect_label(
    layer: &SceneSnapshotSampledImageLayer,
) -> Option<&'static str> {
    if let Some(mask_slot) = native_vulkan_scene_sampled_layer_alpha_slot(layer)
        && let Some(label) = native_vulkan_scene_eye_effect_label(mask_slot.source.as_str())
    {
        return Some(label);
    }
    if layer.image_effect_passes.iter().any(|pass| {
        pass.runtime.as_deref() == Some("native-iris-mask")
            || native_vulkan_scene_effect_file_is_iris_mask(&pass.effect_file)
    }) {
        return Some("iris-base-eye");
    }
    if layer.image_effect_passes.iter().any(|pass| {
        pass.runtime.as_deref() == Some("native-opacity-mask")
            || native_vulkan_scene_effect_file_is_opacity_mask(&pass.effect_file)
    }) {
        return Some("opacity-duplicate-overlay");
    }
    None
}

fn native_vulkan_scene_eye_effect_label(mask_source: &str) -> Option<&'static str> {
    let normalized = mask_source.to_ascii_lowercase();
    if normalized.contains("opacity_mask") || normalized.contains("opacity-mask") {
        return Some("opacity-duplicate-overlay");
    }
    if normalized.contains("iris_mask") || normalized.contains("iris-mask") {
        return Some("iris-base-eye");
    }
    None
}

fn native_vulkan_scene_effect_file_is_iris_mask(effect_file: &str) -> bool {
    let file = effect_file.replace('\\', "/").to_ascii_lowercase();
    file == "effects/iris/effect.json" || file.ends_with("/effects/iris/effect.json")
}

fn native_vulkan_scene_effect_file_is_opacity_mask(effect_file: &str) -> bool {
    let file = effect_file.replace('\\', "/").to_ascii_lowercase();
    file == "effects/opacity/effect.json" || file.ends_with("/effects/opacity/effect.json")
}

fn native_vulkan_scene_sampled_layer_effect_geometry_semantics_label(
    layer: &SceneSnapshotSampledImageLayer,
    vertices: &[NativeVulkanSceneSampledImageVertex],
) -> &'static str {
    let mode = native_vulkan_scene_sampled_layer_effect_mode(layer);
    if native_vulkan_scene_sampled_layer_uses_first_class_effect_target(layer) && vertices.len() > 4
    {
        if matches!(
            mode,
            SceneRenderAlphaTextureMode::Multiply | SceneRenderAlphaTextureMode::Coverage
        ) {
            return "we-opacity-effect-local-target-base-plus-final-quad";
        }
        if matches!(mode, SceneRenderAlphaTextureMode::Iris) {
            return "we-iris-effect-local-target-base-plus-final-quad";
        }
        return "we-image-effect-local-target-base-plus-final-quad";
    }
    if layer.alpha_texture_slot.is_some()
        && layer.mesh.is_some()
        && matches!(mode, SceneRenderAlphaTextureMode::Multiply)
    {
        return "we-opacity-effect-direct-puppet-mesh-material-uv";
    }
    if layer.alpha_texture_slot.is_some()
        && layer.mesh.is_some()
        && matches!(mode, SceneRenderAlphaTextureMode::Iris)
    {
        return "we-iris-effect-direct-puppet-mesh-pass-space-raw-v";
    }
    if layer.mesh.is_some() {
        "direct-puppet-mesh"
    } else {
        "direct-scene-quad"
    }
}

fn native_vulkan_scene_eye_alpha_zero_reason_label(
    alpha_texture_slot: Option<u32>,
    mode: SceneRenderAlphaTextureMode,
) -> &'static str {
    if alpha_texture_slot.is_none() {
        return "no-alpha-texture-on-this-draw; this layer cannot zero pixels by mask";
    }
    match mode {
        SceneRenderAlphaTextureMode::Multiply => {
            "opacity-mask-directly-multiplies-this-image-alpha"
        }
        SceneRenderAlphaTextureMode::Inverse => {
            "inverse-mask-can-zero-this-layer-only; previous layers remain untouched"
        }
        SceneRenderAlphaTextureMode::Iris => {
            "not-alpha-mask; iris mask scales uv offset, output alpha is sampled from g_Texture0/effect-target"
        }
        SceneRenderAlphaTextureMode::Coverage => {
            "coverage-mask-can-zero-this-layer-only; previous layers remain untouched"
        }
        SceneRenderAlphaTextureMode::IrisCoverage => {
            "iris first samples g_Texture0, then coverage texture can zero this layer"
        }
    }
}

fn native_vulkan_scene_eye_alpha_semantics_label(
    mode: SceneRenderAlphaTextureMode,
) -> &'static str {
    match mode {
        SceneRenderAlphaTextureMode::Multiply => {
            "straight_alpha_out = base_texture_alpha * opacity_mask * layer_opacity"
        }
        SceneRenderAlphaTextureMode::Inverse => {
            "straight_alpha_out = effect_target_alpha * (1 - mask) * layer_opacity"
        }
        SceneRenderAlphaTextureMode::Iris => {
            "iris pass perturbs g_Texture0 sampling only; it does not apply the opacity mask and cannot drive pupil alpha to zero"
        }
        SceneRenderAlphaTextureMode::Coverage => {
            "straight_alpha_out = effect_target_alpha * coverage_mask * layer_opacity"
        }
        SceneRenderAlphaTextureMode::IrisCoverage => {
            "iris pass perturbs g_Texture0 sampling and then gates alpha with coverage texture"
        }
    }
}

fn native_vulkan_scene_sampled_layer_base_slot_label(
    layer: &SceneSnapshotSampledImageLayer,
) -> String {
    layer
        .texture_slots
        .iter()
        .find(|slot| slot.slot == 0)
        .map(|slot| {
            format!(
                "{}{}",
                slot.source,
                native_vulkan_scene_texture_slot_extent_label(slot.width, slot.height)
            )
        })
        .unwrap_or_else(|| "<missing>".to_owned())
}

fn native_vulkan_scene_sampled_texture_slots_label(slots: &[SceneTextureSlot]) -> String {
    let mut label = String::new();
    label.push('[');
    for (index, slot) in slots.iter().enumerate() {
        if index > 0 {
            label.push_str(", ");
        }
        label.push_str(&format!(
            "{}:{}{}",
            slot.slot,
            slot.source,
            native_vulkan_scene_texture_slot_extent_label(slot.width, slot.height)
        ));
    }
    label.push(']');
    label
}

fn native_vulkan_scene_sampled_image_effect_passes_label(
    passes: &[crate::core::scene::SceneImageEffectPass],
) -> String {
    if passes.is_empty() {
        return "[]".to_owned();
    }
    let mut label = String::new();
    label.push('[');
    for (index, pass) in passes.iter().enumerate() {
        if index > 0 {
            label.push_str(", ");
        }
        label.push_str(&format!(
            "{}#{} runtime={} shader={} blend={}",
            pass.effect_file,
            pass.pass_index,
            pass.runtime.as_deref().unwrap_or("<none>"),
            pass.shader.as_deref().unwrap_or("<none>"),
            pass.blending.as_deref().unwrap_or("<none>")
        ));
    }
    label.push(']');
    label
}

fn native_vulkan_scene_puppet_animation_frames_label(
    frames: &[ScenePuppetAnimationFrameDebug],
) -> String {
    if frames.is_empty() {
        return "[]".to_owned();
    }
    let mut label = String::new();
    label.push('[');
    for (index, frame) in frames.iter().enumerate() {
        if index > 0 {
            label.push_str(", ");
        }
        label.push_str(&format!(
            "clip={} name={} layer={} frame={:.3} f0={} f1={} mix={:.3} fps={:.3} count={} loop={} rate={:.6} phase={:.3} blend={:.3} additive={} lock_transforms={}",
            frame.clip_id,
            frame.clip_name.as_deref().unwrap_or("<none>"),
            frame.layer_name.as_deref().unwrap_or("<none>"),
            frame.frame,
            frame.frame0,
            frame.frame1,
            frame.mix,
            frame.fps,
            frame.frame_count,
            frame.looping,
            frame.rate,
            frame.initial_phase,
            frame.blend,
            frame.additive,
            frame.lock_transforms,
        ));
    }
    label.push(']');
    label
}

fn native_vulkan_scene_texture_slot_extent_label(
    width: Option<u32>,
    height: Option<u32>,
) -> String {
    match (width, height) {
        (Some(width), Some(height)) => format!("({width}x{height})"),
        _ => String::new(),
    }
}

fn native_vulkan_scene_sampled_layer_geometry_label(
    layer: &SceneSnapshotSampledImageLayer,
) -> String {
    format!(
        "size={}x{} opacity={:.3} transform=({:.3},{:.3}, scale={:.3}/{:.3}, rot={:.3}, anchor={:.3}/{:.3}) mesh={}",
        layer
            .width
            .map(|width| format!("{width:.3}"))
            .unwrap_or_else(|| "<none>".to_owned()),
        layer
            .height
            .map(|height| format!("{height:.3}"))
            .unwrap_or_else(|| "<none>".to_owned()),
        layer.opacity,
        layer.transform.x,
        layer.transform.y,
        layer.transform.scale_x,
        layer.transform.scale_y,
        layer.transform.rotation_deg,
        layer.transform.anchor_x,
        layer.transform.anchor_y,
        layer
            .mesh
            .as_ref()
            .map(|mesh| format!(
                "vertices={} indices={} bounds={}",
                mesh.vertices.len(),
                mesh.indices.len(),
                native_vulkan_scene_mesh_bounds_label(mesh)
            ))
            .unwrap_or_else(|| "<none>".to_owned())
    )
}

fn native_vulkan_scene_mesh_bounds_label(mesh: &SceneMesh) -> String {
    if mesh.vertices.is_empty() {
        return "<empty>".to_owned();
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut min_u = f64::INFINITY;
    let mut min_v = f64::INFINITY;
    let mut max_u = f64::NEG_INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for vertex in &mesh.vertices {
        min_x = min_x.min(vertex.x);
        min_y = min_y.min(vertex.y);
        max_x = max_x.max(vertex.x);
        max_y = max_y.max(vertex.y);
        min_u = min_u.min(vertex.u);
        min_v = min_v.min(vertex.v);
        max_u = max_u.max(vertex.u);
        max_v = max_v.max(vertex.v);
    }
    format!(
        "xy={min_x:.3}..{max_x:.3}/{min_y:.3}..{max_y:.3} uv={min_u:.3}..{max_u:.3}/{min_v:.3}..{max_v:.3}"
    )
}

fn native_vulkan_scene_runtime_effect_uv_space_label(
    space: Option<NativeVulkanSceneEffectUvSpace>,
) -> String {
    let Some(space) = space else {
        return "<none>".to_owned();
    };
    let bounds = space
        .bounds
        .map(|bounds| {
            format!(
                "bounds(left={:.3}, top={:.3}, width={:.3}, height={:.3})",
                bounds.left, bounds.top, bounds.width, bounds.height
            )
        })
        .unwrap_or_else(|| "bounds=<none>".to_owned());
    format!(
        "width={:.3} height={:.3} {} texture_region={:?} transform=({:.3},{:.3}, scale={:.3}/{:.3}, rot={:.3}, anchor={:.3}/{:.3}) {}",
        space.width,
        space.height,
        native_vulkan_scene_runtime_effect_uv_mapping_label(space.mapping),
        space.texture_region,
        space.transform.x,
        space.transform.y,
        space.transform.scale_x,
        space.transform.scale_y,
        space.transform.rotation_deg,
        space.transform.anchor_x,
        space.transform.anchor_y,
        bounds
    )
}

fn native_vulkan_scene_runtime_effect_uv_mapping_label(
    mapping: NativeVulkanSceneEffectUvMapping,
) -> String {
    match mapping {
        NativeVulkanSceneEffectUvMapping::ScenePositionBounds => {
            "mapping=scene-position-bounds".to_owned()
        }
        NativeVulkanSceneEffectUvMapping::MaterialUvTransformed {
            scale_u,
            scale_v,
            offset_u,
            offset_v,
        } => {
            format!(
                "mapping=material-uv-transform(scale={scale_u:.6}/{scale_v:.6}, offset={offset_u:.6}/{offset_v:.6})"
            )
        }
    }
}

fn native_vulkan_scene_sampled_vertices_effect_uv_range_label(
    vertices: &[NativeVulkanSceneSampledImageVertex],
) -> String {
    if vertices.is_empty() {
        return "<empty>".to_owned();
    }
    let mut min_u = f32::INFINITY;
    let mut min_v = f32::INFINITY;
    let mut max_u = f32::NEG_INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    for vertex in vertices {
        min_u = min_u.min(vertex.effect_uv[0]);
        min_v = min_v.min(vertex.effect_uv[1]);
        max_u = max_u.max(vertex.effect_uv[0]);
        max_v = max_v.max(vertex.effect_uv[1]);
    }
    format!("u={min_u:.3}..{max_u:.3} v={min_v:.3}..{max_v:.3}")
}

fn native_vulkan_scene_sampled_vertices_uv_range_label(
    vertices: &[NativeVulkanSceneSampledImageVertex],
) -> String {
    if vertices.is_empty() {
        return "<empty>".to_owned();
    }
    let mut min_u = f32::INFINITY;
    let mut min_v = f32::INFINITY;
    let mut max_u = f32::NEG_INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    for vertex in vertices {
        min_u = min_u.min(vertex.uv[0]);
        min_v = min_v.min(vertex.uv[1]);
        max_u = max_u.max(vertex.uv[0]);
        max_v = max_v.max(vertex.uv[1]);
    }
    format!("u={min_u:.3}..{max_u:.3} v={min_v:.3}..{max_v:.3}")
}

fn native_vulkan_scene_sampled_vertices_position_range_label(
    vertices: &[NativeVulkanSceneSampledImageVertex],
) -> String {
    if vertices.is_empty() {
        return "<empty>".to_owned();
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for vertex in vertices {
        min_x = min_x.min(vertex.position[0]);
        min_y = min_y.min(vertex.position[1]);
        max_x = max_x.max(vertex.position[0]);
        max_y = max_y.max(vertex.position[1]);
    }
    format!("x={min_x:.1}..{max_x:.1} y={min_y:.1}..{max_y:.1}")
}

fn native_vulkan_scene_sampled_vertices_opacity_range_label(
    vertices: &[NativeVulkanSceneSampledImageVertex],
) -> String {
    if vertices.is_empty() {
        return "<empty>".to_owned();
    }
    let mut min_opacity = f32::INFINITY;
    let mut max_opacity = f32::NEG_INFINITY;
    let mut below_one = 0usize;
    for vertex in vertices {
        min_opacity = min_opacity.min(vertex.opacity);
        max_opacity = max_opacity.max(vertex.opacity);
        if vertex.opacity < 0.999 {
            below_one += 1;
        }
    }
    format!(
        "{min_opacity:.3}..{max_opacity:.3} below_one={}/{}",
        below_one,
        vertices.len()
    )
}

fn native_vulkan_scene_render_layer_from_snapshot_for_geometry(
    layer: &SceneSnapshotLayer,
) -> SceneRenderLayer {
    SceneRenderLayer {
        id: String::new(),
        kind: layer.kind,
        source: None,
        texture_slots: Vec::new(),
        alpha_texture_slot: None,
        alpha_texture_mode: Default::default(),
        image_effect_passes: Vec::new(),
        composite_key: layer.composite_key.clone(),
        texture_region: layer.texture_region,
        effect_motion: layer.effect_motion,
        blend_mode: layer.blend_mode,
        audio: Vec::new(),
        color: layer.color.clone(),
        stroke_color: layer.stroke_color.clone(),
        stroke_width: layer.stroke_width,
        corner_radius: layer.corner_radius,
        width: layer.width,
        height: layer.height,
        mesh: layer.mesh.clone(),
        text: layer.text.clone(),
        font_size: layer.font_size,
        font_family: layer.font_family.clone(),
        font_source: None,
        font_weight: layer.font_weight.clone(),
        text_align: layer.text_align,
        path_data: layer.path_data.clone(),
        path_fill_rule: layer.path_fill_rule,
        fit: layer.fit,
        opacity: layer.opacity,
        transform: layer.transform,
    }
}

fn native_vulkan_scene_sampled_source_index(sources: &mut Vec<PathBuf>, source: PathBuf) -> u32 {
    if let Some(index) = sources.iter().position(|existing| existing == &source) {
        return index.min(u32::MAX as usize) as u32;
    }
    let index = sources.len().min(u32::MAX as usize) as u32;
    sources.push(source);
    index
}

fn native_vulkan_scene_render_layer_texture_slot_bindings(
    layer: &SceneRenderLayer,
    base_resource_index: u32,
    sampled_source_indices: Option<&BTreeMap<PathBuf, u32>>,
    sampled_sources: &mut Vec<PathBuf>,
) -> Result<Vec<NativeVulkanVulkanaliaSceneTextureSlotResourceBinding>, String> {
    let mut resources = vec![base_resource_index];
    for slot in layer.texture_slots.iter().filter(|slot| slot.slot > 0) {
        let slot_index = usize::try_from(slot.slot)
            .map_err(|_| format!("scene texture slot {} exceeds usize", slot.slot))?;
        if resources.len() <= slot_index {
            resources.resize(slot_index + 1, base_resource_index);
        }
        resources[slot_index] = if let Some(source_indices) = sampled_source_indices {
            *source_indices.get(&slot.source).ok_or_else(|| {
                format!(
                    "dynamic scene sampled slot source {} is absent from retained sampled image topology",
                    slot.source.display()
                )
            })?
        } else {
            native_vulkan_scene_sampled_source_index(sampled_sources, slot.source.clone())
        };
    }
    Ok(native_vulkan_scene_vulkanalia_texture_slot_bindings_from_resources(resources))
}

fn native_vulkan_scene_snapshot_layer_texture_slot_bindings(
    layer: &SceneSnapshotLayer,
    base_resource_index: u32,
    sampled_source_indices: &BTreeMap<String, u32>,
) -> Result<Vec<NativeVulkanVulkanaliaSceneTextureSlotResourceBinding>, String> {
    let mut resources = vec![base_resource_index];
    for slot in layer.texture_slots.iter().filter(|slot| slot.slot > 0) {
        let slot_index = usize::try_from(slot.slot)
            .map_err(|_| format!("scene texture slot {} exceeds usize", slot.slot))?;
        if resources.len() <= slot_index {
            resources.resize(slot_index + 1, base_resource_index);
        }
        resources[slot_index] = *sampled_source_indices
            .get(slot.source.as_str())
            .ok_or_else(|| {
                format!(
                    "dynamic scene sampled package slot source {} is absent from retained sampled image topology",
                    slot.source.as_str()
                )
            })?;
    }
    Ok(native_vulkan_scene_vulkanalia_texture_slot_bindings_from_resources(resources))
}

fn native_vulkan_scene_vulkanalia_texture_slot_bindings_from_resources(
    resources: Vec<u32>,
) -> Vec<NativeVulkanVulkanaliaSceneTextureSlotResourceBinding> {
    resources
        .into_iter()
        .enumerate()
        .map(
            |(slot, resource_index)| NativeVulkanVulkanaliaSceneTextureSlotResourceBinding {
                slot: slot.min(u32::MAX as usize) as u32,
                resource_index,
            },
        )
        .collect()
}

fn native_vulkan_scene_render_layer_has_no_visual_geometry(layer: &SceneRenderLayer) -> bool {
    if layer.opacity <= 0.0 || native_vulkan_scene_render_layer_is_clear(layer) {
        return true;
    }
    match layer.kind {
        SceneNodeKind::Audio | SceneNodeKind::Script => true,
        SceneNodeKind::Color => layer.color.as_deref().is_none_or(|color| color.is_empty()),
        SceneNodeKind::Rectangle | SceneNodeKind::Ellipse => {
            layer.color.as_deref().is_none_or(|color| color.is_empty())
                && (layer
                    .stroke_color
                    .as_deref()
                    .is_none_or(|color| color.is_empty())
                    || layer.stroke_width.unwrap_or(1.0) <= 0.0)
        }
        _ => false,
    }
}

fn native_vulkan_scene_snapshot_layer_has_no_visual_geometry(layer: &SceneSnapshotLayer) -> bool {
    if layer.opacity <= 0.0 || native_vulkan_scene_snapshot_layer_is_clear(layer) {
        return true;
    }
    match layer.kind {
        SceneNodeKind::Audio | SceneNodeKind::Script => true,
        SceneNodeKind::Color => layer.color.as_deref().is_none_or(|color| color.is_empty()),
        SceneNodeKind::Rectangle | SceneNodeKind::Ellipse => {
            layer.color.as_deref().is_none_or(|color| color.is_empty())
                && (layer
                    .stroke_color
                    .as_deref()
                    .is_none_or(|color| color.is_empty())
                    || layer.stroke_width.unwrap_or(1.0) <= 0.0)
        }
        _ => false,
    }
}

fn native_vulkan_scene_snapshot_layer_is_clear(layer: &SceneSnapshotLayer) -> bool {
    layer.id == "scene-render-clear-color"
        && layer.kind == SceneNodeKind::Color
        && layer.opacity >= 1.0
        && layer.transform == SceneTransform::default()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneTextureSlotSnapshot {
    pub slot: u32,
    pub source: PathBuf,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneTextureSlotResourceBindingSnapshot {
    pub slot: u32,
    pub resource_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneBlendEquationSnapshot {
    pub src_color: &'static str,
    pub dst_color: &'static str,
    pub color_op: &'static str,
    pub src_alpha: &'static str,
    pub dst_alpha: &'static str,
    pub alpha_op: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneBlendStateSnapshot {
    pub mode: SceneBlendMode,
    pub equation: NativeVulkanSceneBlendEquationSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneRenderStateSnapshot {
    pub blend: NativeVulkanSceneBlendStateSnapshot,
    pub depth_test: &'static str,
    pub depth_write: &'static str,
    pub cull_mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneEffectRecordSnapshot {
    pub kind: &'static str,
    pub evaluation_boundary: &'static str,
    pub effect_file: String,
    pub runtime: Option<String>,
    pub pass_index: usize,
    pub command: Option<String>,
    pub source: Option<String>,
    pub target: Option<String>,
    pub binds: BTreeMap<u32, String>,
    pub fbos: Vec<SceneEffectFbo>,
    pub shader: Option<String>,
    pub blending: Option<String>,
    pub texture_slots: Vec<NativeVulkanSceneTextureSlotSnapshot>,
    pub parameter_keys: Vec<String>,
    pub combo_keys: Vec<String>,
    pub depth_test: &'static str,
    pub depth_write: &'static str,
    pub cull_mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneMaterialPassSnapshot {
    pub kind: &'static str,
    pub shader: Option<String>,
    pub blending: Option<String>,
    pub render_state: NativeVulkanSceneRenderStateSnapshot,
    pub alpha_texture_slot: Option<u32>,
    pub alpha_texture_mode: SceneRenderAlphaTextureMode,
    pub texture_slot_count: usize,
    pub effect_kinds: Vec<&'static str>,
    pub combo_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneWeImagePassSnapshot {
    pub pass_index: usize,
    pub role: &'static str,
    pub effect_kind: Option<&'static str>,
    pub effect_file: Option<String>,
    pub command: Option<String>,
    pub source: Option<String>,
    pub target_name: Option<String>,
    pub binds: BTreeMap<u32, String>,
    pub fbos: Vec<SceneEffectFbo>,
    pub shader: Option<String>,
    pub blending: Option<String>,
    pub scene_blend_mode: SceneBlendMode,
    pub render_state: NativeVulkanSceneRenderStateSnapshot,
    pub input: &'static str,
    pub input_name: Option<String>,
    pub target: &'static str,
    pub final_scene_pass: bool,
    pub texture_slots: Vec<NativeVulkanSceneTextureSlotSnapshot>,
    pub texture_slot_count: usize,
    pub parameter_keys: Vec<String>,
    pub combo_keys: Vec<String>,
    pub depth_test: &'static str,
    pub depth_write: &'static str,
    pub cull_mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneWeImagePassChainSnapshot {
    pub execution: &'static str,
    pub local_target_required: bool,
    pub ping_pong_required: bool,
    pub first_pass_blend_moved_to_final: bool,
    pub color_blend_passthrough: bool,
    pub final_scene_blend_mode: SceneBlendMode,
    pub raw_direct_composite_allowed: bool,
    pub unsupported_reason: Option<&'static str>,
    pub passes: Vec<NativeVulkanSceneWeImagePassSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneWeImageGraphStepSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub chain_index: usize,
    pub step_index: usize,
    pub execution: &'static str,
    pub raw_direct_composite_allowed: bool,
    pub unsupported_reason: Option<&'static str>,
    pub input_target_index: Option<u32>,
    pub output_target_index: Option<u32>,
    pub texture_bindings: Vec<NativeVulkanSceneWeImageGraphTextureBindingSnapshot>,
    pub pass: NativeVulkanSceneWeImagePassSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneWeImageGraphTextureBindingSnapshot {
    pub slot: u32,
    pub uniform: String,
    pub source: &'static str,
    pub resource_index: Option<u32>,
    pub planned_graph_resource_index: Option<u32>,
    pub target_index: Option<u32>,
    pub endpoint: Option<&'static str>,
    pub vulkan_effect_target_index: Option<u32>,
    pub bind_name: Option<String>,
    pub source_path: Option<PathBuf>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub resolution: Option<[u32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneWeImageGraphResourceSnapshot {
    pub resource_index: u32,
    pub resource_kind: &'static str,
    pub layer_index: Option<usize>,
    pub layer_id: Option<String>,
    pub chain_index: Option<usize>,
    pub execution: Option<&'static str>,
    pub source_path: Option<PathBuf>,
    pub target_index: Option<u32>,
    pub endpoint: Option<&'static str>,
    pub name: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub allocation: &'static str,
    pub vulkan_effect_target_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneWeImageGraphTargetSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub chain_index: usize,
    pub target_index: u32,
    pub endpoint: &'static str,
    pub name: Option<String>,
    pub format: Option<String>,
    pub scale: Option<f64>,
    pub unique: bool,
    pub execution: &'static str,
    pub planned_graph_resource_index: u32,
    pub vulkan_effect_target_index: Option<u32>,
    pub allocation: &'static str,
    pub width: u32,
    pub height: u32,
    pub first_write_step_index: usize,
    pub write_count: usize,
    pub sampled_by_following_pass: bool,
    pub scene_composite_source: bool,
    pub clear_before_first_write: bool,
}

fn native_vulkan_scene_texture_slot_snapshot(
    slot: &NativeVulkanSceneTextureSlot,
) -> NativeVulkanSceneTextureSlotSnapshot {
    NativeVulkanSceneTextureSlotSnapshot {
        slot: slot.slot,
        source: slot.source.clone(),
        width: slot.width,
        height: slot.height,
    }
}

fn native_vulkan_scene_texture_slot_resource_binding_snapshot(
    binding: NativeVulkanSceneTextureSlotResourceBinding,
) -> NativeVulkanSceneTextureSlotResourceBindingSnapshot {
    NativeVulkanSceneTextureSlotResourceBindingSnapshot {
        slot: binding.slot,
        resource_index: binding.resource_index,
    }
}

fn native_vulkan_scene_blend_state_snapshot(
    blend: NativeVulkanSceneBlendState,
) -> NativeVulkanSceneBlendStateSnapshot {
    NativeVulkanSceneBlendStateSnapshot {
        mode: blend.mode,
        equation: NativeVulkanSceneBlendEquationSnapshot {
            src_color: blend.equation.src_color.as_str(),
            dst_color: blend.equation.dst_color.as_str(),
            color_op: blend.equation.color_op.as_str(),
            src_alpha: blend.equation.src_alpha.as_str(),
            dst_alpha: blend.equation.dst_alpha.as_str(),
            alpha_op: blend.equation.alpha_op.as_str(),
        },
    }
}

fn native_vulkan_scene_render_state_snapshot(
    render_state: &NativeVulkanSceneRenderState,
) -> NativeVulkanSceneRenderStateSnapshot {
    NativeVulkanSceneRenderStateSnapshot {
        blend: native_vulkan_scene_blend_state_snapshot(render_state.blend),
        depth_test: render_state.depth_test.as_str(),
        depth_write: render_state.depth_write.as_str(),
        cull_mode: render_state.cull_mode.label().to_owned(),
    }
}

fn native_vulkan_scene_material_pass_snapshot(
    material: &NativeVulkanSceneMaterialPass,
) -> NativeVulkanSceneMaterialPassSnapshot {
    NativeVulkanSceneMaterialPassSnapshot {
        kind: material.kind.as_str(),
        shader: material.shader.clone(),
        blending: material.blending.clone(),
        render_state: native_vulkan_scene_render_state_snapshot(&material.render_state),
        alpha_texture_slot: material.alpha_texture_slot,
        alpha_texture_mode: material.alpha_texture_mode,
        texture_slot_count: material.texture_slot_count,
        effect_kinds: material
            .effect_kinds
            .iter()
            .map(|kind| kind.as_str())
            .collect(),
        combo_keys: material.combo_keys.clone(),
    }
}

fn native_vulkan_scene_effect_record_snapshot(
    effect: &NativeVulkanSceneEffectRecord,
) -> NativeVulkanSceneEffectRecordSnapshot {
    NativeVulkanSceneEffectRecordSnapshot {
        kind: effect.kind.as_str(),
        evaluation_boundary: effect.evaluation_boundary.as_str(),
        effect_file: effect.effect_file.clone(),
        runtime: effect.runtime.clone(),
        pass_index: effect.pass_index,
        command: effect.command.clone(),
        source: effect.source.clone(),
        target: effect.target.clone(),
        binds: effect.binds.clone(),
        fbos: effect.fbos.clone(),
        shader: effect.shader.clone(),
        blending: effect.blending.clone(),
        texture_slots: effect
            .texture_slots
            .iter()
            .map(native_vulkan_scene_texture_slot_snapshot)
            .collect(),
        parameter_keys: effect.parameter_keys.clone(),
        combo_keys: effect.combo_keys.clone(),
        depth_test: effect.depth_test.as_str(),
        depth_write: effect.depth_write.as_str(),
        cull_mode: effect.cull_mode.label().to_owned(),
    }
}

fn native_vulkan_scene_we_image_pass_chain_snapshot(
    chain: NativeVulkanSceneWeImagePassChain,
) -> NativeVulkanSceneWeImagePassChainSnapshot {
    NativeVulkanSceneWeImagePassChainSnapshot {
        execution: chain.execution.as_str(),
        local_target_required: chain.local_target_required,
        ping_pong_required: chain.ping_pong_required,
        first_pass_blend_moved_to_final: chain.first_pass_blend_moved_to_final,
        color_blend_passthrough: chain.color_blend_passthrough,
        final_scene_blend_mode: chain.final_scene_blend_mode,
        raw_direct_composite_allowed: chain.raw_direct_composite_allowed,
        unsupported_reason: chain.unsupported_reason,
        passes: chain
            .passes
            .into_iter()
            .map(native_vulkan_scene_we_image_pass_snapshot)
            .collect(),
    }
}

fn native_vulkan_scene_we_image_pass_snapshot(
    pass: super::draw_pass::NativeVulkanSceneWeImagePass,
) -> NativeVulkanSceneWeImagePassSnapshot {
    let texture_slots = pass
        .texture_slots
        .iter()
        .map(native_vulkan_scene_texture_slot_snapshot)
        .collect();
    NativeVulkanSceneWeImagePassSnapshot {
        pass_index: pass.pass_index,
        role: pass.role.as_str(),
        effect_kind: pass.effect_kind.map(|kind| kind.as_str()),
        effect_file: pass.effect_file,
        command: pass.command,
        source: pass.source,
        target_name: pass.target_name,
        binds: pass.binds,
        fbos: pass.fbos,
        shader: pass.shader,
        blending: pass.blending,
        scene_blend_mode: pass.scene_blend_mode,
        render_state: native_vulkan_scene_render_state_snapshot(&pass.render_state),
        input: pass.input.as_str(),
        input_name: pass.input_name,
        target: pass.target.as_str(),
        final_scene_pass: pass.final_scene_pass,
        texture_slots,
        texture_slot_count: pass.texture_slot_count,
        parameter_keys: pass.parameter_keys,
        combo_keys: pass.combo_keys,
        depth_test: pass.depth_test.as_str(),
        depth_write: pass.depth_write.as_str(),
        cull_mode: pass.cull_mode.label().to_owned(),
    }
}

fn native_vulkan_scene_we_image_graph_texture_resource_paths(
    graph_plan: &NativeVulkanSceneWeImageGraphPlan,
) -> Vec<PathBuf> {
    let mut resources = Vec::new();
    for step in &graph_plan.steps {
        for binding in &step.texture_bindings {
            let Some(source_path) = binding.source_path.as_ref() else {
                continue;
            };
            if !resources.iter().any(|resource| resource == source_path) {
                resources.push(source_path.clone());
            }
        }
    }
    resources
}

fn native_vulkan_scene_we_image_graph_resources_snapshot(
    graph_plan: &NativeVulkanSceneWeImageGraphPlan,
    effect_targets: &[NativeVulkanSceneSampledImageEffectTarget],
    texture_resources: &[PathBuf],
) -> Vec<NativeVulkanSceneWeImageGraphResourceSnapshot> {
    let mut resources = texture_resources
        .iter()
        .enumerate()
        .map(|(resource_index, source_path)| {
            let (width, height) = native_vulkan_scene_we_image_graph_texture_resource_dimensions(
                graph_plan,
                source_path,
            );
            NativeVulkanSceneWeImageGraphResourceSnapshot {
                resource_index: resource_index.min(u32::MAX as usize) as u32,
                resource_kind: "texture-source",
                layer_index: None,
                layer_id: None,
                chain_index: None,
                execution: None,
                source_path: Some(source_path.clone()),
                target_index: None,
                endpoint: None,
                name: None,
                width,
                height,
                allocation: "file-texture-source",
                vulkan_effect_target_index: None,
            }
        })
        .collect::<Vec<_>>();
    let target_resource_base = texture_resources.len().min(u32::MAX as usize) as u32;
    resources.extend(graph_plan.targets.iter().map(|target| {
        let vulkan_effect_target_index = effect_targets
            .iter()
            .find(|effect_target| {
                effect_target.we_graph_chain_index == Some(target.chain_index)
                    && effect_target.we_graph_target_index == Some(target.target_index)
                    && effect_target.we_graph_endpoint == Some(target.endpoint)
            })
            .map(|effect_target| effect_target.effect_target_index);
        NativeVulkanSceneWeImageGraphResourceSnapshot {
            resource_index: target_resource_base.saturating_add(target.target_index),
            resource_kind: "graph-target",
            layer_index: Some(target.layer_index),
            layer_id: Some(target.layer_id.clone()),
            chain_index: Some(target.chain_index),
            execution: Some(target.execution.as_str()),
            source_path: None,
            target_index: Some(target.target_index),
            endpoint: Some(target.endpoint.as_str()),
            name: target.name.clone(),
            width: Some(target.width),
            height: Some(target.height),
            allocation: if vulkan_effect_target_index.is_some() {
                "allocated-vulkan-effect-target"
            } else {
                "planned-until-graph-executor"
            },
            vulkan_effect_target_index,
        }
    }));
    resources
}

fn native_vulkan_scene_we_image_graph_texture_resource_dimensions(
    graph_plan: &NativeVulkanSceneWeImageGraphPlan,
    source_path: &Path,
) -> (Option<u32>, Option<u32>) {
    graph_plan
        .steps
        .iter()
        .flat_map(|step| &step.texture_bindings)
        .find(|binding| binding.source_path.as_deref() == Some(source_path))
        .map(|binding| (binding.width, binding.height))
        .unwrap_or((None, None))
}

fn native_vulkan_scene_we_image_graph_targets_snapshot(
    graph_plan: &NativeVulkanSceneWeImageGraphPlan,
    effect_targets: &[NativeVulkanSceneSampledImageEffectTarget],
    graph_resource_target_base: u32,
) -> Vec<NativeVulkanSceneWeImageGraphTargetSnapshot> {
    graph_plan
        .targets
        .iter()
        .map(|target| {
            native_vulkan_scene_we_image_graph_target_snapshot(
                target,
                effect_targets,
                graph_resource_target_base,
            )
        })
        .collect()
}

fn native_vulkan_scene_we_image_graph_target_snapshot(
    target: &NativeVulkanSceneWeImageGraphTarget,
    effect_targets: &[NativeVulkanSceneSampledImageEffectTarget],
    graph_resource_target_base: u32,
) -> NativeVulkanSceneWeImageGraphTargetSnapshot {
    let vulkan_effect_target_index = effect_targets
        .iter()
        .find(|effect_target| {
            effect_target.we_graph_chain_index == Some(target.chain_index)
                && effect_target.we_graph_target_index == Some(target.target_index)
                && effect_target.we_graph_endpoint == Some(target.endpoint)
        })
        .map(|effect_target| effect_target.effect_target_index);
    NativeVulkanSceneWeImageGraphTargetSnapshot {
        layer_index: target.layer_index,
        layer_id: target.layer_id.clone(),
        chain_index: target.chain_index,
        target_index: target.target_index,
        endpoint: target.endpoint.as_str(),
        name: target.name.clone(),
        format: target.format.clone(),
        scale: target.scale,
        unique: target.unique,
        execution: target.execution.as_str(),
        planned_graph_resource_index: graph_resource_target_base
            .saturating_add(target.target_index),
        vulkan_effect_target_index,
        allocation: if vulkan_effect_target_index.is_some() {
            "allocated-vulkan-effect-target"
        } else {
            "planned-until-graph-executor"
        },
        width: target.width,
        height: target.height,
        first_write_step_index: target.first_write_step_index,
        write_count: target.write_count,
        sampled_by_following_pass: target.sampled_by_following_pass,
        scene_composite_source: target.scene_composite_source,
        clear_before_first_write: target.clear_before_first_write,
    }
}

fn native_vulkan_scene_we_image_graph_steps_snapshot(
    graph_plan: NativeVulkanSceneWeImageGraphPlan,
    effect_targets: &[NativeVulkanSceneSampledImageEffectTarget],
    sampled_image_sources: &[PathBuf],
    graph_texture_resources: &[PathBuf],
    graph_resource_target_base: u32,
) -> Vec<NativeVulkanSceneWeImageGraphStepSnapshot> {
    graph_plan
        .steps
        .into_iter()
        .map(|step| NativeVulkanSceneWeImageGraphStepSnapshot {
            layer_index: step.layer_index,
            layer_id: step.layer_id,
            chain_index: step.chain_index,
            step_index: step.step_index,
            execution: step.execution.as_str(),
            raw_direct_composite_allowed: step.raw_direct_composite_allowed,
            unsupported_reason: step.unsupported_reason,
            input_target_index: step.input_target_index,
            output_target_index: step.output_target_index,
            texture_bindings: step
                .texture_bindings
                .into_iter()
                .map(|binding| {
                    native_vulkan_scene_we_image_graph_texture_binding_snapshot(
                        binding,
                        step.chain_index,
                        effect_targets,
                        sampled_image_sources,
                        graph_texture_resources,
                        graph_resource_target_base,
                    )
                })
                .collect(),
            pass: native_vulkan_scene_we_image_pass_snapshot(step.pass),
        })
        .collect()
}

fn native_vulkan_scene_we_image_graph_texture_binding_snapshot(
    binding: NativeVulkanSceneWeImageGraphTextureBinding,
    chain_index: usize,
    effect_targets: &[NativeVulkanSceneSampledImageEffectTarget],
    sampled_image_sources: &[PathBuf],
    graph_texture_resources: &[PathBuf],
    graph_resource_target_base: u32,
) -> NativeVulkanSceneWeImageGraphTextureBindingSnapshot {
    let vulkan_effect_target_index = binding.target_index.and_then(|target_index| {
        effect_targets
            .iter()
            .find(|effect_target| {
                effect_target.we_graph_chain_index == Some(chain_index)
                    && effect_target.we_graph_target_index == Some(target_index)
                    && effect_target.we_graph_endpoint == binding.endpoint
            })
            .map(|effect_target| effect_target.effect_target_index)
    });
    let planned_graph_resource_index = binding
        .target_index
        .map(|target_index| graph_resource_target_base.saturating_add(target_index))
        .or_else(|| {
            binding.source_path.as_ref().and_then(|source| {
                graph_texture_resources
                    .iter()
                    .position(|candidate| candidate == source)
                    .map(|index| index.min(u32::MAX as usize) as u32)
            })
        });
    let resource_index = vulkan_effect_target_index
        .map(|target_index| {
            sampled_image_sources
                .len()
                .max(1)
                .saturating_add(target_index as usize)
                .min(u32::MAX as usize) as u32
        })
        .or_else(|| {
            binding.source_path.as_ref().and_then(|source| {
                sampled_image_sources
                    .iter()
                    .position(|candidate| candidate == source)
                    .map(|index| index.min(u32::MAX as usize) as u32)
            })
        });
    NativeVulkanSceneWeImageGraphTextureBindingSnapshot {
        slot: binding.slot,
        uniform: binding.uniform,
        source: binding.source.as_str(),
        resource_index,
        planned_graph_resource_index,
        target_index: binding.target_index,
        endpoint: binding.endpoint.map(|endpoint| endpoint.as_str()),
        vulkan_effect_target_index,
        bind_name: binding.bind_name,
        source_path: binding.source_path,
        width: binding.width,
        height: binding.height,
        resolution: binding.resolution,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneDrawOpSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub kind: &'static str,
    pub opacity: f64,
    pub source: Option<PathBuf>,
    pub texture_slots: Vec<NativeVulkanSceneTextureSlotSnapshot>,
    pub blend_mode: SceneBlendMode,
    pub alpha_texture_slot: Option<u32>,
    pub alpha_texture_mode: SceneRenderAlphaTextureMode,
    pub image_effect_pass_count: usize,
    pub effect_passes: Vec<NativeVulkanSceneEffectRecordSnapshot>,
    pub composite_key: Option<SceneLayerCompositeKey>,
    pub texture_region: Option<SceneTextureRegion>,
    pub color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<f64>,
    pub corner_radius: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub text: Option<String>,
    pub font_size: Option<f64>,
    pub font_family: Option<String>,
    pub font_source: Option<PathBuf>,
    pub font_weight: Option<String>,
    pub text_align: Option<SceneTextAlign>,
    pub path_data: Option<String>,
    pub path_fill_rule: ScenePathFillRule,
    pub fit: FitMode,
    pub transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneRecordableQuadSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub kind: &'static str,
    pub color: String,
    pub rgba: [f32; 4],
    pub blend: NativeVulkanSceneBlendStateSnapshot,
    pub fill_color: Option<String>,
    pub fill_rgba: Option<[f32; 4]>,
    pub stroke_color: Option<String>,
    pub stroke_rgba: Option<[f32; 4]>,
    pub stroke_width: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub corner_radius: Option<f64>,
    pub text: Option<String>,
    pub font_size: Option<f64>,
    pub font_family: Option<String>,
    pub font_source: Option<PathBuf>,
    pub font_weight: Option<String>,
    pub text_align: Option<SceneTextAlign>,
    pub path_data: Option<String>,
    pub path_fill_rule: ScenePathFillRule,
    pub transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneQuadRecordingStepSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub kind: &'static str,
    pub blend: NativeVulkanSceneBlendStateSnapshot,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub vertex_buffer_offset_bytes: u64,
    pub vertex_buffer_size_bytes: u64,
    pub index_buffer_offset_bytes: u64,
    pub index_buffer_size_bytes: u64,
    pub fill_geometry: bool,
    pub stroke_geometry: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneSampledImageQuadSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub source: PathBuf,
    pub texture_slots: Vec<NativeVulkanSceneTextureSlotSnapshot>,
    pub image_effect_pass_count: usize,
    pub material_pass: NativeVulkanSceneMaterialPassSnapshot,
    pub effect_passes: Vec<NativeVulkanSceneEffectRecordSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub we_image_pass_chain: Option<NativeVulkanSceneWeImagePassChainSnapshot>,
    pub composite_key: Option<SceneLayerCompositeKey>,
    pub fit: FitMode,
    pub texture_region: Option<SceneTextureRegion>,
    pub opacity: f64,
    pub width: f64,
    pub height: f64,
    pub transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneSampledImageRecordingStepSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub source: PathBuf,
    pub fit: FitMode,
    pub texture_region: Option<SceneTextureRegion>,
    pub texture_slot_bindings: Vec<NativeVulkanSceneTextureSlotResourceBindingSnapshot>,
    pub material_pass: NativeVulkanSceneMaterialPassSnapshot,
    pub effect_passes: Vec<NativeVulkanSceneEffectRecordSnapshot>,
    pub composite_key: Option<SceneLayerCompositeKey>,
    pub render_target: NativeVulkanSceneSampledImageRenderTargetSnapshot,
    pub we_graph_chain_index: Option<usize>,
    pub we_graph_step_index: Option<usize>,
    pub we_graph_input_target_index: Option<u32>,
    pub we_graph_output_target_index: Option<u32>,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub vertex_buffer_offset_bytes: u64,
    pub vertex_buffer_size_bytes: u64,
    pub index_buffer_offset_bytes: u64,
    pub index_buffer_size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NativeVulkanSceneSampledImageRenderTargetSnapshot {
    Swapchain,
    EffectTarget { target_index: u32, clear: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneSampledImageEffectTargetSnapshot {
    pub effect_target_index: u32,
    pub layer_index: usize,
    pub width: u32,
    pub height: u32,
    pub we_graph_chain_index: Option<usize>,
    pub we_graph_target_index: Option<u32>,
    pub we_graph_endpoint: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneVideoQuadSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub source: PathBuf,
    pub fit: FitMode,
    pub opacity: f64,
    pub width: f64,
    pub height: f64,
    pub transform: SceneTransform,
}

fn native_vulkan_scene_sampled_image_render_target_snapshot(
    target: NativeVulkanSceneSampledImageRenderTarget,
) -> NativeVulkanSceneSampledImageRenderTargetSnapshot {
    match target {
        NativeVulkanSceneSampledImageRenderTarget::Swapchain => {
            NativeVulkanSceneSampledImageRenderTargetSnapshot::Swapchain
        }
        NativeVulkanSceneSampledImageRenderTarget::EffectTarget {
            target_index,
            clear,
        } => NativeVulkanSceneSampledImageRenderTargetSnapshot::EffectTarget {
            target_index,
            clear,
        },
    }
}

fn native_vulkan_scene_sampled_image_effect_target_snapshot(
    target: NativeVulkanSceneSampledImageEffectTarget,
) -> NativeVulkanSceneSampledImageEffectTargetSnapshot {
    NativeVulkanSceneSampledImageEffectTargetSnapshot {
        effect_target_index: target.effect_target_index,
        layer_index: target.layer_index,
        width: target.width,
        height: target.height,
        we_graph_chain_index: target.we_graph_chain_index,
        we_graph_target_index: target.we_graph_target_index,
        we_graph_endpoint: target.we_graph_endpoint.map(|endpoint| endpoint.as_str()),
    }
}

fn native_vulkan_scene_vulkanalia_sampled_image_render_target(
    target: NativeVulkanSceneSampledImageRenderTargetSnapshot,
) -> NativeVulkanVulkanaliaSceneSampledImageRenderTarget {
    match target {
        NativeVulkanSceneSampledImageRenderTargetSnapshot::Swapchain => {
            NativeVulkanVulkanaliaSceneSampledImageRenderTarget::Swapchain
        }
        NativeVulkanSceneSampledImageRenderTargetSnapshot::EffectTarget {
            target_index,
            clear,
        } => NativeVulkanVulkanaliaSceneSampledImageRenderTarget::EffectTarget {
            target_index,
            clear,
        },
    }
}

fn native_vulkan_scene_vulkanalia_we_image_graph_resource(
    resource: NativeVulkanSceneWeImageGraphResourceSnapshot,
) -> NativeVulkanVulkanaliaSceneWeImageGraphResource {
    NativeVulkanVulkanaliaSceneWeImageGraphResource {
        resource_index: resource.resource_index,
        resource_kind: resource.resource_kind,
        layer_index: resource.layer_index,
        layer_id: resource.layer_id,
        chain_index: resource.chain_index,
        execution: resource.execution,
        source_path: resource.source_path,
        target_index: resource.target_index,
        endpoint: resource.endpoint,
        name: resource.name,
        width: resource.width,
        height: resource.height,
        allocation: resource.allocation,
        vulkan_effect_target_index: resource.vulkan_effect_target_index,
    }
}

fn native_vulkan_scene_vulkanalia_texture_slot_resource_binding(
    binding: NativeVulkanSceneTextureSlotResourceBindingSnapshot,
) -> NativeVulkanVulkanaliaSceneTextureSlotResourceBinding {
    NativeVulkanVulkanaliaSceneTextureSlotResourceBinding {
        slot: binding.slot,
        resource_index: binding.resource_index,
    }
}

fn native_vulkan_scene_vulkanalia_blend_state(
    blend: NativeVulkanSceneBlendStateSnapshot,
) -> NativeVulkanVulkanaliaSceneBlendState {
    NativeVulkanVulkanaliaSceneBlendState {
        mode: blend.mode,
        equation: NativeVulkanVulkanaliaSceneBlendEquation {
            src_color: blend.equation.src_color,
            dst_color: blend.equation.dst_color,
            color_op: blend.equation.color_op,
            src_alpha: blend.equation.src_alpha,
            dst_alpha: blend.equation.dst_alpha,
            alpha_op: blend.equation.alpha_op,
        },
    }
}

fn native_vulkan_scene_vulkanalia_render_state(
    render_state: NativeVulkanSceneRenderStateSnapshot,
) -> NativeVulkanVulkanaliaSceneRenderState {
    NativeVulkanVulkanaliaSceneRenderState {
        blend: native_vulkan_scene_vulkanalia_blend_state(render_state.blend),
        depth_test: NativeVulkanVulkanaliaSceneMaterialFlag::from_label(render_state.depth_test),
        depth_write: NativeVulkanVulkanaliaSceneMaterialFlag::from_label(render_state.depth_write),
        cull_mode: NativeVulkanVulkanaliaSceneCullMode::from_label(&render_state.cull_mode),
    }
}

fn native_vulkan_scene_vulkanalia_sampled_image_material(
    material: NativeVulkanSceneMaterialPassSnapshot,
) -> NativeVulkanVulkanaliaSceneSampledImageMaterial {
    NativeVulkanVulkanaliaSceneSampledImageMaterial {
        kind: NativeVulkanVulkanaliaSceneSampledImageMaterialKind::from_label(material.kind),
        shader: material.shader,
        blending: material.blending,
        render_state: native_vulkan_scene_vulkanalia_render_state(material.render_state),
        alpha_texture_slot: material.alpha_texture_slot,
        alpha_texture_mode: material.alpha_texture_mode,
        texture_slot_count: material.texture_slot_count,
        uses_elapsed_push_constants: false,
        effect_kinds: material
            .effect_kinds
            .into_iter()
            .map(NativeVulkanVulkanaliaSceneEffectKind::from_label)
            .collect(),
        combo_keys: material.combo_keys,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneVideoRecordingStepSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub source: PathBuf,
    pub fit: FitMode,
    pub pipeline: &'static str,
    pub resource_index: u32,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub vertex_buffer_offset_bytes: u64,
    pub vertex_buffer_size_bytes: u64,
    pub index_buffer_offset_bytes: u64,
    pub index_buffer_size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NativeVulkanSceneQuadVertexSnapshot {
    pub position: [f32; 2],
    pub rgba: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NativeVulkanSceneSampledImageVertexSnapshot {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub effect_uv: [f32; 2],
    pub opacity: f32,
    pub tint: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneUnsupportedLayerSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub reason: &'static str,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_runtime_snapshot(
    render_item: &NativeVulkanRenderItem,
) -> Option<NativeVulkanSceneRuntimeSnapshot> {
    let plan = native_vulkan_scene_draw_plan(render_item)?;
    let pass_plan = native_vulkan_scene_draw_pass_plan(&plan);
    let vulkanalia_draw_pass = native_vulkan_vulkanalia_scene_draw_pass_snapshot(
        NativeVulkanVulkanaliaSceneDrawPassInput {
            plan_ready: pass_plan.plan_ready,
            native_draw_ready: plan.native_draw_ready(),
            draw_op_count: plan.draw_ops.len(),
            backend_status: pass_plan.backend_status,
            blocking_reason: pass_plan.blocking_reason,
            fast_clear_color_ready: pass_plan.fast_clear_color.is_some(),
            clear_background_op_count: pass_plan.clear_background_op_count,
            quad_recording_ready: pass_plan.quad_recording_ready,
            quad_recording_step_count: pass_plan.quad_recording_steps.len(),
            quad_vertex_buffer_bytes: pass_plan.quad_vertex_buffer_bytes,
            quad_index_buffer_bytes: pass_plan.quad_index_buffer_bytes,
            sampled_image_recording_ready: pass_plan.sampled_image_recording_ready,
            sampled_image_implicit_full_extent_ready: pass_plan
                .sampled_image_implicit_full_extent_ready,
            sampled_image_op_count: pass_plan.sampled_image_op_count,
            sampled_image_recording_step_count: pass_plan.sampled_image_recording_steps.len(),
            sampled_image_vertex_buffer_bytes: pass_plan.sampled_image_vertex_buffer_bytes,
            sampled_image_index_buffer_bytes: pass_plan.sampled_image_index_buffer_bytes,
            color_op_count: pass_plan.color_op_count,
            vector_shape_op_count: pass_plan.vector_shape_op_count,
            text_op_count: pass_plan.text_op_count,
            path_op_count: pass_plan.path_op_count,
        },
    );
    let vulkanalia_sampled_image = native_vulkan_vulkanalia_scene_sampled_image_plan(
        NativeVulkanVulkanaliaSceneSampledImagePlanInput {
            sampled_image_sources: pass_plan.sampled_image_sources.clone(),
            recording_step_count: pass_plan.sampled_image_recording_steps.len(),
            vertex_count: pass_plan.sampled_image_vertices.len(),
            index_count: pass_plan.sampled_image_indices.len(),
            vertex_buffer_bytes: pass_plan.sampled_image_vertex_buffer_bytes,
            index_buffer_bytes: pass_plan.sampled_image_index_buffer_bytes,
        },
    );
    let scene_resource_model =
        native_vulkan_scene_resource_model(pass_plan.backend_status, pass_plan.video_op_count);
    let scene_solid_quad_draw_count = pass_plan.quad_recording_steps.len();
    let scene_sampled_image_resource_count = vulkanalia_sampled_image.resource_count;
    let scene_sampled_image_descriptor_heap_required = scene_sampled_image_resource_count > 0;
    let scene_video_layer_resource_count = pass_plan.required_video_resources.len();
    let scene_binary_ingest = native_vulkan_scene_binary_ingest_runtime_snapshot(render_item);
    let full_scene = native_vulkan_full_scene_runtime_snapshot(
        render_item,
        &plan,
        &pass_plan,
        scene_resource_model,
        scene_sampled_image_descriptor_heap_required,
        scene_binary_ingest.is_some(),
    );
    let scene_video_native_layer_count = full_scene.video_native_layer_count;
    let sampled_image_we_graph_chain_count = pass_plan.sampled_image_we_graph_plan.chain_count;
    let sampled_image_we_graph_step_count = pass_plan.sampled_image_we_graph_plan.step_count;
    let sampled_image_we_graph_first_class_target_chain_count = pass_plan
        .sampled_image_we_graph_plan
        .first_class_target_chain_count;
    let sampled_image_we_graph_temporary_raw_fallback_chain_count = pass_plan
        .sampled_image_we_graph_plan
        .temporary_raw_fallback_chain_count;
    let sampled_image_we_graph_suppressed_chain_count =
        pass_plan.sampled_image_we_graph_plan.suppressed_chain_count;
    let sampled_image_we_graph_target_count = pass_plan.sampled_image_we_graph_plan.target_count;
    let sampled_image_we_graph_final_scene_step_count =
        pass_plan.sampled_image_we_graph_plan.final_scene_step_count;
    let sampled_image_we_graph_effect_kind_counts = pass_plan
        .sampled_image_we_graph_plan
        .effect_kind_counts
        .iter()
        .map(|(kind, count)| ((*kind).to_owned(), *count))
        .collect::<BTreeMap<_, _>>();
    let sampled_image_we_graph_texture_resources =
        native_vulkan_scene_we_image_graph_texture_resource_paths(
            &pass_plan.sampled_image_we_graph_plan,
        );
    let sampled_image_we_graph_target_resource_base = sampled_image_we_graph_texture_resources
        .len()
        .min(u32::MAX as usize) as u32;
    let sampled_image_we_graph_resources = native_vulkan_scene_we_image_graph_resources_snapshot(
        &pass_plan.sampled_image_we_graph_plan,
        &pass_plan.sampled_image_effect_targets,
        &sampled_image_we_graph_texture_resources,
    );
    let sampled_image_we_graph_targets = native_vulkan_scene_we_image_graph_targets_snapshot(
        &pass_plan.sampled_image_we_graph_plan,
        &pass_plan.sampled_image_effect_targets,
        sampled_image_we_graph_target_resource_base,
    );
    let sampled_image_we_graph_steps = native_vulkan_scene_we_image_graph_steps_snapshot(
        pass_plan.sampled_image_we_graph_plan.clone(),
        &pass_plan.sampled_image_effect_targets,
        &pass_plan.sampled_image_sources,
        &sampled_image_we_graph_texture_resources,
        sampled_image_we_graph_target_resource_base,
    );
    Some(NativeVulkanSceneRuntimeSnapshot {
        snapshot_time_ms: plan.snapshot_time_ms,
        scene_size: plan.scene_size,
        scene_fit: plan.scene_fit,
        full_scene,
        scene_input_model: "core scene snapshot layers; groups must be flattened before native Vulkan planning",
        scene_resource_model,
        scene_binary_ingest,
        native_draw_ready: plan.native_draw_ready(),
        runtime_display_available: plan.runtime_display_available,
        draw_pass_plan_ready: pass_plan.plan_ready,
        draw_pass_backend_ready: pass_plan.backend_ready,
        draw_pass_backend_status: pass_plan.backend_status,
        draw_pass_blocking_reason: pass_plan.blocking_reason,
        draw_pass_recordable_op_count: pass_plan.recordable_op_count,
        draw_pass_recordable_quads: pass_plan
            .recordable_quads
            .into_iter()
            .map(|quad| NativeVulkanSceneRecordableQuadSnapshot {
                layer_index: quad.layer_index,
                layer_id: quad.layer_id,
                kind: quad.kind,
                color: quad.color,
                rgba: quad.rgba,
                blend: native_vulkan_scene_blend_state_snapshot(quad.blend),
                fill_color: quad.fill_color,
                fill_rgba: quad.fill_rgba,
                stroke_color: quad.stroke_color,
                stroke_rgba: quad.stroke_rgba,
                stroke_width: quad.stroke_width,
                width: quad.width,
                height: quad.height,
                corner_radius: quad.corner_radius,
                text: quad.text,
                font_size: quad.font_size,
                font_family: quad.font_family,
                font_source: quad.font_source,
                font_weight: quad.font_weight,
                text_align: quad.text_align,
                path_data: quad.path_data,
                path_fill_rule: quad.path_fill_rule,
                transform: quad.transform,
            })
            .collect(),
        draw_pass_quad_recording_ready: pass_plan.quad_recording_ready,
        draw_pass_quad_recording_step_count: pass_plan.quad_recording_steps.len(),
        draw_pass_quad_recording_steps: pass_plan
            .quad_recording_steps
            .into_iter()
            .map(|step| NativeVulkanSceneQuadRecordingStepSnapshot {
                layer_index: step.layer_index,
                layer_id: step.layer_id,
                kind: step.kind,
                blend: native_vulkan_scene_blend_state_snapshot(step.blend),
                first_vertex: step.first_vertex,
                vertex_count: step.vertex_count,
                first_index: step.first_index,
                index_count: step.index_count,
                vertex_buffer_offset_bytes: step.vertex_buffer_offset_bytes,
                vertex_buffer_size_bytes: step.vertex_buffer_size_bytes,
                index_buffer_offset_bytes: step.index_buffer_offset_bytes,
                index_buffer_size_bytes: step.index_buffer_size_bytes,
                fill_geometry: step.fill_geometry,
                stroke_geometry: step.stroke_geometry,
            })
            .collect(),
        draw_pass_quad_vertices: pass_plan
            .quad_vertices
            .into_iter()
            .map(|vertex| NativeVulkanSceneQuadVertexSnapshot {
                position: vertex.position,
                rgba: vertex.rgba,
            })
            .collect(),
        draw_pass_quad_indices: pass_plan.quad_indices,
        draw_pass_quad_vertex_buffer_bytes: pass_plan.quad_vertex_buffer_bytes,
        draw_pass_quad_index_buffer_bytes: pass_plan.quad_index_buffer_bytes,
        draw_pass_sampled_image_quads: pass_plan
            .sampled_image_quads
            .into_iter()
            .map(|quad| {
                let we_image_pass_chain = native_vulkan_scene_we_image_pass_chain(&quad)
                    .map(native_vulkan_scene_we_image_pass_chain_snapshot);
                NativeVulkanSceneSampledImageQuadSnapshot {
                    layer_index: quad.layer_index,
                    layer_id: quad.layer_id,
                    source: quad.source,
                    texture_slots: quad
                        .texture_slots
                        .iter()
                        .map(native_vulkan_scene_texture_slot_snapshot)
                        .collect(),
                    image_effect_pass_count: quad.image_effect_pass_count,
                    material_pass: native_vulkan_scene_material_pass_snapshot(&quad.material_pass),
                    effect_passes: quad
                        .effect_passes
                        .iter()
                        .map(native_vulkan_scene_effect_record_snapshot)
                        .collect(),
                    we_image_pass_chain,
                    composite_key: quad.composite_key,
                    fit: quad.fit,
                    texture_region: quad.texture_region,
                    opacity: quad.opacity,
                    width: quad.width,
                    height: quad.height,
                    transform: quad.transform,
                }
            })
            .collect(),
        draw_pass_sampled_image_we_graph_chain_count: sampled_image_we_graph_chain_count,
        draw_pass_sampled_image_we_graph_step_count: sampled_image_we_graph_step_count,
        draw_pass_sampled_image_we_graph_first_class_target_chain_count:
            sampled_image_we_graph_first_class_target_chain_count,
        draw_pass_sampled_image_we_graph_temporary_raw_fallback_chain_count:
            sampled_image_we_graph_temporary_raw_fallback_chain_count,
        draw_pass_sampled_image_we_graph_suppressed_chain_count:
            sampled_image_we_graph_suppressed_chain_count,
        draw_pass_sampled_image_we_graph_target_count: sampled_image_we_graph_target_count,
        draw_pass_sampled_image_we_graph_final_scene_step_count:
            sampled_image_we_graph_final_scene_step_count,
        draw_pass_sampled_image_we_graph_effect_kind_counts:
            sampled_image_we_graph_effect_kind_counts,
        draw_pass_sampled_image_we_graph_resource_count: sampled_image_we_graph_resources.len(),
        draw_pass_sampled_image_we_graph_texture_resource_count:
            sampled_image_we_graph_texture_resources.len(),
        draw_pass_sampled_image_we_graph_target_resource_count: sampled_image_we_graph_target_count,
        draw_pass_sampled_image_we_graph_resources: sampled_image_we_graph_resources,
        draw_pass_sampled_image_we_graph_targets: sampled_image_we_graph_targets,
        draw_pass_sampled_image_we_graph_steps: sampled_image_we_graph_steps,
        draw_pass_sampled_image_effect_targets: pass_plan
            .sampled_image_effect_targets
            .into_iter()
            .map(native_vulkan_scene_sampled_image_effect_target_snapshot)
            .collect(),
        draw_pass_sampled_image_sources: pass_plan.sampled_image_sources,
        draw_pass_sampled_image_recording_ready: pass_plan.sampled_image_recording_ready,
        draw_pass_sampled_image_implicit_full_extent_ready: pass_plan
            .sampled_image_implicit_full_extent_ready,
        draw_pass_sampled_image_recording_step_count: pass_plan.sampled_image_recording_steps.len(),
        draw_pass_sampled_image_recording_steps: pass_plan
            .sampled_image_recording_steps
            .into_iter()
            .map(|step| NativeVulkanSceneSampledImageRecordingStepSnapshot {
                layer_index: step.layer_index,
                layer_id: step.layer_id,
                source: step.source,
                fit: step.fit,
                texture_region: step.texture_region,
                texture_slot_bindings: step
                    .texture_slot_bindings
                    .into_iter()
                    .map(native_vulkan_scene_texture_slot_resource_binding_snapshot)
                    .collect(),
                material_pass: native_vulkan_scene_material_pass_snapshot(&step.material_pass),
                effect_passes: step
                    .effect_passes
                    .iter()
                    .map(native_vulkan_scene_effect_record_snapshot)
                    .collect(),
                composite_key: step.composite_key,
                render_target: native_vulkan_scene_sampled_image_render_target_snapshot(
                    step.render_target,
                ),
                we_graph_chain_index: step.we_graph_chain_index,
                we_graph_step_index: step.we_graph_step_index,
                we_graph_input_target_index: step.we_graph_input_target_index,
                we_graph_output_target_index: step.we_graph_output_target_index,
                first_vertex: step.first_vertex,
                vertex_count: step.vertex_count,
                first_index: step.first_index,
                index_count: step.index_count,
                vertex_buffer_offset_bytes: step.vertex_buffer_offset_bytes,
                vertex_buffer_size_bytes: step.vertex_buffer_size_bytes,
                index_buffer_offset_bytes: step.index_buffer_offset_bytes,
                index_buffer_size_bytes: step.index_buffer_size_bytes,
            })
            .collect(),
        draw_pass_sampled_image_vertices: pass_plan
            .sampled_image_vertices
            .into_iter()
            .map(|vertex| NativeVulkanSceneSampledImageVertexSnapshot {
                position: vertex.position,
                uv: vertex.uv,
                effect_uv: vertex.effect_uv,
                opacity: vertex.opacity,
                tint: vertex.tint,
            })
            .collect(),
        draw_pass_sampled_image_indices: pass_plan.sampled_image_indices,
        draw_pass_sampled_image_vertex_buffer_bytes: pass_plan.sampled_image_vertex_buffer_bytes,
        draw_pass_sampled_image_index_buffer_bytes: pass_plan.sampled_image_index_buffer_bytes,
        draw_pass_video_quads: pass_plan
            .video_quads
            .into_iter()
            .map(|quad| NativeVulkanSceneVideoQuadSnapshot {
                layer_index: quad.layer_index,
                layer_id: quad.layer_id,
                source: quad.source,
                fit: quad.fit,
                opacity: quad.opacity,
                width: quad.width,
                height: quad.height,
                transform: quad.transform,
            })
            .collect(),
        draw_pass_video_sources: pass_plan.video_sources,
        draw_pass_video_recording_ready: pass_plan.video_recording_ready,
        draw_pass_video_recording_step_count: pass_plan.video_recording_steps.len(),
        draw_pass_video_recording_steps: pass_plan
            .video_recording_steps
            .into_iter()
            .map(|step| NativeVulkanSceneVideoRecordingStepSnapshot {
                layer_index: step.layer_index,
                layer_id: step.layer_id,
                source: step.source,
                fit: step.fit,
                pipeline: step.pipeline,
                resource_index: step.resource_index,
                first_vertex: step.first_vertex,
                vertex_count: step.vertex_count,
                first_index: step.first_index,
                index_count: step.index_count,
                vertex_buffer_offset_bytes: step.vertex_buffer_offset_bytes,
                vertex_buffer_size_bytes: step.vertex_buffer_size_bytes,
                index_buffer_offset_bytes: step.index_buffer_offset_bytes,
                index_buffer_size_bytes: step.index_buffer_size_bytes,
            })
            .collect(),
        draw_pass_video_vertices: pass_plan
            .video_vertices
            .into_iter()
            .map(|vertex| NativeVulkanSceneSampledImageVertexSnapshot {
                position: vertex.position,
                uv: vertex.uv,
                effect_uv: vertex.effect_uv,
                opacity: vertex.opacity,
                tint: vertex.tint,
            })
            .collect(),
        draw_pass_video_indices: pass_plan.video_indices,
        draw_pass_video_vertex_buffer_bytes: pass_plan.video_vertex_buffer_bytes,
        draw_pass_video_index_buffer_bytes: pass_plan.video_index_buffer_bytes,
        draw_pass_clear_background_op_count: pass_plan.clear_background_op_count,
        draw_pass_background_clear_color: pass_plan.background_clear_color,
        draw_pass_color_op_count: pass_plan.color_op_count,
        draw_pass_sampled_image_op_count: pass_plan.sampled_image_op_count,
        scene_solid_quad_draw_count,
        scene_sampled_image_resource_count,
        scene_sampled_image_descriptor_heap_required,
        draw_pass_video_op_count: pass_plan.video_op_count,
        scene_video_layer_resource_count,
        scene_video_native_layer_count,
        draw_pass_vector_shape_op_count: pass_plan.vector_shape_op_count,
        draw_pass_text_op_count: pass_plan.text_op_count,
        draw_pass_path_op_count: pass_plan.path_op_count,
        draw_pass_effect_pass_count: pass_plan.effect_pass_count,
        draw_pass_effect_pass_non_image_layer_count: pass_plan.effect_pass_non_image_layer_count,
        draw_pass_effect_pass_kind_counts: pass_plan
            .effect_pass_kind_counts
            .iter()
            .map(|(kind, count)| ((*kind).to_owned(), *count))
            .collect(),
        draw_pass_required_image_resources: pass_plan.required_image_resources,
        draw_pass_required_video_resources: pass_plan.required_video_resources,
        draw_pass_requires_text_geometry: pass_plan.requires_text_geometry,
        draw_pass_requires_path_tessellation: pass_plan.requires_path_tessellation,
        draw_pass_requires_video_decode: pass_plan.requires_video_decode,
        draw_pass_fast_clear_color: pass_plan.fast_clear_color,
        vulkanalia_draw_pass,
        vulkanalia_sampled_image,
        draw_op_count: plan.draw_ops.len(),
        unsupported_layer_count: plan.unsupported_layers.len(),
        draw_ops: plan
            .draw_ops
            .into_iter()
            .map(|op| {
                let effect_passes =
                    native_vulkan_scene_effect_passes_from_render_passes(&op.image_effect_passes)
                        .iter()
                        .map(native_vulkan_scene_effect_record_snapshot)
                        .collect::<Vec<_>>();
                NativeVulkanSceneDrawOpSnapshot {
                    layer_index: op.layer_index,
                    layer_id: op.layer_id,
                    kind: op.kind.as_str(),
                    opacity: op.opacity,
                    source: op.source,
                    texture_slots: op
                        .texture_slots
                        .into_iter()
                        .map(|slot| NativeVulkanSceneTextureSlotSnapshot {
                            slot: slot.slot,
                            source: slot.source,
                            width: slot.width,
                            height: slot.height,
                        })
                        .collect(),
                    blend_mode: op.blend_mode,
                    alpha_texture_slot: op.alpha_texture_slot,
                    alpha_texture_mode: op.alpha_texture_mode,
                    image_effect_pass_count: op.image_effect_passes.len(),
                    effect_passes,
                    composite_key: op.composite_key,
                    texture_region: op.texture_region,
                    color: op.color,
                    stroke_color: op.stroke_color,
                    stroke_width: op.stroke_width,
                    corner_radius: op.corner_radius,
                    width: op.width,
                    height: op.height,
                    text: op.text,
                    font_size: op.font_size,
                    font_family: op.font_family,
                    font_source: op.font_source,
                    font_weight: op.font_weight,
                    text_align: op.text_align,
                    path_data: op.path_data,
                    path_fill_rule: op.path_fill_rule,
                    fit: op.fit,
                    transform: op.transform,
                }
            })
            .collect(),
        unsupported_layers: plan
            .unsupported_layers
            .into_iter()
            .map(|layer| NativeVulkanSceneUnsupportedLayerSnapshot {
                layer_index: layer.layer_index,
                layer_id: layer.layer_id,
                reason: layer.reason,
            })
            .collect(),
    })
}

fn native_vulkan_full_scene_runtime_snapshot(
    render_item: &NativeVulkanRenderItem,
    plan: &NativeVulkanSceneDrawPlan,
    pass_plan: &super::draw_pass::NativeVulkanSceneDrawPassPlan,
    scene_resource_model: &'static str,
    scene_sampled_image_descriptor_heap_required: bool,
    scene_binary_ingest_ready: bool,
) -> NativeVulkanFullSceneRuntimeSnapshot {
    let (
        source_layer_count,
        timeline_animation_count,
        timeline_animated_layer_count,
        puppet_animation_layer_count,
        property_binding_count,
        cursor_parallax_input_ready,
        scene_scenescript_binding_count,
        scene_material_graph_count,
        scene_material_graph_resource_count,
        scene_effect_graph_count,
        scene_audio_response_detected,
        scene_audio_response_binding_count,
        scene_particle_system_detected,
        scene_particle_system_ready_from_metadata,
        scene_audio_cue_count,
        unsupported_scene_features,
    ) = match render_item {
        NativeVulkanRenderItem::Scene {
            layer_count,
            timeline_animation_count,
            timeline_animated_layer_count,
            puppet_animation_layer_count,
            property_binding_count,
            cursor_parallax_input_ready,
            scene_scenescript_binding_count,
            scene_material_graph_count,
            scene_material_graph_resource_count,
            scene_effect_graph_count,
            scene_audio_response_binding_count,
            unsupported_scene_features,
            scene_systems,
            audio_cue_count,
            ..
        } => (
            *layer_count,
            *timeline_animation_count,
            *timeline_animated_layer_count,
            *puppet_animation_layer_count,
            *property_binding_count,
            *cursor_parallax_input_ready,
            *scene_scenescript_binding_count,
            *scene_material_graph_count,
            *scene_material_graph_resource_count,
            *scene_effect_graph_count,
            matches!(
                scene_systems.audio_response,
                SceneSystemStatus::Detected | SceneSystemStatus::Ready
            ),
            *scene_audio_response_binding_count,
            matches!(
                scene_systems.particles,
                SceneSystemStatus::Detected | SceneSystemStatus::Ready
            ),
            matches!(scene_systems.particles, SceneSystemStatus::Ready),
            *audio_cue_count,
            unsupported_scene_features.clone(),
        ),
        _ => (
            0,
            0,
            0,
            0,
            0,
            false,
            0,
            0,
            0,
            0,
            false,
            0,
            false,
            false,
            0,
            Vec::new(),
        ),
    };
    let scene_scenescript_detected = match render_item {
        NativeVulkanRenderItem::Scene { scene_systems, .. } => matches!(
            scene_systems.scenescript,
            SceneSystemStatus::Detected | SceneSystemStatus::Ready
        ),
        _ => false,
    };
    let scene_scenescript_ready = match render_item {
        NativeVulkanRenderItem::Scene { scene_systems, .. } => {
            matches!(scene_systems.scenescript, SceneSystemStatus::Ready)
                && scene_scenescript_binding_count > 0
                && !unsupported_scene_features
                    .iter()
                    .any(|feature| feature.contains("scenescript"))
        }
        _ => false,
    };
    let scene_shader_material_graph_detected = match render_item {
        NativeVulkanRenderItem::Scene { scene_systems, .. } => {
            matches!(
                scene_systems.shader_material_graph,
                SceneSystemStatus::Detected | SceneSystemStatus::Ready
            ) || scene_material_graph_count > 0
                || scene_effect_graph_count > 0
        }
        _ => false,
    };
    let scene_shader_material_graph_ready = match render_item {
        NativeVulkanRenderItem::Scene { scene_systems, .. } => {
            matches!(
                scene_systems.shader_material_graph,
                SceneSystemStatus::Ready
            ) && (scene_material_graph_count == 0 || scene_material_graph_resource_count > 0)
                && scene_effect_graph_count == 0
                && !unsupported_scene_features
                    .iter()
                    .any(|feature| scene_feature_blocks_shader_material_graph(feature))
        }
        _ => false,
    };
    let scene_audio_response_ready = match render_item {
        NativeVulkanRenderItem::Scene { scene_systems, .. } => {
            matches!(scene_systems.audio_response, SceneSystemStatus::Ready)
                && scene_audio_response_binding_count > 0
                && !unsupported_scene_features
                    .iter()
                    .any(|feature| feature.contains("audio"))
        }
        _ => false,
    };
    let scene_audio_cue_resource_model_ready = scene_audio_cue_count > 0;
    let retained_resource_model_ready = matches!(
        scene_resource_model,
        "fast-clear-only-no-scene-resources"
            | "retained-solid-quad-geometry"
            | "clear-background-and-retained-solid-quad-geometry"
            | "retained-sampled-images-descriptor-heap"
            | "retained-vulkan-video-scene-resource"
            | "clear-background-and-retained-vulkan-video-scene-resource"
            | "initial-visible-vulkan-video-and-retained-scene-resources"
            | "retained-solid-quad-geometry-and-sampled-images-descriptor-heap"
    ) || scene_sampled_image_descriptor_heap_required;
    let timeline_snapshot_runtime_ready = plan.snapshot_time_ms > 0;
    let timeline_animation_runtime_ready = true;
    let property_update_runtime_ready = true;
    let pause_resume_policy_ready = true;
    let package_state_persistence_ready = true;
    let active_scene_layer_count = plan
        .draw_ops
        .len()
        .saturating_add(plan.unsupported_layers.len());
    let clear_background_layer_count =
        pass_plan.clear_background_op_count + usize::from(pass_plan.fast_clear_color.is_some());
    let tessellated_path_layer_count = pass_plan
        .quad_recording_steps
        .iter()
        .filter(|step| step.kind == "path")
        .count();
    let curve_path_layer_count = pass_plan
        .recordable_quads
        .iter()
        .filter(|quad| {
            quad.kind == "path"
                && quad
                    .path_data
                    .as_deref()
                    .is_some_and(native_vulkan_scene_path_uses_curves)
        })
        .count();
    let arc_path_layer_count = pass_plan
        .recordable_quads
        .iter()
        .filter(|quad| {
            quad.kind == "path"
                && quad
                    .path_data
                    .as_deref()
                    .is_some_and(native_vulkan_scene_path_uses_arcs)
        })
        .count();
    let compound_path_layer_count = pass_plan
        .recordable_quads
        .iter()
        .filter(|quad| {
            quad.kind == "path"
                && quad
                    .path_data
                    .as_deref()
                    .is_some_and(native_vulkan_scene_path_uses_compound_subpaths)
        })
        .count();
    let compound_nonzero_path_layer_count = pass_plan
        .recordable_quads
        .iter()
        .filter(|quad| {
            quad.kind == "path"
                && quad.path_fill_rule == ScenePathFillRule::Nonzero
                && quad
                    .path_data
                    .as_deref()
                    .is_some_and(native_vulkan_scene_path_uses_compound_subpaths)
        })
        .count();
    let compound_evenodd_path_layer_count = pass_plan
        .recordable_quads
        .iter()
        .filter(|quad| {
            quad.kind == "path"
                && quad.path_fill_rule == ScenePathFillRule::Evenodd
                && quad
                    .path_data
                    .as_deref()
                    .is_some_and(native_vulkan_scene_path_uses_compound_subpaths)
        })
        .count();
    let text_geometry_layer_count = pass_plan
        .quad_recording_steps
        .iter()
        .filter(|step| step.kind == "text")
        .count();
    let stroke_geometry_layer_count = pass_plan
        .quad_recording_steps
        .iter()
        .filter(|step| step.stroke_geometry)
        .count();
    let rounded_rectangle_layer_count = pass_plan
        .quad_recording_steps
        .iter()
        .filter(|step| step.kind == "rounded-rectangle")
        .count();
    let particle_runtime_layer_count = pass_plan
        .quad_recording_steps
        .iter()
        .filter(|step| step.layer_id.contains("::particle-"))
        .count();
    let scene_particle_system_ready =
        scene_particle_system_ready_from_metadata || particle_runtime_layer_count > 0;
    let solid_geometry_layer_count = pass_plan.quad_recording_steps.len();
    let sampled_image_native_layer_count = if pass_plan.sampled_image_recording_ready {
        pass_plan.sampled_image_op_count
    } else if pass_plan.sampled_image_implicit_full_extent_ready {
        pass_plan.sampled_image_op_count
    } else {
        0
    };
    let scene_video_composition_ready = matches!(
        pass_plan.backend_status,
        "video-layer-vulkan-video-scene-bridge-ready"
            | "clear-background-video-layer-vulkan-video-scene-bridge-ready"
            | "multi-video-layer-vulkan-video-scene-bridge-ready"
    );
    let video_native_layer_count = if scene_video_composition_ready {
        pass_plan.video_op_count
    } else {
        0
    };
    let native_runtime_layer_count = clear_background_layer_count
        .saturating_add(solid_geometry_layer_count)
        .saturating_add(sampled_image_native_layer_count)
        .saturating_add(video_native_layer_count)
        .min(active_scene_layer_count);
    let native_runtime_pending_layer_count =
        active_scene_layer_count.saturating_sub(native_runtime_layer_count);
    let native_runtime_coverage_percent = if active_scene_layer_count == 0 {
        0
    } else {
        ((native_runtime_layer_count.saturating_mul(100)) / active_scene_layer_count).min(100) as u8
    };
    let scene_video_composition_required =
        pass_plan.video_op_count > 0 || pass_plan.requires_video_decode;
    let scene_video_composition_ready =
        !scene_video_composition_required || scene_video_composition_ready;
    let scene_text_geometry_required = pass_plan.text_op_count > 0;
    let scene_text_geometry_ready = pass_plan.text_op_count == text_geometry_layer_count;
    let scene_path_tessellation_required =
        pass_plan.path_op_count > 0 && pass_plan.requires_path_tessellation;
    let scene_path_tessellation_ready = !pass_plan.requires_path_tessellation;
    let mut completed_boundaries = vec![
        "scene-package-to-core-layer-snapshot",
        "flattened-layer-ordering",
        "native-vulkan-draw-plan",
        "dynamic-rendering-present-route-selection",
        "synchronization2-submit2-scene-submit-model",
        "native-runtime-layer-coverage-metric",
        "timeline-animation-runtime",
        "scene-geometry-field-animation-runtime",
        "per-frame-timeline-geometry-runtime",
        "native-scene-graph-transform-opacity-execution",
        "parallax-property-camera-model",
        "property-update-runtime",
        "pause-resume-policy-runtime",
        "package-state-persistence",
    ];
    if clear_background_layer_count > 0 {
        completed_boundaries.push("clear-background-layer-composition");
    }
    if retained_resource_model_ready {
        completed_boundaries.push("retained-scene-resource-model");
    }
    if scene_binary_ingest_ready {
        completed_boundaries.push("streaming-binary-scene-ingest");
    }
    if scene_sampled_image_descriptor_heap_required {
        completed_boundaries.push("descriptor-heap-sampled-image-scene-resources");
    }
    if timeline_snapshot_runtime_ready {
        completed_boundaries.push("time-sampled-scene-state");
    }
    if pass_plan.vector_shape_op_count > 0 && !pass_plan.requires_path_tessellation {
        completed_boundaries.push("solid-vector-shape-quad-geometry");
    }
    if rounded_rectangle_layer_count > 0 {
        completed_boundaries.push("rounded-rectangle-tessellation-runtime");
    }
    if pass_plan.sampled_image_op_count > 0 && sampled_image_native_layer_count > 0 {
        completed_boundaries.push("sampled-image-scene-composition");
    }
    if pass_plan
        .sampled_image_recording_steps
        .iter()
        .any(|step| step.texture_region.is_some())
    {
        completed_boundaries.push("scene-we-spritesheet-atlas-runtime");
    }
    if video_native_layer_count > 0 {
        completed_boundaries.push("vulkan-video-scene-layer-composition");
    }
    if tessellated_path_layer_count > 0 {
        completed_boundaries.push("simple-path-tessellation-runtime");
    }
    if curve_path_layer_count > 0 {
        completed_boundaries.push("curve-path-flattening-runtime");
    }
    if arc_path_layer_count > 0 {
        completed_boundaries.push("arc-path-flattening-runtime");
    }
    if compound_evenodd_path_layer_count > 0 {
        completed_boundaries.push("compound-path-evenodd-fill-runtime");
    }
    if compound_nonzero_path_layer_count > 0 {
        completed_boundaries.push("compound-path-nonzero-fill-runtime");
    }
    if text_geometry_layer_count > 0 {
        completed_boundaries.push("deterministic-text-glyph-geometry-runtime");
    }
    if stroke_geometry_layer_count > 0 {
        completed_boundaries.push("stroke-geometry-runtime");
    }
    if scene_particle_system_ready {
        completed_boundaries.push("native-particle-system-runtime");
    }
    if scene_audio_cue_resource_model_ready {
        completed_boundaries.push("scene-audio-cue-renderer-boundary");
        completed_boundaries.push("scene-audio-cue-pipewire-present-runtime");
    }
    if scene_scenescript_ready {
        completed_boundaries.push("native-scenescript-expression-runtime");
    }
    if scene_shader_material_graph_ready {
        completed_boundaries.push("shader-material-graph");
        completed_boundaries.push("wallpaper-engine-material-graph-texture-runtime");
    }
    if scene_audio_response_ready {
        completed_boundaries.push("native-audio-response-visual-runtime");
    }
    if cursor_parallax_input_ready {
        completed_boundaries.push("cursor-parallax-input-source");
    }

    let mut pending_boundaries = Vec::new();
    let mut unsupported_boundaries = Vec::new();
    if native_runtime_pending_layer_count > 0 {
        pending_boundaries.push("remaining-scene-layer-runtime-coverage");
    }
    if pass_plan.video_op_count > 0 && video_native_layer_count < pass_plan.video_op_count {
        pending_boundaries.push("mixed-video-scene-composition");
    }
    if pass_plan.requires_text_geometry {
        pending_boundaries.push("text-glyph-geometry-runtime");
    }
    if pass_plan.path_op_count > 0 && pass_plan.requires_path_tessellation {
        pending_boundaries.push("path-tessellation-runtime");
    }
    if scene_scenescript_detected && !scene_scenescript_ready {
        pending_boundaries.push("arbitrary-scenescript-runtime");
    }
    if scene_shader_material_graph_detected && !scene_shader_material_graph_ready {
        pending_boundaries.push("shader-material-graph");
    }
    if scene_audio_response_detected && !scene_audio_response_ready {
        pending_boundaries.push("pipewire-audio-response-runtime");
    } else if scene_audio_response_ready {
        pending_boundaries.push("pipewire-audio-spectrum-input-source");
    }
    if scene_particle_system_detected && !scene_particle_system_ready {
        pending_boundaries.push("particle-systems");
    }
    if !cursor_parallax_input_ready {
        unsupported_boundaries.push("cursor-parallax-input-source");
    }
    let full_scene_complete = pending_boundaries.is_empty();
    let progress_estimate_percent = if full_scene_complete { 100 } else { 99 };

    NativeVulkanFullSceneRuntimeSnapshot {
        target_runtime: "native-vulkan-full-scene",
        current_runtime: "native-vulkan-scene-runtime",
        progress_estimate_percent,
        full_scene_complete,
        execution_model: "full scene state is lowered into explicit native Vulkan scene runtime boundaries with native scene graph transform/opacity execution, scene timeline animation, per-frame fixed-topology timeline geometry updates, deterministic SceneScript expression lowering, parallax property camera input, property update, pause/resume policy, state persistence, converted keyframe timeline input, converted WE .tex image resources, spritesheet atlas UV-frame animation, cubic/smooth-cubic/quadratic/smooth-quadratic/arc path flattening, compound even-odd path fill, and scene audio cues resolved into the renderer and played by the native FFmpeg/PipeWire scene present runtime; unsupported Wallpaper Engine systems remain visible instead of falling back to old paths",
        native_scene_graph_lowering_ready: plan.native_draw_ready(),
        native_present_route_ready: pass_plan.backend_ready,
        retained_resource_model_ready,
        scene_binary_ingest_ready,
        timeline_snapshot_runtime_ready,
        timeline_snapshot_time_ms: plan.snapshot_time_ms,
        timeline_animation_runtime_ready,
        timeline_animation_count,
        timeline_animated_layer_count,
        puppet_animation_layer_count,
        source_layer_count,
        active_scene_layer_count,
        flattened_draw_layer_count: plan.draw_ops.len(),
        unsupported_layer_count: plan.unsupported_layers.len(),
        native_runtime_layer_count,
        native_runtime_pending_layer_count,
        native_runtime_coverage_percent,
        clear_background_layer_count,
        solid_geometry_layer_count,
        rounded_rectangle_layer_count,
        sampled_image_native_layer_count,
        video_native_layer_count,
        tessellated_path_layer_count,
        curve_path_layer_count,
        arc_path_layer_count,
        compound_path_layer_count,
        text_geometry_layer_count,
        stroke_geometry_layer_count,
        color_layer_count: pass_plan.color_op_count,
        sampled_image_layer_count: pass_plan.sampled_image_op_count,
        video_layer_count: pass_plan.video_op_count,
        vector_shape_layer_count: pass_plan.vector_shape_op_count,
        text_layer_count: pass_plan.text_op_count,
        path_layer_count: pass_plan.path_op_count,
        property_update_runtime_ready,
        property_binding_count,
        pause_resume_policy_ready,
        package_state_persistence_ready,
        scene_state_persistence_model: "app-state-wallpaper-and-output-property-store",
        scene_audio_cue_count,
        scene_audio_cue_resource_model_ready,
        scene_scenescript_detected,
        scene_scenescript_ready,
        scene_scenescript_binding_count,
        scene_shader_material_graph_detected,
        scene_shader_material_graph_ready,
        scene_material_graph_count,
        scene_material_graph_resource_count,
        scene_effect_graph_count,
        scene_audio_response_detected,
        scene_audio_response_ready,
        scene_audio_response_binding_count,
        scene_particle_system_detected,
        scene_particle_system_ready,
        particle_runtime_layer_count,
        cursor_parallax_input_ready,
        scene_video_composition_required,
        scene_video_composition_ready,
        scene_text_geometry_required,
        scene_text_geometry_ready,
        scene_path_tessellation_required,
        scene_path_tessellation_ready,
        unsupported_scene_feature_count: unsupported_scene_features.len(),
        unsupported_scene_features,
        completed_boundaries,
        pending_boundaries,
        unsupported_boundaries,
    }
}

fn scene_feature_blocks_shader_material_graph(feature: &str) -> bool {
    feature.contains("shader")
        || feature.contains("effect")
        || matches!(
            feature,
            "we-material-texture-runtime"
                | "we-model-material-texture-runtime"
                | "we-runtime-texture"
                | "runtime-texture"
        )
}

fn native_vulkan_scene_path_uses_curves(path: &str) -> bool {
    path.chars()
        .any(|character| matches!(character, 'C' | 'c' | 'S' | 's' | 'Q' | 'q' | 'T' | 't'))
}

fn native_vulkan_scene_path_uses_arcs(path: &str) -> bool {
    path.chars().any(|character| matches!(character, 'A' | 'a'))
}

fn native_vulkan_scene_path_uses_compound_subpaths(path: &str) -> bool {
    path.chars()
        .filter(|character| matches!(character, 'M' | 'm'))
        .take(2)
        .count()
        > 1
}

fn native_vulkan_scene_binary_ingest_runtime_snapshot(
    render_item: &NativeVulkanRenderItem,
) -> Option<NativeVulkanSceneBinaryIngestRuntimeSnapshot> {
    let source = native_vulkan_scene_binary_source_path(render_item)?;
    let mut file = File::open(source).ok()?;
    native_vulkan_scene_binary_ingest_from_reader(&mut file)
        .ok()
        .map(NativeVulkanSceneBinaryIngestRuntimeSnapshot::from_summary)
}

fn native_vulkan_scene_binary_source_path(render_item: &NativeVulkanRenderItem) -> Option<&Path> {
    let NativeVulkanRenderItem::Scene {
        scene_source: Some(source),
        ..
    } = render_item
    else {
        return None;
    };
    if source.extension().and_then(|extension| extension.to_str()) == Some("gscn") {
        Some(source.as_path())
    } else {
        None
    }
}

fn native_vulkan_scene_resource_model(backend_status: &str, video_op_count: usize) -> &'static str {
    match backend_status {
        "fast-clear-color-ready" => "fast-clear-only-no-scene-resources",
        "solid-quad-recording-ready" => "retained-solid-quad-geometry",
        "clear-background-solid-quad-recording-ready" => {
            "clear-background-and-retained-solid-quad-geometry"
        }
        "video-layer-vulkan-video-scene-bridge-ready" => {
            "retained-vulkan-video-and-scene-overlay-resources"
        }
        "clear-background-video-layer-vulkan-video-scene-bridge-ready" => {
            "clear-background-and-retained-vulkan-video-scene-resource"
        }
        "multi-video-layer-vulkan-video-scene-bridge-ready" => {
            "retained-vulkan-video-multi-source-multi-layer-scene-resources"
        }
        "sampled-image-recording-ready"
        | "sampled-image-implicit-full-extent-ready"
        | "clear-background-sampled-image-recording-ready"
        | "clear-background-sampled-image-implicit-full-extent-ready" => {
            "retained-sampled-images-descriptor-heap"
        }
        "mixed-quad-sampled-image-recording-ready"
        | "mixed-quad-sampled-image-implicit-full-extent-ready"
        | "clear-background-mixed-quad-sampled-image-recording-ready"
        | "clear-background-mixed-quad-sampled-image-implicit-full-extent-ready" => {
            "retained-solid-quad-geometry-and-sampled-images-descriptor-heap"
        }
        _ if video_op_count > 0 => "retained-video-layer-vulkan-video-bridge-pending",
        _ => "not-native-vulkan-presentable-yet",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::SceneDocument;
    use crate::core::scene::binary::scene_binary_payloads_from_document;
    use crate::core::scene::{SceneMesh, SceneMeshVertex, SceneSnapshotLayer, SceneTextureSlot};
    use crate::core::{
        FitMode, PackagePath, SceneNodeKind, ScenePathFillRule, SceneSystemStatus, SceneSystems,
        SceneTextAlign, SceneTransform,
    };
    use crate::renderer::native_vulkan::NativeVulkanRenderItem;
    use crate::renderer::{
        SceneDisplayPlan, SceneRenderAlphaTextureMode, SceneRenderAudioCue,
        SceneRenderImageEffectPass, SceneRenderLayer, SceneRenderTextureSlot,
    };
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn texture_slot_binding_snapshots(
        resources: &[u32],
    ) -> Vec<NativeVulkanSceneTextureSlotResourceBindingSnapshot> {
        resources
            .iter()
            .copied()
            .enumerate()
            .map(
                |(slot, resource_index)| NativeVulkanSceneTextureSlotResourceBindingSnapshot {
                    slot: slot.min(u32::MAX as usize) as u32,
                    resource_index,
                },
            )
            .collect()
    }

    fn vulkanalia_texture_slot_bindings(
        resources: &[u32],
    ) -> Vec<NativeVulkanVulkanaliaSceneTextureSlotResourceBinding> {
        resources
            .iter()
            .copied()
            .enumerate()
            .map(
                |(slot, resource_index)| NativeVulkanVulkanaliaSceneTextureSlotResourceBinding {
                    slot: slot.min(u32::MAX as usize) as u32,
                    resource_index,
                },
            )
            .collect()
    }

    fn scene_test_layer(id: &str, kind: SceneNodeKind) -> SceneRenderLayer {
        SceneRenderLayer {
            id: id.to_owned(),
            kind,
            source: None,
            texture_slots: Vec::new(),
            alpha_texture_slot: None,
            alpha_texture_mode: Default::default(),
            image_effect_passes: Vec::new(),
            composite_key: None,
            texture_region: None,
            effect_motion: Default::default(),
            blend_mode: SceneBlendMode::Alpha,
            audio: Vec::new(),
            color: None,
            stroke_color: None,
            stroke_width: None,
            corner_radius: None,
            width: None,
            height: None,
            mesh: None,
            text: None,
            font_size: None,
            font_family: None,
            font_source: None,
            font_weight: None,
            text_align: None,
            path_data: None,
            path_fill_rule: ScenePathFillRule::default(),
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneTransform::default(),
        }
    }

    fn scene_test_snapshot_layer(id: &str, kind: SceneNodeKind) -> SceneSnapshotLayer {
        SceneSnapshotLayer {
            id: id.to_owned(),
            kind,
            source: None,
            texture_slots: Vec::new(),
            alpha_texture_slot: None,
            alpha_texture_mode: Default::default(),
            image_effect_passes: Vec::new(),
            composite_key: None,
            texture_region: None,
            effect_motion: Default::default(),
            blend_mode: SceneBlendMode::Alpha,
            audio: Vec::new(),
            color: None,
            stroke_color: None,
            stroke_width: None,
            corner_radius: None,
            width: None,
            height: None,
            mesh: None,
            parallax_depth: None,
            text: None,
            font_size: None,
            font_family: None,
            font_source: None,
            font_weight: None,
            text_align: None,
            path_data: None,
            path_fill_rule: ScenePathFillRule::default(),
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneTransform::default(),
        }
    }

    fn scene_test_item(
        layers: Vec<SceneRenderLayer>,
        display: Option<SceneDisplayPlan>,
    ) -> NativeVulkanRenderItem {
        scene_test_item_with_scene_metadata(layers, display, Vec::new(), 0, 0, 0)
    }

    fn scene_test_item_with_cursor_parallax(
        layers: Vec<SceneRenderLayer>,
        display: Option<SceneDisplayPlan>,
    ) -> NativeVulkanRenderItem {
        let mut item = scene_test_item(layers, display);
        let NativeVulkanRenderItem::Scene {
            cursor_parallax_input_ready,
            ..
        } = &mut item
        else {
            unreachable!("scene_test_item always returns a scene item");
        };
        *cursor_parallax_input_ready = true;
        item
    }

    fn scene_test_item_with_scene_metadata(
        layers: Vec<SceneRenderLayer>,
        display: Option<SceneDisplayPlan>,
        bound_properties: Vec<String>,
        timeline_animation_count: usize,
        timeline_animated_layer_count: usize,
        property_binding_count: usize,
    ) -> NativeVulkanRenderItem {
        let audio_cue_count = layers.iter().map(|layer| layer.audio.len()).sum();
        NativeVulkanRenderItem::Scene {
            output_name: "HDMI-A-1".to_owned(),
            scene_source: Some(PathBuf::from("/tmp/scene.json")),
            display,
            display_image: None,
            display_color: None,
            manifest_max_fps: Some(60),
            layer_count: layers.len(),
            layers,
            scene_systems: SceneSystems::default(),
            audio_cue_count,
            bound_properties,
            timeline_animation_count,
            timeline_animated_layer_count,
            puppet_animation_layer_count: 0,
            property_binding_count,
            cursor_parallax_input_ready: false,
            dynamic_topology_required: false,
            scene_scenescript_binding_count: 0,
            scene_material_graph_count: 0,
            scene_material_graph_resource_count: 0,
            scene_effect_graph_count: 0,
            scene_audio_response_binding_count: 0,
            unsupported_scene_features: Vec::new(),
            snapshot_time_ms: 1234,
            scene_size: None,
            scene_fit: FitMode::Cover,
            target_max_fps: Some(60),
            renderer_status: "deterministic-scene-snapshot-ready-for-vulkan-passes",
        }
    }

    #[test]
    fn scene_runtime_snapshot_reports_native_draw_ready_layers() {
        let mut image = scene_test_layer("hero", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/scene-hero.png"));
        image.fit = FitMode::Contain;
        let mut rectangle = scene_test_layer("panel", SceneNodeKind::Rectangle);
        rectangle.color = Some("#102030".to_owned());
        rectangle.width = Some(640.0);
        rectangle.height = Some(360.0);
        rectangle.corner_radius = Some(12.0);
        rectangle.transform.x = 24.0;
        rectangle.opacity = 1.25;
        let mut text = scene_test_layer("label", SceneNodeKind::Text);
        text.text = Some("Now Playing".to_owned());
        text.color = Some("#ffffff".to_owned());
        text.font_size = Some(24.0);
        text.font_family = Some("Inter".to_owned());
        text.font_weight = Some("600".to_owned());
        text.text_align = Some(SceneTextAlign::Middle);
        text.image_effect_passes = vec![SceneRenderImageEffectPass {
            effect_file: "effects/scroll/effect.json".to_owned(),
            runtime: Some("wallpaper-engine-effect".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/scroll".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            texture_slots: Vec::new(),
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];
        let mut hidden_group = scene_test_layer("hidden-group", SceneNodeKind::Group);
        hidden_group.opacity = 0.0;
        let item = scene_test_item(
            vec![image, rectangle, text, hidden_group],
            Some(SceneDisplayPlan::Color {
                color: "#010203".to_owned(),
            }),
        );

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert_eq!(snapshot.snapshot_time_ms, 1234);
        assert_eq!(
            snapshot.scene_input_model,
            "core scene snapshot layers; groups must be flattened before native Vulkan planning"
        );
        assert!(snapshot.native_draw_ready);
        assert!(snapshot.runtime_display_available);
        assert!(snapshot.draw_pass_plan_ready);
        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.scene_resource_model,
            "retained-solid-quad-geometry-and-sampled-images-descriptor-heap"
        );
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "mixed-quad-sampled-image-implicit-full-extent-ready"
        );
        assert_eq!(snapshot.draw_pass_blocking_reason, None);
        assert_eq!(snapshot.draw_pass_recordable_op_count, 2);
        assert_eq!(snapshot.draw_pass_color_op_count, 0);
        assert_eq!(snapshot.draw_pass_sampled_image_op_count, 1);
        assert_eq!(snapshot.draw_pass_video_op_count, 0);
        assert_eq!(snapshot.draw_pass_vector_shape_op_count, 1);
        assert_eq!(snapshot.draw_pass_text_op_count, 1);
        assert_eq!(snapshot.draw_pass_path_op_count, 0);
        assert_eq!(snapshot.draw_pass_effect_pass_count, 1);
        assert_eq!(snapshot.draw_pass_effect_pass_non_image_layer_count, 1);
        assert_eq!(
            snapshot
                .draw_pass_effect_pass_kind_counts
                .get("scroll")
                .copied(),
            Some(1)
        );
        assert_eq!(
            snapshot.draw_pass_required_image_resources,
            vec![PathBuf::from("/tmp/scene-hero.png")]
        );
        assert!(snapshot.draw_pass_required_video_resources.is_empty());
        assert!(!snapshot.draw_pass_requires_text_geometry);
        assert!(!snapshot.draw_pass_requires_path_tessellation);
        assert!(!snapshot.draw_pass_requires_video_decode);
        assert_eq!(snapshot.draw_pass_fast_clear_color, None);
        assert_eq!(snapshot.draw_op_count, 3);
        assert_eq!(snapshot.unsupported_layer_count, 0);
        assert_eq!(
            snapshot
                .draw_ops
                .iter()
                .map(|op| op.kind)
                .collect::<Vec<_>>(),
            vec!["image", "rectangle", "text"]
        );
        assert_eq!(snapshot.draw_ops[0].layer_index, 0);
        assert_eq!(snapshot.draw_ops[1].layer_index, 1);
        assert_eq!(snapshot.draw_ops[2].layer_index, 2);
        assert_eq!(
            snapshot.draw_ops[0].source.as_deref(),
            Some(Path::new("/tmp/scene-hero.png"))
        );
        assert_eq!(snapshot.draw_ops[0].fit, FitMode::Contain);
        assert_eq!(snapshot.draw_ops[1].opacity, 1.0);
        assert_eq!(snapshot.draw_ops[1].color.as_deref(), Some("#102030"));
        assert_eq!(snapshot.draw_ops[1].width, Some(640.0));
        assert_eq!(snapshot.draw_ops[1].height, Some(360.0));
        assert_eq!(snapshot.draw_ops[1].corner_radius, Some(12.0));
        assert_eq!(snapshot.draw_ops[1].transform.x, 24.0);
        assert_eq!(snapshot.draw_ops[2].text.as_deref(), Some("Now Playing"));
        assert_eq!(snapshot.draw_ops[2].color.as_deref(), Some("#ffffff"));
        assert_eq!(snapshot.draw_ops[2].font_size, Some(24.0));
        assert_eq!(snapshot.draw_ops[2].font_family.as_deref(), Some("Inter"));
        assert_eq!(snapshot.draw_ops[2].font_weight.as_deref(), Some("600"));
        assert_eq!(
            snapshot.draw_ops[2].text_align,
            Some(SceneTextAlign::Middle)
        );
        assert_eq!(snapshot.draw_ops[2].image_effect_pass_count, 1);
        assert_eq!(snapshot.draw_ops[2].effect_passes[0].kind, "scroll");
        assert_eq!(
            snapshot.draw_ops[2].effect_passes[0].effect_file,
            "effects/scroll/effect.json"
        );
        assert_eq!(
            snapshot.draw_ops[2].effect_passes[0].shader.as_deref(),
            Some("effects/scroll")
        );
        assert_eq!(
            snapshot.draw_pass_recordable_quads[0].kind,
            "rounded-rectangle"
        );
        assert_eq!(
            snapshot.draw_pass_recordable_quads[0].corner_radius,
            Some(12.0)
        );
        assert_eq!(snapshot.draw_pass_recordable_quads[1].kind, "text");
        assert_eq!(
            snapshot.draw_pass_recordable_quads[1].text.as_deref(),
            Some("Now Playing")
        );
        assert_eq!(snapshot.draw_pass_quad_recording_steps.len(), 2);
        assert_eq!(snapshot.draw_pass_quad_recording_steps[1].kind, "text");
        assert!(snapshot.draw_pass_quad_recording_steps[1].vertex_count > 4);
        assert_eq!(snapshot.full_scene.solid_geometry_layer_count, 2);
        assert_eq!(snapshot.full_scene.rounded_rectangle_layer_count, 1);
        assert_eq!(snapshot.full_scene.text_geometry_layer_count, 1);
        assert_eq!(snapshot.full_scene.stroke_geometry_layer_count, 0);
        assert_eq!(snapshot.full_scene.native_runtime_layer_count, 3);
        assert_eq!(snapshot.full_scene.native_runtime_pending_layer_count, 0);
        assert_eq!(snapshot.full_scene.native_runtime_coverage_percent, 100);
        assert_eq!(snapshot.full_scene.progress_estimate_percent, 100);
        assert!(snapshot.full_scene.full_scene_complete);
    }

    #[test]
    fn scene_runtime_snapshot_reports_streaming_binary_scene_ingest_source() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "base", "type": "image", "source": "assets/base.gtex", "width": 64, "height": 64 },
                { "id": "mask", "type": "image", "source": "assets/mask.gtex", "width": 64, "height": 64 }
            ],
            "nodes": [
                {
                    "id": "mesh-node",
                    "type": "image",
                    "resource": "base",
                    "mesh": {
                        "vertices": [
                            { "x": -1.0, "y": -1.0, "u": 0.0, "v": 0.0 },
                            { "x": 1.0, "y": -1.0, "u": 1.0, "v": 0.0 },
                            { "x": 0.0, "y": 1.0, "u": 0.5, "v": 1.0 }
                        ],
                        "indices": [0, 1, 2]
                    },
                    "effects": [
                        {
                            "file": "effects/opacity/effect.json",
                            "properties": { "phase": 0.5 },
                            "passes": [
                                {
                                    "shader": "effects/opacity",
                                    "texture_resources": ["base", "mask"],
                                    "constant_shader_values": { "speed": 2.0 },
                                    "combos": { "MASK": 1 }
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");
        let bytes = scene_binary_payloads_from_document(&document)
            .encode_container(0x80)
            .expect("binary scene");
        let path = std::env::temp_dir().join(format!(
            "gilder-scene-runtime-binary-ingest-{}.gscn",
            std::process::id()
        ));
        fs::write(&path, bytes).expect("write binary scene");
        let mut image = scene_test_layer("hero", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/scene-hero.png"));
        let mut item = scene_test_item(vec![image], None);
        let NativeVulkanRenderItem::Scene { scene_source, .. } = &mut item else {
            unreachable!("scene_test_item always returns a scene item");
        };
        *scene_source = Some(path.clone());

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");
        let _ = fs::remove_file(path);
        let ingest = snapshot
            .scene_binary_ingest
            .expect("binary ingest runtime summary");

        assert_eq!(ingest.input_model, "gscn-versioned-binary-chunks");
        assert_eq!(
            ingest.payload_retention_model,
            "read-header-table-stream-records-drop-source-bytes"
        );
        assert_eq!(ingest.feature_flags, 0x80);
        assert_eq!(ingest.mesh_vertex_stream_bytes, 60);
        assert_eq!(ingest.mesh_index_stream_bytes, 12);
        assert_eq!(
            ingest.retained.record_count,
            ingest.retained.stable_id_count
        );
        assert!(snapshot.full_scene.scene_binary_ingest_ready);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"streaming-binary-scene-ingest")
        );
    }

    #[test]
    fn scene_runtime_snapshot_reports_video_layer_bridge_boundary() {
        let mut video = scene_test_layer("cinematic", SceneNodeKind::Video);
        video.source = Some(PathBuf::from("/tmp/scene-video.mp4"));
        video.fit = FitMode::Cover;
        video.width = Some(1280.0);
        video.height = Some(720.0);
        let item = scene_test_item(vec![video], None);

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert!(snapshot.native_draw_ready);
        assert!(snapshot.draw_pass_plan_ready);
        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "video-layer-vulkan-video-scene-bridge-ready"
        );
        assert_eq!(snapshot.draw_pass_blocking_reason, None);
        assert_eq!(
            snapshot.scene_resource_model,
            "retained-vulkan-video-and-scene-overlay-resources"
        );
        assert_eq!(snapshot.draw_pass_video_op_count, 1);
        assert_eq!(snapshot.scene_video_layer_resource_count, 1);
        assert_eq!(snapshot.scene_video_native_layer_count, 1);
        assert_eq!(snapshot.full_scene.video_native_layer_count, 1);
        assert_eq!(snapshot.full_scene.native_runtime_layer_count, 1);
        assert_eq!(snapshot.full_scene.native_runtime_pending_layer_count, 0);
        assert_eq!(snapshot.full_scene.native_runtime_coverage_percent, 100);
        assert!(snapshot.full_scene.scene_video_composition_required);
        assert!(snapshot.full_scene.scene_video_composition_ready);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"vulkan-video-scene-layer-composition")
        );
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"mixed-video-scene-composition")
        );
        assert_eq!(
            snapshot.draw_pass_required_video_resources,
            vec![PathBuf::from("/tmp/scene-video.mp4")]
        );
        assert!(snapshot.draw_pass_requires_video_decode);
        assert_eq!(snapshot.draw_ops.len(), 1);
        assert_eq!(snapshot.draw_ops[0].kind, "video");
        assert_eq!(
            snapshot.draw_ops[0].source.as_deref(),
            Some(Path::new("/tmp/scene-video.mp4"))
        );
    }

    #[test]
    fn scene_runtime_snapshot_reports_same_source_multi_video_bridge_ready() {
        let mut left = scene_test_layer("left-video", SceneNodeKind::Video);
        left.source = Some(PathBuf::from("/tmp/scene-video.mp4"));
        left.width = Some(640.0);
        left.height = Some(360.0);
        left.transform.x = 0.0;
        let mut right = scene_test_layer("right-video", SceneNodeKind::Video);
        right.source = Some(PathBuf::from("/tmp/scene-video.mp4"));
        right.width = Some(640.0);
        right.height = Some(360.0);
        right.transform.x = 640.0;
        let item = scene_test_item(vec![left, right], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "multi-video-layer-vulkan-video-scene-bridge-ready"
        );
        assert_eq!(
            snapshot.scene_resource_model,
            "retained-vulkan-video-multi-source-multi-layer-scene-resources"
        );
        assert_eq!(snapshot.draw_pass_video_op_count, 2);
        assert_eq!(snapshot.scene_video_layer_resource_count, 1);
        assert_eq!(snapshot.scene_video_native_layer_count, 2);
        assert_eq!(snapshot.full_scene.video_native_layer_count, 2);
        assert_eq!(snapshot.full_scene.native_runtime_layer_count, 2);
        assert_eq!(snapshot.full_scene.native_runtime_pending_layer_count, 0);
        assert!(snapshot.full_scene.scene_video_composition_ready);
        assert!(snapshot.draw_pass_video_recording_ready);
        assert_eq!(snapshot.draw_pass_video_recording_step_count, 2);
        assert_eq!(snapshot.draw_pass_video_vertices.len(), 8);
        assert_eq!(snapshot.draw_pass_video_indices.len(), 12);
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"mixed-video-scene-composition")
        );

        let geometry = snapshot
            .take_vulkanalia_video_layer_geometry_input()
            .expect("same-source multi video geometry");
        assert_eq!(
            geometry.sources,
            vec![PathBuf::from("/tmp/scene-video.mp4")]
        );
        assert_eq!(geometry.draw_steps.len(), 2);
        assert_eq!(geometry.draw_steps[0].resource_index, 0);
        assert_eq!(geometry.draw_steps[1].resource_index, 0);
        assert_eq!(geometry.vertices.len(), 8);
        assert_eq!(geometry.indices.len(), 12);
    }

    #[test]
    fn scene_runtime_snapshot_reports_distinct_n_source_video_bridge_ready() {
        let mut sky = scene_test_layer("sky-video", SceneNodeKind::Video);
        sky.source = Some(PathBuf::from("/tmp/sky.mp4"));
        sky.width = Some(1920.0);
        sky.height = Some(1080.0);
        let mut character = scene_test_layer("character-video", SceneNodeKind::Video);
        character.source = Some(PathBuf::from("/tmp/character.mp4"));
        character.width = Some(640.0);
        character.height = Some(1080.0);
        character.transform.x = 640.0;
        let mut effects = scene_test_layer("effects-video", SceneNodeKind::Video);
        effects.source = Some(PathBuf::from("/tmp/effects.mp4"));
        effects.width = Some(1920.0);
        effects.height = Some(1080.0);
        let item = scene_test_item(vec![sky, character, effects], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "multi-video-layer-vulkan-video-scene-bridge-ready"
        );
        assert_eq!(
            snapshot.scene_resource_model,
            "retained-vulkan-video-multi-source-multi-layer-scene-resources"
        );
        assert_eq!(snapshot.scene_video_layer_resource_count, 3);
        assert_eq!(snapshot.scene_video_native_layer_count, 3);
        assert!(snapshot.full_scene.scene_video_composition_ready);
        assert_eq!(snapshot.full_scene.native_runtime_pending_layer_count, 0);

        let geometry = snapshot
            .take_vulkanalia_video_layer_geometry_input()
            .expect("distinct-source n video geometry");
        assert_eq!(
            geometry.sources,
            vec![
                PathBuf::from("/tmp/sky.mp4"),
                PathBuf::from("/tmp/character.mp4"),
                PathBuf::from("/tmp/effects.mp4")
            ]
        );
        assert_eq!(geometry.draw_steps.len(), 3);
        assert_eq!(geometry.draw_steps[0].resource_index, 0);
        assert_eq!(geometry.draw_steps[1].resource_index, 1);
        assert_eq!(geometry.draw_steps[2].resource_index, 2);
    }

    #[test]
    fn scene_runtime_snapshot_reports_clear_background_video_bridge_boundary() {
        let mut background = scene_test_layer("background", SceneNodeKind::Color);
        background.color = Some("#102030".to_owned());
        let mut video = scene_test_layer("cinematic", SceneNodeKind::Video);
        video.source = Some(PathBuf::from("/tmp/scene-video.mp4"));
        video.fit = FitMode::Contain;
        let item = scene_test_item(vec![background, video], None);

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "clear-background-video-layer-vulkan-video-scene-bridge-ready"
        );
        assert_eq!(snapshot.draw_pass_blocking_reason, None);
        assert_eq!(
            snapshot.scene_resource_model,
            "clear-background-and-retained-vulkan-video-scene-resource"
        );
        assert_eq!(
            snapshot.draw_pass_background_clear_color.as_deref(),
            Some("#102030")
        );
        assert_eq!(snapshot.draw_pass_color_op_count, 1);
        assert_eq!(snapshot.draw_pass_video_op_count, 1);
        assert_eq!(snapshot.scene_video_native_layer_count, 1);
        assert_eq!(snapshot.full_scene.clear_background_layer_count, 1);
        assert_eq!(snapshot.full_scene.video_native_layer_count, 1);
        assert_eq!(snapshot.full_scene.native_runtime_layer_count, 2);
        assert_eq!(snapshot.full_scene.native_runtime_pending_layer_count, 0);
        assert_eq!(snapshot.full_scene.native_runtime_coverage_percent, 100);
        assert!(snapshot.full_scene.scene_video_composition_ready);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"clear-background-layer-composition")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"vulkan-video-scene-layer-composition")
        );
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"mixed-video-scene-composition")
        );
    }

    #[test]
    fn scene_runtime_snapshot_reports_mixed_video_scene_bridge_boundary() {
        let mut video = scene_test_layer("cinematic", SceneNodeKind::Video);
        video.source = Some(PathBuf::from("/tmp/scene-video.mp4"));
        video.width = Some(3840.0);
        video.height = Some(2160.0);
        let mut overlay = scene_test_layer("overlay", SceneNodeKind::Image);
        overlay.source = Some(PathBuf::from("/tmp/overlay.gtex"));
        overlay.width = Some(256.0);
        overlay.height = Some(256.0);
        let mut panel = scene_test_layer("panel", SceneNodeKind::Rectangle);
        panel.color = Some("#102030".to_owned());
        panel.width = Some(320.0);
        panel.height = Some(180.0);
        let item = scene_test_item(vec![video, overlay, panel], None);

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "video-layer-vulkan-video-scene-bridge-ready"
        );
        assert_eq!(
            snapshot.scene_resource_model,
            "retained-vulkan-video-and-scene-overlay-resources"
        );
        assert_eq!(snapshot.draw_pass_video_op_count, 1);
        assert_eq!(snapshot.full_scene.video_native_layer_count, 1);
        assert_eq!(snapshot.full_scene.sampled_image_native_layer_count, 1);
        assert_eq!(snapshot.full_scene.solid_geometry_layer_count, 1);
        assert_eq!(snapshot.full_scene.native_runtime_pending_layer_count, 0);
        assert!(snapshot.full_scene.scene_video_composition_required);
        assert!(snapshot.full_scene.scene_video_composition_ready);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"vulkan-video-scene-layer-composition")
        );
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"mixed-video-scene-composition")
        );
    }

    #[test]
    fn full_scene_runtime_snapshot_tracks_scene_scope_and_remaining_boundaries() {
        let mut background = scene_test_layer("background", SceneNodeKind::Image);
        background.source = Some(PathBuf::from("/tmp/background.png"));
        let mut clip = scene_test_layer("clip", SceneNodeKind::Video);
        clip.source = Some(PathBuf::from("/tmp/clip.mp4"));
        let mut clip_alt = scene_test_layer("clip-alt", SceneNodeKind::Video);
        clip_alt.source = Some(PathBuf::from("/tmp/clip-alt.mp4"));
        let mut label = scene_test_layer("label", SceneNodeKind::Text);
        label.text = Some("Now Playing".to_owned());
        label.color = Some("#ffffff".to_owned());
        let item = scene_test_item_with_scene_metadata(
            vec![background, clip, clip_alt, label],
            None,
            vec!["scene_opacity".to_owned()],
            2,
            1,
            2,
        );

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).unwrap();

        assert_eq!(
            snapshot.full_scene.target_runtime,
            "native-vulkan-full-scene"
        );
        assert_eq!(
            snapshot.full_scene.current_runtime,
            "native-vulkan-scene-runtime"
        );
        assert_eq!(snapshot.full_scene.progress_estimate_percent, 99);
        assert!(!snapshot.full_scene.full_scene_complete);
        assert!(snapshot.full_scene.timeline_snapshot_runtime_ready);
        assert_eq!(snapshot.full_scene.timeline_snapshot_time_ms, 1234);
        assert!(snapshot.full_scene.timeline_animation_runtime_ready);
        assert_eq!(snapshot.full_scene.timeline_animation_count, 2);
        assert_eq!(snapshot.full_scene.timeline_animated_layer_count, 1);
        assert_eq!(snapshot.full_scene.source_layer_count, 4);
        assert_eq!(snapshot.full_scene.active_scene_layer_count, 4);
        assert_eq!(snapshot.full_scene.flattened_draw_layer_count, 4);
        assert_eq!(snapshot.full_scene.native_runtime_layer_count, 2);
        assert_eq!(snapshot.full_scene.native_runtime_pending_layer_count, 2);
        assert_eq!(snapshot.full_scene.native_runtime_coverage_percent, 50);
        assert_eq!(snapshot.full_scene.sampled_image_native_layer_count, 1);
        assert_eq!(snapshot.full_scene.sampled_image_layer_count, 1);
        assert_eq!(snapshot.full_scene.video_layer_count, 2);
        assert_eq!(snapshot.full_scene.text_layer_count, 1);
        assert_eq!(snapshot.full_scene.stroke_geometry_layer_count, 0);
        assert!(snapshot.full_scene.property_update_runtime_ready);
        assert_eq!(snapshot.full_scene.property_binding_count, 2);
        assert!(snapshot.full_scene.pause_resume_policy_ready);
        assert!(snapshot.full_scene.package_state_persistence_ready);
        assert_eq!(
            snapshot.full_scene.scene_state_persistence_model,
            "app-state-wallpaper-and-output-property-store"
        );
        assert!(!snapshot.full_scene.scene_audio_response_ready);
        assert_eq!(snapshot.full_scene.scene_audio_cue_count, 0);
        assert!(!snapshot.full_scene.scene_audio_cue_resource_model_ready);
        assert!(!snapshot.full_scene.scene_audio_response_detected);
        assert!(!snapshot.full_scene.cursor_parallax_input_ready);
        assert!(snapshot.full_scene.scene_video_composition_required);
        assert!(!snapshot.full_scene.scene_video_composition_ready);
        assert!(snapshot.full_scene.scene_text_geometry_required);
        assert!(snapshot.full_scene.scene_text_geometry_ready);
        assert_eq!(snapshot.full_scene.text_geometry_layer_count, 1);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"time-sampled-scene-state")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"timeline-animation-runtime")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"scene-geometry-field-animation-runtime")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"per-frame-timeline-geometry-runtime")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"native-scene-graph-transform-opacity-execution")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"parallax-property-camera-model")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"property-update-runtime")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"pause-resume-policy-runtime")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"package-state-persistence")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"sampled-image-scene-composition")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"deterministic-text-glyph-geometry-runtime")
        );
        assert!(
            snapshot
                .full_scene
                .pending_boundaries
                .contains(&"remaining-scene-layer-runtime-coverage")
        );
        assert!(
            snapshot
                .full_scene
                .pending_boundaries
                .contains(&"mixed-video-scene-composition")
        );
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"pipewire-audio-response-runtime")
        );
        assert!(
            snapshot
                .full_scene
                .unsupported_boundaries
                .contains(&"cursor-parallax-input-source")
        );
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"cursor-parallax-input-source")
        );
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"timeline-animation-runtime")
        );
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"package-state-persistence")
        );
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"full-wallpaper-engine-scene-graph")
        );
    }

    #[test]
    fn full_scene_runtime_snapshot_tracks_scene_audio_cue_boundary() {
        let mut image = scene_test_layer("speaker", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/cover.png"));
        image.audio.push(SceneRenderAudioCue {
            source: PathBuf::from("/tmp/sounds/theme.ogg"),
            playback_mode: Some("loop".to_owned()),
            volume: None,
            start_silent: false,
            active_conditions: Vec::new(),
        });
        let item = scene_test_item(vec![image], None);

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).unwrap();

        assert_eq!(snapshot.full_scene.scene_audio_cue_count, 1);
        assert!(snapshot.full_scene.scene_audio_cue_resource_model_ready);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"scene-audio-cue-renderer-boundary")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"scene-audio-cue-pipewire-present-runtime")
        );
    }

    #[test]
    fn full_scene_runtime_executes_native_audio_response_visual_geometry() {
        let mut response = scene_test_layer("bass-bars", SceneNodeKind::AudioResponse);
        response.color = Some("#44ccff".to_owned());
        response.width = Some(320.0);
        response.height = Some(48.0);
        let mut item = scene_test_item(vec![response], None);
        let NativeVulkanRenderItem::Scene {
            scene_systems,
            scene_audio_response_binding_count,
            ..
        } = &mut item
        else {
            unreachable!("scene_test_item always returns a scene item");
        };
        scene_systems.audio_response = SceneSystemStatus::Ready;
        *scene_audio_response_binding_count = 1;

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).unwrap();

        assert!(snapshot.native_draw_ready);
        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "solid-quad-recording-ready"
        );
        assert_eq!(snapshot.draw_ops[0].kind, "audio-response");
        assert_eq!(
            snapshot.draw_pass_recordable_quads[0].kind,
            "audio-response"
        );
        assert_eq!(snapshot.full_scene.native_runtime_layer_count, 1);
        assert_eq!(snapshot.full_scene.native_runtime_pending_layer_count, 0);
        assert!(snapshot.full_scene.scene_audio_response_detected);
        assert!(snapshot.full_scene.scene_audio_response_ready);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"native-audio-response-visual-runtime")
        );
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"pipewire-audio-response-runtime")
        );
        assert!(
            snapshot
                .full_scene
                .pending_boundaries
                .contains(&"pipewire-audio-spectrum-input-source")
        );
    }

    #[test]
    fn full_scene_runtime_snapshot_tracks_cursor_parallax_input_boundary() {
        let mut panel = scene_test_layer("panel", SceneNodeKind::Rectangle);
        panel.color = Some("#203040".to_owned());
        panel.width = Some(320.0);
        panel.height = Some(180.0);
        let item = scene_test_item_with_cursor_parallax(vec![panel], None);

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).unwrap();

        assert!(snapshot.full_scene.cursor_parallax_input_ready);
        assert_eq!(snapshot.full_scene.progress_estimate_percent, 100);
        assert!(snapshot.full_scene.full_scene_complete);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"cursor-parallax-input-source")
        );
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"cursor-parallax-input-source")
        );
        assert!(
            !snapshot
                .full_scene
                .unsupported_boundaries
                .contains(&"cursor-parallax-input-source")
        );
    }

    #[test]
    fn full_scene_runtime_snapshot_tracks_shader_material_graph_boundary() {
        let mut image = scene_test_layer("hero", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/hero.gtex"));
        let mut item = scene_test_item(vec![image], None);
        let NativeVulkanRenderItem::Scene {
            scene_systems,
            scene_material_graph_count,
            scene_material_graph_resource_count,
            ..
        } = &mut item
        else {
            unreachable!("scene_test_item always returns a scene item");
        };
        scene_systems.shader_material_graph = SceneSystemStatus::Ready;
        *scene_material_graph_count = 1;
        *scene_material_graph_resource_count = 2;

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).unwrap();

        assert!(snapshot.full_scene.scene_shader_material_graph_detected);
        assert!(snapshot.full_scene.scene_shader_material_graph_ready);
        assert_eq!(snapshot.full_scene.scene_material_graph_count, 1);
        assert_eq!(snapshot.full_scene.scene_material_graph_resource_count, 2);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"shader-material-graph")
        );
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"shader-material-graph")
        );
    }

    #[test]
    fn full_scene_runtime_keeps_shader_material_graph_pending_for_effect_graphs() {
        let mut image = scene_test_layer("hero", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/hero.gtex"));
        let mut item = scene_test_item(vec![image], None);
        let NativeVulkanRenderItem::Scene {
            scene_systems,
            scene_material_graph_count,
            scene_material_graph_resource_count,
            scene_effect_graph_count,
            unsupported_scene_features,
            ..
        } = &mut item
        else {
            unreachable!("scene_test_item always returns a scene item");
        };
        scene_systems.shader_material_graph = SceneSystemStatus::Ready;
        *scene_material_graph_count = 1;
        *scene_material_graph_resource_count = 2;
        *scene_effect_graph_count = 1;
        unsupported_scene_features.push("we-effect-runtime".to_owned());

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).unwrap();

        assert!(snapshot.full_scene.scene_shader_material_graph_detected);
        assert!(!snapshot.full_scene.scene_shader_material_graph_ready);
        assert_eq!(snapshot.full_scene.scene_effect_graph_count, 1);
        assert!(
            snapshot
                .full_scene
                .pending_boundaries
                .contains(&"shader-material-graph")
        );
    }

    #[test]
    fn scene_runtime_snapshot_reports_unsupported_layers() {
        let mut color = scene_test_layer("background", SceneNodeKind::Color);
        color.color = Some("#010203".to_owned());
        let image = scene_test_layer("missing-image", SceneNodeKind::Image);
        let mut text = scene_test_layer("missing-text-color", SceneNodeKind::Text);
        text.text = Some("Needs paint".to_owned());
        let mut path = scene_test_layer("missing-path-paint", SceneNodeKind::Path);
        path.path_data = Some("M0,0 L1,1".to_owned());
        let group = scene_test_layer("group", SceneNodeKind::Group);
        let item = scene_test_item(
            vec![color, image, text, path, group],
            Some(SceneDisplayPlan::Color {
                color: "#010203".to_owned(),
            }),
        );

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert!(!snapshot.native_draw_ready);
        assert!(snapshot.runtime_display_available);
        assert!(!snapshot.draw_pass_plan_ready);
        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(snapshot.draw_pass_backend_status, "fast-clear-color-ready");
        assert_eq!(snapshot.draw_pass_blocking_reason, None);
        assert_eq!(snapshot.draw_pass_color_op_count, 1);
        assert_eq!(snapshot.draw_op_count, 1);
        assert_eq!(snapshot.draw_ops[0].kind, "color-quad");
        assert_eq!(snapshot.unsupported_layer_count, 4);
        assert_eq!(
            snapshot
                .unsupported_layers
                .iter()
                .map(|layer| layer.reason)
                .collect::<Vec<_>>(),
            vec![
                "image-layer-missing-source",
                "text-layer-missing-color",
                "path-layer-missing-paint",
                "group-layer-needs-flattened-children"
            ]
        );
    }

    #[test]
    fn scene_runtime_snapshot_reports_fast_clear_draw_pass() {
        let mut color = scene_test_layer("background", SceneNodeKind::Color);
        color.color = Some("#203040".to_owned());
        let item = scene_test_item(vec![color], None);

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert!(snapshot.native_draw_ready);
        assert!(snapshot.draw_pass_plan_ready);
        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.scene_resource_model,
            "fast-clear-only-no-scene-resources"
        );
        assert_eq!(snapshot.scene_solid_quad_draw_count, 0);
        assert_eq!(snapshot.scene_sampled_image_resource_count, 0);
        assert!(!snapshot.scene_sampled_image_descriptor_heap_required);
        assert_eq!(snapshot.draw_pass_backend_status, "fast-clear-color-ready");
        assert_eq!(snapshot.draw_pass_blocking_reason, None);
        assert_eq!(snapshot.draw_pass_recordable_op_count, 1);
        assert_eq!(snapshot.draw_pass_recordable_quads.len(), 1);
        assert_eq!(snapshot.draw_pass_recordable_quads[0].kind, "color-quad");
        assert_eq!(
            snapshot.draw_pass_recordable_quads[0].rgba,
            [32.0 / 255.0, 48.0 / 255.0, 64.0 / 255.0, 1.0]
        );
        assert_eq!(snapshot.draw_pass_color_op_count, 1);
        assert_eq!(snapshot.draw_pass_sampled_image_op_count, 0);
        assert_eq!(
            snapshot.draw_pass_fast_clear_color.as_deref(),
            Some("#203040")
        );
        assert!(!snapshot.vulkanalia_draw_pass.backend_ready);
        assert_eq!(
            snapshot.vulkanalia_draw_pass.backend_status,
            "delegated-to-vulkanalia-clear-present"
        );
        assert_eq!(
            snapshot.vulkanalia_draw_pass.command_order,
            vec!["delegate_to_vulkanalia_clear_present"]
        );
    }

    #[test]
    fn scene_runtime_snapshot_reports_recordable_rectangle_quad() {
        let mut rectangle = scene_test_layer("panel", SceneNodeKind::Rectangle);
        rectangle.color = Some("#336699".to_owned());
        rectangle.opacity = 0.75;
        rectangle.width = Some(320.0);
        rectangle.height = Some(180.0);
        rectangle.transform.y = 12.0;
        let item = scene_test_item(vec![rectangle], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert!(snapshot.native_draw_ready);
        assert!(snapshot.draw_pass_plan_ready);
        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.scene_resource_model,
            "retained-solid-quad-geometry"
        );
        assert_eq!(snapshot.scene_solid_quad_draw_count, 1);
        assert_eq!(snapshot.scene_sampled_image_resource_count, 0);
        assert!(!snapshot.scene_sampled_image_descriptor_heap_required);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "solid-quad-recording-ready"
        );
        assert_eq!(snapshot.draw_pass_blocking_reason, None);
        assert_eq!(snapshot.draw_pass_recordable_op_count, 1);
        assert_eq!(snapshot.draw_pass_recordable_quads.len(), 1);
        assert!(snapshot.draw_pass_quad_recording_ready);
        assert_eq!(snapshot.draw_pass_quad_recording_step_count, 1);
        assert_eq!(snapshot.draw_pass_quad_vertex_buffer_bytes, 96);
        assert_eq!(snapshot.draw_pass_quad_index_buffer_bytes, 24);
        assert_eq!(
            snapshot.draw_pass_quad_recording_steps[0].blend.mode,
            SceneBlendMode::Alpha
        );
        assert_eq!(snapshot.draw_pass_quad_recording_steps[0].vertex_count, 4);
        assert_eq!(snapshot.draw_pass_quad_recording_steps[0].index_count, 6);
        assert_eq!(snapshot.draw_pass_quad_indices, vec![0, 1, 2, 2, 1, 3]);
        assert_eq!(snapshot.draw_pass_quad_vertices.len(), 4);
        assert_eq!(
            snapshot.draw_pass_quad_vertices[0].position,
            [-160.0, -78.0]
        );
        assert_eq!(snapshot.draw_pass_quad_vertices[3].position, [160.0, 102.0]);
        let vulkanalia_geometry = snapshot
            .take_vulkanalia_solid_quad_geometry_input()
            .expect("recordable solid quad geometry");
        assert_eq!(vulkanalia_geometry.source_label, "scene-runtime-draw-plan");
        assert_eq!(vulkanalia_geometry.vertices.len(), 4);
        assert_eq!(vulkanalia_geometry.indices, vec![0, 1, 2, 2, 1, 3]);
        assert_eq!(vulkanalia_geometry.vertices[0].position, [-160.0, -78.0]);
        assert_eq!(vulkanalia_geometry.vertices[3].position, [160.0, 102.0]);
        let quad = &snapshot.draw_pass_recordable_quads[0];
        assert_eq!(quad.layer_id, "panel");
        assert_eq!(quad.kind, "rectangle");
        assert_eq!(quad.color, "#336699");
        assert_eq!(
            quad.rgba,
            [51.0 / 255.0, 102.0 / 255.0, 153.0 / 255.0, 0.75]
        );
        assert_eq!(quad.width, Some(320.0));
        assert_eq!(quad.height, Some(180.0));
        assert_eq!(quad.transform.y, 12.0);
        assert!(snapshot.vulkanalia_draw_pass.backend_ready);
        assert_eq!(
            snapshot.vulkanalia_draw_pass.backend_status,
            "solid-quad-dynamic-rendering-recording-ready"
        );
        assert_eq!(snapshot.vulkanalia_draw_pass.vertex_buffer_bytes, 96);
        assert_eq!(snapshot.vulkanalia_draw_pass.index_buffer_bytes, 24);
        assert_eq!(snapshot.vulkanalia_draw_pass.draw_indexed_count, 1);
        assert!(snapshot.vulkanalia_draw_pass.uses_dynamic_rendering);
        assert!(
            snapshot
                .vulkanalia_draw_pass
                .command_order
                .contains(&"cmd_begin_rendering")
        );
    }

    #[test]
    fn scene_runtime_snapshot_tracks_native_particle_layers() {
        let mut first = scene_test_layer("sparks::particle-0", SceneNodeKind::Rectangle);
        first.color = Some("#ffaa00".to_owned());
        first.width = Some(8.0);
        first.height = Some(8.0);
        let mut second = scene_test_layer("sparks::particle-1", SceneNodeKind::Rectangle);
        second.color = Some("#ffaa00".to_owned());
        second.width = Some(8.0);
        second.height = Some(8.0);
        second.transform.x = 12.0;
        let mut item = scene_test_item(vec![first, second], None);
        if let NativeVulkanRenderItem::Scene { scene_systems, .. } = &mut item {
            scene_systems.particles = SceneSystemStatus::Ready;
        }

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert!(snapshot.native_draw_ready);
        assert_eq!(snapshot.full_scene.particle_runtime_layer_count, 2);
        assert!(snapshot.full_scene.scene_particle_system_detected);
        assert!(snapshot.full_scene.scene_particle_system_ready);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"native-particle-system-runtime")
        );
        assert!(
            !snapshot
                .full_scene
                .pending_boundaries
                .contains(&"particle-systems")
        );
        assert_eq!(snapshot.draw_pass_quad_recording_step_count, 2);
    }

    #[test]
    fn scene_runtime_snapshot_reports_sampled_image_quad_payload() {
        let mut image = scene_test_layer("hero", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/scene-hero.png"));
        image.fit = FitMode::Contain;
        image.opacity = 0.5;
        image.color = Some("#000000".to_owned());
        image.width = Some(200.0);
        image.height = Some(100.0);
        image.transform.x = 10.0;
        let item = scene_test_item(vec![image], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert!(snapshot.native_draw_ready);
        assert!(snapshot.draw_pass_plan_ready);
        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.scene_resource_model,
            "retained-sampled-images-descriptor-heap"
        );
        assert_eq!(snapshot.scene_solid_quad_draw_count, 0);
        assert_eq!(snapshot.scene_sampled_image_resource_count, 1);
        assert!(snapshot.scene_sampled_image_descriptor_heap_required);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "sampled-image-recording-ready"
        );
        assert_eq!(snapshot.draw_pass_blocking_reason, None);
        assert_eq!(snapshot.draw_pass_sampled_image_op_count, 1);
        assert_eq!(snapshot.draw_pass_sampled_image_quads.len(), 1);
        assert!(snapshot.draw_pass_sampled_image_recording_ready);
        assert_eq!(snapshot.draw_pass_sampled_image_recording_step_count, 1);
        assert_eq!(snapshot.draw_pass_sampled_image_vertex_buffer_bytes, 176);
        assert_eq!(snapshot.draw_pass_sampled_image_index_buffer_bytes, 24);
        assert_eq!(
            snapshot.draw_pass_sampled_image_indices,
            vec![0, 1, 2, 2, 1, 3]
        );
        assert_eq!(
            snapshot.draw_pass_sampled_image_quads[0].source,
            PathBuf::from("/tmp/scene-hero.png")
        );
        assert_eq!(
            snapshot.draw_pass_sampled_image_quads[0].fit,
            FitMode::Contain
        );
        assert_eq!(snapshot.draw_pass_sampled_image_quads[0].opacity, 0.5);
        assert_eq!(
            snapshot.draw_pass_sampled_image_recording_steps[0]
                .material_pass
                .render_state
                .blend
                .mode,
            SceneBlendMode::Alpha
        );
        assert_eq!(
            snapshot.draw_pass_sampled_image_vertices[0].position,
            [-90.0, -50.0]
        );
        assert_eq!(
            snapshot.draw_pass_sampled_image_vertices[3].position,
            [110.0, 50.0]
        );
        assert_eq!(snapshot.draw_pass_sampled_image_vertices[0].uv, [0.0, 1.0]);
        assert_eq!(snapshot.draw_pass_sampled_image_vertices[3].uv, [1.0, 0.0]);
        assert_eq!(
            snapshot.draw_pass_sampled_image_vertices[0].tint,
            [0.0, 0.0, 0.0, 1.0]
        );
        let (source, sampled_geometry) = snapshot
            .take_vulkanalia_sampled_image_geometry_input()
            .expect("recordable sampled image geometry");
        assert_eq!(source, PathBuf::from("/tmp/scene-hero.png"));
        assert_eq!(
            sampled_geometry.source_label,
            "scene-runtime-sampled-image-draw-plan"
        );
        assert_eq!(sampled_geometry.vertices.len(), 4);
        assert_eq!(sampled_geometry.indices, vec![0, 1, 2, 2, 1, 3]);
        assert_eq!(sampled_geometry.vertices[0].position, [-90.0, -50.0]);
        assert_eq!(sampled_geometry.vertices[3].uv, [1.0, 0.0]);
        assert_eq!(sampled_geometry.vertices[0].tint, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(sampled_geometry.vertices[0].opacity, 0.5);
        assert!(snapshot.vulkanalia_draw_pass.backend_ready);
        assert_eq!(
            snapshot.vulkanalia_draw_pass.backend_status,
            "sampled-image-dynamic-rendering-recording-ready"
        );
        assert_eq!(snapshot.vulkanalia_draw_pass.blocking_reason, None);
        assert_eq!(snapshot.vulkanalia_draw_pass.descriptor_set_count, 0);
        assert_eq!(snapshot.vulkanalia_draw_pass.vertex_stride_bytes, 44);
        assert_eq!(snapshot.vulkanalia_draw_pass.draw_indexed_count, 1);
        assert!(snapshot.vulkanalia_sampled_image.backend_ready);
        assert_eq!(
            snapshot.vulkanalia_sampled_image.backend_status,
            "sampled-image-dynamic-rendering-recording-ready"
        );
        assert_eq!(snapshot.vulkanalia_sampled_image.blocking_reason, None);
        assert_eq!(
            snapshot.vulkanalia_sampled_image.sampled_image_sources,
            vec![PathBuf::from("/tmp/scene-hero.png")]
        );
        assert_eq!(snapshot.vulkanalia_sampled_image.descriptor_set_count, 0);
        assert_eq!(
            snapshot.vulkanalia_sampled_image.descriptor_type,
            "combined-image-sampler"
        );
        assert_eq!(snapshot.vulkanalia_sampled_image.vertex_buffer_bytes, 176);
        assert_eq!(snapshot.vulkanalia_sampled_image.index_buffer_bytes, 24);
        assert!(
            snapshot
                .vulkanalia_sampled_image
                .command_order
                .contains(&"cmd_copy_buffer_to_image2_chunks")
        );
        assert!(
            snapshot
                .vulkanalia_sampled_image
                .command_order
                .contains(&"cmd_bind_scene_descriptor_heap")
        );
    }

    #[test]
    fn scene_runtime_snapshot_reports_we_graph_resources_as_first_class() {
        let mut opacity = scene_test_layer("eye", SceneNodeKind::Image);
        opacity.source = Some(PathBuf::from("/tmp/eye.gtex"));
        opacity.blend_mode = SceneBlendMode::Normal;
        opacity.width = Some(663.0);
        opacity.height = Some(230.0);
        opacity.texture_slots = vec![
            SceneRenderTextureSlot {
                slot: 0,
                source: PathBuf::from("/tmp/eye.gtex"),
                width: Some(663),
                height: Some(230),
            },
            SceneRenderTextureSlot {
                slot: 1,
                source: PathBuf::from("/tmp/opacity-mask.gtex"),
                width: Some(331),
                height: Some(115),
            },
        ];
        opacity.alpha_texture_slot = Some(1);
        opacity.alpha_texture_mode = SceneRenderAlphaTextureMode::Multiply;
        opacity.image_effect_passes = vec![SceneRenderImageEffectPass {
            effect_file: "effects/opacity/effect.json".to_owned(),
            runtime: Some("wallpaper-engine-effect".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/opacity".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            texture_slots: opacity.texture_slots.clone(),
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];

        let mut water = scene_test_layer("water-carrier", SceneNodeKind::Image);
        water.source = Some(PathBuf::from("/tmp/water-source.gtex"));
        water.blend_mode = SceneBlendMode::Modulate;
        water.width = Some(3450.0);
        water.height = Some(3000.0);
        water.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/water-source.gtex"),
            width: Some(3450),
            height: Some(3000),
        }];
        water.image_effect_passes = vec![SceneRenderImageEffectPass {
            effect_file: "effects/waterripple/effect.json".to_owned(),
            runtime: Some("native-effect-motion".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("effects/waterripple".to_owned()),
            blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            texture_slots: vec![SceneRenderTextureSlot {
                slot: 2,
                source: PathBuf::from("/tmp/waterripplenormal.gtex"),
                width: Some(512),
                height: Some(512),
            }],
            effect_uv_transform: None,
            combos: Default::default(),
            constant_shader_values: Default::default(),
        }];

        let item = scene_test_item(vec![opacity, water], None);

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert_eq!(snapshot.draw_pass_sampled_image_we_graph_chain_count, 2);
        assert_eq!(snapshot.draw_pass_sampled_image_we_graph_step_count, 5);
        assert_eq!(snapshot.draw_pass_sampled_image_we_graph_target_count, 3);
        assert_eq!(
            snapshot.draw_pass_sampled_image_we_graph_texture_resource_count,
            4
        );
        assert_eq!(
            snapshot.draw_pass_sampled_image_we_graph_target_resource_count,
            3
        );
        assert_eq!(snapshot.draw_pass_sampled_image_we_graph_resource_count, 7);
        assert_eq!(
            snapshot
                .draw_pass_sampled_image_we_graph_resources
                .iter()
                .map(|resource| resource.resource_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4, 5, 6]
        );

        let opacity_target = snapshot
            .draw_pass_sampled_image_we_graph_targets
            .iter()
            .find(|target| target.layer_id == "eye")
            .expect("opacity graph target");
        assert_eq!(opacity_target.endpoint, "first-class-effect-target");
        assert_eq!(opacity_target.planned_graph_resource_index, 4);
        assert_eq!(opacity_target.vulkan_effect_target_index, Some(0));
        assert_eq!(opacity_target.allocation, "allocated-vulkan-effect-target");

        let opacity_target_resource = snapshot
            .draw_pass_sampled_image_we_graph_resources
            .iter()
            .find(|resource| resource.resource_index == opacity_target.planned_graph_resource_index)
            .expect("opacity target resource");
        assert_eq!(opacity_target_resource.resource_kind, "graph-target");
        assert_eq!(opacity_target_resource.layer_id.as_deref(), Some("eye"));
        assert_eq!(opacity_target_resource.chain_index, Some(0));
        assert_eq!(opacity_target_resource.target_index, Some(0));
        assert_eq!(
            opacity_target_resource.endpoint,
            Some("first-class-effect-target")
        );
        assert_eq!(
            opacity_target_resource.allocation,
            "allocated-vulkan-effect-target"
        );
        assert_eq!(opacity_target_resource.vulkan_effect_target_index, Some(0));

        let opacity_final_input = snapshot
            .draw_pass_sampled_image_we_graph_steps
            .iter()
            .find(|step| step.layer_id == "eye" && step.step_index == 1)
            .and_then(|step| {
                step.texture_bindings
                    .iter()
                    .find(|binding| binding.slot == 0)
            })
            .expect("opacity final input binding");
        assert_eq!(opacity_final_input.source, "previous-graph-target");
        assert_eq!(opacity_final_input.planned_graph_resource_index, Some(4));
        assert_eq!(opacity_final_input.vulkan_effect_target_index, Some(0));

        let water_targets = snapshot
            .draw_pass_sampled_image_we_graph_targets
            .iter()
            .filter(|target| target.layer_id == "water-carrier")
            .collect::<Vec<_>>();
        assert_eq!(water_targets.len(), 2);
        assert_eq!(water_targets[0].endpoint, "image-local-main");
        assert_eq!(water_targets[0].planned_graph_resource_index, 5);
        assert_eq!(water_targets[0].vulkan_effect_target_index, None);
        assert_eq!(water_targets[0].allocation, "planned-until-graph-executor");
        assert_eq!(water_targets[1].endpoint, "image-local-sub");
        assert_eq!(water_targets[1].planned_graph_resource_index, 6);
        assert_eq!(water_targets[1].vulkan_effect_target_index, None);
        assert_eq!(water_targets[1].allocation, "planned-until-graph-executor");

        let water_target_resources = snapshot
            .draw_pass_sampled_image_we_graph_resources
            .iter()
            .filter(|resource| resource.layer_id.as_deref() == Some("water-carrier"))
            .collect::<Vec<_>>();
        assert_eq!(water_target_resources.len(), 2);
        assert_eq!(water_target_resources[0].resource_index, 5);
        assert_eq!(
            water_target_resources[0].execution,
            Some("suppressed-until-graph-executor")
        );
        assert_eq!(
            water_target_resources[0].allocation,
            "planned-until-graph-executor"
        );
        assert_eq!(water_target_resources[1].resource_index, 6);
        assert_eq!(
            water_target_resources[1].execution,
            Some("suppressed-until-graph-executor")
        );
        assert_eq!(
            water_target_resources[1].allocation,
            "planned-until-graph-executor"
        );

        let water_ripple_input = snapshot
            .draw_pass_sampled_image_we_graph_steps
            .iter()
            .find(|step| step.layer_id == "water-carrier" && step.step_index == 1)
            .and_then(|step| {
                step.texture_bindings
                    .iter()
                    .find(|binding| binding.slot == 0)
            })
            .expect("water ripple input binding");
        assert_eq!(water_ripple_input.source, "previous-graph-target");
        assert_eq!(water_ripple_input.planned_graph_resource_index, Some(5));
        assert_eq!(water_ripple_input.vulkan_effect_target_index, None);

        let water_normal = snapshot
            .draw_pass_sampled_image_we_graph_steps
            .iter()
            .find(|step| step.layer_id == "water-carrier" && step.step_index == 1)
            .and_then(|step| {
                step.texture_bindings
                    .iter()
                    .find(|binding| binding.slot == 2)
            })
            .expect("water normal-map binding");
        assert_eq!(water_normal.source, "pass-texture-slot");
        assert_eq!(
            water_normal.source_path.as_deref(),
            Some(Path::new("/tmp/waterripplenormal.gtex"))
        );
        assert_eq!(water_normal.planned_graph_resource_index, Some(3));
        let water_normal_resource = snapshot
            .draw_pass_sampled_image_we_graph_resources
            .iter()
            .find(|resource| resource.resource_index == 3)
            .expect("water normal-map texture resource");
        assert_eq!(water_normal_resource.resource_kind, "texture-source");
        assert_eq!(
            water_normal_resource.source_path.as_deref(),
            Some(Path::new("/tmp/waterripplenormal.gtex"))
        );
        assert_eq!(water_normal_resource.width, Some(512));
        assert_eq!(water_normal_resource.height, Some(512));

        let mut vulkan_snapshot = snapshot.clone();
        let (_source, vulkan_geometry) = vulkan_snapshot
            .take_vulkanalia_sampled_image_geometry_input()
            .expect("vulkanalia sampled image geometry");
        assert_eq!(vulkan_geometry.we_graph_resources.len(), 7);
        assert_eq!(
            vulkan_geometry
                .we_graph_resources
                .iter()
                .filter(|resource| resource.resource_kind == "texture-source")
                .count(),
            4
        );
        assert_eq!(
            vulkan_geometry
                .we_graph_resources
                .iter()
                .filter(|resource| resource.resource_kind == "graph-target")
                .count(),
            3
        );
        assert_eq!(
            vulkan_geometry.we_graph_resources[4].allocation,
            "allocated-vulkan-effect-target"
        );
        assert_eq!(
            vulkan_geometry.we_graph_resources[5].allocation,
            "planned-until-graph-executor"
        );
    }

    #[test]
    fn scene_runtime_snapshot_exports_mixed_quad_and_sampled_image_geometry() {
        let mut background = scene_test_layer("background", SceneNodeKind::Rectangle);
        background.color = Some("#102030".to_owned());
        background.opacity = 0.8;
        background.width = Some(800.0);
        background.height = Some(450.0);
        let mut image = scene_test_layer("hero", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/scene-hero.png"));
        image.fit = FitMode::Cover;
        image.opacity = 0.5;
        image.width = Some(320.0);
        image.height = Some(180.0);
        let item = scene_test_item(vec![background, image], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");
        let solid_geometry = snapshot
            .take_vulkanalia_mixed_solid_quad_geometry_input()
            .expect("mixed solid quad geometry");
        let (source, sampled_geometry) = snapshot
            .take_vulkanalia_sampled_image_geometry_input()
            .expect("mixed sampled image geometry");

        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "mixed-quad-sampled-image-recording-ready"
        );
        assert_eq!(
            snapshot.scene_resource_model,
            "retained-solid-quad-geometry-and-sampled-images-descriptor-heap"
        );
        assert_eq!(snapshot.scene_solid_quad_draw_count, 1);
        assert_eq!(snapshot.scene_sampled_image_resource_count, 1);
        assert!(snapshot.scene_sampled_image_descriptor_heap_required);
        assert!(snapshot.vulkanalia_draw_pass.backend_ready);
        assert_eq!(
            snapshot.vulkanalia_draw_pass.backend_status,
            "mixed-quad-sampled-image-dynamic-rendering-recording-ready"
        );
        assert_eq!(
            snapshot.vulkanalia_draw_pass.pipeline_labels,
            vec![
                "scene-solid-quad-alpha-blend",
                "scene-solid-quad-normal-blend",
                "scene-solid-quad-additive-blend",
                "scene-solid-quad-multiply-blend",
                "scene-solid-quad-screen-blend",
                "scene-solid-quad-max-blend",
                "scene-solid-quad-modulate-blend",
                "scene-sampled-image-alpha-blend",
                "scene-sampled-image-normal-blend",
                "scene-sampled-image-additive-blend",
                "scene-sampled-image-multiply-blend",
                "scene-sampled-image-screen-blend",
                "scene-sampled-image-max-blend",
                "scene-sampled-image-modulate-blend"
            ]
        );
        assert_eq!(snapshot.vulkanalia_draw_pass.draw_indexed_count, 2);
        assert_eq!(solid_geometry.vertices.len(), 4);
        assert_eq!(solid_geometry.indices, vec![0, 1, 2, 2, 1, 3]);
        assert_eq!(
            solid_geometry.source_label,
            "scene-runtime-mixed-solid-quad-draw-plan"
        );
        assert_eq!(source, PathBuf::from("/tmp/scene-hero.png"));
        assert_eq!(
            sampled_geometry.sources,
            vec![PathBuf::from("/tmp/scene-hero.png")]
        );
        assert_eq!(sampled_geometry.draw_steps.len(), 1);
        assert_eq!(
            sampled_geometry.draw_steps[0].texture_slot_bindings,
            vulkanalia_texture_slot_bindings(&[0])
        );
        assert!(
            snapshot
                .vulkanalia_draw_pass
                .command_order
                .contains(&"cmd_bind_scene_solid_quad_pipeline_as_needed")
        );
        assert!(
            snapshot
                .vulkanalia_draw_pass
                .command_order
                .contains(&"cmd_bind_scene_sampled_image_pipeline_as_needed")
        );
        assert!(
            snapshot
                .vulkanalia_draw_pass
                .command_order
                .contains(&"cmd_draw_indexed_in_scene_layer_order")
        );
    }

    #[test]
    fn scene_runtime_snapshot_uses_clear_background_for_mixed_image_scene() {
        let mut background = scene_test_layer("background", SceneNodeKind::Color);
        background.color = Some("#102030".to_owned());
        let mut image = scene_test_layer("hero", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/scene-hero.png"));
        image.fit = FitMode::Cover;
        image.opacity = 0.75;
        image.width = Some(320.0);
        image.height = Some(180.0);
        let item = scene_test_item(vec![background, image], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");
        let (source, sampled_geometry) = snapshot
            .take_vulkanalia_sampled_image_geometry_input()
            .expect("clear-background sampled image geometry");

        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "clear-background-sampled-image-recording-ready"
        );
        assert_eq!(snapshot.draw_pass_clear_background_op_count, 1);
        assert_eq!(
            snapshot.draw_pass_background_clear_color.as_deref(),
            Some("#102030")
        );
        assert_eq!(snapshot.full_scene.clear_background_layer_count, 1);
        assert_eq!(snapshot.full_scene.native_runtime_layer_count, 2);
        assert_eq!(snapshot.full_scene.native_runtime_pending_layer_count, 0);
        assert_eq!(snapshot.full_scene.native_runtime_coverage_percent, 100);
        assert_eq!(
            snapshot.vulkanalia_draw_pass.backend_status,
            "clear-background-sampled-image-dynamic-rendering-recording-ready"
        );
        assert_eq!(snapshot.vulkanalia_draw_pass.clear_background_op_count, 1);
        assert_eq!(source, PathBuf::from("/tmp/scene-hero.png"));
        assert_eq!(sampled_geometry.draw_steps.len(), 1);
    }

    #[test]
    fn scene_runtime_snapshot_counts_simple_path_tessellation_coverage() {
        let mut path = scene_test_layer("triangle", SceneNodeKind::Path);
        path.path_data = Some("M0,0 L64,0 L32,48 Z".to_owned());
        path.color = Some("#cc8844".to_owned());
        let item = scene_test_item(vec![path], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");
        let solid_geometry = snapshot
            .take_vulkanalia_solid_quad_geometry_input()
            .expect("path solid geometry");

        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "solid-quad-recording-ready"
        );
        assert_eq!(snapshot.draw_pass_path_op_count, 1);
        assert!(!snapshot.draw_pass_requires_path_tessellation);
        assert_eq!(snapshot.full_scene.tessellated_path_layer_count, 1);
        assert_eq!(snapshot.full_scene.curve_path_layer_count, 0);
        assert_eq!(snapshot.full_scene.arc_path_layer_count, 0);
        assert_eq!(snapshot.full_scene.compound_path_layer_count, 0);
        assert_eq!(snapshot.full_scene.native_runtime_coverage_percent, 100);
        assert!(!snapshot.full_scene.scene_path_tessellation_required);
        assert!(snapshot.full_scene.scene_path_tessellation_ready);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"simple-path-tessellation-runtime")
        );
        assert_eq!(solid_geometry.draw_steps.len(), 1);
        assert_eq!(solid_geometry.indices.len(), 3);
    }

    #[test]
    fn scene_runtime_snapshot_counts_curve_path_tessellation_coverage() {
        let mut path = scene_test_layer("wave", SceneNodeKind::Path);
        path.path_data = Some("M0 0 C25 80 75 -80 100 0 S175 80 200 0 L200 80 L0 80 Z".to_owned());
        path.color = Some("#cc8844".to_owned());
        let item = scene_test_item(vec![path], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");
        let solid_geometry = snapshot
            .take_vulkanalia_solid_quad_geometry_input()
            .expect("curve path solid geometry");

        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "solid-quad-recording-ready"
        );
        assert_eq!(snapshot.draw_pass_path_op_count, 1);
        assert!(!snapshot.draw_pass_requires_path_tessellation);
        assert_eq!(snapshot.full_scene.tessellated_path_layer_count, 1);
        assert_eq!(snapshot.full_scene.curve_path_layer_count, 1);
        assert_eq!(snapshot.full_scene.arc_path_layer_count, 0);
        assert_eq!(snapshot.full_scene.compound_path_layer_count, 0);
        assert_eq!(snapshot.full_scene.native_runtime_coverage_percent, 100);
        assert!(!snapshot.full_scene.scene_path_tessellation_required);
        assert!(snapshot.full_scene.scene_path_tessellation_ready);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"simple-path-tessellation-runtime")
        );
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"curve-path-flattening-runtime")
        );
        assert_eq!(solid_geometry.draw_steps.len(), 1);
        assert!(solid_geometry.indices.len() > 6);
    }

    #[test]
    fn scene_runtime_snapshot_counts_arc_path_tessellation_coverage() {
        let mut path = scene_test_layer("orbit", SceneNodeKind::Path);
        path.path_data = Some("M100 50 A50 50 0 1 1 0 50 A50 50 0 1 1 100 50 Z".to_owned());
        path.color = Some("#22aa88".to_owned());
        let item = scene_test_item(vec![path], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");
        let solid_geometry = snapshot
            .take_vulkanalia_solid_quad_geometry_input()
            .expect("arc path solid geometry");

        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "solid-quad-recording-ready"
        );
        assert_eq!(snapshot.draw_pass_path_op_count, 1);
        assert!(!snapshot.draw_pass_requires_path_tessellation);
        assert_eq!(snapshot.full_scene.tessellated_path_layer_count, 1);
        assert_eq!(snapshot.full_scene.curve_path_layer_count, 0);
        assert_eq!(snapshot.full_scene.arc_path_layer_count, 1);
        assert_eq!(snapshot.full_scene.compound_path_layer_count, 0);
        assert_eq!(snapshot.full_scene.native_runtime_coverage_percent, 100);
        assert!(!snapshot.full_scene.scene_path_tessellation_required);
        assert!(snapshot.full_scene.scene_path_tessellation_ready);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"arc-path-flattening-runtime")
        );
        assert_eq!(solid_geometry.draw_steps.len(), 1);
        assert!(solid_geometry.indices.len() > 6);
    }

    #[test]
    fn scene_runtime_snapshot_counts_compound_path_fill_coverage() {
        let mut path = scene_test_layer("compound", SceneNodeKind::Path);
        path.path_data =
            Some("M0 0 L100 0 L100 100 L0 100 Z M25 25 L75 25 L75 75 L25 75 Z".to_owned());
        path.path_fill_rule = ScenePathFillRule::Evenodd;
        path.color = Some("#22aa88".to_owned());
        let item = scene_test_item(vec![path], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");
        let solid_geometry = snapshot
            .take_vulkanalia_solid_quad_geometry_input()
            .expect("compound path solid geometry");

        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "solid-quad-recording-ready"
        );
        assert_eq!(snapshot.draw_pass_path_op_count, 1);
        assert!(!snapshot.draw_pass_requires_path_tessellation);
        assert_eq!(snapshot.full_scene.tessellated_path_layer_count, 1);
        assert_eq!(snapshot.full_scene.curve_path_layer_count, 0);
        assert_eq!(snapshot.full_scene.arc_path_layer_count, 0);
        assert_eq!(snapshot.full_scene.compound_path_layer_count, 1);
        assert_eq!(snapshot.full_scene.native_runtime_coverage_percent, 100);
        assert!(!snapshot.full_scene.scene_path_tessellation_required);
        assert!(snapshot.full_scene.scene_path_tessellation_ready);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"compound-path-evenodd-fill-runtime")
        );
        assert_eq!(solid_geometry.draw_steps.len(), 1);
        assert_eq!(solid_geometry.vertices.len(), 16);
        assert_eq!(solid_geometry.indices.len(), 24);
    }

    #[test]
    fn scene_runtime_snapshot_counts_nonzero_path_fill_coverage() {
        let mut path = scene_test_layer("compound-nonzero", SceneNodeKind::Path);
        path.path_data =
            Some("M0 0 L100 0 L100 100 L0 100 Z M25 25 L75 25 L75 75 L25 75 Z".to_owned());
        path.path_fill_rule = ScenePathFillRule::Nonzero;
        path.color = Some("#22aa88".to_owned());
        let item = scene_test_item(vec![path], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");
        let solid_geometry = snapshot
            .take_vulkanalia_solid_quad_geometry_input()
            .expect("compound nonzero path solid geometry");

        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(snapshot.full_scene.compound_path_layer_count, 1);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"compound-path-nonzero-fill-runtime")
        );
        assert!(
            !snapshot
                .full_scene
                .completed_boundaries
                .contains(&"compound-path-evenodd-fill-runtime")
        );
        assert_eq!(solid_geometry.draw_steps.len(), 1);
        assert_eq!(solid_geometry.vertices.len(), 12);
        assert_eq!(solid_geometry.indices.len(), 18);
    }

    #[test]
    fn scene_runtime_snapshot_counts_stroke_geometry_boundary() {
        let mut path = scene_test_layer("outline", SceneNodeKind::Path);
        path.path_data = Some("M0,0 L64,0 L32,48 Z".to_owned());
        path.stroke_color = Some("#f8fafc".to_owned());
        path.stroke_width = Some(4.0);
        let item = scene_test_item(vec![path], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert!(snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "solid-quad-recording-ready"
        );
        assert_eq!(snapshot.draw_pass_recordable_quads[0].fill_rgba, None);
        assert_eq!(
            snapshot.draw_pass_recordable_quads[0]
                .stroke_color
                .as_deref(),
            Some("#f8fafc")
        );
        assert_eq!(
            snapshot.draw_pass_recordable_quads[0].stroke_rgba,
            Some([248.0 / 255.0, 250.0 / 255.0, 252.0 / 255.0, 1.0])
        );
        assert_eq!(snapshot.draw_pass_quad_recording_steps.len(), 1);
        assert!(!snapshot.draw_pass_quad_recording_steps[0].fill_geometry);
        assert!(snapshot.draw_pass_quad_recording_steps[0].stroke_geometry);
        assert_eq!(snapshot.full_scene.stroke_geometry_layer_count, 1);
        assert_eq!(snapshot.full_scene.tessellated_path_layer_count, 1);
        assert_eq!(snapshot.full_scene.native_runtime_coverage_percent, 100);
        assert!(
            snapshot
                .full_scene
                .completed_boundaries
                .contains(&"stroke-geometry-runtime")
        );
        let solid_geometry = snapshot
            .take_vulkanalia_solid_quad_geometry_input()
            .expect("stroke path solid geometry");
        assert_eq!(solid_geometry.draw_steps.len(), 1);
        assert_eq!(solid_geometry.indices.len(), 18);
    }

    #[test]
    fn scene_runtime_snapshot_builds_batched_sampled_image_geometry() {
        let mut background = scene_test_layer("background", SceneNodeKind::Image);
        background.source = Some(PathBuf::from("/tmp/scene-background.png"));
        background.fit = FitMode::Cover;
        background.width = Some(800.0);
        background.height = Some(450.0);
        let mut overlay = scene_test_layer("overlay", SceneNodeKind::Image);
        overlay.source = Some(PathBuf::from("/tmp/scene-overlay.png"));
        overlay.fit = FitMode::Tile;
        overlay.opacity = 0.75;
        overlay.width = Some(320.0);
        overlay.height = Some(180.0);
        overlay.transform.x = 64.0;
        let item = scene_test_item(vec![background, overlay], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");
        let (source, sampled_geometry) = snapshot
            .take_vulkanalia_sampled_image_geometry_input()
            .expect("batched sampled image geometry");

        assert_eq!(source, PathBuf::from("/tmp/scene-background.png"));
        assert_eq!(
            sampled_geometry.sources,
            vec![
                PathBuf::from("/tmp/scene-background.png"),
                PathBuf::from("/tmp/scene-overlay.png")
            ]
        );
        assert_eq!(sampled_geometry.draw_steps.len(), 2);
        assert_eq!(
            sampled_geometry.draw_steps[0].texture_slot_bindings,
            vulkanalia_texture_slot_bindings(&[0])
        );
        assert_eq!(sampled_geometry.draw_steps[0].first_index, 0);
        assert_eq!(sampled_geometry.draw_steps[0].index_count, 6);
        assert_eq!(sampled_geometry.draw_steps[0].fit, Some(FitMode::Cover));
        assert_eq!(
            sampled_geometry.draw_steps[1].texture_slot_bindings,
            vulkanalia_texture_slot_bindings(&[1])
        );
        assert_eq!(sampled_geometry.draw_steps[1].first_index, 6);
        assert_eq!(sampled_geometry.draw_steps[1].index_count, 6);
        assert_eq!(sampled_geometry.draw_steps[1].fit, Some(FitMode::Tile));
        assert_eq!(sampled_geometry.vertices.len(), 8);
        assert_eq!(
            sampled_geometry.indices,
            vec![0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7]
        );
        assert_eq!(snapshot.vulkanalia_draw_pass.sampled_image_quad_count, 2);
        assert_eq!(snapshot.vulkanalia_draw_pass.draw_indexed_count, 2);
        assert_eq!(
            snapshot.scene_resource_model,
            "retained-sampled-images-descriptor-heap"
        );
        assert_eq!(snapshot.scene_solid_quad_draw_count, 0);
        assert_eq!(snapshot.scene_sampled_image_resource_count, 2);
        assert!(snapshot.scene_sampled_image_descriptor_heap_required);
        assert_eq!(snapshot.vulkanalia_sampled_image.sampled_image_count, 2);
        assert_eq!(snapshot.vulkanalia_sampled_image.draw_indexed_count, 2);
    }

    #[test]
    fn scene_runtime_snapshot_deduplicates_sampled_image_sources() {
        let mut first = scene_test_layer("first", SceneNodeKind::Image);
        first.source = Some(PathBuf::from("/tmp/particle-spark.gtex"));
        first.width = Some(16.0);
        first.height = Some(16.0);
        let mut second = scene_test_layer("second", SceneNodeKind::Image);
        second.source = Some(PathBuf::from("/tmp/particle-spark.gtex"));
        second.width = Some(16.0);
        second.height = Some(16.0);
        second.transform.x = 24.0;
        let item = scene_test_item(vec![first, second], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");
        let (_, sampled_geometry) = snapshot
            .take_vulkanalia_sampled_image_geometry_input()
            .expect("sampled image geometry");

        assert_eq!(
            sampled_geometry.sources,
            vec![PathBuf::from("/tmp/particle-spark.gtex")]
        );
        assert_eq!(sampled_geometry.draw_steps.len(), 2);
        assert_eq!(
            sampled_geometry.draw_steps[0].texture_slot_bindings,
            vulkanalia_texture_slot_bindings(&[0])
        );
        assert_eq!(
            sampled_geometry.draw_steps[1].texture_slot_bindings,
            vulkanalia_texture_slot_bindings(&[0])
        );
        assert_eq!(snapshot.scene_sampled_image_resource_count, 1);
        assert_eq!(snapshot.vulkanalia_sampled_image.sampled_image_count, 1);
    }

    #[test]
    fn scene_runtime_snapshot_preserves_sparse_sampled_image_texture_slots() {
        let composite_key = Some(SceneLayerCompositeKey {
            parent_source_id: Some("parent-puppet".to_owned()),
            puppet_attachment: "eye".to_owned(),
            original_path: "models/eye.json".to_owned(),
            base_source: PackagePath::new("assets/eye-base.gtex").unwrap(),
        });
        let mut eye = scene_test_layer("eye", SceneNodeKind::Image);
        eye.source = Some(PathBuf::from("/tmp/eye-base.gtex"));
        eye.composite_key = composite_key.clone();
        eye.texture_slots = vec![SceneRenderTextureSlot {
            slot: 3,
            source: PathBuf::from("/tmp/eye-opacity-mask.gtex"),
            width: Some(16),
            height: Some(16),
        }];
        eye.alpha_texture_slot = Some(3);
        eye.width = Some(64.0);
        eye.height = Some(32.0);
        let item = scene_test_item(vec![eye], None);

        let mut snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert_eq!(
            snapshot.draw_pass_sampled_image_sources,
            vec![
                PathBuf::from("/tmp/eye-base.gtex"),
                PathBuf::from("/tmp/eye-opacity-mask.gtex")
            ]
        );
        assert_eq!(snapshot.scene_sampled_image_resource_count, 2);
        assert_eq!(
            snapshot.draw_pass_sampled_image_quads[0]
                .material_pass
                .alpha_texture_slot,
            Some(3)
        );
        assert_eq!(
            snapshot.draw_pass_sampled_image_quads[0].composite_key,
            composite_key
        );
        assert_eq!(
            snapshot.draw_pass_sampled_image_recording_steps[0].texture_slot_bindings,
            texture_slot_binding_snapshots(&[0, 0, 0, 1])
        );
        assert_eq!(
            snapshot.draw_pass_sampled_image_recording_steps[0]
                .material_pass
                .alpha_texture_slot,
            Some(3)
        );
        assert_eq!(
            snapshot.draw_pass_sampled_image_recording_steps[0].composite_key,
            composite_key
        );
        assert_eq!(snapshot.draw_ops[0].composite_key, composite_key);

        let (_, sampled_geometry) = snapshot
            .take_vulkanalia_sampled_image_geometry_input()
            .expect("sampled image geometry");
        assert_eq!(
            sampled_geometry.sources,
            vec![
                PathBuf::from("/tmp/eye-base.gtex"),
                PathBuf::from("/tmp/eye-opacity-mask.gtex")
            ]
        );
        assert_eq!(
            sampled_geometry.draw_steps[0].texture_slot_bindings,
            vulkanalia_texture_slot_bindings(&[0, 0, 0, 1])
        );
        assert_eq!(
            sampled_geometry.draw_steps[0].material.alpha_texture_slot,
            Some(3)
        );
    }

    #[test]
    fn dynamic_sampled_vertices_keep_opacity_layer_and_use_material_uv() {
        let base_source = PackagePath::new("assets/eye-base.gtex").unwrap();
        let mask_source = PackagePath::new("assets/eye-mask.gtex").unwrap();
        let composite_key = Some(SceneLayerCompositeKey {
            parent_source_id: Some("parent-puppet".to_owned()),
            puppet_attachment: "eye".to_owned(),
            original_path: "models/eye.json".to_owned(),
            base_source: base_source.clone(),
        });
        let base = SceneSnapshotSampledImageLayer {
            id: "base-eye".to_owned(),
            has_source: true,
            texture_slots: vec![SceneTextureSlot {
                slot: 0,
                source: base_source.clone(),
                width: Some(100),
                height: Some(100),
            }],
            alpha_texture_slot: None,
            alpha_texture_mode: Default::default(),
            image_effect_passes: Vec::new(),
            composite_key: composite_key.clone(),
            texture_region: None,
            width: Some(100.0),
            height: Some(100.0),
            mesh: None,
            effect_motion: Default::default(),
            blend_mode: SceneBlendMode::Alpha,
            tint: [1.0, 1.0, 1.0, 1.0],
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneTransform::default(),
            puppet_animation_frames: Vec::new(),
        };
        let mut carrier = base.clone();
        carrier.texture_slots = vec![
            SceneTextureSlot {
                slot: 0,
                source: base_source,
                width: Some(100),
                height: Some(100),
            },
            SceneTextureSlot {
                slot: 1,
                source: mask_source,
                width: Some(50),
                height: Some(50),
            },
        ];
        carrier.alpha_texture_slot = Some(1);
        carrier.opacity = 0.5;
        carrier.transform.x = 10.0;

        let geometry =
            native_vulkan_scene_sampled_vertex_input_from_sampled_layers(&[base, carrier])
                .expect("dynamic sampled vertices with independent alpha layer");

        assert_eq!(
            geometry.source_label,
            "scene-runtime-direct-sampled-image-retained-topology-vertices"
        );
        assert_eq!(geometry.vertices.len(), 8);
        assert!(geometry.indices.is_empty());
        assert!(geometry.draw_steps.is_empty());
        assert!((geometry.vertices[0].opacity - 1.0).abs() <= f32::EPSILON);
        assert!((geometry.vertices[0].position[0] - -50.0).abs() < 0.0001);
        assert!((geometry.vertices[4].opacity - 0.5).abs() <= f32::EPSILON);
        assert!((geometry.vertices[4].position[0] - -40.0).abs() < 0.0001);
        assert!((geometry.vertices[5].effect_uv[0] - 1.0).abs() < 0.0001);
        assert!((geometry.vertices[5].effect_uv[1] - 1.0).abs() < 0.0001);
    }

    #[test]
    fn dynamic_sampled_vertices_keep_opacity_mask_duplicate_independent() {
        let base_source = PackagePath::new("assets/eye-base.gtex").unwrap();
        let mask_source = PackagePath::new("assets/eye-mask.gtex").unwrap();
        let composite_key = Some(SceneLayerCompositeKey {
            parent_source_id: Some("parent-puppet".to_owned()),
            puppet_attachment: "eye".to_owned(),
            original_path: "models/eye.json".to_owned(),
            base_source: base_source.clone(),
        });
        let base = SceneSnapshotSampledImageLayer {
            id: "base-eye".to_owned(),
            has_source: true,
            texture_slots: vec![SceneTextureSlot {
                slot: 0,
                source: base_source.clone(),
                width: Some(100),
                height: Some(100),
            }],
            alpha_texture_slot: None,
            alpha_texture_mode: Default::default(),
            image_effect_passes: Vec::new(),
            composite_key: composite_key.clone(),
            texture_region: None,
            width: Some(100.0),
            height: Some(100.0),
            mesh: None,
            effect_motion: Default::default(),
            blend_mode: SceneBlendMode::Alpha,
            tint: [1.0, 1.0, 1.0, 1.0],
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneTransform::default(),
            puppet_animation_frames: Vec::new(),
        };
        let mut carrier = base.clone();
        carrier.texture_slots = vec![
            SceneTextureSlot {
                slot: 0,
                source: base_source,
                width: Some(100),
                height: Some(100),
            },
            SceneTextureSlot {
                slot: 1,
                source: mask_source,
                width: Some(50),
                height: Some(50),
            },
        ];
        carrier.alpha_texture_slot = Some(1);
        carrier.alpha_texture_mode = crate::core::SceneAlphaTextureMode::Multiply;

        let geometry =
            native_vulkan_scene_sampled_vertex_input_from_sampled_layers(&[base, carrier])
                .expect("dynamic sampled vertices with independent alpha carrier");

        assert_eq!(geometry.vertices.len(), 8);
        assert!(geometry.indices.is_empty());
        assert!(geometry.draw_steps.is_empty());
        assert!((geometry.vertices[0].opacity - 1.0).abs() <= f32::EPSILON);
        assert!((geometry.vertices[4].opacity - 1.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn dynamic_sampled_vertices_map_opacity_layer_effect_uv_to_material_uv() {
        let base_source = PackagePath::new("assets/eye-base.gtex").unwrap();
        let mask_source = PackagePath::new("assets/eye-mask.gtex").unwrap();
        let composite_key = Some(SceneLayerCompositeKey {
            parent_source_id: Some("parent-puppet".to_owned()),
            puppet_attachment: "eye".to_owned(),
            original_path: "models/eye.json".to_owned(),
            base_source: base_source.clone(),
        });
        let mesh = Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: -10.0,
                    y: -20.0,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 10.0,
                    y: -20.0,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: -10.0,
                    y: 20.0,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2],
            skin: None,
            puppet_clips: Vec::new(),
        });
        let base = SceneSnapshotSampledImageLayer {
            id: "base-eye".to_owned(),
            has_source: true,
            texture_slots: vec![SceneTextureSlot {
                slot: 0,
                source: base_source.clone(),
                width: Some(100),
                height: Some(100),
            }],
            alpha_texture_slot: None,
            alpha_texture_mode: Default::default(),
            image_effect_passes: Vec::new(),
            composite_key: composite_key.clone(),
            texture_region: None,
            width: Some(100.0),
            height: Some(100.0),
            mesh: Some(mesh.clone()),
            effect_motion: Default::default(),
            blend_mode: SceneBlendMode::Alpha,
            tint: [1.0, 1.0, 1.0, 1.0],
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneTransform::default(),
            puppet_animation_frames: Vec::new(),
        };
        let mut carrier = base.clone();
        carrier.texture_slots = vec![
            SceneTextureSlot {
                slot: 0,
                source: base_source,
                width: Some(100),
                height: Some(100),
            },
            SceneTextureSlot {
                slot: 1,
                source: mask_source,
                width: Some(50),
                height: Some(50),
            },
        ];
        carrier.alpha_texture_slot = Some(1);
        carrier.opacity = 0.5;

        let geometry =
            native_vulkan_scene_sampled_vertex_input_from_sampled_layers(&[base, carrier])
                .expect("dynamic sampled vertices with independent alpha layer");

        assert_eq!(geometry.vertices.len(), 6);
        assert_eq!(geometry.vertices[3].effect_uv, [0.0, 0.0]);
        assert_eq!(geometry.vertices[4].effect_uv, [1.0, 0.0]);
        assert_eq!(geometry.vertices[5].effect_uv, [0.0, 1.0]);
    }

    #[test]
    fn dynamic_sampled_vertices_route_opacity_effect_through_first_class_target() {
        let base_source = PackagePath::new("assets/eye-base.gtex").unwrap();
        let mask_source = PackagePath::new("assets/eye-mask.gtex").unwrap();
        let mesh = Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: -10.0,
                    y: -20.0,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 10.0,
                    y: -20.0,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: -10.0,
                    y: 20.0,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2],
            skin: None,
            puppet_clips: Vec::new(),
        });
        let carrier = SceneSnapshotSampledImageLayer {
            id: "opacity-eye".to_owned(),
            has_source: true,
            texture_slots: vec![
                SceneTextureSlot {
                    slot: 0,
                    source: base_source.clone(),
                    width: Some(100),
                    height: Some(100),
                },
                SceneTextureSlot {
                    slot: 1,
                    source: mask_source.clone(),
                    width: Some(50),
                    height: Some(50),
                },
            ],
            alpha_texture_slot: Some(1),
            alpha_texture_mode: crate::core::SceneAlphaTextureMode::Multiply,
            image_effect_passes: vec![crate::core::scene::SceneImageEffectPass {
                effect_file: "effects/opacity/effect.json".to_owned(),
                runtime: Some("native-opacity-mask".to_owned()),
                pass_index: 0,
                command: None,
                source: None,
                target: None,
                binds: Default::default(),
                fbos: Default::default(),
                shader: Some("effects/opacity".to_owned()),
                blending: Some("normal".to_owned()),
                depthtest: None,
                depthwrite: None,
                cullmode: None,
                texture_slots: vec![SceneTextureSlot {
                    slot: 1,
                    source: mask_source,
                    width: Some(50),
                    height: Some(50),
                }],
                effect_uv_transform: None,
                combos: Default::default(),
                constant_shader_values: Default::default(),
            }],
            composite_key: None,
            texture_region: None,
            width: Some(100.0),
            height: Some(100.0),
            mesh: Some(mesh),
            effect_motion: Default::default(),
            blend_mode: SceneBlendMode::Alpha,
            tint: [1.0, 1.0, 1.0, 1.0],
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneTransform::default(),
            puppet_animation_frames: Vec::new(),
        };

        let geometry = native_vulkan_scene_sampled_vertex_input_from_sampled_layers(&[carrier])
            .expect("dynamic opacity effect target vertices");

        assert_eq!(geometry.vertices.len(), 7);
        assert!(geometry.indices.is_empty());
        assert!(geometry.draw_steps.is_empty());
        assert_eq!(geometry.vertices[0].position, [40.0, 70.0]);
        assert_eq!(geometry.vertices[0].uv, [0.0, 1.0]);
        assert_eq!(geometry.vertices[2].uv, [0.0, 0.0]);
        assert_eq!(geometry.vertices[3].uv, [0.0, 1.0]);
        assert_eq!(geometry.vertices[6].uv, [1.0, 0.0]);
        assert_eq!(geometry.vertices[3].effect_uv, [0.0, 1.0]);
        assert_eq!(geometry.vertices[6].effect_uv, [1.0, 0.0]);
    }

    #[test]
    fn dynamic_sampled_vertices_route_iris_effect_through_first_class_target() {
        let base_source = PackagePath::new("assets/eye-base.gtex").unwrap();
        let mask_source = PackagePath::new("assets/iris-mask.gtex").unwrap();
        let mesh = Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: -10.0,
                    y: -20.0,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 10.0,
                    y: -20.0,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: -10.0,
                    y: 20.0,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2],
            skin: None,
            puppet_clips: Vec::new(),
        });
        let layer = SceneSnapshotSampledImageLayer {
            id: "iris-eye".to_owned(),
            has_source: true,
            texture_slots: vec![SceneTextureSlot {
                slot: 0,
                source: base_source,
                width: Some(100),
                height: Some(100),
            }],
            alpha_texture_slot: None,
            alpha_texture_mode: Default::default(),
            image_effect_passes: vec![crate::core::scene::SceneImageEffectPass {
                effect_file: "effects/iris/effect.json".to_owned(),
                runtime: Some("native-iris-mask".to_owned()),
                pass_index: 0,
                command: None,
                source: None,
                target: None,
                binds: Default::default(),
                fbos: Default::default(),
                shader: Some("effects/iris".to_owned()),
                blending: Some("normal".to_owned()),
                depthtest: None,
                depthwrite: None,
                cullmode: None,
                texture_slots: vec![SceneTextureSlot {
                    slot: 1,
                    source: mask_source,
                    width: Some(50),
                    height: Some(50),
                }],
                effect_uv_transform: None,
                combos: Default::default(),
                constant_shader_values: Default::default(),
            }],
            composite_key: None,
            texture_region: None,
            width: Some(100.0),
            height: Some(100.0),
            mesh: Some(mesh),
            effect_motion: Default::default(),
            blend_mode: SceneBlendMode::Alpha,
            tint: [1.0, 1.0, 1.0, 1.0],
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneTransform::default(),
            puppet_animation_frames: Vec::new(),
        };

        let geometry = native_vulkan_scene_sampled_vertex_input_from_sampled_layers(&[layer])
            .expect("dynamic iris effect target vertices");

        assert_eq!(geometry.vertices.len(), 7);
        assert!(geometry.indices.is_empty());
        assert!(geometry.draw_steps.is_empty());
        assert_eq!(geometry.vertices[0].position, [40.0, 70.0]);
        assert_eq!(geometry.vertices[0].uv, [0.0, 1.0]);
        assert_eq!(geometry.vertices[2].uv, [0.0, 0.0]);
        assert_eq!(geometry.vertices[3].uv, [0.0, 1.0]);
        assert_eq!(geometry.vertices[6].uv, [1.0, 0.0]);
        assert_eq!(geometry.vertices[3].effect_uv, [0.0, 1.0]);
        assert_eq!(geometry.vertices[6].effect_uv, [1.0, 0.0]);
    }

    #[test]
    fn dynamic_sampled_geometry_builds_directly_from_render_layers() {
        let mut panel = scene_test_layer("panel", SceneNodeKind::Rectangle);
        panel.color = Some("#102030".to_owned());
        panel.width = Some(320.0);
        panel.height = Some(180.0);
        let mut atlas = scene_test_layer("atlas", SceneNodeKind::Image);
        atlas.source = Some(PathBuf::from("/tmp/atlas.gtex"));
        atlas.fit = FitMode::Tile;
        atlas.width = Some(128.0);
        atlas.height = Some(64.0);
        atlas.texture_region = Some(SceneTextureRegion {
            u_min: 0.0,
            v_min: 0.0,
            u_max: 0.25,
            v_max: 0.5,
            frame_index: 0,
            frame_count: 8,
            columns: 4,
            rows: 2,
            fps: Some(12.0),
            loop_playback: true,
        });

        let geometry = native_vulkan_scene_sampled_geometry_inputs_from_layers(
            120,
            None,
            FitMode::Cover,
            &[panel, atlas],
        )
        .expect("direct dynamic sampled geometry");

        let solid_geometry = geometry
            .solid_geometry
            .expect("mixed solid geometry is retained");
        assert_eq!(
            solid_geometry.source_label,
            "scene-runtime-direct-mixed-solid-quad-draw-plan"
        );
        assert_eq!(solid_geometry.vertices.len(), 4);
        assert_eq!(solid_geometry.indices, vec![0, 1, 2, 2, 1, 3]);
        assert_eq!(
            geometry.sampled_geometry.source_label,
            "scene-runtime-direct-sampled-image-draw-plan"
        );
        assert_eq!(
            geometry.sampled_geometry.sources,
            vec![PathBuf::from("/tmp/atlas.gtex")]
        );
        assert_eq!(geometry.sampled_geometry.draw_steps.len(), 1);
        assert_eq!(geometry.sampled_geometry.draw_steps[0].layer_index, 1);
        assert_eq!(
            geometry.sampled_geometry.draw_steps[0].texture_slot_bindings,
            vulkanalia_texture_slot_bindings(&[0])
        );
        assert_eq!(
            geometry.sampled_geometry.draw_steps[0].fit,
            Some(FitMode::Tile)
        );
        assert_eq!(
            geometry.sampled_geometry.draw_steps[0]
                .texture_region
                .expect("texture region")
                .frame_count,
            8
        );
        assert_eq!(geometry.sampled_geometry.vertices.len(), 4);
        assert_eq!(geometry.sampled_geometry.indices, vec![0, 1, 2, 2, 1, 3]);
    }

    #[test]
    fn dynamic_sampled_geometry_builds_directly_from_snapshot_layers() {
        let mut sprite = scene_test_snapshot_layer("sprite::particle-0", SceneNodeKind::Image);
        sprite.source = Some(PackagePath::new("assets/spark.gtex").unwrap());
        sprite.width = Some(32.0);
        sprite.height = Some(16.0);
        sprite.transform.x = 24.0;
        let mut indices = BTreeMap::new();
        indices.insert("assets/spark.gtex".to_owned(), 7);

        let geometry =
            native_vulkan_scene_sampled_geometry_input_from_snapshot_layers_with_package_source_indices(
                &[sprite],
                &indices,
            )
            .expect("snapshot sampled geometry");

        assert_eq!(
            geometry.source_label,
            "scene-runtime-direct-snapshot-sampled-image-draw-plan"
        );
        assert!(geometry.sources.is_empty());
        assert_eq!(geometry.draw_steps.len(), 1);
        assert_eq!(
            geometry.draw_steps[0].texture_slot_bindings,
            vulkanalia_texture_slot_bindings(&[7])
        );
        assert_eq!(geometry.draw_steps[0].first_index, 0);
        assert_eq!(geometry.draw_steps[0].index_count, 6);
        assert_eq!(geometry.vertices.len(), 4);
        assert_eq!(geometry.indices, vec![0, 1, 2, 2, 1, 3]);
    }

    #[test]
    fn dynamic_scene_geometry_skips_audio_cue_layers() {
        let mut panel = scene_test_layer("panel", SceneNodeKind::Rectangle);
        panel.color = Some("#102030".to_owned());
        panel.width = Some(320.0);
        panel.height = Some(180.0);
        let audio = scene_test_layer("music", SceneNodeKind::Audio);

        let solid_geometry = native_vulkan_scene_solid_quad_geometry_input_from_layers(
            0,
            None,
            FitMode::Cover,
            &[panel.clone(), audio.clone()],
        )
        .expect("audio cue layers do not block direct solid geometry");

        assert_eq!(solid_geometry.draw_steps.len(), 1);
        assert_eq!(solid_geometry.draw_steps[0].layer_index, 0);

        let mut image = scene_test_layer("sprite", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/sprite.gtex"));
        image.width = Some(128.0);
        image.height = Some(64.0);
        let sampled_geometry = native_vulkan_scene_sampled_geometry_inputs_from_layers(
            0,
            None,
            FitMode::Cover,
            &[audio, panel, image],
        )
        .expect("audio cue layers do not block direct sampled geometry");

        assert_eq!(
            sampled_geometry
                .solid_geometry
                .expect("solid overlay")
                .draw_steps[0]
                .layer_index,
            1
        );
        assert_eq!(sampled_geometry.sampled_geometry.draw_steps.len(), 1);
        assert_eq!(
            sampled_geometry.sampled_geometry.draw_steps[0].layer_index,
            2
        );
    }

    #[test]
    fn scene_runtime_snapshot_skips_non_visual_metadata_layers() {
        let empty_parent = scene_test_layer("empty-parent", SceneNodeKind::Rectangle);
        let controller = scene_test_layer("native-controller", SceneNodeKind::Script);
        let audio = scene_test_layer("audio-cues", SceneNodeKind::Audio);
        let mut panel = scene_test_layer("panel", SceneNodeKind::Rectangle);
        panel.color = Some("#102030".to_owned());
        panel.width = Some(320.0);
        panel.height = Some(180.0);
        let item = scene_test_item(vec![empty_parent, controller, audio, panel], None);

        let snapshot = native_vulkan_scene_runtime_snapshot(&item).expect("scene snapshot");

        assert!(snapshot.native_draw_ready);
        assert_eq!(snapshot.unsupported_layer_count, 0);
        assert_eq!(snapshot.draw_op_count, 1);
        assert_eq!(snapshot.draw_ops[0].layer_id, "panel");
        assert_eq!(snapshot.draw_pass_quad_recording_step_count, 1);
    }
}
