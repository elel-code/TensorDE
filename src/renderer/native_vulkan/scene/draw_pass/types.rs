use std::path::PathBuf;
use std::sync::Arc;

use crate::core::scene::{SceneLayerCompositeKey, SceneMesh, SceneNativeEffectMotion};
use crate::core::{
    FitMode, SceneBlendMode, ScenePathFillRule, SceneTextAlign, SceneTextureRegion, SceneTransform,
};
use crate::renderer::SceneRenderAlphaTextureMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneTextureSlot {
    pub(in crate::renderer::native_vulkan::scene) slot: u32,
    pub(in crate::renderer::native_vulkan::scene) source: PathBuf,
    pub(in crate::renderer::native_vulkan::scene) width: Option<u32>,
    pub(in crate::renderer::native_vulkan::scene) height: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneTextureSlotResourceBinding {
    pub(in crate::renderer::native_vulkan::scene) slot: u32,
    pub(in crate::renderer::native_vulkan::scene) resource_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneSampledImageEffectPass {
    pub(in crate::renderer::native_vulkan::scene) texture_slots: Vec<NativeVulkanSceneTextureSlot>,
    pub(in crate::renderer::native_vulkan::scene) alpha_texture_slot: Option<u32>,
    pub(in crate::renderer::native_vulkan::scene) alpha_texture_mode: SceneRenderAlphaTextureMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) enum NativeVulkanSceneEffectKind {
    OpacityMask,
    Iris,
    WaterRipple,
    WaterWaves,
    WaterFlow,
    WaterCaustics,
    Blur,
    SwayShake,
    Flutter,
    Drift,
    CompositeLayer,
    UserBindings,
    ShaderMaterial,
}

impl NativeVulkanSceneEffectKind {
    pub(in crate::renderer::native_vulkan::scene) fn as_str(self) -> &'static str {
        match self {
            Self::OpacityMask => "opacity-mask",
            Self::Iris => "iris",
            Self::WaterRipple => "water-ripple",
            Self::WaterWaves => "water-waves",
            Self::WaterFlow => "water-flow",
            Self::WaterCaustics => "water-caustics",
            Self::Blur => "blur",
            Self::SwayShake => "sway-shake",
            Self::Flutter => "flutter",
            Self::Drift => "drift",
            Self::CompositeLayer => "composite-layer",
            Self::UserBindings => "user-bindings",
            Self::ShaderMaterial => "shader-material",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) enum NativeVulkanSceneMaterialKind {
    SampledImage,
    SampledImageEffectBase,
    SampledImageEffectComposite,
}

impl NativeVulkanSceneMaterialKind {
    pub(in crate::renderer::native_vulkan::scene) fn as_str(self) -> &'static str {
        match self {
            Self::SampledImage => "sampled-image",
            Self::SampledImageEffectBase => "sampled-image-effect-base",
            Self::SampledImageEffectComposite => "sampled-image-effect-composite",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) enum NativeVulkanSceneMaterialFlag {
    Unspecified,
    Enabled,
    Disabled,
}

impl NativeVulkanSceneMaterialFlag {
    pub(in crate::renderer::native_vulkan::scene) fn as_str(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) enum NativeVulkanSceneCullMode {
    Unspecified,
    None,
    Back,
    Front,
    FrontAndBack,
    Named(String),
}

impl NativeVulkanSceneCullMode {
    pub(in crate::renderer::native_vulkan::scene) fn label(&self) -> &str {
        match self {
            Self::Unspecified => "unspecified",
            Self::None => "none",
            Self::Back => "back",
            Self::Front => "front",
            Self::FrontAndBack => "front-and-back",
            Self::Named(value) => value.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneEffectRecord {
    pub(in crate::renderer::native_vulkan::scene) kind: NativeVulkanSceneEffectKind,
    pub(in crate::renderer::native_vulkan::scene) effect_file: String,
    pub(in crate::renderer::native_vulkan::scene) runtime: Option<String>,
    pub(in crate::renderer::native_vulkan::scene) pass_index: usize,
    pub(in crate::renderer::native_vulkan::scene) shader: Option<String>,
    pub(in crate::renderer::native_vulkan::scene) blending: Option<String>,
    pub(in crate::renderer::native_vulkan::scene) texture_slots: Vec<NativeVulkanSceneTextureSlot>,
    pub(in crate::renderer::native_vulkan::scene) parameter_keys: Vec<String>,
    pub(in crate::renderer::native_vulkan::scene) combo_keys: Vec<String>,
    pub(in crate::renderer::native_vulkan::scene) depth_test: NativeVulkanSceneMaterialFlag,
    pub(in crate::renderer::native_vulkan::scene) depth_write: NativeVulkanSceneMaterialFlag,
    pub(in crate::renderer::native_vulkan::scene) cull_mode: NativeVulkanSceneCullMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneMaterialPass {
    pub(in crate::renderer::native_vulkan::scene) kind: NativeVulkanSceneMaterialKind,
    pub(in crate::renderer::native_vulkan::scene) shader: Option<String>,
    pub(in crate::renderer::native_vulkan::scene) blending: Option<String>,
    pub(in crate::renderer::native_vulkan::scene) blend_mode: SceneBlendMode,
    pub(in crate::renderer::native_vulkan::scene) alpha_texture_slot: Option<u32>,
    pub(in crate::renderer::native_vulkan::scene) alpha_texture_mode: SceneRenderAlphaTextureMode,
    pub(in crate::renderer::native_vulkan::scene) depth_test: NativeVulkanSceneMaterialFlag,
    pub(in crate::renderer::native_vulkan::scene) depth_write: NativeVulkanSceneMaterialFlag,
    pub(in crate::renderer::native_vulkan::scene) cull_mode: NativeVulkanSceneCullMode,
    pub(in crate::renderer::native_vulkan::scene) texture_slot_count: usize,
    pub(in crate::renderer::native_vulkan::scene) effect_kinds: Vec<NativeVulkanSceneEffectKind>,
    pub(in crate::renderer::native_vulkan::scene) combo_keys: Vec<String>,
    pub(in crate::renderer::native_vulkan::scene) pipeline: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneRecordableQuad {
    pub(in crate::renderer::native_vulkan::scene) layer_index: usize,
    pub(in crate::renderer::native_vulkan::scene) layer_id: String,
    pub(in crate::renderer::native_vulkan::scene) kind: &'static str,
    pub(in crate::renderer::native_vulkan::scene) color: String,
    pub(in crate::renderer::native_vulkan::scene) rgba: [f32; 4],
    pub(in crate::renderer::native_vulkan::scene) blend_mode: SceneBlendMode,
    pub(in crate::renderer::native_vulkan::scene) fill_color: Option<String>,
    pub(in crate::renderer::native_vulkan::scene) fill_rgba: Option<[f32; 4]>,
    pub(in crate::renderer::native_vulkan::scene) stroke_color: Option<String>,
    pub(in crate::renderer::native_vulkan::scene) stroke_rgba: Option<[f32; 4]>,
    pub(in crate::renderer::native_vulkan::scene) stroke_width: Option<f64>,
    pub(in crate::renderer::native_vulkan::scene) width: Option<f64>,
    pub(in crate::renderer::native_vulkan::scene) height: Option<f64>,
    pub(in crate::renderer::native_vulkan::scene) corner_radius: Option<f64>,
    pub(in crate::renderer::native_vulkan::scene) text: Option<String>,
    pub(in crate::renderer::native_vulkan::scene) font_size: Option<f64>,
    pub(in crate::renderer::native_vulkan::scene) font_family: Option<String>,
    pub(in crate::renderer::native_vulkan::scene) font_source: Option<PathBuf>,
    pub(in crate::renderer::native_vulkan::scene) font_weight: Option<String>,
    pub(in crate::renderer::native_vulkan::scene) text_align: Option<SceneTextAlign>,
    pub(in crate::renderer::native_vulkan::scene) path_data: Option<String>,
    pub(in crate::renderer::native_vulkan::scene) path_fill_rule: ScenePathFillRule,
    pub(in crate::renderer::native_vulkan::scene) transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneQuadRecordingStep {
    pub(in crate::renderer::native_vulkan::scene) layer_index: usize,
    pub(in crate::renderer::native_vulkan::scene) layer_id: String,
    pub(in crate::renderer::native_vulkan::scene) kind: &'static str,
    pub(in crate::renderer::native_vulkan::scene) blend_mode: SceneBlendMode,
    pub(in crate::renderer::native_vulkan::scene) pipeline: &'static str,
    pub(in crate::renderer::native_vulkan::scene) first_vertex: u32,
    pub(in crate::renderer::native_vulkan::scene) vertex_count: u32,
    pub(in crate::renderer::native_vulkan::scene) first_index: u32,
    pub(in crate::renderer::native_vulkan::scene) index_count: u32,
    pub(in crate::renderer::native_vulkan::scene) vertex_buffer_offset_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) vertex_buffer_size_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) index_buffer_offset_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) index_buffer_size_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) fill_geometry: bool,
    pub(in crate::renderer::native_vulkan::scene) stroke_geometry: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneSampledImageQuad {
    pub(in crate::renderer::native_vulkan::scene) layer_index: usize,
    pub(in crate::renderer::native_vulkan::scene) layer_id: String,
    pub(in crate::renderer::native_vulkan::scene) source: PathBuf,
    pub(in crate::renderer::native_vulkan::scene) texture_slots: Vec<NativeVulkanSceneTextureSlot>,
    pub(in crate::renderer::native_vulkan::scene) image_effect_pass_count: usize,
    pub(in crate::renderer::native_vulkan::scene) effect_target_pass:
        Option<NativeVulkanSceneSampledImageEffectPass>,
    pub(in crate::renderer::native_vulkan::scene) material_pass: NativeVulkanSceneMaterialPass,
    pub(in crate::renderer::native_vulkan::scene) effect_passes: Vec<NativeVulkanSceneEffectRecord>,
    pub(in crate::renderer::native_vulkan::scene) composite_key: Option<SceneLayerCompositeKey>,
    pub(in crate::renderer::native_vulkan::scene) fit: FitMode,
    pub(in crate::renderer::native_vulkan::scene) opacity: f64,
    pub(in crate::renderer::native_vulkan::scene) tint: [f32; 4],
    pub(in crate::renderer::native_vulkan::scene) width: f64,
    pub(in crate::renderer::native_vulkan::scene) height: f64,
    pub(in crate::renderer::native_vulkan::scene) mesh: Option<Arc<SceneMesh>>,
    pub(in crate::renderer::native_vulkan::scene) effect_uv_space:
        Option<super::NativeVulkanSceneEffectUvSpace>,
    pub(in crate::renderer::native_vulkan::scene) effect_motion: SceneNativeEffectMotion,
    pub(in crate::renderer::native_vulkan::scene) texture_region: Option<SceneTextureRegion>,
    pub(in crate::renderer::native_vulkan::scene) transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneSampledImageRecordingStep {
    pub(in crate::renderer::native_vulkan::scene) layer_index: usize,
    pub(in crate::renderer::native_vulkan::scene) layer_id: String,
    pub(in crate::renderer::native_vulkan::scene) source: PathBuf,
    pub(in crate::renderer::native_vulkan::scene) fit: FitMode,
    pub(in crate::renderer::native_vulkan::scene) texture_region: Option<SceneTextureRegion>,
    pub(in crate::renderer::native_vulkan::scene) resource_index: u32,
    pub(in crate::renderer::native_vulkan::scene) texture_slot_bindings:
        Vec<NativeVulkanSceneTextureSlotResourceBinding>,
    pub(in crate::renderer::native_vulkan::scene) material_pass: NativeVulkanSceneMaterialPass,
    pub(in crate::renderer::native_vulkan::scene) effect_passes: Vec<NativeVulkanSceneEffectRecord>,
    pub(in crate::renderer::native_vulkan::scene) composite_key: Option<SceneLayerCompositeKey>,
    pub(in crate::renderer::native_vulkan::scene) render_target:
        NativeVulkanSceneSampledImageRenderTarget,
    pub(in crate::renderer::native_vulkan::scene) first_vertex: u32,
    pub(in crate::renderer::native_vulkan::scene) vertex_count: u32,
    pub(in crate::renderer::native_vulkan::scene) first_index: u32,
    pub(in crate::renderer::native_vulkan::scene) index_count: u32,
    pub(in crate::renderer::native_vulkan::scene) vertex_buffer_offset_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) vertex_buffer_size_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) index_buffer_offset_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) index_buffer_size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneSampledImageEffectTarget {
    pub(in crate::renderer::native_vulkan::scene) layer_index: usize,
    pub(in crate::renderer::native_vulkan::scene) width: u32,
    pub(in crate::renderer::native_vulkan::scene) height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) enum NativeVulkanSceneSampledImageRenderTarget {
    Swapchain,
    EffectTarget { target_index: u32, clear: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneVideoQuad {
    pub(in crate::renderer::native_vulkan::scene) layer_index: usize,
    pub(in crate::renderer::native_vulkan::scene) layer_id: String,
    pub(in crate::renderer::native_vulkan::scene) source: PathBuf,
    pub(in crate::renderer::native_vulkan::scene) fit: FitMode,
    pub(in crate::renderer::native_vulkan::scene) opacity: f64,
    pub(in crate::renderer::native_vulkan::scene) width: f64,
    pub(in crate::renderer::native_vulkan::scene) height: f64,
    pub(in crate::renderer::native_vulkan::scene) transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneVideoRecordingStep {
    pub(in crate::renderer::native_vulkan::scene) layer_index: usize,
    pub(in crate::renderer::native_vulkan::scene) layer_id: String,
    pub(in crate::renderer::native_vulkan::scene) source: PathBuf,
    pub(in crate::renderer::native_vulkan::scene) fit: FitMode,
    pub(in crate::renderer::native_vulkan::scene) pipeline: &'static str,
    pub(in crate::renderer::native_vulkan::scene) resource_index: u32,
    pub(in crate::renderer::native_vulkan::scene) first_vertex: u32,
    pub(in crate::renderer::native_vulkan::scene) vertex_count: u32,
    pub(in crate::renderer::native_vulkan::scene) first_index: u32,
    pub(in crate::renderer::native_vulkan::scene) index_count: u32,
    pub(in crate::renderer::native_vulkan::scene) vertex_buffer_offset_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) vertex_buffer_size_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) index_buffer_offset_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) index_buffer_size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneQuadVertex {
    pub(in crate::renderer::native_vulkan::scene) position: [f32; 2],
    pub(in crate::renderer::native_vulkan::scene) rgba: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneSampledImageVertex {
    pub(in crate::renderer::native_vulkan::scene) position: [f32; 2],
    pub(in crate::renderer::native_vulkan::scene) uv: [f32; 2],
    pub(in crate::renderer::native_vulkan::scene) effect_uv: [f32; 2],
    pub(in crate::renderer::native_vulkan::scene) opacity: f32,
    pub(in crate::renderer::native_vulkan::scene) tint: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneSampledImageGeometryRange {
    pub(in crate::renderer::native_vulkan::scene) first_vertex: u32,
    pub(in crate::renderer::native_vulkan::scene) vertex_count: u32,
    pub(in crate::renderer::native_vulkan::scene) first_index: u32,
    pub(in crate::renderer::native_vulkan::scene) index_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneDrawPassPlan {
    pub(in crate::renderer::native_vulkan::scene) plan_ready: bool,
    pub(in crate::renderer::native_vulkan::scene) backend_ready: bool,
    pub(in crate::renderer::native_vulkan::scene) backend_status: &'static str,
    pub(in crate::renderer::native_vulkan::scene) blocking_reason: Option<&'static str>,
    pub(in crate::renderer::native_vulkan::scene) recordable_op_count: usize,
    pub(in crate::renderer::native_vulkan::scene) recordable_quads:
        Vec<NativeVulkanSceneRecordableQuad>,
    pub(in crate::renderer::native_vulkan::scene) quad_recording_ready: bool,
    pub(in crate::renderer::native_vulkan::scene) quad_recording_steps:
        Vec<NativeVulkanSceneQuadRecordingStep>,
    pub(in crate::renderer::native_vulkan::scene) quad_vertices: Vec<NativeVulkanSceneQuadVertex>,
    pub(in crate::renderer::native_vulkan::scene) quad_indices: Vec<u32>,
    pub(in crate::renderer::native_vulkan::scene) quad_vertex_buffer_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) quad_index_buffer_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) sampled_image_quads:
        Vec<NativeVulkanSceneSampledImageQuad>,
    pub(in crate::renderer::native_vulkan::scene) sampled_image_effect_targets:
        Vec<NativeVulkanSceneSampledImageEffectTarget>,
    pub(in crate::renderer::native_vulkan::scene) sampled_image_sources: Vec<PathBuf>,
    pub(in crate::renderer::native_vulkan::scene) sampled_image_recording_ready: bool,
    pub(in crate::renderer::native_vulkan::scene) sampled_image_implicit_full_extent_ready: bool,
    pub(in crate::renderer::native_vulkan::scene) sampled_image_recording_steps:
        Vec<NativeVulkanSceneSampledImageRecordingStep>,
    pub(in crate::renderer::native_vulkan::scene) sampled_image_vertices:
        Vec<NativeVulkanSceneSampledImageVertex>,
    pub(in crate::renderer::native_vulkan::scene) sampled_image_indices: Vec<u32>,
    pub(in crate::renderer::native_vulkan::scene) sampled_image_vertex_buffer_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) sampled_image_index_buffer_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) video_quads: Vec<NativeVulkanSceneVideoQuad>,
    pub(in crate::renderer::native_vulkan::scene) video_sources: Vec<PathBuf>,
    pub(in crate::renderer::native_vulkan::scene) video_recording_ready: bool,
    pub(in crate::renderer::native_vulkan::scene) video_recording_steps:
        Vec<NativeVulkanSceneVideoRecordingStep>,
    pub(in crate::renderer::native_vulkan::scene) video_vertices:
        Vec<NativeVulkanSceneSampledImageVertex>,
    pub(in crate::renderer::native_vulkan::scene) video_indices: Vec<u32>,
    pub(in crate::renderer::native_vulkan::scene) video_vertex_buffer_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) video_index_buffer_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) clear_background_op_count: usize,
    pub(in crate::renderer::native_vulkan::scene) background_clear_color: Option<String>,
    pub(in crate::renderer::native_vulkan::scene) color_op_count: usize,
    pub(in crate::renderer::native_vulkan::scene) sampled_image_op_count: usize,
    pub(in crate::renderer::native_vulkan::scene) video_op_count: usize,
    pub(in crate::renderer::native_vulkan::scene) vector_shape_op_count: usize,
    pub(in crate::renderer::native_vulkan::scene) text_op_count: usize,
    pub(in crate::renderer::native_vulkan::scene) path_op_count: usize,
    pub(in crate::renderer::native_vulkan::scene) required_image_resources: Vec<PathBuf>,
    pub(in crate::renderer::native_vulkan::scene) required_video_resources: Vec<PathBuf>,
    pub(in crate::renderer::native_vulkan::scene) requires_text_geometry: bool,
    pub(in crate::renderer::native_vulkan::scene) requires_path_tessellation: bool,
    pub(in crate::renderer::native_vulkan::scene) requires_video_decode: bool,
    pub(in crate::renderer::native_vulkan::scene) fast_clear_color: Option<String>,
}
