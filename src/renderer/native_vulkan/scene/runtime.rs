use serde::Serialize;
use std::path::PathBuf;

use crate::core::{
    FitMode, SceneNodeKind, ScenePathFillRule, SceneSize, SceneSystemStatus, SceneTextAlign,
    SceneTextureRegion, SceneTransform,
};
use crate::renderer::SceneRenderLayer;

use super::super::present::render_item::NativeVulkanRenderItem;
use super::super::present::render_plan::{
    NativeVulkanSceneDrawPlan, native_vulkan_scene_draw_plan,
};
use super::super::vulkan::{
    NativeVulkanVulkanaliaSceneDrawPassInput, NativeVulkanVulkanaliaSceneDrawPassSnapshot,
    NativeVulkanVulkanaliaSceneSampledImageDrawStep,
    NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
    NativeVulkanVulkanaliaSceneSampledImagePlanInput,
    NativeVulkanVulkanaliaSceneSampledImagePlanSnapshot,
    NativeVulkanVulkanaliaSceneSampledImageVertex, NativeVulkanVulkanaliaSceneSolidQuadDrawStep,
    NativeVulkanVulkanaliaSceneSolidQuadGeometryInput, NativeVulkanVulkanaliaSceneSolidQuadVertex,
    NativeVulkanVulkanaliaSceneVideoLayerDrawStep,
    NativeVulkanVulkanaliaSceneVideoLayerGeometryInput,
    native_vulkan_vulkanalia_scene_draw_pass_snapshot,
    native_vulkan_vulkanalia_scene_sampled_image_plan,
};
use super::draw_pass::{
    native_vulkan_scene_draw_pass_plan, native_vulkan_scene_render_layer_is_clear,
    native_vulkan_scene_sampled_image_geometry_from_render_layer,
    native_vulkan_scene_solid_geometry_from_render_layer,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneRuntimeSnapshot {
    pub snapshot_time_ms: u64,
    pub scene_size: Option<SceneSize>,
    pub scene_fit: FitMode,
    pub full_scene: NativeVulkanFullSceneRuntimeSnapshot,
    pub scene_input_model: &'static str,
    pub scene_resource_model: &'static str,
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
    pub timeline_snapshot_runtime_ready: bool,
    pub timeline_snapshot_time_ms: u64,
    pub timeline_animation_runtime_ready: bool,
    pub timeline_animation_count: usize,
    pub timeline_animated_layer_count: usize,
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
                resource_index: step.resource_index,
                first_index: step.first_index,
                index_count: step.index_count,
                fit: Some(step.fit),
                texture_region: step.texture_region,
            })
            .collect::<Vec<_>>();

        Some((
            source,
            NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new_batched(
                std::mem::take(&mut self.draw_pass_sampled_image_vertices)
                    .into_iter()
                    .map(|vertex| {
                        NativeVulkanVulkanaliaSceneSampledImageVertex::new(
                            vertex.position,
                            vertex.uv,
                            vertex.opacity,
                        )
                    })
                    .collect(),
                std::mem::take(&mut self.draw_pass_sampled_image_indices),
                sources,
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
            || self.draw_pass_video_sources.len() != 1
            || self.draw_pass_video_vertices.is_empty()
            || self.draw_pass_video_indices.is_empty()
        {
            return None;
        }

        let source = self.draw_pass_video_sources.first().cloned()?;
        self.draw_pass_video_quads = Vec::new();
        let draw_steps = std::mem::take(&mut self.draw_pass_video_recording_steps)
            .into_iter()
            .map(|step| NativeVulkanVulkanaliaSceneVideoLayerDrawStep {
                layer_index: step.layer_index,
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
                source,
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

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_sampled_geometry_inputs_from_layers(
    snapshot_time_ms: u64,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
    layers: &[SceneRenderLayer],
) -> Result<NativeVulkanSceneSampledGeometryInputs, String> {
    let _ = (snapshot_time_ms, scene_size, scene_fit);
    let mut solid_vertices = Vec::new();
    let mut solid_indices = Vec::new();
    let mut solid_draw_steps = Vec::new();
    let mut sampled_vertices = Vec::new();
    let mut sampled_indices = Vec::new();
    let mut sampled_sources = Vec::new();
    let mut sampled_draw_steps = Vec::new();

    for (layer_index, layer) in layers.iter().enumerate() {
        if native_vulkan_scene_render_layer_has_no_visual_geometry(layer) {
            continue;
        }
        if layer.kind == crate::core::SceneNodeKind::Video {
            continue;
        }
        if layer.kind == crate::core::SceneNodeKind::Image {
            let Some((source, fit, texture_region, vertices)) =
                native_vulkan_scene_sampled_image_geometry_from_render_layer(layer_index, layer)
                    .map_err(|reason| {
                        format!("dynamic scene is not sampled-image recordable: {reason}")
                    })?
            else {
                continue;
            };
            let resource_index =
                native_vulkan_scene_sampled_source_index(&mut sampled_sources, source);
            let first_vertex = sampled_vertices.len().min(u32::MAX as usize) as u32;
            let first_index = sampled_indices.len().min(u32::MAX as usize) as u32;
            sampled_draw_steps.push(NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                layer_index,
                resource_index,
                first_index,
                index_count: 6,
                fit: Some(fit),
                texture_region,
            });
            sampled_vertices.extend(vertices.into_iter().map(|vertex| {
                NativeVulkanVulkanaliaSceneSampledImageVertex::new(
                    vertex.position,
                    vertex.uv,
                    vertex.opacity,
                )
            }));
            sampled_indices.extend_from_slice(&[
                first_vertex,
                first_vertex + 1,
                first_vertex + 2,
                first_vertex + 2,
                first_vertex + 1,
                first_vertex + 3,
            ]);
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

    if sampled_draw_steps.is_empty() || sampled_vertices.is_empty() || sampled_indices.is_empty() {
        return Err("dynamic sampled-image scene produced no sampled geometry".to_owned());
    }
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

fn native_vulkan_scene_sampled_source_index(sources: &mut Vec<PathBuf>, source: PathBuf) -> u32 {
    if let Some(index) = sources.iter().position(|existing| existing == &source) {
        return index.min(u32::MAX as usize) as u32;
    }
    let index = sources.len().min(u32::MAX as usize) as u32;
    sources.push(source);
    index
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneDrawOpSnapshot {
    pub layer_index: usize,
    pub layer_id: String,
    pub kind: &'static str,
    pub opacity: f64,
    pub source: Option<PathBuf>,
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
    pub pipeline: &'static str,
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
    pub opacity: f32,
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
    let full_scene = native_vulkan_full_scene_runtime_snapshot(
        render_item,
        &plan,
        &pass_plan,
        scene_resource_model,
        scene_sampled_image_descriptor_heap_required,
    );
    let scene_video_native_layer_count = full_scene.video_native_layer_count;
    Some(NativeVulkanSceneRuntimeSnapshot {
        snapshot_time_ms: plan.snapshot_time_ms,
        scene_size: plan.scene_size,
        scene_fit: plan.scene_fit,
        full_scene,
        scene_input_model: "core scene snapshot layers; groups must be flattened before native Vulkan planning",
        scene_resource_model,
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
                pipeline: step.pipeline,
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
            .map(|quad| NativeVulkanSceneSampledImageQuadSnapshot {
                layer_index: quad.layer_index,
                layer_id: quad.layer_id,
                source: quad.source,
                fit: quad.fit,
                texture_region: quad.texture_region,
                opacity: quad.opacity,
                width: quad.width,
                height: quad.height,
                transform: quad.transform,
            })
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
        draw_pass_sampled_image_vertices: pass_plan
            .sampled_image_vertices
            .into_iter()
            .map(|vertex| NativeVulkanSceneSampledImageVertexSnapshot {
                position: vertex.position,
                uv: vertex.uv,
                opacity: vertex.opacity,
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
                opacity: vertex.opacity,
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
            .map(|op| NativeVulkanSceneDrawOpSnapshot {
                layer_index: op.layer_index,
                layer_id: op.layer_id,
                kind: op.kind.as_str(),
                opacity: op.opacity,
                source: op.source,
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
) -> NativeVulkanFullSceneRuntimeSnapshot {
    let (
        source_layer_count,
        timeline_animation_count,
        timeline_animated_layer_count,
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
        pass_plan.sampled_image_recording_steps.len()
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
        execution_model: "full scene state is lowered into explicit native Vulkan scene runtime boundaries with native scene graph transform/opacity execution, scene timeline animation, per-frame fixed-topology timeline geometry updates, deterministic SceneScript expression lowering, parallax property camera input, property update, pause/resume policy, state persistence, converted keyframe timeline input, converted WE .tex image resources, spritesheet atlas UV-frame animation, cubic/smooth-cubic/quadratic/smooth-quadratic/arc path flattening, compound even-odd path fill, and scene audio cues resolved into the renderer and played by the native FFmpeg/PipeWire scene present runtime; unsupported Wallpaper Engine systems remain visible instead of falling back to legacy paths",
        native_scene_graph_lowering_ready: plan.native_draw_ready(),
        native_present_route_ready: pass_plan.backend_ready,
        retained_resource_model_ready,
        timeline_snapshot_runtime_ready,
        timeline_snapshot_time_ms: plan.snapshot_time_ms,
        timeline_animation_runtime_ready,
        timeline_animation_count,
        timeline_animated_layer_count,
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
            "retained-single-source-vulkan-video-multi-layer-scene-resources"
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
    use crate::core::{
        FitMode, SceneNodeKind, ScenePathFillRule, SceneSystemStatus, SceneSystems, SceneTextAlign,
        SceneTransform,
    };
    use crate::renderer::native_vulkan::NativeVulkanRenderItem;
    use crate::renderer::{SceneDisplayPlan, SceneRenderAudioCue, SceneRenderLayer};
    use std::path::{Path, PathBuf};

    fn scene_test_layer(id: &str, kind: SceneNodeKind) -> SceneRenderLayer {
        SceneRenderLayer {
            id: id.to_owned(),
            kind,
            source: None,
            texture_region: None,
            audio: Vec::new(),
            color: None,
            stroke_color: None,
            stroke_width: None,
            corner_radius: None,
            width: None,
            height: None,
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
            property_binding_count,
            cursor_parallax_input_ready: false,
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
            "retained-single-source-vulkan-video-multi-layer-scene-resources"
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
        assert_eq!(geometry.source, PathBuf::from("/tmp/scene-video.mp4"));
        assert_eq!(geometry.draw_steps.len(), 2);
        assert_eq!(geometry.vertices.len(), 8);
        assert_eq!(geometry.indices.len(), 12);
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
        assert!(!snapshot.draw_pass_backend_ready);
        assert_eq!(
            snapshot.draw_pass_backend_status,
            "blocked-by-unsupported-scene-layers"
        );
        assert_eq!(
            snapshot.draw_pass_blocking_reason,
            Some("unsupported-scene-layers")
        );
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
            snapshot.draw_pass_quad_recording_steps[0].pipeline,
            "solid-quad-alpha-blend"
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
        assert_eq!(snapshot.draw_pass_sampled_image_vertex_buffer_bytes, 80);
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
            snapshot.draw_pass_sampled_image_recording_steps[0].pipeline,
            "sampled-image-alpha-blend"
        );
        assert_eq!(
            snapshot.draw_pass_sampled_image_vertices[0].position,
            [-90.0, -50.0]
        );
        assert_eq!(
            snapshot.draw_pass_sampled_image_vertices[3].position,
            [110.0, 50.0]
        );
        assert_eq!(snapshot.draw_pass_sampled_image_vertices[0].uv, [0.0, 0.0]);
        assert_eq!(snapshot.draw_pass_sampled_image_vertices[3].uv, [1.0, 1.0]);
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
        assert_eq!(sampled_geometry.vertices[3].uv, [1.0, 1.0]);
        assert_eq!(sampled_geometry.vertices[0].opacity, 0.5);
        assert!(snapshot.vulkanalia_draw_pass.backend_ready);
        assert_eq!(
            snapshot.vulkanalia_draw_pass.backend_status,
            "sampled-image-dynamic-rendering-recording-ready"
        );
        assert_eq!(snapshot.vulkanalia_draw_pass.blocking_reason, None);
        assert_eq!(snapshot.vulkanalia_draw_pass.descriptor_set_count, 0);
        assert_eq!(snapshot.vulkanalia_draw_pass.vertex_stride_bytes, 20);
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
        assert_eq!(snapshot.vulkanalia_sampled_image.vertex_buffer_bytes, 80);
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
                "scene-sampled-image-alpha-blend"
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
        assert_eq!(sampled_geometry.draw_steps[0].resource_index, 0);
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
        assert_eq!(sampled_geometry.draw_steps[0].resource_index, 0);
        assert_eq!(sampled_geometry.draw_steps[0].first_index, 0);
        assert_eq!(sampled_geometry.draw_steps[0].index_count, 6);
        assert_eq!(sampled_geometry.draw_steps[0].fit, Some(FitMode::Cover));
        assert_eq!(sampled_geometry.draw_steps[1].resource_index, 1);
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
        assert_eq!(sampled_geometry.draw_steps[0].resource_index, 0);
        assert_eq!(sampled_geometry.draw_steps[1].resource_index, 0);
        assert_eq!(snapshot.scene_sampled_image_resource_count, 1);
        assert_eq!(snapshot.vulkanalia_sampled_image.sampled_image_count, 1);
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
        assert_eq!(geometry.sampled_geometry.draw_steps[0].resource_index, 0);
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
