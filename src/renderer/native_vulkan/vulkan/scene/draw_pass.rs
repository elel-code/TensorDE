#![allow(dead_code)]

use std::sync::atomic::AtomicUsize;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder};

use crate::core::SceneBlendMode;
use crate::renderer::native_vulkan::effect_debug::{
    native_vulkan_effect_debug_enabled, native_vulkan_effect_debug_log_limited,
};

use super::descriptor_heap::{
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    VulkanaliaDescriptorHeapImageSamplerResources,
    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_bind_info_for_image,
    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info_for_image,
};
use super::scene_sampled_image::VulkanaliaSceneSampledImageResources;

mod blend;

use self::blend::{
    native_vulkan_vulkanalia_scene_advanced_color_blend_state,
    native_vulkan_vulkanalia_scene_blend_mode_label,
    native_vulkan_vulkanalia_scene_color_attachment,
    native_vulkan_vulkanalia_scene_fragment_module_for_blend,
    native_vulkan_vulkanalia_scene_sampled_image_pipeline_from_set,
    native_vulkan_vulkanalia_scene_solid_quad_pipeline,
};

const SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES: u32 = 24;
const SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES: u32 = 44;
const SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES: u32 = 8;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES: u32 = 256;
pub(in crate::renderer::native_vulkan::vulkan) const SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT:
    usize = 8;
const SCENE_SAMPLED_IMAGE_ALPHA_TEXTURE_SLOT_DISABLED: u32 = u32::MAX;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_MASK_OFFSET_BYTES: usize = 20;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_SYSTEM_UNIFORM_COUNT_OFFSET_BYTES: usize = 24;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_UNIFORM_COUNT_OFFSET_BYTES: usize = 28;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_BASE_OFFSET_BYTES: usize = 32;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_STRIDE_BYTES: usize = 8;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES: usize = 96;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_PASSTHROUGH_BLEND_MODE_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_ANIMATION_SPEED_OFFSET_BYTES: usize = 100;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCALE_OFFSET_BYTES: usize = 104;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCROLL_SPEED_OFFSET_BYTES: usize = 108;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_DIRECTION_OFFSET_BYTES: usize = 112;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_RATIO_OFFSET_BYTES: usize = 116;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_SCROLL_SPEED_X_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_SCROLL_SPEED_Y_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_ANIMATION_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_SCROLL_REPEAT_X_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCALE_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_SCROLL_REPEAT_Y_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCROLL_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_TOP_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_BOTTOM_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_ANIMATION_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_LEFT_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCALE_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_RIGHT_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCROLL_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_FLAGS_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_DIRECTION_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_COLOR_R_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_COLOR_G_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_ANIMATION_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_COLOR_B_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCALE_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_ALPHA_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCROLL_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SPEED_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_DIRECTION_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SKEW_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_RATIO_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_SCALE_X_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_SCALE_Y_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_ANIMATION_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_SPEED_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCALE_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_ROUGH_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCROLL_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_NOISE_AMOUNT_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_DIRECTION_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_PHASE_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_RATIO_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_OPACITY_ALPHA_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERFLOW_STRENGTH_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERFLOW_SPEED_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_ANIMATION_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERFLOW_FEATHER_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCALE_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERFLOW_PHASE_SCALE_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCROLL_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_STRENGTH_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_SPEED_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_ANIMATION_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_PHASE_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCALE_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_POWER_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCROLL_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_NOISE_SCALE_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_DIRECTION_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_RATIO_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_RATIO_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_STRENGTH_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_DAMPING_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_ANIMATION_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_X_FEATHER_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCALE_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_SPEED_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCROLL_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_INERTIA_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_DIRECTION_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_SEGMENT_COUNT_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_RATIO_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_STRENGTH_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_SPEED_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_ANIMATION_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_SCALE_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCALE_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_EXPONENT_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCROLL_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_DIRECTION_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_DIRECTION_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_SPEED2_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_RATIO_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES: usize = 120;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_DIRECTION_OFFSET_BYTES: usize = 124;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_FLAGS_OFFSET_BYTES: usize = 128;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_SCALE2_OFFSET_BYTES: usize = 124;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_OFFSET2_OFFSET_BYTES: usize = 128;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_EXPONENT2_OFFSET_BYTES: usize = 132;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_DIRECTION2_OFFSET_BYTES: usize = 136;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_FLAGS_OFFSET_BYTES: usize = 140;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_RING_RADIUS_OFFSET_BYTES: usize = 124;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_RING_WIDTH_OFFSET_BYTES: usize = 128;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_RING_SEGMENT_COUNT_OFFSET_BYTES: usize = 132;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_RING_SEGMENT_WIDTH_OFFSET_BYTES: usize = 136;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SECTOR_OFFSET_OFFSET_BYTES: usize = 140;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SECTOR_WIDTH_OFFSET_BYTES: usize = 144;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SECTOR_SEGMENT_COUNT_OFFSET_BYTES: usize = 148;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SECTOR_SEGMENT_WIDTH_OFFSET_BYTES: usize = 152;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_FLAGS_OFFSET_BYTES: usize = 156;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_COLOR_R_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_COLOR_R_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_COLOR_G_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_COLOR_G_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_COLOR_B_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_COLOR_B_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_OPACITY_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_ALPHA_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_BAR_COUNT_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_VOLUME_FACTOR_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SKEW_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_BAR_SPACING_OFFSET_BYTES: usize = 124;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_MIN_HEIGHT_OFFSET_BYTES: usize = 128;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_BOUNDS_LOW_OFFSET_BYTES: usize = 132;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_BOUNDS_HIGH_OFFSET_BYTES: usize = 136;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_FLAGS_OFFSET_BYTES: usize = 140;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_AA_X_OFFSET_BYTES: usize = 144;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_AA_Y_OFFSET_BYTES: usize = 148;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_RADIUS_OFFSET_BYTES: usize = 152;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_BRIGHTNESS_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_GLOW_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_ANIMATION_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_SCALE_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCALE_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_SPEED_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCROLL_SPEED_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_TIME_OFFSET_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_DIRECTION_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_DISTORTION_OFFSET_BYTES: usize =
    SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_RATIO_OFFSET_BYTES;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_CHROMATIC_OFFSET_BYTES: usize = 124;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_BLUR_OFFSET_BYTES: usize = 128;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_COLOR1_OFFSET_BYTES: usize = 132;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_COLOR2_OFFSET_BYTES: usize = 144;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_FLAGS_OFFSET_BYTES: usize = 156;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_GLOBAL_TIME_OFFSET_BYTES: usize = 124;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_GLOBAL_WIND_OFFSET_BYTES: usize = 128;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_WEIGHT_CENTER_OFFSET_BYTES: usize = 132;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_SMOOTH_DISTANCE_OFFSET_BYTES: usize = 136;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_DIRECTIONAL_COMPENSATION_OFFSET_BYTES: usize = 140;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_CENTERS_OFFSET_BYTES: usize = 144;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_SIZES_OFFSET_BYTES: usize = 176;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_ANGLES_OFFSET_BYTES: usize = 192;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_TIME_OFFSETS_OFFSET_BYTES: usize = 208;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_FLAGS_OFFSET_BYTES: usize = 224;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_OUTPUT_FLAGS_OFFSET_BYTES: usize = 228;
const SCENE_SAMPLED_IMAGE_OUTPUT_FLAG_PREMULTIPLY_RGB: u32 = 1;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_GENERIC: u32 = 0;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_WATERRIPPLE: u32 = 1;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_SCROLL: u32 = 2;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_WATERWAVES: u32 = 3;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_IRIS: u32 = 4;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_OPACITY: u32 = 5;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_WATERFLOW: u32 = 6;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_FOLIAGE_SWAY: u32 = 7;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_AUTO_SWAY: u32 = 8;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_WATERCAUSTICS: u32 = 9;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_SKEW: u32 = 10;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_PASSTHROUGHBLEND: u32 = 11;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_TECHCIRCLE: u32 = 12;
const SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_AUDIOBARS: u32 = 13;
const SCENE_SAMPLED_IMAGE_WATERWAVES_FLAG_MASK: u32 = 1;
const SCENE_SAMPLED_IMAGE_WATERWAVES_FLAG_DUAL: u32 = 2;
const SCENE_SAMPLED_IMAGE_WATERWAVES_FLAG_TIMEOFFSET: u32 = 4;
const SCENE_SAMPLED_IMAGE_FOLIAGE_SWAY_FLAG_MASK: u32 = 1;
const SCENE_SAMPLED_IMAGE_AUTO_SWAY_FLAG_MASK: u32 = 1;
const SCENE_SAMPLED_IMAGE_CAUSTICS_FLAG_FRAMEBUFFER_OVERLAY: u32 = 1 << 16;
const SCENE_SAMPLED_IMAGE_SKEW_FLAG_REPEAT: u32 = 1;
const SCENE_SAMPLED_IMAGE_SKEW_FLAG_VERTEX_MODE: u32 = 2;
static SCENE_DRAW_PASS_EFFECT_DEBUG_LOG_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeVulkanVulkanaliaSceneDrawPassInput {
    pub(crate) plan_ready: bool,
    pub(crate) native_draw_ready: bool,
    pub(crate) draw_op_count: usize,
    pub(crate) backend_status: &'static str,
    pub(crate) blocking_reason: Option<&'static str>,
    pub(crate) fast_clear_color_ready: bool,
    pub(crate) clear_background_op_count: usize,
    pub(crate) quad_recording_ready: bool,
    pub(crate) quad_recording_step_count: usize,
    pub(crate) quad_vertex_buffer_bytes: u64,
    pub(crate) quad_index_buffer_bytes: u64,
    pub(crate) sampled_image_recording_ready: bool,
    pub(crate) sampled_image_implicit_full_extent_ready: bool,
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
pub struct NativeVulkanVulkanaliaSceneDrawPassSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub backend_ready: bool,
    pub backend_status: &'static str,
    pub blocking_reason: Option<&'static str>,
    pub draw_op_count: usize,
    pub color_op_count: usize,
    pub clear_background_op_count: usize,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub target_format: String,
    pub extent: (u32, u32),
    pub shader_modules_created: bool,
    pub pipeline_layout_created: bool,
    pub pipeline_created: bool,
    pub rasterization_samples: &'static str,
    pub render_pass_compatibility: &'static str,
    pub primitive_topology: &'static str,
    pub vertex_input_binding_count: u32,
    pub vertex_input_attribute_count: u32,
    pub vertex_stride_bytes: u32,
    pub vertex_position_format: &'static str,
    pub vertex_color_format: &'static str,
    pub push_constant_bytes: u32,
    pub push_constant_model: &'static str,
    pub blend_model: &'static str,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSolidQuadCommandSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub extent: (u32, u32),
    pub index_count: u32,
    pub command_buffer_recorded: bool,
    pub vertex_buffer_bound: bool,
    pub index_buffer_bound: bool,
    pub push_constant_bytes: u32,
    pub swapchain_layout_transition: &'static str,
    pub render_model: &'static str,
    pub command_order: Vec<&'static str>,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub target_format: String,
    pub extent: (u32, u32),
    pub shader_modules_created: bool,
    pub descriptor_set_layout_created: bool,
    pub pipeline_layout_created: bool,
    pub pipeline_created: bool,
    pub pass_specific_fragment_pipeline_count: u32,
    pub rasterization_samples: &'static str,
    pub render_pass_compatibility: &'static str,
    pub primitive_topology: &'static str,
    pub vertex_input_binding_count: u32,
    pub vertex_input_attribute_count: u32,
    pub vertex_stride_bytes: u32,
    pub vertex_position_format: &'static str,
    pub vertex_uv_format: &'static str,
    pub vertex_effect_uv_format: &'static str,
    pub vertex_opacity_format: &'static str,
    pub vertex_tint_format: &'static str,
    pub descriptor_set_count: u32,
    pub descriptor_model: &'static str,
    pub descriptor_heap_mapping_enabled: bool,
    pub descriptor_heap_pipeline_flag_enabled: bool,
    pub descriptor_set_layout_create_flags: Vec<&'static str>,
    pub descriptor_type: &'static str,
    pub descriptor_binding: u32,
    pub push_constant_bytes: u32,
    pub push_constant_model: &'static str,
    pub blend_model: &'static str,
    pub sampled_image_model: &'static str,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub uses_push_descriptor_fast_path: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSampledImageCommandSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub extent: (u32, u32),
    pub index_count: u32,
    pub command_buffer_recorded: bool,
    pub vertex_buffer_bound: bool,
    pub index_buffer_bound: bool,
    pub draw_call_count: u32,
    pub solid_quad_draw_call_count: u32,
    pub sampled_image_draw_call_count: u32,
    pub pipeline_bind_count: u32,
    pub descriptor_set_bound: bool,
    pub push_descriptor_set_recorded: bool,
    pub descriptor_heap_bound: bool,
    pub descriptor_set_bind_count: u32,
    pub push_descriptor_set_recorded_count: u32,
    pub descriptor_heap_draw_count: u32,
    pub framebuffer_snapshot_required: bool,
    pub framebuffer_snapshot_copy_count: u32,
    pub solid_passthroughblend_draw_count: u32,
    pub sampled_passthroughblend_draw_count: u32,
    pub solid_framebuffer_snapshot_descriptor_group_base_index: Option<u32>,
    pub descriptor_model: &'static str,
    pub push_constant_bytes: u32,
    pub swapchain_layout_transition: &'static str,
    pub sampled_image_layout: &'static str,
    pub render_model: &'static str,
    pub command_order: Vec<&'static str>,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSolidQuadPipelineResources {
    pub(in crate::renderer::native_vulkan::vulkan) pipeline_layout: vk::PipelineLayout,
    pub(in crate::renderer::native_vulkan::vulkan) alpha_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) normal_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) additive_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) multiply_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) screen_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) max_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) modulate_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) hsl_color_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) alpha_to_coverage_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) hsl_color_passthrough_pipeline:
        Option<vk::Pipeline>,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot,
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneMsaaColorTarget {
    pub(in crate::renderer::native_vulkan::vulkan) image: vk::Image,
    pub(in crate::renderer::native_vulkan::vulkan) image_view: vk::ImageView,
    pub(in crate::renderer::native_vulkan::vulkan) sample_count: vk::SampleCountFlags,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSampledImagePipelineResources {
    pub(in crate::renderer::native_vulkan::vulkan) pipeline_layout: vk::PipelineLayout,
    pub(in crate::renderer::native_vulkan::vulkan) generic_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) water_ripple_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) water_waves_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) water_flow_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) water_caustics_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) foliage_sway_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) auto_sway_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) scroll_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) skew_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) iris_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) opacity_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) tech_circle_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) audio_bars_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) passthroughblend_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot,
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSampledImagePipelineSet {
    pub(in crate::renderer::native_vulkan::vulkan) alpha_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) normal_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) additive_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) multiply_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) screen_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) max_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) modulate_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) hsl_color_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) alpha_to_coverage_pipeline: vk::Pipeline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) enum VulkanaliaSceneSampledImageDescriptorBinding {
    DescriptorHeap {
        descriptor_group_base_index: u32,
        texture_slot_bindings:
            Vec<super::present::NativeVulkanVulkanaliaSceneTextureSlotResourceBinding>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) enum VulkanaliaSceneSampledImageRenderTarget {
    Swapchain,
    EffectTarget { target_index: u32, clear: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSampledImageDrawCommand {
    pub(in crate::renderer::native_vulkan::vulkan) layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) last_layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) material:
        super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    pub(in crate::renderer::native_vulkan::vulkan) descriptor_binding:
        VulkanaliaSceneSampledImageDescriptorBinding,
    pub(in crate::renderer::native_vulkan::vulkan) render_target:
        VulkanaliaSceneSampledImageRenderTarget,
    pub(in crate::renderer::native_vulkan::vulkan) first_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) index_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSolidQuadDrawCommand {
    pub(in crate::renderer::native_vulkan::vulkan) layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) last_layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) blend:
        super::present::NativeVulkanVulkanaliaSceneBlendState,
    pub(in crate::renderer::native_vulkan::vulkan) first_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) index_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VulkanaliaSceneOrderedDrawPipeline {
    SolidQuad,
    SampledImage,
}

impl VulkanaliaSceneOrderedDrawPipeline {
    fn sort_rank(self) -> u8 {
        match self {
            Self::SolidQuad => 0,
            Self::SampledImage => 1,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SolidQuad => "solid-quad",
            Self::SampledImage => "sampled-image",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VulkanaliaSceneOrderedDrawStep {
    layer_index: usize,
    pipeline: VulkanaliaSceneOrderedDrawPipeline,
    command_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VulkanaliaSceneBoundDrawPipeline {
    SolidQuad(super::present::NativeVulkanVulkanaliaSceneBlendState),
    SampledImage {
        blend: super::present::NativeVulkanVulkanaliaSceneBlendState,
        shader_program: VulkanaliaSceneSampledImageShaderProgram,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VulkanaliaSceneSampledImageShaderProgram {
    Generic,
    WaterRipple,
    WaterWaves,
    WaterFlow,
    WaterCaustics,
    FoliageSway,
    AutoSway,
    Scroll,
    Skew,
    Iris,
    Opacity,
    TechCircle,
    AudioBars,
    PassthroughBlend,
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSolidQuadDrawResources<'a> {
    pub(in crate::renderer::native_vulkan::vulkan) pipeline_resources:
        &'a VulkanaliaSceneSolidQuadPipelineResources,
    pub(in crate::renderer::native_vulkan::vulkan) vertex_buffer: vk::Buffer,
    pub(in crate::renderer::native_vulkan::vulkan) index_buffer: vk::Buffer,
    pub(in crate::renderer::native_vulkan::vulkan) draw_commands:
        &'a [VulkanaliaSceneSolidQuadDrawCommand],
    pub(in crate::renderer::native_vulkan::vulkan) framebuffer_snapshot_descriptor_group_base_index:
        Option<u32>,
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneDescriptorHeapDrawResources<'a>
{
    pub(in crate::renderer::native_vulkan::vulkan) resources:
        &'a VulkanaliaDescriptorHeapImageSamplerResources,
}

fn native_vulkan_vulkanalia_scene_ordered_draw_steps(
    solid_commands: &[VulkanaliaSceneSolidQuadDrawCommand],
    sampled_commands: &[VulkanaliaSceneSampledImageDrawCommand],
) -> Vec<VulkanaliaSceneOrderedDrawStep> {
    let mut ordered =
        Vec::with_capacity(solid_commands.len().saturating_add(sampled_commands.len()));
    for (command_index, command) in solid_commands.iter().enumerate() {
        ordered.push(VulkanaliaSceneOrderedDrawStep {
            layer_index: command.layer_index,
            pipeline: VulkanaliaSceneOrderedDrawPipeline::SolidQuad,
            command_index,
        });
    }
    for (command_index, command) in sampled_commands.iter().enumerate() {
        ordered.push(VulkanaliaSceneOrderedDrawStep {
            layer_index: command.layer_index,
            pipeline: VulkanaliaSceneOrderedDrawPipeline::SampledImage,
            command_index,
        });
    }
    ordered.sort_by(|left, right| {
        left.layer_index
            .cmp(&right.layer_index)
            .then(left.pipeline.sort_rank().cmp(&right.pipeline.sort_rank()))
            .then(left.command_index.cmp(&right.command_index))
    });
    ordered
}

fn native_vulkan_vulkanalia_scene_bound_pipeline_key(
    draw: &VulkanaliaSceneOrderedDrawStep,
    solid_commands: &[VulkanaliaSceneSolidQuadDrawCommand],
    sampled_commands: &[VulkanaliaSceneSampledImageDrawCommand],
) -> VulkanaliaSceneBoundDrawPipeline {
    match draw.pipeline {
        VulkanaliaSceneOrderedDrawPipeline::SolidQuad => {
            VulkanaliaSceneBoundDrawPipeline::SolidQuad(solid_commands[draw.command_index].blend)
        }
        VulkanaliaSceneOrderedDrawPipeline::SampledImage => {
            let material = &sampled_commands[draw.command_index].material;
            VulkanaliaSceneBoundDrawPipeline::SampledImage {
                blend: material.render_state.blend,
                shader_program: scene_sampled_image_shader_program(material),
            }
        }
    }
}

fn scene_sampled_image_shader_program(
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
) -> VulkanaliaSceneSampledImageShaderProgram {
    if scene_sampled_image_material_uses_passthroughblend(material) {
        return VulkanaliaSceneSampledImageShaderProgram::PassthroughBlend;
    }
    if material.effect_kinds.len() == 1
        && material.effect_kinds[0]
            == super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterRipple
    {
        return VulkanaliaSceneSampledImageShaderProgram::WaterRipple;
    }
    if material.effect_kinds.len() == 1
        && material.effect_kinds[0]
            == super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterWaves
    {
        return VulkanaliaSceneSampledImageShaderProgram::WaterWaves;
    }
    if material.effect_kinds.len() == 1
        && material.effect_kinds[0]
            == super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterFlow
    {
        return VulkanaliaSceneSampledImageShaderProgram::WaterFlow;
    }
    if material.effect_kinds.len() == 1
        && material.effect_kinds[0]
            == super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterCaustics
    {
        return VulkanaliaSceneSampledImageShaderProgram::WaterCaustics;
    }
    if material.effect_kinds.len() == 1
        && material.effect_kinds[0]
            == super::present::NativeVulkanVulkanaliaSceneEffectKind::FoliageSway
    {
        return VulkanaliaSceneSampledImageShaderProgram::FoliageSway;
    }
    if material.effect_kinds.len() == 1
        && material.effect_kinds[0]
            == super::present::NativeVulkanVulkanaliaSceneEffectKind::AutoSway
    {
        return VulkanaliaSceneSampledImageShaderProgram::AutoSway;
    }
    if material.effect_kinds.len() == 1
        && material.effect_kinds[0] == super::present::NativeVulkanVulkanaliaSceneEffectKind::Scroll
    {
        return VulkanaliaSceneSampledImageShaderProgram::Scroll;
    }
    if material.effect_kinds.len() == 1
        && material.effect_kinds[0] == super::present::NativeVulkanVulkanaliaSceneEffectKind::Skew
    {
        return VulkanaliaSceneSampledImageShaderProgram::Skew;
    }
    if material.effect_kinds.len() == 1
        && material.effect_kinds[0] == super::present::NativeVulkanVulkanaliaSceneEffectKind::Iris
    {
        return VulkanaliaSceneSampledImageShaderProgram::Iris;
    }
    if material.effect_kinds.len() == 1
        && material.effect_kinds[0]
            == super::present::NativeVulkanVulkanaliaSceneEffectKind::OpacityMask
    {
        return VulkanaliaSceneSampledImageShaderProgram::Opacity;
    }
    if material.effect_kinds.len() == 1
        && material.effect_kinds[0]
            == super::present::NativeVulkanVulkanaliaSceneEffectKind::TechCircle
    {
        return VulkanaliaSceneSampledImageShaderProgram::TechCircle;
    }
    if material.effect_kinds.len() == 1
        && material.effect_kinds[0]
            == super::present::NativeVulkanVulkanaliaSceneEffectKind::AudioBars
    {
        return VulkanaliaSceneSampledImageShaderProgram::AudioBars;
    }
    VulkanaliaSceneSampledImageShaderProgram::Generic
}

fn scene_sampled_image_material_uses_passthroughblend(
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
) -> bool {
    material.shader.as_deref() == Some("util/effectpassthrough")
        || (material.effect_kinds.is_empty()
            && material.combo_values.contains_key("BLENDMODE")
            && material.texture_slot_count >= 2)
}

fn native_vulkan_vulkanalia_scene_sampled_image_pipeline_for_material(
    resources: &VulkanaliaSceneSampledImagePipelineResources,
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
) -> vk::Pipeline {
    let pipelines = match scene_sampled_image_shader_program(material) {
        VulkanaliaSceneSampledImageShaderProgram::Generic => &resources.generic_pipelines,
        VulkanaliaSceneSampledImageShaderProgram::WaterRipple => &resources.water_ripple_pipelines,
        VulkanaliaSceneSampledImageShaderProgram::WaterWaves => &resources.water_waves_pipelines,
        VulkanaliaSceneSampledImageShaderProgram::WaterFlow => &resources.water_flow_pipelines,
        VulkanaliaSceneSampledImageShaderProgram::WaterCaustics => {
            &resources.water_caustics_pipelines
        }
        VulkanaliaSceneSampledImageShaderProgram::FoliageSway => &resources.foliage_sway_pipelines,
        VulkanaliaSceneSampledImageShaderProgram::AutoSway => &resources.auto_sway_pipelines,
        VulkanaliaSceneSampledImageShaderProgram::Scroll => &resources.scroll_pipelines,
        VulkanaliaSceneSampledImageShaderProgram::Skew => &resources.skew_pipelines,
        VulkanaliaSceneSampledImageShaderProgram::Iris => &resources.iris_pipelines,
        VulkanaliaSceneSampledImageShaderProgram::Opacity => &resources.opacity_pipelines,
        VulkanaliaSceneSampledImageShaderProgram::TechCircle => &resources.tech_circle_pipelines,
        VulkanaliaSceneSampledImageShaderProgram::AudioBars => &resources.audio_bars_pipelines,
        VulkanaliaSceneSampledImageShaderProgram::PassthroughBlend => {
            &resources.passthroughblend_pipelines
        }
    };
    native_vulkan_vulkanalia_scene_sampled_image_pipeline_from_set(
        pipelines,
        material.render_state.blend.mode,
    )
}

fn native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_recordable(status: &str) -> bool {
    native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_solid_quad(status)
        || native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_sampled_image_recording(
            status,
        )
        || native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_sampled_image_full_extent(
            status,
        )
        || native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_mixed_recording(status)
        || native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_mixed_full_extent(status)
}

fn native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_solid_quad(status: &str) -> bool {
    matches!(
        status,
        "solid-quad-recording-ready" | "clear-background-solid-quad-recording-ready"
    )
}

fn native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_sampled_image_recording(
    status: &str,
) -> bool {
    matches!(
        status,
        "sampled-image-recording-ready" | "clear-background-sampled-image-recording-ready"
    )
}

fn native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_sampled_image_full_extent(
    status: &str,
) -> bool {
    matches!(
        status,
        "sampled-image-implicit-full-extent-ready"
            | "clear-background-sampled-image-implicit-full-extent-ready"
    )
}

fn native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_mixed_recording(
    status: &str,
) -> bool {
    matches!(
        status,
        "mixed-quad-sampled-image-recording-ready"
            | "clear-background-mixed-quad-sampled-image-recording-ready"
    )
}

fn native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_mixed_full_extent(
    status: &str,
) -> bool {
    matches!(
        status,
        "mixed-quad-sampled-image-implicit-full-extent-ready"
            | "clear-background-mixed-quad-sampled-image-implicit-full-extent-ready"
    )
}

pub(crate) fn native_vulkan_vulkanalia_scene_draw_pass_snapshot(
    input: NativeVulkanVulkanaliaSceneDrawPassInput,
) -> NativeVulkanVulkanaliaSceneDrawPassSnapshot {
    let status_driven_ready =
        native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_recordable(input.backend_status);
    let graph_ready = (input.plan_ready && input.native_draw_ready) || status_driven_ready;
    let solid_quad_ready =
        native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_solid_quad(input.backend_status)
            || (graph_ready
                && input.quad_recording_ready
                && input
                    .quad_recording_step_count
                    .saturating_add(input.clear_background_op_count)
                    == input.draw_op_count
                && input.sampled_image_op_count == 0);
    let sampled_image_pending =
        native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_sampled_image_recording(
            input.backend_status,
        ) || (graph_ready
            && input.sampled_image_recording_ready
            && input.sampled_image_recording_step_count <= input.sampled_image_op_count
            && input
                .sampled_image_op_count
                .saturating_add(input.clear_background_op_count)
                == input.draw_op_count);
    let sampled_image_implicit_full_extent_ready =
        native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_sampled_image_full_extent(
            input.backend_status,
        ) || (graph_ready
            && input.sampled_image_implicit_full_extent_ready
            && input
                .sampled_image_op_count
                .saturating_add(input.clear_background_op_count)
                == input.draw_op_count);
    let mixed_quad_sampled_image_implicit_full_extent_ready =
        native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_mixed_full_extent(
            input.backend_status,
        ) || (graph_ready
            && input.sampled_image_implicit_full_extent_ready
            && input.quad_recording_step_count > 0
            && input.sampled_image_op_count == 1
            && input
                .quad_recording_step_count
                .saturating_add(input.sampled_image_op_count)
                .saturating_add(input.clear_background_op_count)
                == input.draw_op_count);
    let mixed_quad_sampled_image_ready =
        native_vulkan_vulkanalia_scene_draw_pass_backend_status_is_mixed_recording(
            input.backend_status,
        ) || (graph_ready
            && input.quad_recording_step_count > 0
            && input.sampled_image_recording_ready
            && input.sampled_image_recording_step_count <= input.sampled_image_op_count
            && input
                .quad_recording_step_count
                .saturating_add(input.sampled_image_op_count)
                .saturating_add(input.clear_background_op_count)
                == input.draw_op_count);

    let (backend_ready, backend_status, blocking_reason) = if solid_quad_ready {
        if input.clear_background_op_count > 0 {
            (
                true,
                "clear-background-solid-quad-dynamic-rendering-recording-ready",
                None,
            )
        } else {
            (true, "solid-quad-dynamic-rendering-recording-ready", None)
        }
    } else if mixed_quad_sampled_image_ready {
        if input.clear_background_op_count > 0 {
            (
                true,
                "clear-background-mixed-quad-sampled-image-dynamic-rendering-recording-ready",
                None,
            )
        } else {
            (
                true,
                "mixed-quad-sampled-image-dynamic-rendering-recording-ready",
                None,
            )
        }
    } else if mixed_quad_sampled_image_implicit_full_extent_ready {
        if input.clear_background_op_count > 0 {
            (
                true,
                "clear-background-mixed-quad-sampled-image-implicit-full-extent-present-ready",
                None,
            )
        } else {
            (
                true,
                "mixed-quad-sampled-image-implicit-full-extent-present-ready",
                None,
            )
        }
    } else if sampled_image_implicit_full_extent_ready {
        if input.clear_background_op_count > 0 {
            (
                true,
                "clear-background-sampled-image-implicit-full-extent-present-ready",
                None,
            )
        } else {
            (
                true,
                "sampled-image-implicit-full-extent-present-ready",
                None,
            )
        }
    } else if sampled_image_pending {
        if input.clear_background_op_count > 0 {
            (
                true,
                "clear-background-sampled-image-dynamic-rendering-recording-ready",
                None,
            )
        } else {
            (
                true,
                "sampled-image-dynamic-rendering-recording-ready",
                None,
            )
        }
    } else if !graph_ready {
        (
            false,
            "blocked-by-scene-draw-plan",
            input.blocking_reason.or(Some("scene-draw-plan-not-ready")),
        )
    } else if input.fast_clear_color_ready {
        (
            false,
            "delegated-to-vulkanalia-clear-present",
            Some("fast-clear-uses-clear-present-not-draw-pass"),
        )
    } else {
        (
            false,
            input.backend_status,
            input
                .blocking_reason
                .or(Some("vulkanalia-scene-recording-not-ready")),
        )
    };

    let pipeline_labels = if solid_quad_ready {
        vec![
            "scene-solid-quad-alpha-blend",
            "scene-solid-quad-normal-blend",
            "scene-solid-quad-additive-blend",
            "scene-solid-quad-multiply-blend",
            "scene-solid-quad-screen-blend",
            "scene-solid-quad-max-blend",
            "scene-solid-quad-modulate-blend",
            "scene-solid-quad-hsl-color-blend",
            "scene-solid-quad-alpha-to-coverage",
        ]
    } else if mixed_quad_sampled_image_ready || mixed_quad_sampled_image_implicit_full_extent_ready
    {
        vec![
            "scene-solid-quad-alpha-blend",
            "scene-solid-quad-normal-blend",
            "scene-solid-quad-additive-blend",
            "scene-solid-quad-multiply-blend",
            "scene-solid-quad-screen-blend",
            "scene-solid-quad-max-blend",
            "scene-solid-quad-modulate-blend",
            "scene-solid-quad-hsl-color-blend",
            "scene-solid-quad-alpha-to-coverage",
            "scene-sampled-image-alpha-blend",
            "scene-sampled-image-normal-blend",
            "scene-sampled-image-additive-blend",
            "scene-sampled-image-multiply-blend",
            "scene-sampled-image-screen-blend",
            "scene-sampled-image-max-blend",
            "scene-sampled-image-modulate-blend",
            "scene-sampled-image-hsl-color-blend",
            "scene-sampled-image-alpha-to-coverage",
        ]
    } else if sampled_image_pending || sampled_image_implicit_full_extent_ready {
        vec![
            "scene-sampled-image-alpha-blend",
            "scene-sampled-image-normal-blend",
            "scene-sampled-image-additive-blend",
            "scene-sampled-image-multiply-blend",
            "scene-sampled-image-screen-blend",
            "scene-sampled-image-max-blend",
            "scene-sampled-image-modulate-blend",
            "scene-sampled-image-hsl-color-blend",
            "scene-sampled-image-alpha-to-coverage",
        ]
    } else {
        Vec::new()
    };
    let descriptor_set_count = 0;
    let (vertex_buffer_bytes, index_buffer_bytes, vertex_stride_bytes) =
        if mixed_quad_sampled_image_ready || mixed_quad_sampled_image_implicit_full_extent_ready {
            (
                input
                    .quad_vertex_buffer_bytes
                    .saturating_add(input.sampled_image_vertex_buffer_bytes),
                input
                    .quad_index_buffer_bytes
                    .saturating_add(input.sampled_image_index_buffer_bytes),
                0,
            )
        } else if sampled_image_pending {
            (
                input.sampled_image_vertex_buffer_bytes,
                input.sampled_image_index_buffer_bytes,
                SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES,
            )
        } else if sampled_image_implicit_full_extent_ready {
            (0, 0, SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES)
        } else {
            (
                input.quad_vertex_buffer_bytes,
                input.quad_index_buffer_bytes,
                24,
            )
        };

    NativeVulkanVulkanaliaSceneDrawPassSnapshot {
        binding: "vulkanalia",
        route: "scene-dynamic-rendering-draw-pass",
        backend_ready,
        backend_status,
        blocking_reason,
        draw_op_count: input.draw_op_count,
        color_op_count: input.color_op_count,
        clear_background_op_count: input.clear_background_op_count,
        solid_quad_count: saturating_u32(input.quad_recording_step_count),
        sampled_image_quad_count: if sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
        {
            saturating_u32(input.sampled_image_op_count)
        } else {
            saturating_u32(input.sampled_image_recording_step_count)
        },
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
        } else if mixed_quad_sampled_image_ready {
            saturating_u32(
                input
                    .quad_recording_step_count
                    .saturating_add(input.sampled_image_recording_step_count),
            )
        } else if mixed_quad_sampled_image_implicit_full_extent_ready {
            saturating_u32(
                input
                    .quad_recording_step_count
                    .saturating_add(input.sampled_image_op_count),
            )
        } else if sampled_image_pending {
            saturating_u32(input.sampled_image_recording_step_count)
        } else if sampled_image_implicit_full_extent_ready {
            saturating_u32(input.sampled_image_op_count)
        } else {
            0
        },
        render_pass_compatibility: if solid_quad_ready
            || sampled_image_pending
            || sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_ready
        {
            "dynamic-rendering-no-render-pass"
        } else {
            "not-recordable-yet"
        },
        render_model: if solid_quad_ready {
            "scene solid quad vertices -> Vulkan 1.3/1.4 dynamic rendering indexed draw -> Wayland swapchain"
        } else if mixed_quad_sampled_image_ready {
            "scene solid quad buffers + retained sampled images -> Vulkan 1.4 dynamic rendering ordered draws -> Wayland swapchain"
        } else if mixed_quad_sampled_image_implicit_full_extent_ready {
            "scene solid quad buffers + extent-derived sampled-image geometry -> Vulkan 1.4 dynamic rendering ordered draws -> Wayland swapchain"
        } else if sampled_image_pending {
            "scene image quad vertices -> retained sampled image descriptor heap -> Vulkan 1.4 dynamic rendering indexed draw -> Wayland swapchain"
        } else if sampled_image_implicit_full_extent_ready {
            "scene image layer -> extent-derived sampled-image geometry -> retained sampled image descriptor heap -> Vulkan 1.4 dynamic rendering indexed draw -> Wayland swapchain"
        } else {
            "scene draw pass has not reached a vulkanalia-recordable backend"
        },
        command_order: native_vulkan_vulkanalia_scene_draw_pass_command_order(
            solid_quad_ready,
            sampled_image_pending || sampled_image_implicit_full_extent_ready,
            input.fast_clear_color_ready,
            mixed_quad_sampled_image_ready || mixed_quad_sampled_image_implicit_full_extent_ready,
        )
        .to_vec(),
        uses_pipeline_rendering_create_info: solid_quad_ready
            || sampled_image_pending
            || sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_ready,
        uses_dynamic_rendering: solid_quad_ready
            || sampled_image_pending
            || sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_ready,
        uses_synchronization2: solid_quad_ready
            || sampled_image_pending
            || sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_ready,
        uses_submit2: solid_quad_ready
            || sampled_image_pending
            || sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_ready,
        uses_vulkan_1_4_dynamic_rendering_local_read: false,
        vulkan_1_4_dynamic_rendering_local_read_policy: "not-required-for-single-pass-solid-quad; reserve-for-multipass-scene-local-read",
        zero_copy_scope: "scene-graph-geometry-to-swapchain; no decoded-video frame copy or scene snapshot upload",
        primary_reference: "Vulkan dynamic rendering; FFmpeg remains first reference for video clock/queue discipline",
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_scene_solid_quad_pipeline_resources(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: Option<&NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot>,
) -> Result<VulkanaliaSceneSolidQuadPipelineResources, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene solid quad pipeline requires non-zero extent".to_owned());
    }

    let push_range = vk::PushConstantRange::builder()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES)
        .build();
    let push_ranges = [push_range];
    let pipeline_layout_info =
        vk::PipelineLayoutCreateInfo::builder().push_constant_ranges(&push_ranges);
    let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
        .map_err(|err| format!("vkCreatePipelineLayout(vulkanalia scene quad): {err:?}"))?;

    let result = (|| -> Result<VulkanaliaSceneSolidQuadPipelineResources, String> {
        let vertex_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_VERTEX_SPIRV,
            "scene solid quad vertex",
        )?;
        let result = (|| -> Result<VulkanaliaSceneSolidQuadPipelineResources, String> {
            let fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
                device,
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_FRAGMENT_SPIRV,
                "scene solid quad fragment",
            )?;
            let result = (|| -> Result<VulkanaliaSceneSolidQuadPipelineResources, String> {
                let premultiplied_fragment_module =
                native_vulkan_vulkanalia_scene_create_shader_module(
                    device,
                    &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_PREMULTIPLIED_FRAGMENT_SPIRV,
                    "scene solid quad premultiplied fragment",
                )?;
                let passthrough_fragment_module = if descriptor_heap_plan.is_some() {
                    Some(native_vulkan_vulkanalia_scene_create_shader_module(
                        device,
                        &NATIVE_VULKAN_VULKANALIA_SCENE_SOLID_QUAD_PASSTHROUGHBLEND_FRAGMENT_SPIRV,
                        "scene solid quad passthroughblend fragment",
                    )?)
                } else {
                    None
                };
                let result = native_vulkan_vulkanalia_create_scene_solid_quad_blend_pipelines(
                    device,
                    target_format,
                    extent,
                    pipeline_layout,
                    vertex_module,
                    fragment_module,
                    premultiplied_fragment_module,
                    descriptor_heap_plan,
                    passthrough_fragment_module,
                );
                unsafe {
                    if let Some(module) = passthrough_fragment_module {
                        device.destroy_shader_module(module, None);
                    }
                    device.destroy_shader_module(premultiplied_fragment_module, None);
                }
                result
            })();
            unsafe {
                device.destroy_shader_module(fragment_module, None);
            }
            result
        })();
        unsafe {
            device.destroy_shader_module(vertex_module, None);
        }
        result
    })();

    if result.is_err() {
        unsafe {
            device.destroy_pipeline_layout(pipeline_layout, None);
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_create_scene_solid_quad_blend_pipelines(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    pipeline_layout: vk::PipelineLayout,
    vertex_module: vk::ShaderModule,
    fragment_module: vk::ShaderModule,
    premultiplied_fragment_module: vk::ShaderModule,
    descriptor_heap_plan: Option<&NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot>,
    passthrough_fragment_module: Option<vk::ShaderModule>,
) -> Result<VulkanaliaSceneSolidQuadPipelineResources, String> {
    let shader_entry = b"main\0";
    let create_pipeline = |blend_mode| -> Result<vk::Pipeline, String> {
        let selected_fragment_module = native_vulkan_vulkanalia_scene_fragment_module_for_blend(
            blend_mode,
            fragment_module,
            premultiplied_fragment_module,
        );
        let stages = [
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_module)
                .name(shader_entry)
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(selected_fragment_module)
                .name(shader_entry)
                .build(),
        ];
        let binding = vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build();
        let attributes = [
            vk::VertexInputAttributeDescription::builder()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(0)
                .build(),
            vk::VertexInputAttributeDescription::builder()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(8)
                .build(),
        ];
        let bindings = [binding];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attributes)
            .build();
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .build();
        let viewport = vk::Viewport::builder()
            .x(0.0)
            .y(0.0)
            .width(extent.width as f32)
            .height(extent.height as f32)
            .min_depth(0.0)
            .max_depth(1.0)
            .build();
        let scissor = vk::Rect2D::builder()
            .offset(vk::Offset2D { x: 0, y: 0 })
            .extent(extent)
            .build();
        let viewports = [viewport];
        let scissors = [scissor];
        let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
            .viewports(&viewports)
            .scissors(&scissors)
            .build();
        let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0)
            .build();
        let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
            .rasterization_samples(vk::SampleCountFlags::_1)
            .alpha_to_coverage_enable(blend_mode == SceneBlendMode::AlphaToCoverage)
            .build();
        let color_attachment = native_vulkan_vulkanalia_scene_color_attachment(blend_mode);
        let color_attachments = [color_attachment];
        let mut advanced_blend =
            native_vulkan_vulkanalia_scene_advanced_color_blend_state(blend_mode);
        let mut color_blend_builder =
            vk::PipelineColorBlendStateCreateInfo::builder().attachments(&color_attachments);
        if let Some(advanced_blend) = advanced_blend.as_mut() {
            color_blend_builder = color_blend_builder.push_next(advanced_blend);
        }
        let color_blend = color_blend_builder.build();
        let color_attachment_formats = [target_format];
        let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
            .color_attachment_formats(&color_attachment_formats)
            .build();
        let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization)
            .multisample_state(&multisample)
            .color_blend_state(&color_blend)
            .layout(pipeline_layout)
            .render_pass(vk::RenderPass::null())
            .subpass(0)
            .push_next(&mut rendering_info)
            .build();
        let (pipelines, _success_code) = unsafe {
            device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
        }
        .map_err(|err| {
            format!(
                "vkCreateGraphicsPipelines(vulkanalia scene quad {}): {err:?}",
                native_vulkan_vulkanalia_scene_blend_mode_label(blend_mode)
            )
        })?;
        Ok(pipelines[0])
    };

    let mut created_pipelines = Vec::with_capacity(9);
    let mut create_tracked_pipeline = |blend_mode| -> Result<vk::Pipeline, String> {
        let pipeline = create_pipeline(blend_mode)?;
        created_pipelines.push(pipeline);
        Ok(pipeline)
    };
    let result = (|| -> Result<VulkanaliaSceneSolidQuadPipelineResources, String> {
        let alpha_pipeline = create_tracked_pipeline(SceneBlendMode::Alpha)?;
        let normal_pipeline = create_tracked_pipeline(SceneBlendMode::Normal)?;
        let additive_pipeline = create_tracked_pipeline(SceneBlendMode::Additive)?;
        let multiply_pipeline = create_tracked_pipeline(SceneBlendMode::Multiply)?;
        let screen_pipeline = create_tracked_pipeline(SceneBlendMode::Screen)?;
        let max_pipeline = create_tracked_pipeline(SceneBlendMode::Max)?;
        let modulate_pipeline = create_tracked_pipeline(SceneBlendMode::Modulate)?;
        let hsl_color_pipeline = create_tracked_pipeline(SceneBlendMode::HslColor)?;
        let alpha_to_coverage_pipeline = create_tracked_pipeline(SceneBlendMode::AlphaToCoverage)?;
        let hsl_color_passthrough_pipeline =
            if let (Some(descriptor_heap_plan), Some(passthrough_fragment_module)) =
                (descriptor_heap_plan, passthrough_fragment_module)
            {
                let pipeline =
                    native_vulkan_vulkanalia_create_scene_solid_quad_passthrough_pipeline(
                        device,
                        target_format,
                        extent,
                        descriptor_heap_plan,
                        pipeline_layout,
                        vertex_module,
                        passthrough_fragment_module,
                    )?;
                Some(pipeline)
            } else {
                None
            };
        Ok(VulkanaliaSceneSolidQuadPipelineResources {
            pipeline_layout,
            alpha_pipeline,
            normal_pipeline,
            additive_pipeline,
            multiply_pipeline,
            screen_pipeline,
            max_pipeline,
            modulate_pipeline,
            hsl_color_pipeline,
            alpha_to_coverage_pipeline,
            hsl_color_passthrough_pipeline,
            snapshot: native_vulkan_vulkanalia_scene_solid_quad_pipeline_snapshot(
                target_format,
                extent,
                vk::SampleCountFlags::_1,
            ),
        })
    })();
    if result.is_err() {
        unsafe {
            for pipeline in created_pipelines {
                device.destroy_pipeline(pipeline, None);
            }
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_create_scene_solid_quad_passthrough_pipeline(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    pipeline_layout: vk::PipelineLayout,
    vertex_module: vk::ShaderModule,
    fragment_module: vk::ShaderModule,
) -> Result<vk::Pipeline, String> {
    let shader_entry = b"main\0";
    let descriptor_heap_mapping =
        native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
            descriptor_heap_plan,
            0,
            0,
        )?;
    let descriptor_heap_mappings = [descriptor_heap_mapping];
    let mut descriptor_heap_mapping_info =
        vk::ShaderDescriptorSetAndBindingMappingInfoEXT::builder()
            .mappings(&descriptor_heap_mappings)
            .build();
    let mut fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::FRAGMENT)
        .module(fragment_module)
        .name(shader_entry);
    fragment_stage = fragment_stage.push_next(&mut descriptor_heap_mapping_info);
    let stages = [
        vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_module)
            .name(shader_entry)
            .build(),
        fragment_stage.build(),
    ];
    let binding = vk::VertexInputBindingDescription::builder()
        .binding(0)
        .stride(SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES)
        .input_rate(vk::VertexInputRate::VERTEX)
        .build();
    let attributes = [
        vk::VertexInputAttributeDescription::builder()
            .location(0)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(1)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(8)
            .build(),
    ];
    let bindings = [binding];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attributes)
        .build();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .build();
    let viewport = vk::Viewport::builder()
        .x(0.0)
        .y(0.0)
        .width(extent.width as f32)
        .height(extent.height as f32)
        .min_depth(0.0)
        .max_depth(1.0)
        .build();
    let scissor = vk::Rect2D::builder()
        .offset(vk::Offset2D { x: 0, y: 0 })
        .extent(extent)
        .build();
    let viewports = [viewport];
    let scissors = [scissor];
    let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
        .viewports(&viewports)
        .scissors(&scissors)
        .build();
    let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0)
        .build();
    let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(vk::SampleCountFlags::_1)
        .build();
    let color_attachment = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Normal);
    let color_attachments = [color_attachment];
    let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
        .attachments(&color_attachments)
        .build();
    let color_attachment_formats = [target_format];
    let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
        .color_attachment_formats(&color_attachment_formats)
        .build();
    let mut pipeline_flags2 = vk::PipelineCreateFlags2CreateInfo::builder()
        .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT)
        .build();
    let mut pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .layout(pipeline_layout)
        .render_pass(vk::RenderPass::null())
        .subpass(0)
        .push_next(&mut rendering_info);
    pipeline_info = pipeline_info.push_next(&mut pipeline_flags2);
    let pipeline_info = pipeline_info.build();
    let (pipelines, _success_code) = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|err| {
        format!("vkCreateGraphicsPipelines(vulkanalia scene solid passthroughblend): {err:?}")
    })?;
    Ok(pipelines[0])
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_scene_solid_quad_pipeline_resources(
    device: &Device,
    resources: VulkanaliaSceneSolidQuadPipelineResources,
) {
    unsafe {
        device.destroy_pipeline(resources.alpha_pipeline, None);
        device.destroy_pipeline(resources.normal_pipeline, None);
        device.destroy_pipeline(resources.additive_pipeline, None);
        device.destroy_pipeline(resources.multiply_pipeline, None);
        device.destroy_pipeline(resources.screen_pipeline, None);
        device.destroy_pipeline(resources.max_pipeline, None);
        device.destroy_pipeline(resources.modulate_pipeline, None);
        device.destroy_pipeline(resources.hsl_color_pipeline, None);
        device.destroy_pipeline(resources.alpha_to_coverage_pipeline, None);
        if let Some(pipeline) = resources.hsl_color_passthrough_pipeline {
            device.destroy_pipeline(pipeline, None);
        }
        device.destroy_pipeline_layout(resources.pipeline_layout, None);
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_scene_solid_quad_pipeline_snapshot(
    target_format: vk::Format,
    extent: vk::Extent2D,
    sample_count: vk::SampleCountFlags,
) -> NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot {
    NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot {
        binding: "vulkanalia",
        route: "scene-solid-quad-dynamic-rendering-pipeline",
        target_format: format!("{target_format:?}"),
        extent: (extent.width, extent.height),
        shader_modules_created: true,
        pipeline_layout_created: true,
        pipeline_created: true,
        rasterization_samples: scene_sample_count_label(sample_count),
        render_pass_compatibility: "dynamic-rendering-no-render-pass",
        primitive_topology: "triangle-list-indexed-quad",
        vertex_input_binding_count: 1,
        vertex_input_attribute_count: 2,
        vertex_stride_bytes: SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES,
        vertex_position_format: "R32G32_SFLOAT",
        vertex_color_format: "R32G32B32A32_SFLOAT",
        push_constant_bytes: SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES,
        push_constant_model: "scene-space pixel extent -> NDC conversion in vertex shader",
        blend_model: "solid rgba with opacity; alpha/normal/additive/multiply/screen/max/modulate/hsl-color blend pipeline selected per draw command",
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
    }
}

fn scene_sample_count_label(sample_count: vk::SampleCountFlags) -> &'static str {
    if sample_count == vk::SampleCountFlags::_1 {
        "1x"
    } else if sample_count == vk::SampleCountFlags::_2 {
        "2x"
    } else if sample_count == vk::SampleCountFlags::_4 {
        "4x"
    } else if sample_count == vk::SampleCountFlags::_8 {
        "8x"
    } else if sample_count == vk::SampleCountFlags::_16 {
        "16x"
    } else if sample_count == vk::SampleCountFlags::_32 {
        "32x"
    } else if sample_count == vk::SampleCountFlags::_64 {
        "64x"
    } else {
        "unknown"
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_resources(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    sample_count: vk::SampleCountFlags,
) -> Result<VulkanaliaSceneSampledImagePipelineResources, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene sampled-image pipeline requires non-zero extent".to_owned());
    }
    if !descriptor_heap_plan.backend_ready {
        return Err(
            "scene sampled-image pipeline requires a ready VK_EXT_descriptor_heap plan".to_owned(),
        );
    }

    let push_range = vk::PushConstantRange::builder()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES)
        .build();
    let push_ranges = [push_range];
    let pipeline_layout_info =
        vk::PipelineLayoutCreateInfo::builder().push_constant_ranges(&push_ranges);
    let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
        .map_err(|err| {
            format!("vkCreatePipelineLayout(vulkanalia scene sampled image): {err:?}")
        })?;

    let result = (|| -> Result<VulkanaliaSceneSampledImagePipelineResources, String> {
        let vertex_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_VERTEX_SPIRV,
            "scene sampled image vertex",
        )?;
        let fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV,
            "scene sampled image fragment",
        )?;
        let premultiplied_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PREMULTIPLIED_FRAGMENT_SPIRV,
            "scene sampled image premultiplied fragment",
        )?;
        let water_ripple_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERRIPPLE_FRAGMENT_SPIRV,
            "scene sampled image waterripple fragment",
        )?;
        let water_waves_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERWAVES_FRAGMENT_SPIRV,
            "scene sampled image waterwaves fragment",
        )?;
        let water_flow_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERFLOW_FRAGMENT_SPIRV,
            "scene sampled image waterflow fragment",
        )?;
        let water_caustics_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERCAUSTICS_FRAGMENT_SPIRV,
            "scene sampled image watercaustics fragment",
        )?;
        let foliage_sway_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FOLIAGE_SWAY_FRAGMENT_SPIRV,
            "scene sampled image foliagesway fragment",
        )?;
        let auto_sway_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_AUTO_SWAY_FRAGMENT_SPIRV,
            "scene sampled image autosway fragment",
        )?;
        let scroll_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SCROLL_FRAGMENT_SPIRV,
            "scene sampled image scroll fragment",
        )?;
        let skew_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SKEW_FRAGMENT_SPIRV,
            "scene sampled image skew fragment",
        )?;
        let iris_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_IRIS_FRAGMENT_SPIRV,
            "scene sampled image iris fragment",
        )?;
        let opacity_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_OPACITY_FRAGMENT_SPIRV,
            "scene sampled image opacity fragment",
        )?;
        let tech_circle_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_TECHCIRCLE_FRAGMENT_SPIRV,
            "scene sampled image techcircle fragment",
        )?;
        let audio_bars_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_AUDIOBARS_FRAGMENT_SPIRV,
            "scene sampled image audiobars fragment",
        )?;
        let passthroughblend_fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PASSTHROUGHBLEND_FRAGMENT_SPIRV,
            "scene sampled image passthroughblend fragment",
        )?;

        let result = (|| -> Result<VulkanaliaSceneSampledImagePipelineResources, String> {
            let generic_pipelines =
                native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    fragment_module,
                    premultiplied_fragment_module,
                )?;
            let water_ripple_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    water_ripple_fragment_module,
                    water_ripple_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        return Err(err);
                    }
                };
            let water_waves_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    water_waves_fragment_module,
                    water_waves_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_ripple_pipelines,
                        );
                        return Err(err);
                    }
                };
            let water_flow_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    water_flow_fragment_module,
                    water_flow_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_ripple_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_waves_pipelines,
                        );
                        return Err(err);
                    }
                };
            let scroll_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    scroll_fragment_module,
                    scroll_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_ripple_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_waves_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_flow_pipelines,
                        );
                        return Err(err);
                    }
                };
            let skew_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    skew_fragment_module,
                    skew_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_ripple_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_waves_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_flow_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            scroll_pipelines,
                        );
                        return Err(err);
                    }
                };
            let water_caustics_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    water_caustics_fragment_module,
                    water_caustics_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_ripple_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_waves_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_flow_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            scroll_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            skew_pipelines,
                        );
                        return Err(err);
                    }
                };
            let foliage_sway_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    foliage_sway_fragment_module,
                    foliage_sway_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_ripple_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_waves_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_flow_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            scroll_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            skew_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_caustics_pipelines,
                        );
                        return Err(err);
                    }
                };
            let auto_sway_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    auto_sway_fragment_module,
                    auto_sway_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_ripple_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_waves_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_flow_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            scroll_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            skew_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_caustics_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            foliage_sway_pipelines,
                        );
                        return Err(err);
                    }
                };
            let iris_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    iris_fragment_module,
                    iris_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_ripple_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_waves_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            scroll_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            skew_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_flow_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_caustics_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            foliage_sway_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            auto_sway_pipelines,
                        );
                        return Err(err);
                    }
                };
            let opacity_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    opacity_fragment_module,
                    opacity_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_ripple_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_waves_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            scroll_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            skew_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_flow_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_caustics_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            foliage_sway_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            auto_sway_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            iris_pipelines,
                        );
                        return Err(err);
                    }
                };
            let tech_circle_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    tech_circle_fragment_module,
                    tech_circle_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_ripple_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_waves_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            scroll_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            skew_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_flow_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_caustics_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            foliage_sway_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            auto_sway_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            iris_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            opacity_pipelines,
                        );
                        return Err(err);
                    }
                };
            let audio_bars_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    audio_bars_fragment_module,
                    audio_bars_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_ripple_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_waves_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            scroll_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            skew_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_flow_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_caustics_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            foliage_sway_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            auto_sway_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            iris_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            opacity_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            tech_circle_pipelines,
                        );
                        return Err(err);
                    }
                };
            let passthroughblend_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    pipeline_layout,
                    vertex_module,
                    passthroughblend_fragment_module,
                    passthroughblend_fragment_module,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_ripple_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_waves_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            scroll_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            skew_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_flow_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            water_caustics_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            foliage_sway_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            auto_sway_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            iris_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            opacity_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            tech_circle_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            audio_bars_pipelines,
                        );
                        return Err(err);
                    }
                };
            Ok(VulkanaliaSceneSampledImagePipelineResources {
                pipeline_layout,
                generic_pipelines,
                water_ripple_pipelines,
                water_waves_pipelines,
                water_flow_pipelines,
                water_caustics_pipelines,
                foliage_sway_pipelines,
                auto_sway_pipelines,
                scroll_pipelines,
                skew_pipelines,
                iris_pipelines,
                opacity_pipelines,
                tech_circle_pipelines,
                audio_bars_pipelines,
                passthroughblend_pipelines,
                snapshot: native_vulkan_vulkanalia_scene_sampled_image_pipeline_snapshot(
                    target_format,
                    extent,
                    sample_count,
                ),
            })
        })();

        unsafe {
            device.destroy_shader_module(passthroughblend_fragment_module, None);
            device.destroy_shader_module(audio_bars_fragment_module, None);
            device.destroy_shader_module(tech_circle_fragment_module, None);
            device.destroy_shader_module(opacity_fragment_module, None);
            device.destroy_shader_module(iris_fragment_module, None);
            device.destroy_shader_module(skew_fragment_module, None);
            device.destroy_shader_module(scroll_fragment_module, None);
            device.destroy_shader_module(auto_sway_fragment_module, None);
            device.destroy_shader_module(foliage_sway_fragment_module, None);
            device.destroy_shader_module(water_caustics_fragment_module, None);
            device.destroy_shader_module(water_flow_fragment_module, None);
            device.destroy_shader_module(water_waves_fragment_module, None);
            device.destroy_shader_module(water_ripple_fragment_module, None);
            device.destroy_shader_module(premultiplied_fragment_module, None);
            device.destroy_shader_module(fragment_module, None);
            device.destroy_shader_module(vertex_module, None);
        }
        result
    })();

    if result.is_err() {
        unsafe {
            device.destroy_pipeline_layout(pipeline_layout, None);
        }
    }
    result
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(
    device: &Device,
    resources: VulkanaliaSceneSampledImagePipelineResources,
) {
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.generic_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.water_ripple_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.water_waves_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.water_flow_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.water_caustics_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.foliage_sway_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.auto_sway_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.scroll_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.skew_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.iris_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.opacity_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.tech_circle_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.audio_bars_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.passthroughblend_pipelines,
    );
    unsafe {
        device.destroy_pipeline_layout(resources.pipeline_layout, None);
    }
}

#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    pipeline_layout: vk::PipelineLayout,
    vertex_module: vk::ShaderModule,
    fragment_module: vk::ShaderModule,
    premultiplied_fragment_module: vk::ShaderModule,
) -> Result<VulkanaliaSceneSampledImagePipelineSet, String> {
    let create_pipeline = |blend_mode| {
        let selected_fragment_module = native_vulkan_vulkanalia_scene_fragment_module_for_blend(
            blend_mode,
            fragment_module,
            premultiplied_fragment_module,
        );
        native_vulkan_vulkanalia_create_scene_sampled_image_pipeline(
            device,
            target_format,
            extent,
            descriptor_heap_plan,
            pipeline_layout,
            vertex_module,
            selected_fragment_module,
            blend_mode,
        )
    };
    let mut created_pipelines = Vec::with_capacity(9);
    let mut create_tracked_pipeline = |blend_mode| -> Result<vk::Pipeline, String> {
        let pipeline = create_pipeline(blend_mode)?;
        created_pipelines.push(pipeline);
        Ok(pipeline)
    };
    let result = (|| -> Result<VulkanaliaSceneSampledImagePipelineSet, String> {
        let alpha_pipeline = create_tracked_pipeline(SceneBlendMode::Alpha)?;
        let normal_pipeline = create_tracked_pipeline(SceneBlendMode::Normal)?;
        let additive_pipeline = create_tracked_pipeline(SceneBlendMode::Additive)?;
        let multiply_pipeline = create_tracked_pipeline(SceneBlendMode::Multiply)?;
        let screen_pipeline = create_tracked_pipeline(SceneBlendMode::Screen)?;
        let max_pipeline = create_tracked_pipeline(SceneBlendMode::Max)?;
        let modulate_pipeline = create_tracked_pipeline(SceneBlendMode::Modulate)?;
        let hsl_color_pipeline = create_tracked_pipeline(SceneBlendMode::HslColor)?;
        let alpha_to_coverage_pipeline = create_tracked_pipeline(SceneBlendMode::AlphaToCoverage)?;
        Ok(VulkanaliaSceneSampledImagePipelineSet {
            alpha_pipeline,
            normal_pipeline,
            additive_pipeline,
            multiply_pipeline,
            screen_pipeline,
            max_pipeline,
            modulate_pipeline,
            hsl_color_pipeline,
            alpha_to_coverage_pipeline,
        })
    })();
    if result.is_err() {
        unsafe {
            for pipeline in created_pipelines {
                device.destroy_pipeline(pipeline, None);
            }
        }
    }
    result
}

fn native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
    device: &Device,
    resources: VulkanaliaSceneSampledImagePipelineSet,
) {
    unsafe {
        device.destroy_pipeline(resources.alpha_pipeline, None);
        device.destroy_pipeline(resources.normal_pipeline, None);
        device.destroy_pipeline(resources.additive_pipeline, None);
        device.destroy_pipeline(resources.multiply_pipeline, None);
        device.destroy_pipeline(resources.screen_pipeline, None);
        device.destroy_pipeline(resources.max_pipeline, None);
        device.destroy_pipeline(resources.modulate_pipeline, None);
        device.destroy_pipeline(resources.hsl_color_pipeline, None);
        device.destroy_pipeline(resources.alpha_to_coverage_pipeline, None);
    }
}

#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_create_scene_sampled_image_pipeline(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    pipeline_layout: vk::PipelineLayout,
    vertex_module: vk::ShaderModule,
    fragment_module: vk::ShaderModule,
    blend_mode: SceneBlendMode,
) -> Result<vk::Pipeline, String> {
    let shader_entry = b"main\0";
    if descriptor_heap_plan.image_count < SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT {
        return Err(format!(
            "scene sampled-image pipeline requires at least {} descriptor heap texture slots, got {}",
            SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT, descriptor_heap_plan.image_count
        ));
    }
    let descriptor_heap_mappings = (0..SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT)
        .map(|binding| {
            native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
                descriptor_heap_plan,
                binding as u32,
                binding,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut descriptor_heap_mapping_info =
        vk::ShaderDescriptorSetAndBindingMappingInfoEXT::builder()
            .mappings(&descriptor_heap_mappings)
            .build();
    let mut fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::FRAGMENT)
        .module(fragment_module)
        .name(shader_entry);
    fragment_stage = fragment_stage.push_next(&mut descriptor_heap_mapping_info);
    let stages = [
        vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_module)
            .name(shader_entry)
            .build(),
        fragment_stage.build(),
    ];
    let binding = vk::VertexInputBindingDescription::builder()
        .binding(0)
        .stride(SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES)
        .input_rate(vk::VertexInputRate::VERTEX)
        .build();
    let attributes = [
        vk::VertexInputAttributeDescription::builder()
            .location(0)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(1)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(8)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(2)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(16)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(3)
            .binding(0)
            .format(vk::Format::R32_SFLOAT)
            .offset(24)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(4)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(28)
            .build(),
    ];
    let bindings = [binding];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attributes)
        .build();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .build();
    let viewport = vk::Viewport::builder()
        .x(0.0)
        .y(0.0)
        .width(extent.width as f32)
        .height(extent.height as f32)
        .min_depth(0.0)
        .max_depth(1.0)
        .build();
    let scissor = vk::Rect2D::builder()
        .offset(vk::Offset2D { x: 0, y: 0 })
        .extent(extent)
        .build();
    let viewports = [viewport];
    let scissors = [scissor];
    let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
        .viewports(&viewports)
        .scissors(&scissors)
        .build();
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder()
        .dynamic_states(&dynamic_states)
        .build();
    let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0)
        .build();
    let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(vk::SampleCountFlags::_1)
        .alpha_to_coverage_enable(blend_mode == SceneBlendMode::AlphaToCoverage)
        .build();
    let color_attachment = native_vulkan_vulkanalia_scene_color_attachment(blend_mode);
    let color_attachments = [color_attachment];
    let mut advanced_blend = native_vulkan_vulkanalia_scene_advanced_color_blend_state(blend_mode);
    let mut color_blend_builder =
        vk::PipelineColorBlendStateCreateInfo::builder().attachments(&color_attachments);
    if let Some(advanced_blend) = advanced_blend.as_mut() {
        color_blend_builder = color_blend_builder.push_next(advanced_blend);
    }
    let color_blend = color_blend_builder.build();
    let color_attachment_formats = [target_format];
    let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
        .color_attachment_formats(&color_attachment_formats)
        .build();
    let mut pipeline_flags2 = vk::PipelineCreateFlags2CreateInfo::builder()
        .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT)
        .build();
    let mut pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .dynamic_state(&dynamic_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .layout(pipeline_layout)
        .render_pass(vk::RenderPass::null())
        .subpass(0)
        .push_next(&mut rendering_info);
    pipeline_info = pipeline_info.push_next(&mut pipeline_flags2);
    let pipeline_info = pipeline_info.build();
    let (pipelines, _success_code) = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|err| {
        format!(
            "vkCreateGraphicsPipelines(vulkanalia scene sampled image {}): {err:?}",
            native_vulkan_vulkanalia_scene_blend_mode_label(blend_mode)
        )
    })?;
    Ok(pipelines[0])
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_scene_sampled_image_pipeline_snapshot(
    target_format: vk::Format,
    extent: vk::Extent2D,
    sample_count: vk::SampleCountFlags,
) -> NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot {
    NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot {
        binding: "vulkanalia",
        route: "scene-sampled-image-dynamic-rendering-pipeline",
        target_format: format!("{target_format:?}"),
        extent: (extent.width, extent.height),
        shader_modules_created: true,
        descriptor_set_layout_created: false,
        pipeline_layout_created: true,
        pipeline_created: true,
        pass_specific_fragment_pipeline_count: 126,
        rasterization_samples: scene_sample_count_label(sample_count),
        render_pass_compatibility: "dynamic-rendering-no-render-pass",
        primitive_topology: "triangle-list-indexed-image-quad",
        vertex_input_binding_count: 1,
        vertex_input_attribute_count: 5,
        vertex_stride_bytes: SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES,
        vertex_position_format: "R32G32_SFLOAT",
        vertex_uv_format: "R32G32_SFLOAT",
        vertex_effect_uv_format: "R32G32_SFLOAT",
        vertex_opacity_format: "R32_SFLOAT",
        vertex_tint_format: "R32G32B32A32_SFLOAT",
        descriptor_set_count: 0,
        descriptor_model: "VK_EXT_descriptor_heap",
        descriptor_heap_mapping_enabled: true,
        descriptor_heap_pipeline_flag_enabled: true,
        descriptor_set_layout_create_flags: Vec::new(),
        descriptor_type: "combined-image-sampler",
        descriptor_binding: 0,
        push_constant_bytes: SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES,
        push_constant_model: "scene-space pixel extent, alpha/mask state, elapsed time, WE g_TextureNResolution rows, and pass-specific effect parameter rows",
        blend_model: "sampled rgba with opacity; alpha/normal/additive/multiply/screen/max/modulate/hsl-color blend pipeline selected per draw command; WE passthroughblend uses shader framebuffer sampling plus normal replace output",
        sampled_image_model: "retained native sampled image -> VK_EXT_descriptor_heap constant-offset mapping -> generic, framebuffer-passthrough, or pass-specific fragment shader",
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
        uses_push_descriptor_fast_path: false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_record_scene_solid_quad_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    swapchain_view: vk::ImageView,
    extent: vk::Extent2D,
    pipeline_resources: &VulkanaliaSceneSolidQuadPipelineResources,
    vertex_buffer: vk::Buffer,
    index_buffer: vk::Buffer,
    index_count: u32,
    clear_color: [f32; 4],
) -> Result<NativeVulkanVulkanaliaSceneSolidQuadCommandSnapshot, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene solid quad command requires non-zero extent".to_owned());
    }
    if index_count == 0 {
        return Err("scene solid quad command requires at least one index".to_owned());
    }

    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| format!("vkResetCommandBuffer(vulkanalia scene quad): {err:?}"))?;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::empty())
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| format!("vkBeginCommandBuffer(vulkanalia scene quad): {err:?}"))?;

        let swapchain_to_attachment = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(native_vulkan_vulkanalia_scene_color_subresource_range())
            .build();
        let image_barriers = [swapchain_to_attachment];
        let dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&image_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &dependency);

        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: clear_color,
            },
        };
        let color_attachment = vk::RenderingAttachmentInfo::builder()
            .image_view(swapchain_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(clear_value)
            .build();
        let color_attachments = [color_attachment];
        let render_area = vk::Rect2D::builder()
            .offset(vk::Offset2D { x: 0, y: 0 })
            .extent(extent)
            .build();
        let rendering_info = vk::RenderingInfo::builder()
            .render_area(render_area)
            .layer_count(1)
            .color_attachments(&color_attachments)
            .build();
        device.cmd_begin_rendering(command_buffer, &rendering_info);
        device.cmd_bind_pipeline(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            pipeline_resources.alpha_pipeline,
        );
        let vertex_buffers = [vertex_buffer];
        let vertex_offsets = [0u64];
        device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
        device.cmd_bind_index_buffer(command_buffer, index_buffer, 0, vk::IndexType::UINT32);
        let push_constants = [extent.width as f32, extent.height as f32];
        let push_constant_bytes = std::slice::from_raw_parts(
            push_constants.as_ptr().cast::<u8>(),
            SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES as usize,
        );
        device.cmd_push_constants(
            command_buffer,
            pipeline_resources.pipeline_layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            push_constant_bytes,
        );
        device.cmd_draw_indexed(command_buffer, index_count, 1, 0, 0, 0);
        device.cmd_end_rendering(command_buffer);

        let swapchain_to_present = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
            .dst_access_mask(vk::AccessFlags2::empty())
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(native_vulkan_vulkanalia_scene_color_subresource_range())
            .build();
        let present_barriers = [swapchain_to_present];
        let present_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&present_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &present_dependency);

        device
            .end_command_buffer(command_buffer)
            .map_err(|err| format!("vkEndCommandBuffer(vulkanalia scene quad): {err:?}"))?;
    }

    Ok(NativeVulkanVulkanaliaSceneSolidQuadCommandSnapshot {
        binding: "vulkanalia",
        route: "scene-solid-quad-dynamic-rendering-command-buffer",
        extent: (extent.width, extent.height),
        index_count,
        command_buffer_recorded: true,
        vertex_buffer_bound: true,
        index_buffer_bound: true,
        push_constant_bytes: SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES,
        swapchain_layout_transition: "undefined -> color-attachment-optimal [-> transfer-src-optimal -> color-attachment-optimal for framebuffer passthrough] -> present-src-khr",
        render_model: "scene solid quad vertex/index buffers -> dynamic rendering indexed draw -> Wayland swapchain",
        command_order: native_vulkan_vulkanalia_scene_draw_pass_command_order(
            true, false, false, false,
        )
        .to_vec(),
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_record_scene_solid_quad_draws_inside_rendering(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    extent: vk::Extent2D,
    solid_quad_draw: VulkanaliaSceneSolidQuadDrawResources<'_>,
) -> Result<u32, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene solid draw requires non-zero extent".to_owned());
    }
    if solid_quad_draw.draw_commands.is_empty() {
        return Err("scene solid draw requires non-empty draw steps".to_owned());
    }
    for solid_draw in solid_quad_draw.draw_commands {
        if solid_draw.index_count == 0 {
            return Err("scene solid draw requires non-empty indices".to_owned());
        }
    }

    unsafe {
        let solid_push_constants = [extent.width as f32, extent.height as f32];
        let solid_push_constant_bytes = std::slice::from_raw_parts(
            solid_push_constants.as_ptr().cast::<u8>(),
            SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES as usize,
        );
        let mut bound_pipeline = None;
        for solid_draw in solid_quad_draw.draw_commands {
            if bound_pipeline != Some(solid_draw.blend) {
                device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    native_vulkan_vulkanalia_scene_solid_quad_pipeline(
                        solid_quad_draw.pipeline_resources,
                        solid_draw.blend.mode,
                    ),
                );
                let vertex_buffers = [solid_quad_draw.vertex_buffer];
                let vertex_offsets = [0u64];
                device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
                device.cmd_bind_index_buffer(
                    command_buffer,
                    solid_quad_draw.index_buffer,
                    0,
                    vk::IndexType::UINT32,
                );
                device.cmd_push_constants(
                    command_buffer,
                    solid_quad_draw.pipeline_resources.pipeline_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    solid_push_constant_bytes,
                );
                bound_pipeline = Some(solid_draw.blend);
            }
            device.cmd_draw_indexed(
                command_buffer,
                solid_draw.index_count,
                1,
                solid_draw.first_index,
                0,
                0,
            );
        }
    }

    Ok(solid_quad_draw
        .draw_commands
        .iter()
        .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count)))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_record_scene_sampled_image_draws_inside_rendering(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    extent: vk::Extent2D,
    solid_quad_draw: Option<VulkanaliaSceneSolidQuadDrawResources<'_>>,
    descriptor_heap_draw: Option<VulkanaliaSceneDescriptorHeapDrawResources<'_>>,
    pipeline_resources: &VulkanaliaSceneSampledImagePipelineResources,
    draw_commands: &[VulkanaliaSceneSampledImageDrawCommand],
    vertex_buffer: vk::Buffer,
    index_buffer: vk::Buffer,
) -> Result<u32, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene sampled-image draw requires non-zero extent".to_owned());
    }
    if draw_commands.is_empty() {
        return Err("scene sampled-image draw requires at least one draw".to_owned());
    }
    if let Some(draw) = solid_quad_draw {
        if draw.draw_commands.is_empty() {
            return Err("scene mixed draw requires non-empty solid draw steps".to_owned());
        }
        for solid_draw in draw.draw_commands {
            if solid_draw.index_count == 0 {
                return Err("scene mixed draw requires non-empty solid draw indices".to_owned());
            }
        }
    }
    for draw in draw_commands {
        if draw.index_count == 0 {
            return Err("scene sampled-image draw requires at least one index".to_owned());
        }
        if draw.render_target != VulkanaliaSceneSampledImageRenderTarget::Swapchain {
            return Err(
                "scene sampled-image inside-rendering helper only supports swapchain draw targets"
                    .to_owned(),
            );
        }
        match &draw.descriptor_binding {
            VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                descriptor_group_base_index,
                texture_slot_bindings,
            } => {
                let Some(descriptor_heap_draw) = descriptor_heap_draw else {
                    return Err(
                        "scene sampled-image descriptor heap draw requires heap resources"
                            .to_owned(),
                    );
                };
                let descriptor_group_end = *descriptor_group_base_index as usize
                    + SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT;
                if descriptor_group_end > descriptor_heap_draw.resources.plan.image_count {
                    return Err(format!(
                        "scene sampled-image descriptor heap group {}..{} exceeds heap image count {}",
                        descriptor_group_base_index,
                        descriptor_group_end,
                        descriptor_heap_draw.resources.plan.image_count
                    ));
                }
                if texture_slot_bindings.is_empty()
                    || texture_slot_bindings.len() > SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT
                {
                    return Err(format!(
                        "scene sampled-image texture slot count {} exceeds descriptor binding count {}",
                        texture_slot_bindings.len(),
                        SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT
                    ));
                }
                if let Some(alpha_texture_slot) = draw.material.alpha_texture_slot
                    && !texture_slot_bindings
                        .iter()
                        .any(|binding| binding.slot == alpha_texture_slot)
                {
                    return Err(format!(
                        "scene sampled-image alpha texture slot {alpha_texture_slot} has no resource binding"
                    ));
                }
            }
        }
    }
    if descriptor_heap_draw.is_none() {
        return Err("scene sampled-image draw requires descriptor heap resources".to_owned());
    }

    let solid_draw_commands: &[VulkanaliaSceneSolidQuadDrawCommand] =
        solid_quad_draw.map_or(&[], |draw| draw.draw_commands);
    let ordered_draws =
        native_vulkan_vulkanalia_scene_ordered_draw_steps(solid_draw_commands, draw_commands);

    unsafe {
        set_scene_dynamic_viewport_and_scissor(device, command_buffer, extent);
        let solid_push_constants = [extent.width as f32, extent.height as f32];
        let solid_push_constant_bytes = std::slice::from_raw_parts(
            solid_push_constants.as_ptr().cast::<u8>(),
            SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES as usize,
        );
        let mut bound_pipeline: Option<VulkanaliaSceneBoundDrawPipeline> = None;
        let mut bound_descriptor_heap_group: Option<u32> = None;
        for draw in &ordered_draws {
            match draw.pipeline {
                VulkanaliaSceneOrderedDrawPipeline::SolidQuad => {
                    let solid_draw = &solid_draw_commands[draw.command_index];
                    let pipeline_key =
                        VulkanaliaSceneBoundDrawPipeline::SolidQuad(solid_draw.blend);
                    if bound_pipeline != Some(pipeline_key) {
                        let solid_resources = solid_quad_draw
                            .as_ref()
                            .expect("solid draw resources present");
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            native_vulkan_vulkanalia_scene_solid_quad_pipeline(
                                solid_resources.pipeline_resources,
                                solid_draw.blend.mode,
                            ),
                        );
                        let vertex_buffers = [solid_resources.vertex_buffer];
                        let vertex_offsets = [0u64];
                        device.cmd_bind_vertex_buffers(
                            command_buffer,
                            0,
                            &vertex_buffers,
                            &vertex_offsets,
                        );
                        device.cmd_bind_index_buffer(
                            command_buffer,
                            solid_resources.index_buffer,
                            0,
                            vk::IndexType::UINT32,
                        );
                        device.cmd_push_constants(
                            command_buffer,
                            solid_resources.pipeline_resources.pipeline_layout,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                            0,
                            solid_push_constant_bytes,
                        );
                        bound_pipeline = Some(pipeline_key);
                    }
                    device.cmd_draw_indexed(
                        command_buffer,
                        solid_draw.index_count,
                        1,
                        solid_draw.first_index,
                        0,
                        0,
                    );
                }
                VulkanaliaSceneOrderedDrawPipeline::SampledImage => {
                    let sampled_draw = &draw_commands[draw.command_index];
                    let pipeline_key = VulkanaliaSceneBoundDrawPipeline::SampledImage {
                        blend: sampled_draw.material.render_state.blend,
                        shader_program: scene_sampled_image_shader_program(&sampled_draw.material),
                    };
                    if bound_pipeline != Some(pipeline_key) {
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            native_vulkan_vulkanalia_scene_sampled_image_pipeline_for_material(
                                pipeline_resources,
                                &sampled_draw.material,
                            ),
                        );
                        let vertex_buffers = [vertex_buffer];
                        let vertex_offsets = [0u64];
                        device.cmd_bind_vertex_buffers(
                            command_buffer,
                            0,
                            &vertex_buffers,
                            &vertex_offsets,
                        );
                        device.cmd_bind_index_buffer(
                            command_buffer,
                            index_buffer,
                            0,
                            vk::IndexType::UINT32,
                        );
                        bound_pipeline = Some(pipeline_key);
                    }
                    let VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                        descriptor_group_base_index,
                        ..
                    } = &sampled_draw.descriptor_binding;
                    if bound_descriptor_heap_group != Some(*descriptor_group_base_index) {
                        let descriptor_heap_draw =
                            descriptor_heap_draw.expect("descriptor heap draw resources present");
                        bind_scene_sampled_image_descriptor_heap_for_descriptor_group(
                            device,
                            command_buffer,
                            descriptor_heap_draw,
                            *descriptor_group_base_index,
                        )?;
                        bound_descriptor_heap_group = Some(*descriptor_group_base_index);
                    }
                    push_scene_sampled_image_constants(
                        device,
                        command_buffer,
                        pipeline_resources.pipeline_layout,
                        extent,
                        &sampled_draw.material,
                        0,
                    );
                    device.cmd_draw_indexed(
                        command_buffer,
                        sampled_draw.index_count,
                        1,
                        sampled_draw.first_index,
                        0,
                        0,
                    );
                }
            }
        }
    }

    let sampled_image_index_count = draw_commands
        .iter()
        .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count));
    let solid_quad_index_count = solid_quad_draw.map_or(0, |draw| {
        draw.draw_commands
            .iter()
            .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count))
    });
    Ok(solid_quad_index_count.saturating_add(sampled_image_index_count))
}

fn push_scene_sampled_image_constants(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    pipeline_layout: vk::PipelineLayout,
    extent: vk::Extent2D,
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    elapsed_ms: u64,
) {
    let time_seconds = (elapsed_ms as f32) * 0.001;
    let push_constant_bytes = scene_sampled_image_push_constant_bytes(extent, material, elapsed_ms);
    let texture_resolution_mask = scene_sampled_image_push_constant_u32(
        &push_constant_bytes,
        SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_MASK_OFFSET_BYTES,
    );
    if native_vulkan_effect_debug_enabled() && material.alpha_texture_slot.is_some() {
        native_vulkan_effect_debug_log_limited(
            &SCENE_DRAW_PASS_EFFECT_DEBUG_LOG_COUNT,
            48,
            "vulkan.push-constants",
            format_args!(
                "extent={}x{} alpha_slot={:?} mode={} shader_code={} time_seconds={:.3} texture_resolution_mask=0x{texture_resolution_mask:02x}",
                extent.width,
                extent.height,
                material.alpha_texture_slot,
                material.alpha_texture_mode.as_str(),
                material.alpha_texture_mode.shader_code(),
                time_seconds,
            ),
        );
    }
    unsafe {
        device.cmd_push_constants(
            command_buffer,
            pipeline_layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &push_constant_bytes,
        );
    }
}

fn scene_sampled_image_push_constant_bytes(
    extent: vk::Extent2D,
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    elapsed_ms: u64,
) -> [u8; SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES as usize] {
    let time_seconds = (elapsed_ms as f32) * 0.001;
    let alpha_texture_slot = material
        .alpha_texture_slot
        .unwrap_or(SCENE_SAMPLED_IMAGE_ALPHA_TEXTURE_SLOT_DISABLED);
    let mut push_constant_bytes = [0u8; SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES as usize];
    push_constant_bytes[0..4].copy_from_slice(&(extent.width as f32).to_ne_bytes());
    push_constant_bytes[4..8].copy_from_slice(&(extent.height as f32).to_ne_bytes());
    push_constant_bytes[8..12].copy_from_slice(&alpha_texture_slot.to_ne_bytes());
    push_constant_bytes[12..16]
        .copy_from_slice(&material.alpha_texture_mode.shader_code().to_ne_bytes());
    push_constant_bytes[16..20].copy_from_slice(&time_seconds.to_ne_bytes());
    let output_flags = scene_sampled_image_output_flags(material.render_state.blend.mode);
    push_constant_bytes[SCENE_FULL_SAMPLED_IMAGE_PUSH_OUTPUT_FLAGS_OFFSET_BYTES
        ..SCENE_FULL_SAMPLED_IMAGE_PUSH_OUTPUT_FLAGS_OFFSET_BYTES + 4]
        .copy_from_slice(&output_flags.to_ne_bytes());
    let mut texture_resolution_mask = 0u32;
    let mut system_uniform_count = 0u32;
    for uniform in &material.system_shader_uniforms {
        system_uniform_count = system_uniform_count.saturating_add(1);
        let Some(slot) = scene_sampled_image_texture_resolution_uniform_slot(&uniform.name) else {
            continue;
        };
        if slot >= SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT {
            continue;
        }
        let values = uniform.float_values();
        if values.len() < 2 {
            continue;
        }
        let offset = SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_BASE_OFFSET_BYTES
            + slot * SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_STRIDE_BYTES;
        push_constant_bytes[offset..offset + 4].copy_from_slice(&values[0].to_ne_bytes());
        push_constant_bytes[offset + 4..offset + 8].copy_from_slice(&values[1].to_ne_bytes());
        texture_resolution_mask |= 1u32 << slot;
    }
    push_constant_bytes[SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_MASK_OFFSET_BYTES
        ..SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_MASK_OFFSET_BYTES + 4]
        .copy_from_slice(&texture_resolution_mask.to_ne_bytes());
    push_constant_bytes[SCENE_FULL_SAMPLED_IMAGE_PUSH_SYSTEM_UNIFORM_COUNT_OFFSET_BYTES
        ..SCENE_FULL_SAMPLED_IMAGE_PUSH_SYSTEM_UNIFORM_COUNT_OFFSET_BYTES + 4]
        .copy_from_slice(&system_uniform_count.to_ne_bytes());
    push_constant_bytes[SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_UNIFORM_COUNT_OFFSET_BYTES
        ..SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_UNIFORM_COUNT_OFFSET_BYTES + 4]
        .copy_from_slice(
            &(material
                .constant_shader_uniforms
                .len()
                .min(u32::MAX as usize) as u32)
                .to_ne_bytes(),
        );
    match scene_sampled_image_shader_program(material) {
        VulkanaliaSceneSampledImageShaderProgram::WaterRipple => {
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["ripplestrength", "strength", "g_Strength"],
                    0.1,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_ANIMATION_SPEED_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["animationspeed", "animation_speed", "g_AnimationSpeed"],
                    0.15,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCALE_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["scale", "g_Scale"], 1.0),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCROLL_SPEED_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["scrollspeed", "scroll_speed", "g_ScrollSpeed"],
                    0.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_DIRECTION_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["scrolldirection", "direction", "g_Direction"],
                    0.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_RATIO_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["ratio", "g_Ratio"], 1.0),
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::WaterWaves => {
            let has_mask_texture = (texture_resolution_mask & (1u32 << 1)) != 0;
            let has_time_offset_texture = (texture_resolution_mask & (1u32 << 2)) != 0;
            let has_dual_wave = scene_sampled_image_material_combo_enabled(material, "DUALWAVES");
            let mut flags = 0u32;
            if has_mask_texture {
                flags |= SCENE_SAMPLED_IMAGE_WATERWAVES_FLAG_MASK;
            }
            if has_dual_wave {
                flags |= SCENE_SAMPLED_IMAGE_WATERWAVES_FLAG_DUAL;
            }
            if has_time_offset_texture
                && scene_sampled_image_material_combo_enabled(material, "TIMEOFFSET")
            {
                flags |= SCENE_SAMPLED_IMAGE_WATERWAVES_FLAG_TIMEOFFSET;
            }
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_STRENGTH_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["strength", "g_Strength"],
                    0.1,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_SPEED_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["speed", "g_Speed"], 5.0),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_SCALE_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["scale", "scale1", "g_Scale"],
                    200.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_EXPONENT_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["exponent", "g_Exponent"],
                    1.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_DIRECTION_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["direction", "g_Direction"],
                    0.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_SPEED2_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["speed2", "g_Speed2"], 3.0),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_SCALE2_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["scale2", "g_Scale2"],
                    66.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_OFFSET2_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["offset2", "g_Offset2"],
                    0.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_EXPONENT2_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["exponent2", "g_Exponent2"],
                    1.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_DIRECTION2_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["direction2", "g_Direction2"],
                    0.0,
                ),
            );
            scene_sampled_image_write_push_constant_u32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_FLAGS_OFFSET_BYTES,
                flags,
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::WaterFlow => {
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERFLOW_STRENGTH_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["strength", "flowamp", "g_FlowAmp"],
                    1.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERFLOW_SPEED_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["speed", "flowspeed", "g_FlowSpeed"],
                    1.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERFLOW_FEATHER_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["feather", "phasefeather", "g_PhaseFeather"],
                    0.4,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERFLOW_PHASE_SCALE_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &[
                        "phasescale",
                        "phase_scale",
                        "flowphasescale",
                        "g_FlowPhaseScale",
                    ],
                    2.0,
                ),
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::WaterCaustics => {
            let color1 = scene_sampled_image_material_constant_vec3(
                material,
                &["ui_editor_properties_color_start", "color1", "u_color1"],
                [0.7, 0.9, 1.0],
            );
            let color2 = scene_sampled_image_material_constant_vec3(
                material,
                &["ui_editor_properties_color_end", "color2", "u_color2"],
                [0.4, 0.6, 1.0],
            );
            let blend_mode = scene_sampled_image_material_combo_value(material, "BLENDMODE")
                .unwrap_or(32)
                .clamp(0, u32::MAX as i64) as u32;
            let style_mode = scene_sampled_image_material_combo_value(material, "MODE")
                .unwrap_or(0)
                .clamp(0, u32::MAX as i64) as u32;
            let flags = (blend_mode & 0xff) | ((style_mode & 0xff) << 8);
            let flags = if scene_sampled_image_material_combo_enabled(
                material,
                "GILDER_FRAMEBUFFER_OVERLAY",
            ) {
                flags | SCENE_SAMPLED_IMAGE_CAUSTICS_FLAG_FRAMEBUFFER_OVERLAY
            } else {
                flags
            };
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_BRIGHTNESS_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &[
                        "ui_editor_properties_brightness",
                        "brightness",
                        "u_brightness",
                    ],
                    1.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_GLOW_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["ui_editor_properties_glow", "glow", "u_glow"],
                    0.5,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_SCALE_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["ui_editor_properties_granularity", "scale", "u_scale"],
                    2.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_SPEED_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["ui_editor_properties_speed", "speed", "u_speed"],
                    1.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_TIME_OFFSET_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &[
                        "ui_editor_properties_time_offset",
                        "timeoffset",
                        "u_timeoffset",
                    ],
                    0.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_DISTORTION_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &[
                        "ui_editor_properties_distortion",
                        "distortion",
                        "u_distortion",
                    ],
                    1.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_CHROMATIC_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &[
                        "ui_editor_properties_chromatic_aberration",
                        "chromatic",
                        "u_chromatic",
                    ],
                    1.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_BLUR_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["ui_editor_properties_blur", "blur", "u_blur"],
                    0.0,
                ),
            );
            for (index, value) in color1.into_iter().enumerate() {
                scene_sampled_image_push_constant_f32(
                    &mut push_constant_bytes,
                    SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_COLOR1_OFFSET_BYTES + index * 4,
                    value,
                );
            }
            for (index, value) in color2.into_iter().enumerate() {
                scene_sampled_image_push_constant_f32(
                    &mut push_constant_bytes,
                    SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_COLOR2_OFFSET_BYTES + index * 4,
                    value,
                );
            }
            scene_sampled_image_write_push_constant_u32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_FLAGS_OFFSET_BYTES,
                flags,
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::FoliageSway => {
            let has_mask_texture = (texture_resolution_mask & (1u32 << 1)) != 0;
            let mut flags = 0u32;
            if has_mask_texture {
                flags |= SCENE_SAMPLED_IMAGE_FOLIAGE_SWAY_FLAG_MASK;
            }
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_STRENGTH_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["strength", "g_Strength"],
                    0.4,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_SPEED_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["speeduv", "speed", "g_Speed"],
                    5.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_PHASE_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["phase", "g_Phase"], 0.5),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_POWER_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["power", "g_Power"], 1.0),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_NOISE_SCALE_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["scale", "noisescale", "g_NoiseScale"],
                    0.05,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_RATIO_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["ratio", "g_Ratio"], 0.3),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_DIRECTION_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["scrolldirection", "direction", "g_Direction"],
                    0.0,
                ),
            );
            scene_sampled_image_write_push_constant_u32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_FLAGS_OFFSET_BYTES,
                flags,
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::AutoSway => {
            let has_mask_texture = (texture_resolution_mask & (1u32 << 1)) != 0;
            let mut flags = 0u32;
            if has_mask_texture {
                flags |= SCENE_SAMPLED_IMAGE_AUTO_SWAY_FLAG_MASK;
            }
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_STRENGTH_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["strength", "g_Strength"],
                    0.25,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_DAMPING_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["末端阻尼", "damping", "u_Damping"],
                    0.25,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_X_FEATHER_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["xFeather", "x_feather", "g_xFeather"],
                    0.2,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_SPEED_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["speed", "g_Speed"], 0.75),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_INERTIA_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["inertia", "g_Inertia"],
                    0.3,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_SEGMENT_COUNT_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["sigment", "segment", "g_SigmentCount"],
                    1.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_GLOBAL_TIME_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["timeoffset", "g_GlobalTimeOffset"],
                    0.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_GLOBAL_WIND_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["windDirectionOffset", "g_GlobalWindOffset"],
                    0.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_WEIGHT_CENTER_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["weightCenterOffset", "g_WeightCenterOffset"],
                    0.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_SMOOTH_DISTANCE_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["smoothDistance", "g_SmoothDistance"],
                    1.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_DIRECTIONAL_COMPENSATION_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["directionalCompensation", "g_DirectionalCompensation"],
                    0.0,
                ),
            );
            for index in 0..4 {
                let center = scene_sampled_image_material_constant_vec2(
                    material,
                    &[
                        &format!("center{}", index + 1),
                        &format!("g_SpinCenter{}", index + 1),
                    ],
                    [0.0, 0.5],
                );
                let offset =
                    SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_CENTERS_OFFSET_BYTES + index * 8;
                scene_sampled_image_push_constant_f32(&mut push_constant_bytes, offset, center[0]);
                scene_sampled_image_push_constant_f32(
                    &mut push_constant_bytes,
                    offset + 4,
                    center[1],
                );
            }
            for index in 0..4 {
                scene_sampled_image_push_constant_f32(
                    &mut push_constant_bytes,
                    SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_SIZES_OFFSET_BYTES + index * 4,
                    scene_sampled_image_material_constant_float(
                        material,
                        &[
                            &format!("size{}", index + 1),
                            &format!("g_Size{}", index + 1),
                        ],
                        0.1,
                    ),
                );
            }
            for index in 0..4 {
                scene_sampled_image_push_constant_f32(
                    &mut push_constant_bytes,
                    SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_ANGLES_OFFSET_BYTES + index * 4,
                    scene_sampled_image_material_constant_float(
                        material,
                        &[
                            &format!("angle{}", index + 2),
                            &format!("g_WindDirection{}", index + 2),
                        ],
                        -1.57075,
                    ),
                );
            }
            for index in 0..4 {
                scene_sampled_image_push_constant_f32(
                    &mut push_constant_bytes,
                    SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_TIME_OFFSETS_OFFSET_BYTES + index * 4,
                    scene_sampled_image_material_constant_float(
                        material,
                        &[
                            &format!("timeoffset{}", index + 1),
                            &format!("g_TimeOffset{}", index + 1),
                        ],
                        0.0,
                    ),
                );
            }
            scene_sampled_image_write_push_constant_u32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_FLAGS_OFFSET_BYTES,
                flags,
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::Scroll => {
            let scroll_repeat = scene_sampled_image_material_constant_vec2(
                material,
                &["repeat", "scale", "g_Scale"],
                [1.0, 1.0],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SCROLL_SPEED_X_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["speedx", "scrollx", "g_ScrollX"],
                    0.2,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SCROLL_SPEED_Y_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["speedy", "scrolly", "g_ScrollY"],
                    0.2,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SCROLL_REPEAT_X_OFFSET_BYTES,
                scroll_repeat[0],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SCROLL_REPEAT_Y_OFFSET_BYTES,
                scroll_repeat[1],
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::Skew => {
            let repeat_enabled =
                scene_sampled_image_material_combo_value(material, "REPEAT").unwrap_or(1) != 0;
            // The 3742497499 audio mark stores no MODE override but the WE
            // reference image is the vertex-skew variant. Treat an absent MODE
            // as Vertex while keeping explicit MODE=0 on the UV variant.
            let vertex_mode =
                scene_sampled_image_material_combo_value(material, "MODE").unwrap_or(1) != 0;
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_TOP_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["top", "g_Top"], 0.0),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_BOTTOM_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["bottom", "g_Bottom"], 0.0),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_LEFT_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["left", "g_Left"], 0.0),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_RIGHT_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["right", "g_Right"], 0.0),
            );
            scene_sampled_image_write_push_constant_u32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_FLAGS_OFFSET_BYTES,
                if repeat_enabled {
                    SCENE_SAMPLED_IMAGE_SKEW_FLAG_REPEAT
                } else {
                    0
                } | if vertex_mode {
                    SCENE_SAMPLED_IMAGE_SKEW_FLAG_VERTEX_MODE
                } else {
                    0
                },
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::Iris => {
            let iris_scale = scene_sampled_image_material_constant_vec2(
                material,
                &["scale", "g_Scale"],
                [1.0, 1.0],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_SCALE_X_OFFSET_BYTES,
                iris_scale[0],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_SCALE_Y_OFFSET_BYTES,
                iris_scale[1],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_SPEED_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["speed", "g_Speed"], 1.0),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_ROUGH_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["rough", "g_Rough"], 0.2),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_NOISE_AMOUNT_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["noiseamount", "noise_amount", "g_NoiseAmount"],
                    0.5,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_PHASE_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["phase", "phaseoffset", "g_PhaseOffset"],
                    0.0,
                ),
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::Opacity => {
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_OPACITY_ALPHA_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["alpha", "useralpha", "g_UserAlpha"],
                    1.0,
                ),
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::TechCircle => {
            let color = scene_sampled_image_material_constant_vec3(
                material,
                &["color", "ui_editor_properties_1_color", "Color"],
                [1.0, 1.0, 1.0],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_COLOR_R_OFFSET_BYTES,
                color[0],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_COLOR_G_OFFSET_BYTES,
                color[1],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_COLOR_B_OFFSET_BYTES,
                color[2],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_ALPHA_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["alpha", "ui_editor_properties_2_alpha"],
                    1.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SPEED_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["speed", "ui_editor_properties_3_speed"],
                    0.1,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SKEW_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["skew", "ui_editor_properties_6_skew"],
                    0.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_RING_RADIUS_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["ringRadius", "ui_editor_properties_4_ring_1_radius"],
                    0.5,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_RING_WIDTH_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["ringWidth", "ui_editor_properties_4_ring_1_width"],
                    0.2,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_RING_SEGMENT_COUNT_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &[
                        "ringSegmentCount",
                        "ui_editor_properties_4_ring_2_segment_count",
                    ],
                    2.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_RING_SEGMENT_WIDTH_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &[
                        "ringSegmentWidth",
                        "ui_editor_properties_4_ring_2_segment_width",
                    ],
                    0.25,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SECTOR_OFFSET_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["sectorOffset", "ui_editor_properties_5_sector_1_offset"],
                    0.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SECTOR_WIDTH_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["sectorWidth", "ui_editor_properties_5_sector_1_width"],
                    0.3,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SECTOR_SEGMENT_COUNT_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &[
                        "sectorSegmentCount",
                        "ui_editor_properties_5_sector_segment_count",
                    ],
                    5.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_SECTOR_SEGMENT_WIDTH_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &[
                        "sectorSegmentWidth",
                        "ui_editor_properties_5_sector_segment_width",
                    ],
                    0.75,
                ),
            );
            let coord_sys = scene_sampled_image_material_combo_value(material, "COORD_SYS")
                .unwrap_or(1)
                .clamp(0, 3) as u32;
            let ring_segments = u32::from(scene_sampled_image_material_combo_enabled(
                material,
                "RING_SEGMENTS",
            ));
            let sector_segments =
                scene_sampled_image_material_combo_value(material, "SECTOR_SEGMENTS")
                    .unwrap_or(0)
                    .clamp(0, 3) as u32;
            let ratio_correction = u32::from(scene_sampled_image_material_combo_enabled(
                material,
                "RATIO_CORRECTION",
            ));
            let flags =
                coord_sys | (ring_segments << 4) | (sector_segments << 5) | (ratio_correction << 8);
            scene_sampled_image_write_push_constant_u32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TECH_FLAGS_OFFSET_BYTES,
                flags,
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::AudioBars => {
            let color = scene_sampled_image_material_constant_vec3(
                material,
                &["u_BarColor", "Bar Color", "ui_editor_properties_color"],
                [1.0, 1.0, 1.0],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_COLOR_R_OFFSET_BYTES,
                color[0],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_COLOR_G_OFFSET_BYTES,
                color[1],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_COLOR_B_OFFSET_BYTES,
                color[2],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_OPACITY_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["u_BarOpacity", "ui_editor_properties_opacity"],
                    1.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_BAR_COUNT_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["u_BarCount", "Bar Count"],
                    32.0,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_VOLUME_FACTOR_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["u_VolumeFactor", "Volume Factor"],
                    0.5,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_BAR_SPACING_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &["u_BarSpacing", "Bar Spacing"],
                    0.1,
                ),
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_MIN_HEIGHT_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(
                    material,
                    &[
                        "u_minHeight",
                        "u_minHeightForC",
                        "Minimum Height (Will be multiplied by the bar width)",
                        "Minimum Height (Will be multiplied by the bar width) ",
                    ],
                    0.0,
                ),
            );
            let bounds = scene_sampled_image_material_constant_vec2(
                material,
                &["u_BarBounds", "Lower/Upper Bar Bounds"],
                [0.0, 1.0],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_BOUNDS_LOW_OFFSET_BYTES,
                bounds[0],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_BOUNDS_HIGH_OFFSET_BYTES,
                bounds[1],
            );
            let shape = scene_sampled_image_material_combo_value(material, "SHAPE")
                .unwrap_or(0)
                .clamp(0, u32::MAX as i64) as u32;
            scene_sampled_image_write_push_constant_u32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_FLAGS_OFFSET_BYTES,
                shape,
            );
            let rounded_aa = scene_sampled_image_material_constant_vec2(
                material,
                &["u_rAASmoothness", "Anti-alias blurring "],
                [0.05, 0.0],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_AA_X_OFFSET_BYTES,
                rounded_aa[0],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_AA_Y_OFFSET_BYTES,
                rounded_aa[1],
            );
            scene_sampled_image_push_constant_f32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_RADIUS_OFFSET_BYTES,
                scene_sampled_image_material_constant_float(material, &["u_Radius", "Radius"], 1.0),
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::PassthroughBlend => {
            let blend_mode = scene_sampled_image_material_combo_value(material, "BLENDMODE")
                .unwrap_or(0)
                .clamp(0, u32::MAX as i64) as u32;
            scene_sampled_image_write_push_constant_u32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_PASSTHROUGH_BLEND_MODE_OFFSET_BYTES,
                blend_mode,
            );
        }
        VulkanaliaSceneSampledImageShaderProgram::Generic => {}
    }
    push_constant_bytes[SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
        ..SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES + 4]
        .copy_from_slice(
            &scene_sampled_image_shader_program(material)
                .push_constant_code()
                .to_ne_bytes(),
        );
    push_constant_bytes
}

fn scene_sampled_image_output_flags(blend_mode: SceneBlendMode) -> u32 {
    if matches!(
        blend_mode,
        SceneBlendMode::Multiply
            | SceneBlendMode::Screen
            | SceneBlendMode::Max
            | SceneBlendMode::Modulate
    ) {
        SCENE_SAMPLED_IMAGE_OUTPUT_FLAG_PREMULTIPLY_RGB
    } else {
        0
    }
}

impl VulkanaliaSceneSampledImageShaderProgram {
    fn push_constant_code(self) -> u32 {
        match self {
            Self::Generic => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_GENERIC,
            Self::WaterRipple => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_WATERRIPPLE,
            Self::WaterWaves => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_WATERWAVES,
            Self::WaterFlow => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_WATERFLOW,
            Self::WaterCaustics => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_WATERCAUSTICS,
            Self::FoliageSway => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_FOLIAGE_SWAY,
            Self::AutoSway => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_AUTO_SWAY,
            Self::Scroll => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_SCROLL,
            Self::Skew => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_SKEW,
            Self::Iris => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_IRIS,
            Self::Opacity => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_OPACITY,
            Self::TechCircle => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_TECHCIRCLE,
            Self::AudioBars => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_AUDIOBARS,
            Self::PassthroughBlend => SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_PASSTHROUGHBLEND,
        }
    }
}

fn scene_sampled_image_texture_resolution_uniform_slot(name: &str) -> Option<usize> {
    let slot = name.strip_prefix("g_Texture")?.strip_suffix("Resolution")?;
    slot.parse::<usize>().ok()
}

fn scene_sampled_image_push_constant_f32(
    push_constant_bytes: &mut [u8; SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES as usize],
    offset: usize,
    value: f32,
) {
    push_constant_bytes[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
}

fn scene_sampled_image_write_push_constant_u32(
    push_constant_bytes: &mut [u8; SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES as usize],
    offset: usize,
    value: u32,
) {
    push_constant_bytes[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
}

fn scene_sampled_image_material_constant_float(
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    names: &[&str],
    default_value: f32,
) -> f32 {
    names
        .iter()
        .find_map(|name| scene_sampled_image_material_constant_named_float(material, name))
        .unwrap_or(default_value)
}

fn scene_sampled_image_material_constant_named_float(
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    name: &str,
) -> Option<f32> {
    material
        .constant_shader_uniforms
        .iter()
        .find(|uniform| uniform.name == name)
        .and_then(scene_sampled_image_effect_uniform_first_float)
        .or_else(|| {
            material
                .constant_shader_values
                .get(name)
                .and_then(scene_sampled_image_constant_value_float)
        })
}

fn scene_sampled_image_material_has_combo_key(
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    key: &str,
) -> bool {
    material
        .combo_keys
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(key))
}

fn scene_sampled_image_material_combo_value(
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    key: &str,
) -> Option<i64> {
    material
        .combo_values
        .iter()
        .find_map(|(candidate, value)| candidate.eq_ignore_ascii_case(key).then_some(*value))
}

fn scene_sampled_image_material_combo_enabled(
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    key: &str,
) -> bool {
    scene_sampled_image_material_combo_value(material, key)
        .map(|value| value != 0)
        .unwrap_or_else(|| scene_sampled_image_material_has_combo_key(material, key))
}

fn scene_sampled_image_material_constant_vec2(
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    names: &[&str],
    default_value: [f32; 2],
) -> [f32; 2] {
    names
        .iter()
        .find_map(|name| scene_sampled_image_material_constant_named_vec2(material, name))
        .unwrap_or(default_value)
}

fn scene_sampled_image_material_constant_named_vec2(
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    name: &str,
) -> Option<[f32; 2]> {
    material
        .constant_shader_uniforms
        .iter()
        .find(|uniform| uniform.name == name)
        .and_then(scene_sampled_image_effect_uniform_vec2)
        .or_else(|| {
            material
                .constant_shader_values
                .get(name)
                .and_then(scene_sampled_image_constant_value_vec2)
        })
}

fn scene_sampled_image_material_constant_vec3(
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    names: &[&str],
    default_value: [f32; 3],
) -> [f32; 3] {
    names
        .iter()
        .find_map(|name| scene_sampled_image_material_constant_named_vec3(material, name))
        .unwrap_or(default_value)
}

fn scene_sampled_image_material_constant_named_vec3(
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    name: &str,
) -> Option<[f32; 3]> {
    material
        .constant_shader_uniforms
        .iter()
        .find(|uniform| uniform.name == name)
        .and_then(scene_sampled_image_effect_uniform_vec3)
        .or_else(|| {
            material
                .constant_shader_values
                .get(name)
                .and_then(scene_sampled_image_constant_value_vec3)
        })
}

fn scene_sampled_image_constant_value_float(value: &serde_json::Value) -> Option<f32> {
    match value {
        serde_json::Value::Number(value) => {
            let value = value.as_f64()? as f32;
            value.is_finite().then_some(value)
        }
        serde_json::Value::String(value) => {
            let value = value.trim().parse::<f32>().ok()?;
            value.is_finite().then_some(value)
        }
        serde_json::Value::Object(values) => values
            .get("value")
            .and_then(scene_sampled_image_constant_value_float),
        _ => None,
    }
}

fn scene_sampled_image_constant_value_vec3(value: &serde_json::Value) -> Option<[f32; 3]> {
    match value {
        serde_json::Value::Number(_) | serde_json::Value::String(_) => {
            let values = scene_sampled_image_constant_value_float_list(value, 3)?;
            match values.as_slice() {
                [value] => Some([*value, *value, *value]),
                [x, y] => Some([*x, *y, 1.0]),
                [x, y, z, ..] => Some([*x, *y, *z]),
                [] => None,
            }
        }
        serde_json::Value::Array(values) => {
            let mut parsed = Vec::with_capacity(values.len().min(3));
            for value in values.iter().take(3) {
                parsed.push(scene_sampled_image_constant_value_float(value)?);
            }
            match parsed.as_slice() {
                [value] => Some([*value, *value, *value]),
                [x, y] => Some([*x, *y, 1.0]),
                [x, y, z] => Some([*x, *y, *z]),
                _ => None,
            }
        }
        serde_json::Value::Object(values) => values
            .get("value")
            .and_then(scene_sampled_image_constant_value_vec3),
        _ => None,
    }
}

fn scene_sampled_image_constant_value_vec2(value: &serde_json::Value) -> Option<[f32; 2]> {
    match value {
        serde_json::Value::Number(_) | serde_json::Value::String(_) => {
            let values = scene_sampled_image_constant_value_float_list(value, 2)?;
            match values.as_slice() {
                [value] => Some([*value, *value]),
                [x, y, ..] => Some([*x, *y]),
                [] => None,
            }
        }
        serde_json::Value::Array(values) => {
            let mut parsed = Vec::with_capacity(values.len().min(2));
            for value in values.iter().take(2) {
                parsed.push(scene_sampled_image_constant_value_float(value)?);
            }
            match parsed.as_slice() {
                [value] => Some([*value, *value]),
                [x, y] => Some([*x, *y]),
                _ => None,
            }
        }
        serde_json::Value::Object(values) => values
            .get("value")
            .and_then(scene_sampled_image_constant_value_vec2),
        _ => None,
    }
}

fn scene_sampled_image_constant_value_float_list(
    value: &serde_json::Value,
    limit: usize,
) -> Option<Vec<f32>> {
    match value {
        serde_json::Value::Number(_) => {
            scene_sampled_image_constant_value_float(value).map(|value| vec![value])
        }
        serde_json::Value::String(value) => {
            let values: Vec<f32> = value
                .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
                .filter(|part| !part.is_empty())
                .take(limit)
                .map(|part| part.parse::<f32>())
                .collect::<Result<_, _>>()
                .ok()?;
            values
                .iter()
                .all(|value| value.is_finite())
                .then_some(values)
        }
        serde_json::Value::Object(values) => values
            .get("value")
            .and_then(|value| scene_sampled_image_constant_value_float_list(value, limit)),
        _ => None,
    }
}

fn scene_sampled_image_effect_uniform_first_float(
    uniform: &super::present::NativeVulkanVulkanaliaSceneEffectUniform,
) -> Option<f32> {
    if uniform.component_count == 0 {
        return None;
    }
    match uniform.value_kind {
        "float" | "vec2" | "vec3" | "vec4" => Some(f32::from_bits(uniform.float_bits[0])),
        "int" | "bool" => Some(uniform.int_values[0] as f32),
        _ => None,
    }
    .filter(|value| value.is_finite())
}

fn scene_sampled_image_effect_uniform_vec2(
    uniform: &super::present::NativeVulkanVulkanaliaSceneEffectUniform,
) -> Option<[f32; 2]> {
    if uniform.component_count == 0 {
        return None;
    }
    if uniform.component_count == 1 {
        return scene_sampled_image_effect_uniform_first_float(uniform).map(|value| [value, value]);
    }
    match uniform.value_kind {
        "vec2" | "vec3" | "vec4" => {
            let values = [
                f32::from_bits(uniform.float_bits[0]),
                f32::from_bits(uniform.float_bits[1]),
            ];
            values[0]
                .is_finite()
                .then_some(values)
                .filter(|values| values[1].is_finite())
        }
        _ => None,
    }
}

fn scene_sampled_image_effect_uniform_vec3(
    uniform: &super::present::NativeVulkanVulkanaliaSceneEffectUniform,
) -> Option<[f32; 3]> {
    if uniform.component_count == 0 {
        return None;
    }
    if uniform.component_count == 1 {
        return scene_sampled_image_effect_uniform_first_float(uniform)
            .map(|value| [value, value, value]);
    }
    if uniform.component_count == 2 {
        return scene_sampled_image_effect_uniform_vec2(uniform)
            .map(|value| [value[0], value[1], 1.0]);
    }
    match uniform.value_kind {
        "vec3" | "vec4" => {
            let values = [
                f32::from_bits(uniform.float_bits[0]),
                f32::from_bits(uniform.float_bits[1]),
                f32::from_bits(uniform.float_bits[2]),
            ];
            values[0]
                .is_finite()
                .then_some(values)
                .filter(|values| values[1].is_finite() && values[2].is_finite())
        }
        _ => None,
    }
}

fn scene_sampled_image_push_constant_u32(
    push_constant_bytes: &[u8; SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES as usize],
    offset: usize,
) -> u32 {
    u32::from_ne_bytes(push_constant_bytes[offset..offset + 4].try_into().unwrap())
}

fn bind_scene_sampled_image_descriptor_heap_for_descriptor_group(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    descriptor_heap_draw: VulkanaliaSceneDescriptorHeapDrawResources<'_>,
    descriptor_group_base_index: u32,
) -> Result<(), String> {
    let image_index = usize::try_from(descriptor_group_base_index).map_err(|_| {
        format!(
            "scene sampled-image descriptor group base index {descriptor_group_base_index} exceeds usize"
        )
    })?;
    let resource_bind = native_vulkan_vulkanalia_descriptor_heap_resource_bind_info_for_image(
        descriptor_heap_draw.resources,
        image_index,
    )?;
    let sampler_bind = native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info_for_image(
        descriptor_heap_draw.resources,
        image_index,
    )?;
    unsafe {
        device.cmd_bind_resource_heap_ext(command_buffer, &resource_bind);
        device.cmd_bind_sampler_heap_ext(command_buffer, &sampler_bind);
    }
    Ok(())
}

fn set_scene_dynamic_viewport_and_scissor(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    extent: vk::Extent2D,
) {
    let viewport = vk::Viewport::builder()
        .x(0.0)
        .y(0.0)
        .width(extent.width as f32)
        .height(extent.height as f32)
        .min_depth(0.0)
        .max_depth(1.0)
        .build();
    let scissor = vk::Rect2D::builder()
        .offset(vk::Offset2D { x: 0, y: 0 })
        .extent(extent)
        .build();
    unsafe {
        device.cmd_set_viewport(command_buffer, 0, &[viewport]);
        device.cmd_set_scissor(command_buffer, 0, &[scissor]);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneSampledImageActiveRenderingTarget {
    Swapchain,
    EffectTarget(u32),
}

fn scene_color_image_transition(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage_mask: vk::PipelineStageFlags2,
    src_access_mask: vk::AccessFlags2,
    dst_stage_mask: vk::PipelineStageFlags2,
    dst_access_mask: vk::AccessFlags2,
) {
    let barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(src_stage_mask)
        .src_access_mask(src_access_mask)
        .dst_stage_mask(dst_stage_mask)
        .dst_access_mask(dst_access_mask)
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(native_vulkan_vulkanalia_scene_color_subresource_range())
        .build();
    let barriers = [barrier];
    let dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&barriers)
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &dependency);
    }
}

fn begin_scene_color_rendering(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image_view: vk::ImageView,
    extent: vk::Extent2D,
    load_op: vk::AttachmentLoadOp,
    clear_color: [f32; 4],
) {
    let clear_value = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: clear_color,
        },
    };
    let color_attachment = vk::RenderingAttachmentInfo::builder()
        .image_view(image_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(load_op)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear_value)
        .build();
    let color_attachments = [color_attachment];
    let render_area = vk::Rect2D::builder()
        .offset(vk::Offset2D { x: 0, y: 0 })
        .extent(extent)
        .build();
    let rendering_info = vk::RenderingInfo::builder()
        .render_area(render_area)
        .layer_count(1)
        .color_attachments(&color_attachments)
        .build();
    unsafe {
        device.cmd_begin_rendering(command_buffer, &rendering_info);
    }
    set_scene_dynamic_viewport_and_scissor(device, command_buffer, extent);
}

fn end_scene_color_rendering(device: &Device, command_buffer: vk::CommandBuffer) {
    unsafe {
        device.cmd_end_rendering(command_buffer);
    }
}

fn copy_scene_framebuffer_to_snapshot(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    snapshot: &VulkanaliaSceneSampledImageResources,
    extent: vk::Extent2D,
    snapshot_old_layout: vk::ImageLayout,
) {
    scene_color_image_transition(
        device,
        command_buffer,
        swapchain_image,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::AccessFlags2::TRANSFER_READ,
    );
    scene_color_image_transition(
        device,
        command_buffer,
        snapshot.image,
        snapshot_old_layout,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        if snapshot_old_layout == vk::ImageLayout::UNDEFINED {
            vk::PipelineStageFlags2::TOP_OF_PIPE
        } else {
            vk::PipelineStageFlags2::FRAGMENT_SHADER
        },
        if snapshot_old_layout == vk::ImageLayout::UNDEFINED {
            vk::AccessFlags2::empty()
        } else {
            vk::AccessFlags2::SHADER_SAMPLED_READ
        },
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::AccessFlags2::TRANSFER_WRITE,
    );

    let subresource = vk::ImageSubresourceLayers::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1)
        .build();
    let copy = vk::ImageCopy2::builder()
        .src_subresource(subresource)
        .src_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .dst_subresource(subresource)
        .dst_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
        .build();
    let regions = [copy];
    let copy_info = vk::CopyImageInfo2::builder()
        .src_image(swapchain_image)
        .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
        .dst_image(snapshot.image)
        .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .regions(&regions)
        .build();
    unsafe {
        device.cmd_copy_image2(command_buffer, &copy_info);
    }

    scene_color_image_transition(
        device,
        command_buffer,
        snapshot.image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::AccessFlags2::TRANSFER_WRITE,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
    );
    scene_color_image_transition(
        device,
        command_buffer,
        swapchain_image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::AccessFlags2::TRANSFER_READ,
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
    );
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_record_scene_sampled_image_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    swapchain_view: vk::ImageView,
    extent: vk::Extent2D,
    solid_quad_draw: Option<VulkanaliaSceneSolidQuadDrawResources<'_>>,
    descriptor_heap_draw: Option<VulkanaliaSceneDescriptorHeapDrawResources<'_>>,
    pipeline_resources: &VulkanaliaSceneSampledImagePipelineResources,
    draw_commands: &[VulkanaliaSceneSampledImageDrawCommand],
    effect_target_resources: &[VulkanaliaSceneSampledImageResources],
    framebuffer_snapshot_resource: Option<&VulkanaliaSceneSampledImageResources>,
    framebuffer_snapshot_initial_layout: vk::ImageLayout,
    vertex_buffer: vk::Buffer,
    index_buffer: vk::Buffer,
    clear_color: [f32; 4],
    elapsed_ms: u64,
) -> Result<NativeVulkanVulkanaliaSceneSampledImageCommandSnapshot, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene sampled-image command requires non-zero extent".to_owned());
    }
    if draw_commands.is_empty() {
        return Err("scene sampled-image command requires at least one draw".to_owned());
    }
    if let Some(draw) = solid_quad_draw {
        if draw.draw_commands.is_empty() {
            return Err("scene mixed command requires non-empty solid draw steps".to_owned());
        }
        for solid_draw in draw.draw_commands {
            if solid_draw.index_count == 0 {
                return Err("scene mixed command requires non-empty solid draw indices".to_owned());
            }
        }
    }
    for draw in draw_commands {
        if draw.index_count == 0 {
            return Err("scene sampled-image draw requires at least one index".to_owned());
        }
        if let VulkanaliaSceneSampledImageRenderTarget::EffectTarget { target_index, .. } =
            draw.render_target
            && target_index as usize >= effect_target_resources.len()
        {
            return Err(format!(
                "scene sampled-image draw effect target index {target_index} exceeds effect target resource count {}",
                effect_target_resources.len()
            ));
        }
        match &draw.descriptor_binding {
            VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                descriptor_group_base_index,
                texture_slot_bindings,
            } => {
                let Some(descriptor_heap_draw) = descriptor_heap_draw else {
                    return Err(
                        "scene sampled-image descriptor heap draw requires heap resources"
                            .to_owned(),
                    );
                };
                let descriptor_group_end = *descriptor_group_base_index as usize
                    + SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT;
                if descriptor_group_end > descriptor_heap_draw.resources.plan.image_count {
                    return Err(format!(
                        "scene sampled-image descriptor heap group {}..{} exceeds heap image count {}",
                        descriptor_group_base_index,
                        descriptor_group_end,
                        descriptor_heap_draw.resources.plan.image_count
                    ));
                }
                if texture_slot_bindings.is_empty()
                    || texture_slot_bindings.len() > SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT
                {
                    return Err(format!(
                        "scene sampled-image texture slot count {} exceeds descriptor binding count {}",
                        texture_slot_bindings.len(),
                        SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT
                    ));
                }
                if let Some(alpha_texture_slot) = draw.material.alpha_texture_slot
                    && !texture_slot_bindings
                        .iter()
                        .any(|binding| binding.slot == alpha_texture_slot)
                {
                    return Err(format!(
                        "scene sampled-image alpha texture slot {alpha_texture_slot} has no resource binding"
                    ));
                }
            }
        }
    }
    if descriptor_heap_draw.is_none() {
        return Err("scene sampled-image command requires descriptor heap resources".to_owned());
    }
    if let Some(draw) = solid_quad_draw
        && let Some(descriptor_group_base_index) =
            draw.framebuffer_snapshot_descriptor_group_base_index
    {
        let descriptor_heap_draw = descriptor_heap_draw.expect("descriptor heap checked above");
        let descriptor_group_end = descriptor_group_base_index as usize + 1;
        if descriptor_group_end > descriptor_heap_draw.resources.plan.image_count {
            return Err(format!(
                "scene solid passthroughblend descriptor group {}..{} exceeds heap image count {}",
                descriptor_group_base_index,
                descriptor_group_end,
                descriptor_heap_draw.resources.plan.image_count
            ));
        }
    }
    if solid_quad_draw.is_some_and(|draw| {
        draw.framebuffer_snapshot_descriptor_group_base_index
            .is_some()
            && draw
                .draw_commands
                .iter()
                .any(|draw| draw.blend.mode == SceneBlendMode::HslColor)
    }) && framebuffer_snapshot_resource.is_none()
    {
        return Err(
            "scene solid passthroughblend command requires framebuffer snapshot resource"
                .to_owned(),
        );
    }
    if draw_commands.iter().any(|draw| {
        scene_sampled_image_shader_program(&draw.material)
            == VulkanaliaSceneSampledImageShaderProgram::PassthroughBlend
    }) && framebuffer_snapshot_resource.is_none()
    {
        return Err(
            "scene sampled-image passthroughblend command requires framebuffer snapshot resource"
                .to_owned(),
        );
    }
    if !draw_commands
        .iter()
        .any(|draw| draw.render_target == VulkanaliaSceneSampledImageRenderTarget::Swapchain)
    {
        return Err("scene sampled-image command requires at least one swapchain draw".to_owned());
    }

    let solid_draw_commands: &[VulkanaliaSceneSolidQuadDrawCommand] =
        solid_quad_draw.map_or(&[], |draw| draw.draw_commands);
    let ordered_draws =
        native_vulkan_vulkanalia_scene_ordered_draw_steps(solid_draw_commands, draw_commands);
    let solid_passthroughblend_draw_count = solid_quad_draw.map_or(0usize, |draw| {
        if draw
            .framebuffer_snapshot_descriptor_group_base_index
            .is_none()
        {
            0
        } else {
            draw.draw_commands
                .iter()
                .filter(|draw| draw.blend.mode == SceneBlendMode::HslColor)
                .count()
        }
    });
    let sampled_passthroughblend_draw_count = draw_commands
        .iter()
        .filter(|draw| {
            scene_sampled_image_shader_program(&draw.material)
                == VulkanaliaSceneSampledImageShaderProgram::PassthroughBlend
        })
        .count();
    let framebuffer_snapshot_copy_count =
        solid_passthroughblend_draw_count.saturating_add(sampled_passthroughblend_draw_count);
    let framebuffer_snapshot_required = framebuffer_snapshot_copy_count > 0;

    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| {
                format!("vkResetCommandBuffer(vulkanalia scene sampled image): {err:?}")
            })?;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::empty())
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| {
                format!("vkBeginCommandBuffer(vulkanalia scene sampled image): {err:?}")
            })?;

        let solid_push_constants = [extent.width as f32, extent.height as f32];
        let solid_push_constant_bytes = std::slice::from_raw_parts(
            solid_push_constants.as_ptr().cast::<u8>(),
            SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES as usize,
        );
        let mut active_target: Option<SceneSampledImageActiveRenderingTarget> = None;
        let mut active_extent = extent;
        let mut swapchain_started = false;
        let mut framebuffer_snapshot_layout = framebuffer_snapshot_initial_layout;
        let mut bound_pipeline: Option<VulkanaliaSceneBoundDrawPipeline> = None;
        let mut bound_descriptor_heap_group: Option<u32> = None;
        for draw in &ordered_draws {
            let desired_target = match draw.pipeline {
                VulkanaliaSceneOrderedDrawPipeline::SolidQuad => {
                    SceneSampledImageActiveRenderingTarget::Swapchain
                }
                VulkanaliaSceneOrderedDrawPipeline::SampledImage => {
                    match draw_commands[draw.command_index].render_target {
                        VulkanaliaSceneSampledImageRenderTarget::Swapchain => {
                            SceneSampledImageActiveRenderingTarget::Swapchain
                        }
                        VulkanaliaSceneSampledImageRenderTarget::EffectTarget {
                            target_index,
                            ..
                        } => SceneSampledImageActiveRenderingTarget::EffectTarget(target_index),
                    }
                }
            };
            if active_target != Some(desired_target) {
                if let Some(current_target) = active_target.take() {
                    end_scene_color_rendering(device, command_buffer);
                    if let SceneSampledImageActiveRenderingTarget::EffectTarget(target_index) =
                        current_target
                    {
                        let target = &effect_target_resources[target_index as usize];
                        scene_color_image_transition(
                            device,
                            command_buffer,
                            target.image,
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                            vk::PipelineStageFlags2::FRAGMENT_SHADER,
                            vk::AccessFlags2::SHADER_SAMPLED_READ,
                        );
                    } else {
                        scene_color_image_transition(
                            device,
                            command_buffer,
                            swapchain_image,
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::COLOR_ATTACHMENT_READ
                                | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                        );
                    }
                    bound_pipeline = None;
                    bound_descriptor_heap_group = None;
                }

                match desired_target {
                    SceneSampledImageActiveRenderingTarget::Swapchain => {
                        let load_op = if swapchain_started {
                            vk::AttachmentLoadOp::LOAD
                        } else {
                            scene_color_image_transition(
                                device,
                                command_buffer,
                                swapchain_image,
                                vk::ImageLayout::UNDEFINED,
                                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                                vk::PipelineStageFlags2::TOP_OF_PIPE,
                                vk::AccessFlags2::empty(),
                                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                            );
                            swapchain_started = true;
                            vk::AttachmentLoadOp::CLEAR
                        };
                        active_extent = extent;
                        begin_scene_color_rendering(
                            device,
                            command_buffer,
                            swapchain_view,
                            active_extent,
                            load_op,
                            clear_color,
                        );
                    }
                    SceneSampledImageActiveRenderingTarget::EffectTarget(target_index) => {
                        let sampled_draw = &draw_commands[draw.command_index];
                        let VulkanaliaSceneSampledImageRenderTarget::EffectTarget { clear, .. } =
                            sampled_draw.render_target
                        else {
                            unreachable!("desired effect target came from sampled draw target");
                        };
                        let target = &effect_target_resources[target_index as usize];
                        active_extent = vk::Extent2D {
                            width: target.snapshot.extent.0,
                            height: target.snapshot.extent.1,
                        };
                        scene_color_image_transition(
                            device,
                            command_buffer,
                            target.image,
                            if clear {
                                vk::ImageLayout::UNDEFINED
                            } else {
                                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                            },
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            if clear {
                                vk::PipelineStageFlags2::TOP_OF_PIPE
                            } else {
                                vk::PipelineStageFlags2::FRAGMENT_SHADER
                            },
                            if clear {
                                vk::AccessFlags2::empty()
                            } else {
                                vk::AccessFlags2::SHADER_SAMPLED_READ
                            },
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                        );
                        begin_scene_color_rendering(
                            device,
                            command_buffer,
                            target.image_view,
                            active_extent,
                            if clear {
                                vk::AttachmentLoadOp::CLEAR
                            } else {
                                vk::AttachmentLoadOp::LOAD
                            },
                            [0.0, 0.0, 0.0, 0.0],
                        );
                    }
                }
                active_target = Some(desired_target);
            }

            match draw.pipeline {
                VulkanaliaSceneOrderedDrawPipeline::SolidQuad => {
                    let solid_draw = &solid_draw_commands[draw.command_index];
                    let solid_resources = solid_quad_draw
                        .as_ref()
                        .expect("solid draw resources present");
                    let solid_framebuffer_passthrough = solid_draw.blend.mode
                        == SceneBlendMode::HslColor
                        && solid_resources
                            .framebuffer_snapshot_descriptor_group_base_index
                            .is_some();
                    if solid_framebuffer_passthrough {
                        if active_target != Some(SceneSampledImageActiveRenderingTarget::Swapchain)
                        {
                            return Err(
                                "scene solid passthroughblend must execute on swapchain target"
                                    .to_owned(),
                            );
                        }
                        let snapshot = framebuffer_snapshot_resource.expect(
                            "solid passthroughblend snapshot resource checked before command recording",
                        );
                        end_scene_color_rendering(device, command_buffer);
                        copy_scene_framebuffer_to_snapshot(
                            device,
                            command_buffer,
                            swapchain_image,
                            snapshot,
                            extent,
                            framebuffer_snapshot_layout,
                        );
                        framebuffer_snapshot_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
                        begin_scene_color_rendering(
                            device,
                            command_buffer,
                            swapchain_view,
                            extent,
                            vk::AttachmentLoadOp::LOAD,
                            clear_color,
                        );
                        active_target = Some(SceneSampledImageActiveRenderingTarget::Swapchain);
                        active_extent = extent;
                        bound_pipeline = None;
                        bound_descriptor_heap_group = None;
                    }
                    let pipeline_key =
                        VulkanaliaSceneBoundDrawPipeline::SolidQuad(solid_draw.blend);
                    if bound_pipeline != Some(pipeline_key) {
                        let pipeline = if solid_framebuffer_passthrough {
                            solid_resources
                                .pipeline_resources
                                .hsl_color_passthrough_pipeline
                                .ok_or_else(|| {
                                    "scene solid passthroughblend pipeline was not created"
                                        .to_owned()
                                })?
                        } else {
                            native_vulkan_vulkanalia_scene_solid_quad_pipeline(
                                solid_resources.pipeline_resources,
                                solid_draw.blend.mode,
                            )
                        };
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipeline,
                        );
                        let vertex_buffers = [solid_resources.vertex_buffer];
                        let vertex_offsets = [0u64];
                        device.cmd_bind_vertex_buffers(
                            command_buffer,
                            0,
                            &vertex_buffers,
                            &vertex_offsets,
                        );
                        device.cmd_bind_index_buffer(
                            command_buffer,
                            solid_resources.index_buffer,
                            0,
                            vk::IndexType::UINT32,
                        );
                        device.cmd_push_constants(
                            command_buffer,
                            solid_resources.pipeline_resources.pipeline_layout,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                            0,
                            solid_push_constant_bytes,
                        );
                        bound_pipeline = Some(pipeline_key);
                    }
                    if let Some(descriptor_group_base_index) = solid_resources
                        .framebuffer_snapshot_descriptor_group_base_index
                        .filter(|_| solid_framebuffer_passthrough)
                        && bound_descriptor_heap_group != Some(descriptor_group_base_index)
                    {
                        let descriptor_heap_draw =
                            descriptor_heap_draw.expect("descriptor heap draw resources present");
                        bind_scene_sampled_image_descriptor_heap_for_descriptor_group(
                            device,
                            command_buffer,
                            descriptor_heap_draw,
                            descriptor_group_base_index,
                        )?;
                        bound_descriptor_heap_group = Some(descriptor_group_base_index);
                    }
                    device.cmd_draw_indexed(
                        command_buffer,
                        solid_draw.index_count,
                        1,
                        solid_draw.first_index,
                        0,
                        0,
                    );
                }
                VulkanaliaSceneOrderedDrawPipeline::SampledImage => {
                    let sampled_draw = &draw_commands[draw.command_index];
                    if scene_sampled_image_shader_program(&sampled_draw.material)
                        == VulkanaliaSceneSampledImageShaderProgram::PassthroughBlend
                    {
                        if active_target != Some(SceneSampledImageActiveRenderingTarget::Swapchain)
                        {
                            return Err(
                                "scene sampled-image passthroughblend must execute on swapchain target"
                                    .to_owned(),
                            );
                        }
                        let snapshot = framebuffer_snapshot_resource.expect(
                            "passthroughblend snapshot resource checked before command recording",
                        );
                        end_scene_color_rendering(device, command_buffer);
                        copy_scene_framebuffer_to_snapshot(
                            device,
                            command_buffer,
                            swapchain_image,
                            snapshot,
                            extent,
                            framebuffer_snapshot_layout,
                        );
                        framebuffer_snapshot_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
                        begin_scene_color_rendering(
                            device,
                            command_buffer,
                            swapchain_view,
                            extent,
                            vk::AttachmentLoadOp::LOAD,
                            clear_color,
                        );
                        active_target = Some(SceneSampledImageActiveRenderingTarget::Swapchain);
                        active_extent = extent;
                        bound_pipeline = None;
                        bound_descriptor_heap_group = None;
                    }
                    let pipeline_key = VulkanaliaSceneBoundDrawPipeline::SampledImage {
                        blend: sampled_draw.material.render_state.blend,
                        shader_program: scene_sampled_image_shader_program(&sampled_draw.material),
                    };
                    if bound_pipeline != Some(pipeline_key) {
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            native_vulkan_vulkanalia_scene_sampled_image_pipeline_for_material(
                                pipeline_resources,
                                &sampled_draw.material,
                            ),
                        );
                        let vertex_buffers = [vertex_buffer];
                        let vertex_offsets = [0u64];
                        device.cmd_bind_vertex_buffers(
                            command_buffer,
                            0,
                            &vertex_buffers,
                            &vertex_offsets,
                        );
                        device.cmd_bind_index_buffer(
                            command_buffer,
                            index_buffer,
                            0,
                            vk::IndexType::UINT32,
                        );
                        bound_pipeline = Some(pipeline_key);
                    }
                    let VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                        descriptor_group_base_index,
                        ..
                    } = &sampled_draw.descriptor_binding;
                    if bound_descriptor_heap_group != Some(*descriptor_group_base_index) {
                        let descriptor_heap_draw =
                            descriptor_heap_draw.expect("descriptor heap draw resources present");
                        bind_scene_sampled_image_descriptor_heap_for_descriptor_group(
                            device,
                            command_buffer,
                            descriptor_heap_draw,
                            *descriptor_group_base_index,
                        )?;
                        bound_descriptor_heap_group = Some(*descriptor_group_base_index);
                    }
                    push_scene_sampled_image_constants(
                        device,
                        command_buffer,
                        pipeline_resources.pipeline_layout,
                        active_extent,
                        &sampled_draw.material,
                        elapsed_ms,
                    );
                    device.cmd_draw_indexed(
                        command_buffer,
                        sampled_draw.index_count,
                        1,
                        sampled_draw.first_index,
                        0,
                        0,
                    );
                }
            }
        }
        if let Some(current_target) = active_target.take() {
            end_scene_color_rendering(device, command_buffer);
            if let SceneSampledImageActiveRenderingTarget::EffectTarget(target_index) =
                current_target
            {
                let target = &effect_target_resources[target_index as usize];
                scene_color_image_transition(
                    device,
                    command_buffer,
                    target.image,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    vk::PipelineStageFlags2::FRAGMENT_SHADER,
                    vk::AccessFlags2::SHADER_SAMPLED_READ,
                );
            }
        }

        scene_color_image_transition(
            device,
            command_buffer,
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
            vk::AccessFlags2::empty(),
        );

        device.end_command_buffer(command_buffer).map_err(|err| {
            format!("vkEndCommandBuffer(vulkanalia scene sampled image): {err:?}")
        })?;
    }

    let descriptor_set_bind_count = 0;
    let push_descriptor_set_recorded_count = 0;
    let sampled_descriptor_heap_draw_count = draw_commands
        .iter()
        .filter(|draw| {
            matches!(
                draw.descriptor_binding,
                VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap { .. }
            )
        })
        .count();
    let descriptor_heap_draw_count = saturating_u32(
        sampled_descriptor_heap_draw_count.saturating_add(solid_passthroughblend_draw_count),
    );
    let framebuffer_snapshot_copy_count = saturating_u32(framebuffer_snapshot_copy_count);
    let solid_passthroughblend_draw_count = saturating_u32(solid_passthroughblend_draw_count);
    let sampled_passthroughblend_draw_count = saturating_u32(sampled_passthroughblend_draw_count);
    let sampled_image_index_count = draw_commands
        .iter()
        .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count));
    let solid_quad_index_count = solid_quad_draw.map_or(0, |draw| {
        draw.draw_commands
            .iter()
            .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count))
    });
    let solid_quad_draw_call_count =
        solid_quad_draw.map_or(0, |draw| saturating_u32(draw.draw_commands.len()));
    let sampled_image_draw_call_count = saturating_u32(draw_commands.len());
    let draw_call_count = solid_quad_draw_call_count.saturating_add(sampled_image_draw_call_count);
    let mut last_pipeline = None;
    let mut pipeline_bind_count = 0u32;
    for draw in &ordered_draws {
        let pipeline_key = native_vulkan_vulkanalia_scene_bound_pipeline_key(
            draw,
            solid_quad_draw.map_or(&[], |draw| draw.draw_commands),
            draw_commands,
        );
        if last_pipeline != Some(pipeline_key) {
            pipeline_bind_count = pipeline_bind_count.saturating_add(1);
            last_pipeline = Some(pipeline_key);
        }
    }

    Ok(NativeVulkanVulkanaliaSceneSampledImageCommandSnapshot {
        binding: "vulkanalia",
        route: if solid_quad_draw.is_some() {
            "scene-mixed-quad-sampled-image-dynamic-rendering-command-buffer"
        } else {
            "scene-sampled-image-dynamic-rendering-command-buffer"
        },
        extent: (extent.width, extent.height),
        index_count: solid_quad_index_count.saturating_add(sampled_image_index_count),
        command_buffer_recorded: true,
        vertex_buffer_bound: true,
        index_buffer_bound: true,
        draw_call_count,
        solid_quad_draw_call_count,
        sampled_image_draw_call_count,
        pipeline_bind_count,
        descriptor_set_bound: descriptor_set_bind_count > 0,
        push_descriptor_set_recorded: push_descriptor_set_recorded_count > 0,
        descriptor_heap_bound: descriptor_heap_draw.is_some() && descriptor_heap_draw_count > 0,
        descriptor_set_bind_count,
        push_descriptor_set_recorded_count,
        descriptor_heap_draw_count,
        framebuffer_snapshot_required,
        framebuffer_snapshot_copy_count,
        solid_passthroughblend_draw_count,
        sampled_passthroughblend_draw_count,
        solid_framebuffer_snapshot_descriptor_group_base_index: solid_quad_draw
            .and_then(|draw| draw.framebuffer_snapshot_descriptor_group_base_index),
        descriptor_model: "VK_EXT_descriptor_heap",
        push_constant_bytes: SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES,
        swapchain_layout_transition: "undefined -> color-attachment-optimal -> present-src-khr",
        sampled_image_layout: "shader-read-only-optimal",
        render_model: if solid_quad_draw.is_some() {
            "scene solid quad buffers then sampled image buffers/descriptor heap -> dynamic rendering with framebuffer snapshot sampling where WE passthroughblend requires it -> Wayland swapchain"
        } else {
            "scene sampled image vertex/index buffers + VK_EXT_descriptor_heap combined-image-sampler mapping -> dynamic rendering indexed draw, with framebuffer snapshot sampling where WE passthroughblend requires it -> Wayland swapchain"
        },
        command_order: native_vulkan_vulkanalia_scene_draw_pass_command_order(
            false,
            true,
            false,
            solid_quad_draw.is_some(),
        )
        .to_vec(),
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
    })
}

fn native_vulkan_vulkanalia_scene_draw_pass_command_order(
    solid_quad_ready: bool,
    sampled_image_pending: bool,
    fast_clear_color_ready: bool,
    mixed_quad_sampled_image_ready: bool,
) -> &'static [&'static str] {
    if mixed_quad_sampled_image_ready {
        &[
            "cmd_pipeline_barrier2_swapchain_attachment",
            "cmd_begin_rendering",
            "cmd_bind_scene_solid_quad_pipeline_as_needed",
            "cmd_bind_scene_sampled_image_pipeline_as_needed",
            "cmd_bind_scene_geometry_for_next_layer",
            "cmd_bind_scene_descriptor_heap_when_needed",
            "cmd_draw_indexed_in_scene_layer_order",
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
        ]
    } else if solid_quad_ready {
        &[
            "cmd_pipeline_barrier2_swapchain_attachment",
            "cmd_begin_rendering",
            "cmd_bind_scene_solid_quad_pipeline",
            "cmd_bind_scene_vertex_buffer",
            "cmd_bind_scene_index_buffer",
            "cmd_draw_indexed_per_quad",
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
        ]
    } else if sampled_image_pending {
        &[
            "cmd_pipeline_barrier2_swapchain_attachment",
            "cmd_begin_rendering",
            "cmd_bind_scene_sampled_image_pipeline",
            "cmd_bind_sampled_image_vertex_buffer",
            "cmd_bind_sampled_image_index_buffer",
            "cmd_bind_scene_descriptor_heap",
            "cmd_draw_indexed_per_image_quad",
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
        ]
    } else if fast_clear_color_ready {
        &["delegate_to_vulkanalia_clear_present"]
    } else {
        &["wait_for_scene_recordable_draw_ops"]
    }
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

fn native_vulkan_vulkanalia_scene_create_shader_module(
    device: &Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, String> {
    if code.first().copied() != Some(0x0723_0203) {
        return Err(format!("{label} shader is not valid SPIR-V bytecode"));
    }
    let create_info = vk::ShaderModuleCreateInfo::builder()
        .code(code)
        .code_size(native_vulkan_vulkanalia_scene_shader_code_size_bytes(code));
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("vkCreateShaderModule(vulkanalia {label}): {err:?}"))
}

fn native_vulkan_vulkanalia_scene_shader_code_size_bytes(code: &[u32]) -> usize {
    std::mem::size_of_val(code)
}

fn native_vulkan_vulkanalia_scene_color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_VERTEX_SPIRV: [u32; 379] = [
    119734787, 65536, 524299, 54, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 589839, 0, 4, 1852399981, 0, 11, 42, 50, 52, 196611, 2, 450, 262149, 4,
    1852399981, 0, 327685, 9, 1836216174, 2053729377, 25701, 262149, 11, 1885302377, 29551, 393221,
    13, 1852138323, 1953057893, 1937068133, 104, 327686, 13, 0, 1702131813, 29806, 196613, 15,
    25456, 196613, 22, 6513774, 393221, 40, 1348430951, 1700164197, 2019914866, 0, 393222, 40, 0,
    1348430951, 1953067887, 7237481, 458758, 40, 1, 1348430951, 1953393007, 1702521171, 0, 458758,
    40, 2, 1130327143, 1148217708, 1635021673, 6644590, 458758, 40, 3, 1130327143, 1147956341,
    1635021673, 6644590, 196613, 42, 0, 262149, 50, 1868783478, 7499628, 327685, 52, 1667198569,
    1919904879, 0, 262215, 11, 30, 0, 196679, 13, 2, 327752, 13, 0, 35, 0, 196679, 40, 2, 327752,
    40, 0, 11, 0, 327752, 40, 1, 11, 1, 327752, 40, 2, 11, 3, 327752, 40, 3, 11, 4, 262215, 50, 30,
    0, 262215, 52, 30, 1, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 2, 262176, 8, 7, 7,
    262176, 10, 1, 7, 262203, 10, 11, 1, 196638, 13, 7, 262176, 14, 9, 13, 262203, 14, 15, 9,
    262165, 16, 32, 1, 262187, 16, 17, 0, 262176, 18, 9, 7, 262165, 23, 32, 0, 262187, 23, 24, 0,
    262176, 25, 7, 6, 262187, 6, 28, 1073741824, 262187, 6, 30, 1065353216, 262187, 23, 32, 1,
    262167, 38, 6, 4, 262172, 39, 6, 32, 393246, 40, 38, 6, 39, 39, 262176, 41, 3, 40, 262203, 41,
    42, 3, 262187, 6, 44, 0, 262176, 48, 3, 38, 262203, 48, 50, 3, 262176, 51, 1, 38, 262203, 51,
    52, 1, 327734, 2, 4, 0, 3, 131320, 5, 262203, 8, 9, 7, 262203, 8, 22, 7, 262205, 7, 12, 11,
    327745, 18, 19, 15, 17, 262205, 7, 20, 19, 327816, 7, 21, 12, 20, 196670, 9, 21, 327745, 25,
    26, 9, 24, 262205, 6, 27, 26, 327813, 6, 29, 27, 28, 327811, 6, 31, 29, 30, 327745, 25, 33, 9,
    32, 262205, 6, 34, 33, 327813, 6, 35, 34, 28, 327811, 6, 36, 30, 35, 327760, 7, 37, 31, 36,
    196670, 22, 37, 262205, 7, 43, 22, 327761, 6, 45, 43, 0, 327761, 6, 46, 43, 1, 458832, 38, 47,
    45, 46, 44, 30, 327745, 48, 49, 42, 17, 196670, 49, 47, 262205, 38, 53, 52, 196670, 50, 53,
    65789, 65592,
];

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_FRAGMENT_SPIRV: [u32; 94] = [
    119734787, 65536, 524299, 13, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 458767, 4, 4, 1852399981, 0, 9, 11, 196624, 4, 7, 196611, 2, 450, 262149, 4,
    1852399981, 0, 327685, 9, 1601467759, 1869377379, 114, 262149, 11, 1868783478, 7499628, 262215,
    9, 30, 0, 262215, 11, 30, 0, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 4, 262176,
    8, 3, 7, 262203, 8, 9, 3, 262176, 10, 1, 7, 262203, 10, 11, 1, 327734, 2, 4, 0, 3, 131320, 5,
    262205, 7, 12, 11, 196670, 9, 12, 65789, 65592,
];

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_PREMULTIPLIED_FRAGMENT_SPIRV: [u32; 164] = [
    119734787, 65536, 524299, 27, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 458767, 4, 4, 1852399981, 0, 9, 11, 196624, 4, 7, 196611, 2, 450, 262149, 4,
    1852399981, 0, 327685, 9, 1601467759, 1869377379, 114, 327685, 11, 1667198569, 1919904879, 0,
    262215, 9, 30, 0, 262215, 11, 30, 0, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 4,
    262176, 8, 3, 7, 262203, 8, 9, 3, 262176, 10, 1, 7, 262203, 10, 11, 1, 262167, 12, 6, 3,
    262165, 15, 32, 0, 262187, 15, 16, 3, 262176, 17, 1, 6, 327734, 2, 4, 0, 3, 131320, 5, 262205,
    7, 13, 11, 524367, 12, 14, 13, 13, 0, 1, 2, 327745, 17, 18, 11, 16, 262205, 6, 19, 18, 327822,
    12, 20, 14, 19, 327745, 17, 21, 11, 16, 262205, 6, 22, 21, 327761, 6, 23, 20, 0, 327761, 6, 24,
    20, 1, 327761, 6, 25, 20, 2, 458832, 7, 26, 23, 24, 25, 22, 196670, 9, 26, 65789, 65592,
];

const NATIVE_VULKAN_VULKANALIA_SCENE_SOLID_QUAD_PASSTHROUGHBLEND_FRAGMENT_SPIRV: [u32; 1785] =
    include!("shaders/solid_quad_passthroughblend.frag.spv.rs");

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_VERTEX_SPIRV: [u32; 506] = [
    0x07230203, 0x00010000, 0x0008000b, 0x0000003d, 0x00000000, 0x00020011, 0x00000001, 0x0006000b,
    0x00000001, 0x4c534c47, 0x6474732e, 0x3035342e, 0x00000000, 0x0003000e, 0x00000000, 0x00000001,
    0x000f000f, 0x00000000, 0x00000004, 0x6e69616d, 0x00000000, 0x0000000b, 0x0000001c, 0x0000002e,
    0x0000002f, 0x00000031, 0x00000032, 0x00000035, 0x00000037, 0x00000039, 0x0000003b, 0x00030003,
    0x00000002, 0x000001c2, 0x00040005, 0x00000004, 0x6e69616d, 0x00000000, 0x00050005, 0x00000009,
    0x6d726f6e, 0x7a696c61, 0x00006465, 0x00050005, 0x0000000b, 0x705f6e69, 0x7469736f, 0x006e6f69,
    0x00050005, 0x0000000e, 0x6e656353, 0x73755065, 0x00000068, 0x00050006, 0x0000000e, 0x00000000,
    0x65747865, 0x0000746e, 0x00080006, 0x0000000e, 0x00000001, 0x68706c61, 0x65745f61, 0x72757478,
    0x6c735f65, 0x0000746f, 0x00080006, 0x0000000e, 0x00000002, 0x68706c61, 0x65745f61, 0x72757478,
    0x6f6d5f65, 0x00006564, 0x00070006, 0x0000000e, 0x00000003, 0x656d6974, 0x6365735f, 0x73646e6f,
    0x00000000, 0x00030005, 0x00000010, 0x00006370, 0x00060005, 0x0000001a, 0x505f6c67, 0x65567265,
    0x78657472, 0x00000000, 0x00060006, 0x0000001a, 0x00000000, 0x505f6c67, 0x7469736f, 0x006e6f69,
    0x00070006, 0x0000001a, 0x00000001, 0x505f6c67, 0x746e696f, 0x657a6953, 0x00000000, 0x00070006,
    0x0000001a, 0x00000002, 0x435f6c67, 0x4470696c, 0x61747369, 0x0065636e, 0x00070006, 0x0000001a,
    0x00000003, 0x435f6c67, 0x446c6c75, 0x61747369, 0x0065636e, 0x00030005, 0x0000001c, 0x00000000,
    0x00040005, 0x0000002e, 0x76755f76, 0x00000000, 0x00040005, 0x0000002f, 0x755f6e69, 0x00000076,
    0x00050005, 0x00000031, 0x66655f76, 0x74636566, 0x0076755f, 0x00060005, 0x00000032, 0x655f6e69,
    0x63656666, 0x76755f74, 0x00000000, 0x00050005, 0x00000035, 0x706f5f76, 0x74696361, 0x00000079,
    0x00050005, 0x00000037, 0x6f5f6e69, 0x69636170, 0x00007974, 0x00040005, 0x00000039, 0x69745f76,
    0x0000746e, 0x00040005, 0x0000003b, 0x745f6e69, 0x00746e69, 0x00040047, 0x0000000b, 0x0000001e,
    0x00000000, 0x00030047, 0x0000000e, 0x00000002, 0x00050048, 0x0000000e, 0x00000000, 0x00000023,
    0x00000000, 0x00050048, 0x0000000e, 0x00000001, 0x00000023, 0x00000008, 0x00050048, 0x0000000e,
    0x00000002, 0x00000023, 0x0000000c, 0x00050048, 0x0000000e, 0x00000003, 0x00000023, 0x00000010,
    0x00030047, 0x0000001a, 0x00000002, 0x00050048, 0x0000001a, 0x00000000, 0x0000000b, 0x00000000,
    0x00050048, 0x0000001a, 0x00000001, 0x0000000b, 0x00000001, 0x00050048, 0x0000001a, 0x00000002,
    0x0000000b, 0x00000003, 0x00050048, 0x0000001a, 0x00000003, 0x0000000b, 0x00000004, 0x00040047,
    0x0000002e, 0x0000001e, 0x00000000, 0x00040047, 0x0000002f, 0x0000001e, 0x00000001, 0x00040047,
    0x00000031, 0x0000001e, 0x00000001, 0x00040047, 0x00000032, 0x0000001e, 0x00000002, 0x00040047,
    0x00000035, 0x0000001e, 0x00000002, 0x00040047, 0x00000037, 0x0000001e, 0x00000003, 0x00040047,
    0x00000039, 0x0000001e, 0x00000003, 0x00040047, 0x0000003b, 0x0000001e, 0x00000004, 0x00020013,
    0x00000002, 0x00030021, 0x00000003, 0x00000002, 0x00030016, 0x00000006, 0x00000020, 0x00040017,
    0x00000007, 0x00000006, 0x00000002, 0x00040020, 0x00000008, 0x00000007, 0x00000007, 0x00040020,
    0x0000000a, 0x00000001, 0x00000007, 0x0004003b, 0x0000000a, 0x0000000b, 0x00000001, 0x00040015,
    0x0000000d, 0x00000020, 0x00000000, 0x0006001e, 0x0000000e, 0x00000007, 0x0000000d, 0x0000000d,
    0x00000006, 0x00040020, 0x0000000f, 0x00000009, 0x0000000e, 0x0004003b, 0x0000000f, 0x00000010,
    0x00000009, 0x00040015, 0x00000011, 0x00000020, 0x00000001, 0x0004002b, 0x00000011, 0x00000012,
    0x00000000, 0x00040020, 0x00000013, 0x00000009, 0x00000007, 0x00040017, 0x00000017, 0x00000006,
    0x00000004, 0x0004002b, 0x0000000d, 0x00000018, 0x00000001, 0x0004001c, 0x00000019, 0x00000006,
    0x00000018, 0x0006001e, 0x0000001a, 0x00000017, 0x00000006, 0x00000019, 0x00000019, 0x00040020,
    0x0000001b, 0x00000003, 0x0000001a, 0x0004003b, 0x0000001b, 0x0000001c, 0x00000003, 0x0004002b,
    0x0000000d, 0x0000001d, 0x00000000, 0x00040020, 0x0000001e, 0x00000007, 0x00000006, 0x0004002b,
    0x00000006, 0x00000021, 0x40000000, 0x0004002b, 0x00000006, 0x00000023, 0x3f800000, 0x0004002b,
    0x00000006, 0x00000029, 0x00000000, 0x00040020, 0x0000002b, 0x00000003, 0x00000017, 0x00040020,
    0x0000002d, 0x00000003, 0x00000007, 0x0004003b, 0x0000002d, 0x0000002e, 0x00000003, 0x0004003b,
    0x0000000a, 0x0000002f, 0x00000001, 0x0004003b, 0x0000002d, 0x00000031, 0x00000003, 0x0004003b,
    0x0000000a, 0x00000032, 0x00000001, 0x00040020, 0x00000034, 0x00000003, 0x00000006, 0x0004003b,
    0x00000034, 0x00000035, 0x00000003, 0x00040020, 0x00000036, 0x00000001, 0x00000006, 0x0004003b,
    0x00000036, 0x00000037, 0x00000001, 0x0004003b, 0x0000002b, 0x00000039, 0x00000003, 0x00040020,
    0x0000003a, 0x00000001, 0x00000017, 0x0004003b, 0x0000003a, 0x0000003b, 0x00000001, 0x00050036,
    0x00000002, 0x00000004, 0x00000000, 0x00000003, 0x000200f8, 0x00000005, 0x0004003b, 0x00000008,
    0x00000009, 0x00000007, 0x0004003d, 0x00000007, 0x0000000c, 0x0000000b, 0x00050041, 0x00000013,
    0x00000014, 0x00000010, 0x00000012, 0x0004003d, 0x00000007, 0x00000015, 0x00000014, 0x00050088,
    0x00000007, 0x00000016, 0x0000000c, 0x00000015, 0x0003003e, 0x00000009, 0x00000016, 0x00050041,
    0x0000001e, 0x0000001f, 0x00000009, 0x0000001d, 0x0004003d, 0x00000006, 0x00000020, 0x0000001f,
    0x00050085, 0x00000006, 0x00000022, 0x00000020, 0x00000021, 0x00050083, 0x00000006, 0x00000024,
    0x00000022, 0x00000023, 0x00050041, 0x0000001e, 0x00000025, 0x00000009, 0x00000018, 0x0004003d,
    0x00000006, 0x00000026, 0x00000025, 0x00050085, 0x00000006, 0x00000027, 0x00000026, 0x00000021,
    0x00050083, 0x00000006, 0x00000028, 0x00000023, 0x00000027, 0x00070050, 0x00000017, 0x0000002a,
    0x00000024, 0x00000028, 0x00000029, 0x00000023, 0x00050041, 0x0000002b, 0x0000002c, 0x0000001c,
    0x00000012, 0x0003003e, 0x0000002c, 0x0000002a, 0x0004003d, 0x00000007, 0x00000030, 0x0000002f,
    0x0003003e, 0x0000002e, 0x00000030, 0x0004003d, 0x00000007, 0x00000033, 0x00000032, 0x0003003e,
    0x00000031, 0x00000033, 0x0004003d, 0x00000006, 0x00000038, 0x00000037, 0x0003003e, 0x00000035,
    0x00000038, 0x0004003d, 0x00000017, 0x0000003c, 0x0000003b, 0x0003003e, 0x00000039, 0x0000003c,
    0x000100fd, 0x00010038,
];

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV: [u32; 1944] = [
    0x07230203, 0x00010000, 0x0008000b, 0x0000014f, 0x00000000, 0x00020011, 0x00000001, 0x0006000b,
    0x00000001, 0x4c534c47, 0x6474732e, 0x3035342e, 0x00000000, 0x0003000e, 0x00000000, 0x00000001,
    0x000a000f, 0x00000004, 0x00000004, 0x6e69616d, 0x00000000, 0x000000fe, 0x00000108, 0x00000142,
    0x00000146, 0x0000014d, 0x00030010, 0x00000004, 0x00000007, 0x00030003, 0x00000002, 0x000001c2,
    0x00040005, 0x00000004, 0x6e69616d, 0x00000000, 0x00070005, 0x0000000b, 0x5f776172, 0x68706c61,
    0x616d5f61, 0x76286b73, 0x003b3266, 0x00030005, 0x0000000a, 0x00007675, 0x00060005, 0x0000000e,
    0x68706c61, 0x616d5f61, 0x76286b73, 0x003b3266, 0x00030005, 0x0000000d, 0x00007675, 0x00070005,
    0x00000011, 0x73697269, 0x746f6d5f, 0x5f6e6f69, 0x7366666f, 0x00287465, 0x00060005, 0x00000015,
    0x706d6173, 0x5f64656c, 0x6f6c6f63, 0x00002872, 0x00050005, 0x00000018, 0x6e656353, 0x73755065,
    0x00000068, 0x00050006, 0x00000018, 0x00000000, 0x65747865, 0x0000746e, 0x00080006, 0x00000018,
    0x00000001, 0x68706c61, 0x65745f61, 0x72757478, 0x6c735f65, 0x0000746f, 0x00080006, 0x00000018,
    0x00000002, 0x68706c61, 0x65745f61, 0x72757478, 0x6f6d5f65, 0x00006564, 0x00070006, 0x00000018,
    0x00000003, 0x656d6974, 0x6365735f, 0x73646e6f, 0x00000000, 0x00030005, 0x0000001a, 0x00006370,
    0x00050005, 0x00000028, 0x65545f67, 0x72757478, 0x00003165, 0x00050005, 0x00000035, 0x65545f67,
    0x72757478, 0x00003265, 0x00050005, 0x00000041, 0x65545f67, 0x72757478, 0x00003365, 0x00050005,
    0x0000004d, 0x65545f67, 0x72757478, 0x00003465, 0x00050005, 0x00000059, 0x65545f67, 0x72757478,
    0x00003565, 0x00050005, 0x00000065, 0x65545f67, 0x72757478, 0x00003665, 0x00050005, 0x00000071,
    0x65545f67, 0x72757478, 0x00003765, 0x00040005, 0x00000082, 0x6b73616d, 0x00000000, 0x00040005,
    0x00000083, 0x61726170, 0x0000006d, 0x00040005, 0x00000092, 0x69545f67, 0x0000656d, 0x00040005,
    0x00000097, 0x63535f67, 0x00656c61, 0x00040005, 0x00000099, 0x70535f67, 0x00646565, 0x00040005,
    0x0000009a, 0x6f525f67, 0x00686775, 0x00060005, 0x0000009c, 0x6f4e5f67, 0x41657369, 0x6e756f6d,
    0x00000074, 0x00060005, 0x0000009e, 0x68505f67, 0x4f657361, 0x65736666, 0x00000074, 0x00040005,
    0x000000a0, 0x656d6974, 0x00000000, 0x00040005, 0x000000a6, 0x44776f6c, 0x00000074, 0x00040005,
    0x000000a9, 0x69746f6d, 0x00326e6f, 0x00040005, 0x000000b2, 0x69746f6d, 0x00346e6f, 0x00050005,
    0x000000bd, 0x65766f6d, 0x72617453, 0x00000074, 0x00040005, 0x000000c3, 0x65766f6d, 0x00646e45,
    0x00030005, 0x000000c9, 0x00006164, 0x00040005, 0x000000fc, 0x6b73616d, 0x00000000, 0x00050005,
    0x000000fe, 0x66655f76, 0x74636566, 0x0076755f, 0x00040005, 0x000000ff, 0x61726170, 0x0000006d,
    0x00050005, 0x00000102, 0x73697269, 0x66666f5f, 0x00746573, 0x00050005, 0x00000106, 0x65545f67,
    0x72757478, 0x00003065, 0x00040005, 0x00000108, 0x76755f76, 0x00000000, 0x00050005, 0x00000119,
    0x73697269, 0x73616d5f, 0x0000006b, 0x00040005, 0x0000011a, 0x61726170, 0x0000006d, 0x00050005,
    0x0000011d, 0x73697269, 0x66666f5f, 0x00746573, 0x00040005, 0x00000121, 0x6f6c6f63, 0x00000072,
    0x00040005, 0x00000131, 0x6f6c6f63, 0x00000072, 0x00040005, 0x00000135, 0x61726170, 0x0000006d,
    0x00040005, 0x0000013f, 0x6f6c6f63, 0x00000072, 0x00040005, 0x00000142, 0x69745f76, 0x0000746e,
    0x00050005, 0x00000146, 0x706f5f76, 0x74696361, 0x00000079, 0x00050005, 0x0000014d, 0x5f74756f,
    0x6f6c6f63, 0x00000072, 0x00030047, 0x00000018, 0x00000002, 0x00050048, 0x00000018, 0x00000000,
    0x00000023, 0x00000000, 0x00050048, 0x00000018, 0x00000001, 0x00000023, 0x00000008, 0x00050048,
    0x00000018, 0x00000002, 0x00000023, 0x0000000c, 0x00050048, 0x00000018, 0x00000003, 0x00000023,
    0x00000010, 0x00040047, 0x00000028, 0x00000021, 0x00000001, 0x00040047, 0x00000028, 0x00000022,
    0x00000000, 0x00040047, 0x00000035, 0x00000021, 0x00000002, 0x00040047, 0x00000035, 0x00000022,
    0x00000000, 0x00040047, 0x00000041, 0x00000021, 0x00000003, 0x00040047, 0x00000041, 0x00000022,
    0x00000000, 0x00040047, 0x0000004d, 0x00000021, 0x00000004, 0x00040047, 0x0000004d, 0x00000022,
    0x00000000, 0x00040047, 0x00000059, 0x00000021, 0x00000005, 0x00040047, 0x00000059, 0x00000022,
    0x00000000, 0x00040047, 0x00000065, 0x00000021, 0x00000006, 0x00040047, 0x00000065, 0x00000022,
    0x00000000, 0x00040047, 0x00000071, 0x00000021, 0x00000007, 0x00040047, 0x00000071, 0x00000022,
    0x00000000, 0x00040047, 0x000000fe, 0x0000001e, 0x00000001, 0x00040047, 0x00000106, 0x00000021,
    0x00000000, 0x00040047, 0x00000106, 0x00000022, 0x00000000, 0x00040047, 0x00000108, 0x0000001e,
    0x00000000, 0x00040047, 0x00000142, 0x0000001e, 0x00000003, 0x00040047, 0x00000146, 0x0000001e,
    0x00000002, 0x00040047, 0x0000014d, 0x0000001e, 0x00000000, 0x00020013, 0x00000002, 0x00030021,
    0x00000003, 0x00000002, 0x00030016, 0x00000006, 0x00000020, 0x00040017, 0x00000007, 0x00000006,
    0x00000002, 0x00040020, 0x00000008, 0x00000007, 0x00000007, 0x00040021, 0x00000009, 0x00000006,
    0x00000008, 0x00030021, 0x00000010, 0x00000007, 0x00040017, 0x00000013, 0x00000006, 0x00000004,
    0x00030021, 0x00000014, 0x00000013, 0x00040015, 0x00000017, 0x00000020, 0x00000000, 0x0006001e,
    0x00000018, 0x00000007, 0x00000017, 0x00000017, 0x00000006, 0x00040020, 0x00000019, 0x00000009,
    0x00000018, 0x0004003b, 0x00000019, 0x0000001a, 0x00000009, 0x00040015, 0x0000001b, 0x00000020,
    0x00000001, 0x0004002b, 0x0000001b, 0x0000001c, 0x00000001, 0x00040020, 0x0000001d, 0x00000009,
    0x00000017, 0x0004002b, 0x00000017, 0x00000020, 0x00000001, 0x00020014, 0x00000021, 0x00090019,
    0x00000025, 0x00000006, 0x00000001, 0x00000000, 0x00000000, 0x00000000, 0x00000001, 0x00000000,
    0x0003001b, 0x00000026, 0x00000025, 0x00040020, 0x00000027, 0x00000000, 0x00000026, 0x0004003b,
    0x00000027, 0x00000028, 0x00000000, 0x0004002b, 0x00000017, 0x0000002c, 0x00000000, 0x0004002b,
    0x00000017, 0x00000031, 0x00000002, 0x0004003b, 0x00000027, 0x00000035, 0x00000000, 0x0004002b,
    0x00000017, 0x0000003d, 0x00000003, 0x0004003b, 0x00000027, 0x00000041, 0x00000000, 0x0004002b,
    0x00000017, 0x00000049, 0x00000004, 0x0004003b, 0x00000027, 0x0000004d, 0x00000000, 0x0004002b,
    0x00000017, 0x00000055, 0x00000005, 0x0004003b, 0x00000027, 0x00000059, 0x00000000, 0x0004002b,
    0x00000017, 0x00000061, 0x00000006, 0x0004003b, 0x00000027, 0x00000065, 0x00000000, 0x0004002b,
    0x00000017, 0x0000006d, 0x00000007, 0x0004003b, 0x00000027, 0x00000071, 0x00000000, 0x0004002b,
    0x00000006, 0x00000077, 0x3f800000, 0x0004002b, 0x00000017, 0x0000007c, 0xffffffff, 0x00040020,
    0x00000081, 0x00000007, 0x00000006, 0x0004002b, 0x0000001b, 0x00000086, 0x00000002, 0x0004002b,
    0x0000001b, 0x00000093, 0x00000003, 0x00040020, 0x00000094, 0x00000009, 0x00000006, 0x0005002c,
    0x00000007, 0x00000098, 0x00000077, 0x00000077, 0x0004002b, 0x00000006, 0x0000009b, 0x3e4ccccd,
    0x0004002b, 0x00000006, 0x0000009d, 0x3f000000, 0x0004002b, 0x00000006, 0x0000009f, 0x00000000,
    0x0004002b, 0x00000006, 0x000000aa, 0x3ff33333, 0x0005002c, 0x00000007, 0x000000ac, 0x0000009f,
    0x00000077, 0x00040020, 0x000000b1, 0x00000007, 0x00000013, 0x0004002b, 0x00000006, 0x000000b3,
    0x40200000, 0x0007002c, 0x00000013, 0x000000b5, 0x0000009f, 0x0000009f, 0x00000077, 0x00000077,
    0x0004002b, 0x00000006, 0x000000b9, 0x40000000, 0x0007002c, 0x00000013, 0x000000ba, 0x00000077,
    0x000000b9, 0x00000077, 0x000000b9, 0x0004002b, 0x00000006, 0x000000d0, 0x40490fdb, 0x0004002b,
    0x00000006, 0x000000d3, 0xbf000000, 0x0004002b, 0x00000006, 0x000000ea, 0x3a83126f, 0x00040020,
    0x000000fd, 0x00000001, 0x00000007, 0x0004003b, 0x000000fd, 0x000000fe, 0x00000001, 0x0004003b,
    0x00000027, 0x00000106, 0x00000000, 0x0004003b, 0x000000fd, 0x00000108, 0x00000001, 0x00040020,
    0x00000141, 0x00000001, 0x00000013, 0x0004003b, 0x00000141, 0x00000142, 0x00000001, 0x00040020,
    0x00000145, 0x00000001, 0x00000006, 0x0004003b, 0x00000145, 0x00000146, 0x00000001, 0x00040020,
    0x0000014c, 0x00000003, 0x00000013, 0x0004003b, 0x0000014c, 0x0000014d, 0x00000003, 0x00050036,
    0x00000002, 0x00000004, 0x00000000, 0x00000003, 0x000200f8, 0x00000005, 0x0004003b, 0x000000b1,
    0x0000013f, 0x00000007, 0x00040039, 0x00000013, 0x00000140, 0x00000015, 0x0004003d, 0x00000013,
    0x00000143, 0x00000142, 0x00050085, 0x00000013, 0x00000144, 0x00000140, 0x00000143, 0x0003003e,
    0x0000013f, 0x00000144, 0x0004003d, 0x00000006, 0x00000147, 0x00000146, 0x00050041, 0x00000081,
    0x00000148, 0x0000013f, 0x0000003d, 0x0004003d, 0x00000006, 0x00000149, 0x00000148, 0x00050085,
    0x00000006, 0x0000014a, 0x00000149, 0x00000147, 0x00050041, 0x00000081, 0x0000014b, 0x0000013f,
    0x0000003d, 0x0003003e, 0x0000014b, 0x0000014a, 0x0004003d, 0x00000013, 0x0000014e, 0x0000013f,
    0x0003003e, 0x0000014d, 0x0000014e, 0x000100fd, 0x00010038, 0x00050036, 0x00000006, 0x0000000b,
    0x00000000, 0x00000009, 0x00030037, 0x00000008, 0x0000000a, 0x000200f8, 0x0000000c, 0x00050041,
    0x0000001d, 0x0000001e, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000001f, 0x0000001e,
    0x000500aa, 0x00000021, 0x00000022, 0x0000001f, 0x00000020, 0x000300f7, 0x00000024, 0x00000000,
    0x000400fa, 0x00000022, 0x00000023, 0x00000024, 0x000200f8, 0x00000023, 0x0004003d, 0x00000026,
    0x00000029, 0x00000028, 0x0004003d, 0x00000007, 0x0000002a, 0x0000000a, 0x00050057, 0x00000013,
    0x0000002b, 0x00000029, 0x0000002a, 0x00050051, 0x00000006, 0x0000002d, 0x0000002b, 0x00000000,
    0x000200fe, 0x0000002d, 0x000200f8, 0x00000024, 0x00050041, 0x0000001d, 0x0000002f, 0x0000001a,
    0x0000001c, 0x0004003d, 0x00000017, 0x00000030, 0x0000002f, 0x000500aa, 0x00000021, 0x00000032,
    0x00000030, 0x00000031, 0x000300f7, 0x00000034, 0x00000000, 0x000400fa, 0x00000032, 0x00000033,
    0x00000034, 0x000200f8, 0x00000033, 0x0004003d, 0x00000026, 0x00000036, 0x00000035, 0x0004003d,
    0x00000007, 0x00000037, 0x0000000a, 0x00050057, 0x00000013, 0x00000038, 0x00000036, 0x00000037,
    0x00050051, 0x00000006, 0x00000039, 0x00000038, 0x00000000, 0x000200fe, 0x00000039, 0x000200f8,
    0x00000034, 0x00050041, 0x0000001d, 0x0000003b, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017,
    0x0000003c, 0x0000003b, 0x000500aa, 0x00000021, 0x0000003e, 0x0000003c, 0x0000003d, 0x000300f7,
    0x00000040, 0x00000000, 0x000400fa, 0x0000003e, 0x0000003f, 0x00000040, 0x000200f8, 0x0000003f,
    0x0004003d, 0x00000026, 0x00000042, 0x00000041, 0x0004003d, 0x00000007, 0x00000043, 0x0000000a,
    0x00050057, 0x00000013, 0x00000044, 0x00000042, 0x00000043, 0x00050051, 0x00000006, 0x00000045,
    0x00000044, 0x00000000, 0x000200fe, 0x00000045, 0x000200f8, 0x00000040, 0x00050041, 0x0000001d,
    0x00000047, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x00000048, 0x00000047, 0x000500aa,
    0x00000021, 0x0000004a, 0x00000048, 0x00000049, 0x000300f7, 0x0000004c, 0x00000000, 0x000400fa,
    0x0000004a, 0x0000004b, 0x0000004c, 0x000200f8, 0x0000004b, 0x0004003d, 0x00000026, 0x0000004e,
    0x0000004d, 0x0004003d, 0x00000007, 0x0000004f, 0x0000000a, 0x00050057, 0x00000013, 0x00000050,
    0x0000004e, 0x0000004f, 0x00050051, 0x00000006, 0x00000051, 0x00000050, 0x00000000, 0x000200fe,
    0x00000051, 0x000200f8, 0x0000004c, 0x00050041, 0x0000001d, 0x00000053, 0x0000001a, 0x0000001c,
    0x0004003d, 0x00000017, 0x00000054, 0x00000053, 0x000500aa, 0x00000021, 0x00000056, 0x00000054,
    0x00000055, 0x000300f7, 0x00000058, 0x00000000, 0x000400fa, 0x00000056, 0x00000057, 0x00000058,
    0x000200f8, 0x00000057, 0x0004003d, 0x00000026, 0x0000005a, 0x00000059, 0x0004003d, 0x00000007,
    0x0000005b, 0x0000000a, 0x00050057, 0x00000013, 0x0000005c, 0x0000005a, 0x0000005b, 0x00050051,
    0x00000006, 0x0000005d, 0x0000005c, 0x00000000, 0x000200fe, 0x0000005d, 0x000200f8, 0x00000058,
    0x00050041, 0x0000001d, 0x0000005f, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x00000060,
    0x0000005f, 0x000500aa, 0x00000021, 0x00000062, 0x00000060, 0x00000061, 0x000300f7, 0x00000064,
    0x00000000, 0x000400fa, 0x00000062, 0x00000063, 0x00000064, 0x000200f8, 0x00000063, 0x0004003d,
    0x00000026, 0x00000066, 0x00000065, 0x0004003d, 0x00000007, 0x00000067, 0x0000000a, 0x00050057,
    0x00000013, 0x00000068, 0x00000066, 0x00000067, 0x00050051, 0x00000006, 0x00000069, 0x00000068,
    0x00000000, 0x000200fe, 0x00000069, 0x000200f8, 0x00000064, 0x00050041, 0x0000001d, 0x0000006b,
    0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000006c, 0x0000006b, 0x000500aa, 0x00000021,
    0x0000006e, 0x0000006c, 0x0000006d, 0x000300f7, 0x00000070, 0x00000000, 0x000400fa, 0x0000006e,
    0x0000006f, 0x00000070, 0x000200f8, 0x0000006f, 0x0004003d, 0x00000026, 0x00000072, 0x00000071,
    0x0004003d, 0x00000007, 0x00000073, 0x0000000a, 0x00050057, 0x00000013, 0x00000074, 0x00000072,
    0x00000073, 0x00050051, 0x00000006, 0x00000075, 0x00000074, 0x00000000, 0x000200fe, 0x00000075,
    0x000200f8, 0x00000070, 0x000200fe, 0x00000077, 0x00010038, 0x00050036, 0x00000006, 0x0000000e,
    0x00000000, 0x00000009, 0x00030037, 0x00000008, 0x0000000d, 0x000200f8, 0x0000000f, 0x0004003b,
    0x00000081, 0x00000082, 0x00000007, 0x0004003b, 0x00000008, 0x00000083, 0x00000007, 0x00050041,
    0x0000001d, 0x0000007a, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000007b, 0x0000007a,
    0x000500aa, 0x00000021, 0x0000007d, 0x0000007b, 0x0000007c, 0x000300f7, 0x0000007f, 0x00000000,
    0x000400fa, 0x0000007d, 0x0000007e, 0x0000007f, 0x000200f8, 0x0000007e, 0x000200fe, 0x00000077,
    0x000200f8, 0x0000007f, 0x0004003d, 0x00000007, 0x00000084, 0x0000000d, 0x0003003e, 0x00000083,
    0x00000084, 0x00050039, 0x00000006, 0x00000085, 0x0000000b, 0x00000083, 0x0003003e, 0x00000082,
    0x00000085, 0x00050041, 0x0000001d, 0x00000087, 0x0000001a, 0x00000086, 0x0004003d, 0x00000017,
    0x00000088, 0x00000087, 0x000500aa, 0x00000021, 0x00000089, 0x00000088, 0x00000020, 0x000300f7,
    0x0000008b, 0x00000000, 0x000400fa, 0x00000089, 0x0000008a, 0x0000008b, 0x000200f8, 0x0000008a,
    0x0004003d, 0x00000006, 0x0000008c, 0x00000082, 0x00050083, 0x00000006, 0x0000008d, 0x00000077,
    0x0000008c, 0x000200fe, 0x0000008d, 0x000200f8, 0x0000008b, 0x0004003d, 0x00000006, 0x0000008f,
    0x00000082, 0x000200fe, 0x0000008f, 0x00010038, 0x00050036, 0x00000007, 0x00000011, 0x00000000,
    0x00000010, 0x000200f8, 0x00000012, 0x0004003b, 0x00000081, 0x00000092, 0x00000007, 0x0004003b,
    0x00000008, 0x00000097, 0x00000007, 0x0004003b, 0x00000081, 0x00000099, 0x00000007, 0x0004003b,
    0x00000081, 0x0000009a, 0x00000007, 0x0004003b, 0x00000081, 0x0000009c, 0x00000007, 0x0004003b,
    0x00000081, 0x0000009e, 0x00000007, 0x0004003b, 0x00000081, 0x000000a0, 0x00000007, 0x0004003b,
    0x00000081, 0x000000a6, 0x00000007, 0x0004003b, 0x00000008, 0x000000a9, 0x00000007, 0x0004003b,
    0x000000b1, 0x000000b2, 0x00000007, 0x0004003b, 0x00000008, 0x000000bd, 0x00000007, 0x0004003b,
    0x00000008, 0x000000c3, 0x00000007, 0x0004003b, 0x00000008, 0x000000c9, 0x00000007, 0x00050041,
    0x00000094, 0x00000095, 0x0000001a, 0x00000093, 0x0004003d, 0x00000006, 0x00000096, 0x00000095,
    0x0003003e, 0x00000092, 0x00000096, 0x0003003e, 0x00000097, 0x00000098, 0x0003003e, 0x00000099,
    0x00000077, 0x0003003e, 0x0000009a, 0x0000009b, 0x0003003e, 0x0000009c, 0x0000009d, 0x0003003e,
    0x0000009e, 0x0000009f, 0x0004003d, 0x00000006, 0x000000a1, 0x00000092, 0x0004003d, 0x00000006,
    0x000000a2, 0x00000099, 0x00050085, 0x00000006, 0x000000a3, 0x000000a1, 0x000000a2, 0x0004003d,
    0x00000006, 0x000000a4, 0x0000009e, 0x00050081, 0x00000006, 0x000000a5, 0x000000a3, 0x000000a4,
    0x0003003e, 0x000000a0, 0x000000a5, 0x0004003d, 0x00000006, 0x000000a7, 0x000000a0, 0x0006000c,
    0x00000006, 0x000000a8, 0x00000001, 0x00000008, 0x000000a7, 0x0003003e, 0x000000a6, 0x000000a8,
    0x0004003d, 0x00000006, 0x000000ab, 0x000000a6, 0x00050050, 0x00000007, 0x000000ad, 0x000000ab,
    0x000000ab, 0x00050081, 0x00000007, 0x000000ae, 0x000000ad, 0x000000ac, 0x0005008e, 0x00000007,
    0x000000af, 0x000000ae, 0x000000aa, 0x0006000c, 0x00000007, 0x000000b0, 0x00000001, 0x0000000d,
    0x000000af, 0x0003003e, 0x000000a9, 0x000000b0, 0x0004003d, 0x00000006, 0x000000b4, 0x000000a6,
    0x00070050, 0x00000013, 0x000000b6, 0x000000b4, 0x000000b4, 0x000000b4, 0x000000b4, 0x00050081,
    0x00000013, 0x000000b7, 0x000000b6, 0x000000b5, 0x0005008e, 0x00000013, 0x000000b8, 0x000000b7,
    0x000000b3, 0x00050081, 0x00000013, 0x000000bb, 0x000000b8, 0x000000ba, 0x0006000c, 0x00000013,
    0x000000bc, 0x00000001, 0x0000000d, 0x000000bb, 0x0003003e, 0x000000b2, 0x000000bc, 0x0004003d,
    0x00000007, 0x000000be, 0x000000a9, 0x0007004f, 0x00000007, 0x000000bf, 0x000000be, 0x000000be,
    0x00000000, 0x00000000, 0x0004003d, 0x00000013, 0x000000c0, 0x000000b2, 0x0007004f, 0x00000007,
    0x000000c1, 0x000000c0, 0x000000c0, 0x00000000, 0x00000001, 0x00050081, 0x00000007, 0x000000c2,
    0x000000bf, 0x000000c1, 0x0003003e, 0x000000bd, 0x000000c2, 0x0004003d, 0x00000007, 0x000000c4,
    0x000000a9, 0x0007004f, 0x00000007, 0x000000c5, 0x000000c4, 0x000000c4, 0x00000001, 0x00000001,
    0x0004003d, 0x00000013, 0x000000c6, 0x000000b2, 0x0007004f, 0x00000007, 0x000000c7, 0x000000c6,
    0x000000c6, 0x00000002, 0x00000003, 0x00050081, 0x00000007, 0x000000c8, 0x000000c5, 0x000000c7,
    0x0003003e, 0x000000c3, 0x000000c8, 0x0004003d, 0x00000007, 0x000000ca, 0x000000bd, 0x0004003d,
    0x00000007, 0x000000cb, 0x000000c3, 0x0004003d, 0x00000006, 0x000000cc, 0x0000009a, 0x00050083,
    0x00000006, 0x000000cd, 0x00000077, 0x000000cc, 0x0004003d, 0x00000006, 0x000000ce, 0x000000a0,
    0x0006000c, 0x00000006, 0x000000cf, 0x00000001, 0x0000000a, 0x000000ce, 0x00050085, 0x00000006,
    0x000000d1, 0x000000cf, 0x000000d0, 0x0006000c, 0x00000006, 0x000000d2, 0x00000001, 0x0000000e,
    0x000000d1, 0x00050085, 0x00000006, 0x000000d4, 0x000000d2, 0x000000d3, 0x00050081, 0x00000006,
    0x000000d5, 0x000000d4, 0x0000009d, 0x0008000c, 0x00000006, 0x000000d6, 0x00000001, 0x00000031,
    0x000000cd, 0x00000077, 0x000000d5, 0x00050050, 0x00000007, 0x000000d7, 0x000000d6, 0x000000d6,
    0x0008000c, 0x00000007, 0x000000d8, 0x00000001, 0x0000002e, 0x000000ca, 0x000000cb, 0x000000d7,
    0x0003003e, 0x000000c9, 0x000000d8, 0x0004003d, 0x00000006, 0x000000d9, 0x000000a0, 0x0006000c,
    0x00000006, 0x000000da, 0x00000001, 0x0000000d, 0x000000d9, 0x0004003d, 0x00000006, 0x000000db,
    0x0000009c, 0x00050085, 0x00000006, 0x000000dc, 0x000000da, 0x000000db, 0x00050041, 0x00000081,
    0x000000dd, 0x000000c9, 0x0000002c, 0x0004003d, 0x00000006, 0x000000de, 0x000000dd, 0x00050081,
    0x00000006, 0x000000df, 0x000000de, 0x000000dc, 0x00050041, 0x00000081, 0x000000e0, 0x000000c9,
    0x0000002c, 0x0003003e, 0x000000e0, 0x000000df, 0x0004003d, 0x00000006, 0x000000e1, 0x000000a0,
    0x0006000c, 0x00000006, 0x000000e2, 0x00000001, 0x0000000e, 0x000000e1, 0x0004003d, 0x00000006,
    0x000000e3, 0x0000009c, 0x00050085, 0x00000006, 0x000000e4, 0x000000e2, 0x000000e3, 0x00050041,
    0x00000081, 0x000000e5, 0x000000c9, 0x00000020, 0x0004003d, 0x00000006, 0x000000e6, 0x000000e5,
    0x00050081, 0x00000006, 0x000000e7, 0x000000e6, 0x000000e4, 0x00050041, 0x00000081, 0x000000e8,
    0x000000c9, 0x00000020, 0x0003003e, 0x000000e8, 0x000000e7, 0x0004003d, 0x00000007, 0x000000e9,
    0x00000097, 0x0005008e, 0x00000007, 0x000000eb, 0x000000e9, 0x000000ea, 0x0004003d, 0x00000007,
    0x000000ec, 0x000000c9, 0x00050085, 0x00000007, 0x000000ed, 0x000000ec, 0x000000eb, 0x0003003e,
    0x000000c9, 0x000000ed, 0x0004003d, 0x00000007, 0x000000ee, 0x000000c9, 0x000200fe, 0x000000ee,
    0x00010038, 0x00050036, 0x00000013, 0x00000015, 0x00000000, 0x00000014, 0x000200f8, 0x00000016,
    0x0004003b, 0x00000081, 0x000000fc, 0x00000007, 0x0004003b, 0x00000008, 0x000000ff, 0x00000007,
    0x0004003b, 0x00000008, 0x00000102, 0x00000007, 0x0004003b, 0x00000081, 0x00000119, 0x00000007,
    0x0004003b, 0x00000008, 0x0000011a, 0x00000007, 0x0004003b, 0x00000008, 0x0000011d, 0x00000007,
    0x0004003b, 0x000000b1, 0x00000121, 0x00000007, 0x0004003b, 0x000000b1, 0x00000131, 0x00000007,
    0x0004003b, 0x00000008, 0x00000135, 0x00000007, 0x00050041, 0x0000001d, 0x000000f1, 0x0000001a,
    0x0000001c, 0x0004003d, 0x00000017, 0x000000f2, 0x000000f1, 0x000500ab, 0x00000021, 0x000000f3,
    0x000000f2, 0x0000007c, 0x000300f7, 0x000000f5, 0x00000000, 0x000400fa, 0x000000f3, 0x000000f4,
    0x000000f5, 0x000200f8, 0x000000f4, 0x00050041, 0x0000001d, 0x000000f6, 0x0000001a, 0x00000086,
    0x0004003d, 0x00000017, 0x000000f7, 0x000000f6, 0x000500aa, 0x00000021, 0x000000f8, 0x000000f7,
    0x00000031, 0x000200f9, 0x000000f5, 0x000200f8, 0x000000f5, 0x000700f5, 0x00000021, 0x000000f9,
    0x000000f3, 0x00000016, 0x000000f8, 0x000000f4, 0x000300f7, 0x000000fb, 0x00000000, 0x000400fa,
    0x000000f9, 0x000000fa, 0x000000fb, 0x000200f8, 0x000000fa, 0x0004003d, 0x00000007, 0x00000100,
    0x000000fe, 0x0003003e, 0x000000ff, 0x00000100, 0x00050039, 0x00000006, 0x00000101, 0x0000000b,
    0x000000ff, 0x0003003e, 0x000000fc, 0x00000101, 0x00040039, 0x00000007, 0x00000103, 0x00000011,
    0x0004003d, 0x00000006, 0x00000104, 0x000000fc, 0x0005008e, 0x00000007, 0x00000105, 0x00000103,
    0x00000104, 0x0003003e, 0x00000102, 0x00000105, 0x0004003d, 0x00000026, 0x00000107, 0x00000106,
    0x0004003d, 0x00000007, 0x00000109, 0x00000108, 0x0004003d, 0x00000007, 0x0000010a, 0x00000102,
    0x00050081, 0x00000007, 0x0000010b, 0x00000109, 0x0000010a, 0x00050057, 0x00000013, 0x0000010c,
    0x00000107, 0x0000010b, 0x000200fe, 0x0000010c, 0x000200f8, 0x000000fb, 0x00050041, 0x0000001d,
    0x0000010e, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000010f, 0x0000010e, 0x000500ab,
    0x00000021, 0x00000110, 0x0000010f, 0x0000007c, 0x000300f7, 0x00000112, 0x00000000, 0x000400fa,
    0x00000110, 0x00000111, 0x00000112, 0x000200f8, 0x00000111, 0x00050041, 0x0000001d, 0x00000113,
    0x0000001a, 0x00000086, 0x0004003d, 0x00000017, 0x00000114, 0x00000113, 0x000500aa, 0x00000021,
    0x00000115, 0x00000114, 0x00000049, 0x000200f9, 0x00000112, 0x000200f8, 0x00000112, 0x000700f5,
    0x00000021, 0x00000116, 0x00000110, 0x000000fb, 0x00000115, 0x00000111, 0x000300f7, 0x00000118,
    0x00000000, 0x000400fa, 0x00000116, 0x00000117, 0x00000118, 0x000200f8, 0x00000117, 0x0004003d,
    0x00000007, 0x0000011b, 0x000000fe, 0x0003003e, 0x0000011a, 0x0000011b, 0x00050039, 0x00000006,
    0x0000011c, 0x0000000b, 0x0000011a, 0x0003003e, 0x00000119, 0x0000011c, 0x00040039, 0x00000007,
    0x0000011e, 0x00000011, 0x0004003d, 0x00000006, 0x0000011f, 0x00000119, 0x0005008e, 0x00000007,
    0x00000120, 0x0000011e, 0x0000011f, 0x0003003e, 0x0000011d, 0x00000120, 0x0004003d, 0x00000026,
    0x00000122, 0x00000106, 0x0004003d, 0x00000007, 0x00000123, 0x00000108, 0x0004003d, 0x00000007,
    0x00000124, 0x0000011d, 0x00050081, 0x00000007, 0x00000125, 0x00000123, 0x00000124, 0x00050057,
    0x00000013, 0x00000126, 0x00000122, 0x00000125, 0x0003003e, 0x00000121, 0x00000126, 0x0004003d,
    0x00000026, 0x00000127, 0x00000035, 0x0004003d, 0x00000007, 0x00000128, 0x000000fe, 0x00050057,
    0x00000013, 0x00000129, 0x00000127, 0x00000128, 0x00050051, 0x00000006, 0x0000012a, 0x00000129,
    0x00000000, 0x00050041, 0x00000081, 0x0000012b, 0x00000121, 0x0000003d, 0x0004003d, 0x00000006,
    0x0000012c, 0x0000012b, 0x00050085, 0x00000006, 0x0000012d, 0x0000012c, 0x0000012a, 0x00050041,
    0x00000081, 0x0000012e, 0x00000121, 0x0000003d, 0x0003003e, 0x0000012e, 0x0000012d, 0x0004003d,
    0x00000013, 0x0000012f, 0x00000121, 0x000200fe, 0x0000012f, 0x000200f8, 0x00000118, 0x0004003d,
    0x00000026, 0x00000132, 0x00000106, 0x0004003d, 0x00000007, 0x00000133, 0x00000108, 0x00050057,
    0x00000013, 0x00000134, 0x00000132, 0x00000133, 0x0003003e, 0x00000131, 0x00000134, 0x0004003d,
    0x00000007, 0x00000136, 0x000000fe, 0x0003003e, 0x00000135, 0x00000136, 0x00050039, 0x00000006,
    0x00000137, 0x0000000e, 0x00000135, 0x00050041, 0x00000081, 0x00000138, 0x00000131, 0x0000003d,
    0x0004003d, 0x00000006, 0x00000139, 0x00000138, 0x00050085, 0x00000006, 0x0000013a, 0x00000139,
    0x00000137, 0x00050041, 0x00000081, 0x0000013b, 0x00000131, 0x0000003d, 0x0003003e, 0x0000013b,
    0x0000013a, 0x0004003d, 0x00000013, 0x0000013c, 0x00000131, 0x000200fe, 0x0000013c, 0x00010038,
];

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PREMULTIPLIED_FRAGMENT_SPIRV: [u32; 2001] = [
    0x07230203, 0x00010000, 0x0008000b, 0x0000015a, 0x00000000, 0x00020011, 0x00000001, 0x0006000b,
    0x00000001, 0x4c534c47, 0x6474732e, 0x3035342e, 0x00000000, 0x0003000e, 0x00000000, 0x00000001,
    0x000a000f, 0x00000004, 0x00000004, 0x6e69616d, 0x00000000, 0x000000fe, 0x00000108, 0x00000142,
    0x00000146, 0x0000014d, 0x00030010, 0x00000004, 0x00000007, 0x00030003, 0x00000002, 0x000001c2,
    0x00040005, 0x00000004, 0x6e69616d, 0x00000000, 0x00070005, 0x0000000b, 0x5f776172, 0x68706c61,
    0x616d5f61, 0x76286b73, 0x003b3266, 0x00030005, 0x0000000a, 0x00007675, 0x00060005, 0x0000000e,
    0x68706c61, 0x616d5f61, 0x76286b73, 0x003b3266, 0x00030005, 0x0000000d, 0x00007675, 0x00070005,
    0x00000011, 0x73697269, 0x746f6d5f, 0x5f6e6f69, 0x7366666f, 0x00287465, 0x00060005, 0x00000015,
    0x706d6173, 0x5f64656c, 0x6f6c6f63, 0x00002872, 0x00050005, 0x00000018, 0x6e656353, 0x73755065,
    0x00000068, 0x00050006, 0x00000018, 0x00000000, 0x65747865, 0x0000746e, 0x00080006, 0x00000018,
    0x00000001, 0x68706c61, 0x65745f61, 0x72757478, 0x6c735f65, 0x0000746f, 0x00080006, 0x00000018,
    0x00000002, 0x68706c61, 0x65745f61, 0x72757478, 0x6f6d5f65, 0x00006564, 0x00070006, 0x00000018,
    0x00000003, 0x656d6974, 0x6365735f, 0x73646e6f, 0x00000000, 0x00030005, 0x0000001a, 0x00006370,
    0x00050005, 0x00000028, 0x65545f67, 0x72757478, 0x00003165, 0x00050005, 0x00000035, 0x65545f67,
    0x72757478, 0x00003265, 0x00050005, 0x00000041, 0x65545f67, 0x72757478, 0x00003365, 0x00050005,
    0x0000004d, 0x65545f67, 0x72757478, 0x00003465, 0x00050005, 0x00000059, 0x65545f67, 0x72757478,
    0x00003565, 0x00050005, 0x00000065, 0x65545f67, 0x72757478, 0x00003665, 0x00050005, 0x00000071,
    0x65545f67, 0x72757478, 0x00003765, 0x00040005, 0x00000082, 0x6b73616d, 0x00000000, 0x00040005,
    0x00000083, 0x61726170, 0x0000006d, 0x00040005, 0x00000092, 0x69545f67, 0x0000656d, 0x00040005,
    0x00000097, 0x63535f67, 0x00656c61, 0x00040005, 0x00000099, 0x70535f67, 0x00646565, 0x00040005,
    0x0000009a, 0x6f525f67, 0x00686775, 0x00060005, 0x0000009c, 0x6f4e5f67, 0x41657369, 0x6e756f6d,
    0x00000074, 0x00060005, 0x0000009e, 0x68505f67, 0x4f657361, 0x65736666, 0x00000074, 0x00040005,
    0x000000a0, 0x656d6974, 0x00000000, 0x00040005, 0x000000a6, 0x44776f6c, 0x00000074, 0x00040005,
    0x000000a9, 0x69746f6d, 0x00326e6f, 0x00040005, 0x000000b2, 0x69746f6d, 0x00346e6f, 0x00050005,
    0x000000bd, 0x65766f6d, 0x72617453, 0x00000074, 0x00040005, 0x000000c3, 0x65766f6d, 0x00646e45,
    0x00030005, 0x000000c9, 0x00006164, 0x00040005, 0x000000fc, 0x6b73616d, 0x00000000, 0x00050005,
    0x000000fe, 0x66655f76, 0x74636566, 0x0076755f, 0x00040005, 0x000000ff, 0x61726170, 0x0000006d,
    0x00050005, 0x00000102, 0x73697269, 0x66666f5f, 0x00746573, 0x00050005, 0x00000106, 0x65545f67,
    0x72757478, 0x00003065, 0x00040005, 0x00000108, 0x76755f76, 0x00000000, 0x00050005, 0x00000119,
    0x73697269, 0x73616d5f, 0x0000006b, 0x00040005, 0x0000011a, 0x61726170, 0x0000006d, 0x00050005,
    0x0000011d, 0x73697269, 0x66666f5f, 0x00746573, 0x00040005, 0x00000121, 0x6f6c6f63, 0x00000072,
    0x00040005, 0x00000131, 0x6f6c6f63, 0x00000072, 0x00040005, 0x00000135, 0x61726170, 0x0000006d,
    0x00040005, 0x0000013f, 0x6f6c6f63, 0x00000072, 0x00040005, 0x00000142, 0x69745f76, 0x0000746e,
    0x00050005, 0x00000146, 0x706f5f76, 0x74696361, 0x00000079, 0x00050005, 0x0000014d, 0x5f74756f,
    0x6f6c6f63, 0x00000072, 0x00030047, 0x00000018, 0x00000002, 0x00050048, 0x00000018, 0x00000000,
    0x00000023, 0x00000000, 0x00050048, 0x00000018, 0x00000001, 0x00000023, 0x00000008, 0x00050048,
    0x00000018, 0x00000002, 0x00000023, 0x0000000c, 0x00050048, 0x00000018, 0x00000003, 0x00000023,
    0x00000010, 0x00040047, 0x00000028, 0x00000021, 0x00000001, 0x00040047, 0x00000028, 0x00000022,
    0x00000000, 0x00040047, 0x00000035, 0x00000021, 0x00000002, 0x00040047, 0x00000035, 0x00000022,
    0x00000000, 0x00040047, 0x00000041, 0x00000021, 0x00000003, 0x00040047, 0x00000041, 0x00000022,
    0x00000000, 0x00040047, 0x0000004d, 0x00000021, 0x00000004, 0x00040047, 0x0000004d, 0x00000022,
    0x00000000, 0x00040047, 0x00000059, 0x00000021, 0x00000005, 0x00040047, 0x00000059, 0x00000022,
    0x00000000, 0x00040047, 0x00000065, 0x00000021, 0x00000006, 0x00040047, 0x00000065, 0x00000022,
    0x00000000, 0x00040047, 0x00000071, 0x00000021, 0x00000007, 0x00040047, 0x00000071, 0x00000022,
    0x00000000, 0x00040047, 0x000000fe, 0x0000001e, 0x00000001, 0x00040047, 0x00000106, 0x00000021,
    0x00000000, 0x00040047, 0x00000106, 0x00000022, 0x00000000, 0x00040047, 0x00000108, 0x0000001e,
    0x00000000, 0x00040047, 0x00000142, 0x0000001e, 0x00000003, 0x00040047, 0x00000146, 0x0000001e,
    0x00000002, 0x00040047, 0x0000014d, 0x0000001e, 0x00000000, 0x00020013, 0x00000002, 0x00030021,
    0x00000003, 0x00000002, 0x00030016, 0x00000006, 0x00000020, 0x00040017, 0x00000007, 0x00000006,
    0x00000002, 0x00040020, 0x00000008, 0x00000007, 0x00000007, 0x00040021, 0x00000009, 0x00000006,
    0x00000008, 0x00030021, 0x00000010, 0x00000007, 0x00040017, 0x00000013, 0x00000006, 0x00000004,
    0x00030021, 0x00000014, 0x00000013, 0x00040015, 0x00000017, 0x00000020, 0x00000000, 0x0006001e,
    0x00000018, 0x00000007, 0x00000017, 0x00000017, 0x00000006, 0x00040020, 0x00000019, 0x00000009,
    0x00000018, 0x0004003b, 0x00000019, 0x0000001a, 0x00000009, 0x00040015, 0x0000001b, 0x00000020,
    0x00000001, 0x0004002b, 0x0000001b, 0x0000001c, 0x00000001, 0x00040020, 0x0000001d, 0x00000009,
    0x00000017, 0x0004002b, 0x00000017, 0x00000020, 0x00000001, 0x00020014, 0x00000021, 0x00090019,
    0x00000025, 0x00000006, 0x00000001, 0x00000000, 0x00000000, 0x00000000, 0x00000001, 0x00000000,
    0x0003001b, 0x00000026, 0x00000025, 0x00040020, 0x00000027, 0x00000000, 0x00000026, 0x0004003b,
    0x00000027, 0x00000028, 0x00000000, 0x0004002b, 0x00000017, 0x0000002c, 0x00000000, 0x0004002b,
    0x00000017, 0x00000031, 0x00000002, 0x0004003b, 0x00000027, 0x00000035, 0x00000000, 0x0004002b,
    0x00000017, 0x0000003d, 0x00000003, 0x0004003b, 0x00000027, 0x00000041, 0x00000000, 0x0004002b,
    0x00000017, 0x00000049, 0x00000004, 0x0004003b, 0x00000027, 0x0000004d, 0x00000000, 0x0004002b,
    0x00000017, 0x00000055, 0x00000005, 0x0004003b, 0x00000027, 0x00000059, 0x00000000, 0x0004002b,
    0x00000017, 0x00000061, 0x00000006, 0x0004003b, 0x00000027, 0x00000065, 0x00000000, 0x0004002b,
    0x00000017, 0x0000006d, 0x00000007, 0x0004003b, 0x00000027, 0x00000071, 0x00000000, 0x0004002b,
    0x00000006, 0x00000077, 0x3f800000, 0x0004002b, 0x00000017, 0x0000007c, 0xffffffff, 0x00040020,
    0x00000081, 0x00000007, 0x00000006, 0x0004002b, 0x0000001b, 0x00000086, 0x00000002, 0x0004002b,
    0x0000001b, 0x00000093, 0x00000003, 0x00040020, 0x00000094, 0x00000009, 0x00000006, 0x0005002c,
    0x00000007, 0x00000098, 0x00000077, 0x00000077, 0x0004002b, 0x00000006, 0x0000009b, 0x3e4ccccd,
    0x0004002b, 0x00000006, 0x0000009d, 0x3f000000, 0x0004002b, 0x00000006, 0x0000009f, 0x00000000,
    0x0004002b, 0x00000006, 0x000000aa, 0x3ff33333, 0x0005002c, 0x00000007, 0x000000ac, 0x0000009f,
    0x00000077, 0x00040020, 0x000000b1, 0x00000007, 0x00000013, 0x0004002b, 0x00000006, 0x000000b3,
    0x40200000, 0x0007002c, 0x00000013, 0x000000b5, 0x0000009f, 0x0000009f, 0x00000077, 0x00000077,
    0x0004002b, 0x00000006, 0x000000b9, 0x40000000, 0x0007002c, 0x00000013, 0x000000ba, 0x00000077,
    0x000000b9, 0x00000077, 0x000000b9, 0x0004002b, 0x00000006, 0x000000d0, 0x40490fdb, 0x0004002b,
    0x00000006, 0x000000d3, 0xbf000000, 0x0004002b, 0x00000006, 0x000000ea, 0x3a83126f, 0x00040020,
    0x000000fd, 0x00000001, 0x00000007, 0x0004003b, 0x000000fd, 0x000000fe, 0x00000001, 0x0004003b,
    0x00000027, 0x00000106, 0x00000000, 0x0004003b, 0x000000fd, 0x00000108, 0x00000001, 0x00040020,
    0x00000141, 0x00000001, 0x00000013, 0x0004003b, 0x00000141, 0x00000142, 0x00000001, 0x00040020,
    0x00000145, 0x00000001, 0x00000006, 0x0004003b, 0x00000145, 0x00000146, 0x00000001, 0x00040020,
    0x0000014c, 0x00000003, 0x00000013, 0x0004003b, 0x0000014c, 0x0000014d, 0x00000003, 0x00040017,
    0x0000014e, 0x00000006, 0x00000003, 0x00050036, 0x00000002, 0x00000004, 0x00000000, 0x00000003,
    0x000200f8, 0x00000005, 0x0004003b, 0x000000b1, 0x0000013f, 0x00000007, 0x00040039, 0x00000013,
    0x00000140, 0x00000015, 0x0004003d, 0x00000013, 0x00000143, 0x00000142, 0x00050085, 0x00000013,
    0x00000144, 0x00000140, 0x00000143, 0x0003003e, 0x0000013f, 0x00000144, 0x0004003d, 0x00000006,
    0x00000147, 0x00000146, 0x00050041, 0x00000081, 0x00000148, 0x0000013f, 0x0000003d, 0x0004003d,
    0x00000006, 0x00000149, 0x00000148, 0x00050085, 0x00000006, 0x0000014a, 0x00000149, 0x00000147,
    0x00050041, 0x00000081, 0x0000014b, 0x0000013f, 0x0000003d, 0x0003003e, 0x0000014b, 0x0000014a,
    0x0004003d, 0x00000013, 0x0000014f, 0x0000013f, 0x0008004f, 0x0000014e, 0x00000150, 0x0000014f,
    0x0000014f, 0x00000000, 0x00000001, 0x00000002, 0x00050041, 0x00000081, 0x00000151, 0x0000013f,
    0x0000003d, 0x0004003d, 0x00000006, 0x00000152, 0x00000151, 0x0005008e, 0x0000014e, 0x00000153,
    0x00000150, 0x00000152, 0x00050041, 0x00000081, 0x00000154, 0x0000013f, 0x0000003d, 0x0004003d,
    0x00000006, 0x00000155, 0x00000154, 0x00050051, 0x00000006, 0x00000156, 0x00000153, 0x00000000,
    0x00050051, 0x00000006, 0x00000157, 0x00000153, 0x00000001, 0x00050051, 0x00000006, 0x00000158,
    0x00000153, 0x00000002, 0x00070050, 0x00000013, 0x00000159, 0x00000156, 0x00000157, 0x00000158,
    0x00000155, 0x0003003e, 0x0000014d, 0x00000159, 0x000100fd, 0x00010038, 0x00050036, 0x00000006,
    0x0000000b, 0x00000000, 0x00000009, 0x00030037, 0x00000008, 0x0000000a, 0x000200f8, 0x0000000c,
    0x00050041, 0x0000001d, 0x0000001e, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000001f,
    0x0000001e, 0x000500aa, 0x00000021, 0x00000022, 0x0000001f, 0x00000020, 0x000300f7, 0x00000024,
    0x00000000, 0x000400fa, 0x00000022, 0x00000023, 0x00000024, 0x000200f8, 0x00000023, 0x0004003d,
    0x00000026, 0x00000029, 0x00000028, 0x0004003d, 0x00000007, 0x0000002a, 0x0000000a, 0x00050057,
    0x00000013, 0x0000002b, 0x00000029, 0x0000002a, 0x00050051, 0x00000006, 0x0000002d, 0x0000002b,
    0x00000000, 0x000200fe, 0x0000002d, 0x000200f8, 0x00000024, 0x00050041, 0x0000001d, 0x0000002f,
    0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x00000030, 0x0000002f, 0x000500aa, 0x00000021,
    0x00000032, 0x00000030, 0x00000031, 0x000300f7, 0x00000034, 0x00000000, 0x000400fa, 0x00000032,
    0x00000033, 0x00000034, 0x000200f8, 0x00000033, 0x0004003d, 0x00000026, 0x00000036, 0x00000035,
    0x0004003d, 0x00000007, 0x00000037, 0x0000000a, 0x00050057, 0x00000013, 0x00000038, 0x00000036,
    0x00000037, 0x00050051, 0x00000006, 0x00000039, 0x00000038, 0x00000000, 0x000200fe, 0x00000039,
    0x000200f8, 0x00000034, 0x00050041, 0x0000001d, 0x0000003b, 0x0000001a, 0x0000001c, 0x0004003d,
    0x00000017, 0x0000003c, 0x0000003b, 0x000500aa, 0x00000021, 0x0000003e, 0x0000003c, 0x0000003d,
    0x000300f7, 0x00000040, 0x00000000, 0x000400fa, 0x0000003e, 0x0000003f, 0x00000040, 0x000200f8,
    0x0000003f, 0x0004003d, 0x00000026, 0x00000042, 0x00000041, 0x0004003d, 0x00000007, 0x00000043,
    0x0000000a, 0x00050057, 0x00000013, 0x00000044, 0x00000042, 0x00000043, 0x00050051, 0x00000006,
    0x00000045, 0x00000044, 0x00000000, 0x000200fe, 0x00000045, 0x000200f8, 0x00000040, 0x00050041,
    0x0000001d, 0x00000047, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x00000048, 0x00000047,
    0x000500aa, 0x00000021, 0x0000004a, 0x00000048, 0x00000049, 0x000300f7, 0x0000004c, 0x00000000,
    0x000400fa, 0x0000004a, 0x0000004b, 0x0000004c, 0x000200f8, 0x0000004b, 0x0004003d, 0x00000026,
    0x0000004e, 0x0000004d, 0x0004003d, 0x00000007, 0x0000004f, 0x0000000a, 0x00050057, 0x00000013,
    0x00000050, 0x0000004e, 0x0000004f, 0x00050051, 0x00000006, 0x00000051, 0x00000050, 0x00000000,
    0x000200fe, 0x00000051, 0x000200f8, 0x0000004c, 0x00050041, 0x0000001d, 0x00000053, 0x0000001a,
    0x0000001c, 0x0004003d, 0x00000017, 0x00000054, 0x00000053, 0x000500aa, 0x00000021, 0x00000056,
    0x00000054, 0x00000055, 0x000300f7, 0x00000058, 0x00000000, 0x000400fa, 0x00000056, 0x00000057,
    0x00000058, 0x000200f8, 0x00000057, 0x0004003d, 0x00000026, 0x0000005a, 0x00000059, 0x0004003d,
    0x00000007, 0x0000005b, 0x0000000a, 0x00050057, 0x00000013, 0x0000005c, 0x0000005a, 0x0000005b,
    0x00050051, 0x00000006, 0x0000005d, 0x0000005c, 0x00000000, 0x000200fe, 0x0000005d, 0x000200f8,
    0x00000058, 0x00050041, 0x0000001d, 0x0000005f, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017,
    0x00000060, 0x0000005f, 0x000500aa, 0x00000021, 0x00000062, 0x00000060, 0x00000061, 0x000300f7,
    0x00000064, 0x00000000, 0x000400fa, 0x00000062, 0x00000063, 0x00000064, 0x000200f8, 0x00000063,
    0x0004003d, 0x00000026, 0x00000066, 0x00000065, 0x0004003d, 0x00000007, 0x00000067, 0x0000000a,
    0x00050057, 0x00000013, 0x00000068, 0x00000066, 0x00000067, 0x00050051, 0x00000006, 0x00000069,
    0x00000068, 0x00000000, 0x000200fe, 0x00000069, 0x000200f8, 0x00000064, 0x00050041, 0x0000001d,
    0x0000006b, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000006c, 0x0000006b, 0x000500aa,
    0x00000021, 0x0000006e, 0x0000006c, 0x0000006d, 0x000300f7, 0x00000070, 0x00000000, 0x000400fa,
    0x0000006e, 0x0000006f, 0x00000070, 0x000200f8, 0x0000006f, 0x0004003d, 0x00000026, 0x00000072,
    0x00000071, 0x0004003d, 0x00000007, 0x00000073, 0x0000000a, 0x00050057, 0x00000013, 0x00000074,
    0x00000072, 0x00000073, 0x00050051, 0x00000006, 0x00000075, 0x00000074, 0x00000000, 0x000200fe,
    0x00000075, 0x000200f8, 0x00000070, 0x000200fe, 0x00000077, 0x00010038, 0x00050036, 0x00000006,
    0x0000000e, 0x00000000, 0x00000009, 0x00030037, 0x00000008, 0x0000000d, 0x000200f8, 0x0000000f,
    0x0004003b, 0x00000081, 0x00000082, 0x00000007, 0x0004003b, 0x00000008, 0x00000083, 0x00000007,
    0x00050041, 0x0000001d, 0x0000007a, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000007b,
    0x0000007a, 0x000500aa, 0x00000021, 0x0000007d, 0x0000007b, 0x0000007c, 0x000300f7, 0x0000007f,
    0x00000000, 0x000400fa, 0x0000007d, 0x0000007e, 0x0000007f, 0x000200f8, 0x0000007e, 0x000200fe,
    0x00000077, 0x000200f8, 0x0000007f, 0x0004003d, 0x00000007, 0x00000084, 0x0000000d, 0x0003003e,
    0x00000083, 0x00000084, 0x00050039, 0x00000006, 0x00000085, 0x0000000b, 0x00000083, 0x0003003e,
    0x00000082, 0x00000085, 0x00050041, 0x0000001d, 0x00000087, 0x0000001a, 0x00000086, 0x0004003d,
    0x00000017, 0x00000088, 0x00000087, 0x000500aa, 0x00000021, 0x00000089, 0x00000088, 0x00000020,
    0x000300f7, 0x0000008b, 0x00000000, 0x000400fa, 0x00000089, 0x0000008a, 0x0000008b, 0x000200f8,
    0x0000008a, 0x0004003d, 0x00000006, 0x0000008c, 0x00000082, 0x00050083, 0x00000006, 0x0000008d,
    0x00000077, 0x0000008c, 0x000200fe, 0x0000008d, 0x000200f8, 0x0000008b, 0x0004003d, 0x00000006,
    0x0000008f, 0x00000082, 0x000200fe, 0x0000008f, 0x00010038, 0x00050036, 0x00000007, 0x00000011,
    0x00000000, 0x00000010, 0x000200f8, 0x00000012, 0x0004003b, 0x00000081, 0x00000092, 0x00000007,
    0x0004003b, 0x00000008, 0x00000097, 0x00000007, 0x0004003b, 0x00000081, 0x00000099, 0x00000007,
    0x0004003b, 0x00000081, 0x0000009a, 0x00000007, 0x0004003b, 0x00000081, 0x0000009c, 0x00000007,
    0x0004003b, 0x00000081, 0x0000009e, 0x00000007, 0x0004003b, 0x00000081, 0x000000a0, 0x00000007,
    0x0004003b, 0x00000081, 0x000000a6, 0x00000007, 0x0004003b, 0x00000008, 0x000000a9, 0x00000007,
    0x0004003b, 0x000000b1, 0x000000b2, 0x00000007, 0x0004003b, 0x00000008, 0x000000bd, 0x00000007,
    0x0004003b, 0x00000008, 0x000000c3, 0x00000007, 0x0004003b, 0x00000008, 0x000000c9, 0x00000007,
    0x00050041, 0x00000094, 0x00000095, 0x0000001a, 0x00000093, 0x0004003d, 0x00000006, 0x00000096,
    0x00000095, 0x0003003e, 0x00000092, 0x00000096, 0x0003003e, 0x00000097, 0x00000098, 0x0003003e,
    0x00000099, 0x00000077, 0x0003003e, 0x0000009a, 0x0000009b, 0x0003003e, 0x0000009c, 0x0000009d,
    0x0003003e, 0x0000009e, 0x0000009f, 0x0004003d, 0x00000006, 0x000000a1, 0x00000092, 0x0004003d,
    0x00000006, 0x000000a2, 0x00000099, 0x00050085, 0x00000006, 0x000000a3, 0x000000a1, 0x000000a2,
    0x0004003d, 0x00000006, 0x000000a4, 0x0000009e, 0x00050081, 0x00000006, 0x000000a5, 0x000000a3,
    0x000000a4, 0x0003003e, 0x000000a0, 0x000000a5, 0x0004003d, 0x00000006, 0x000000a7, 0x000000a0,
    0x0006000c, 0x00000006, 0x000000a8, 0x00000001, 0x00000008, 0x000000a7, 0x0003003e, 0x000000a6,
    0x000000a8, 0x0004003d, 0x00000006, 0x000000ab, 0x000000a6, 0x00050050, 0x00000007, 0x000000ad,
    0x000000ab, 0x000000ab, 0x00050081, 0x00000007, 0x000000ae, 0x000000ad, 0x000000ac, 0x0005008e,
    0x00000007, 0x000000af, 0x000000ae, 0x000000aa, 0x0006000c, 0x00000007, 0x000000b0, 0x00000001,
    0x0000000d, 0x000000af, 0x0003003e, 0x000000a9, 0x000000b0, 0x0004003d, 0x00000006, 0x000000b4,
    0x000000a6, 0x00070050, 0x00000013, 0x000000b6, 0x000000b4, 0x000000b4, 0x000000b4, 0x000000b4,
    0x00050081, 0x00000013, 0x000000b7, 0x000000b6, 0x000000b5, 0x0005008e, 0x00000013, 0x000000b8,
    0x000000b7, 0x000000b3, 0x00050081, 0x00000013, 0x000000bb, 0x000000b8, 0x000000ba, 0x0006000c,
    0x00000013, 0x000000bc, 0x00000001, 0x0000000d, 0x000000bb, 0x0003003e, 0x000000b2, 0x000000bc,
    0x0004003d, 0x00000007, 0x000000be, 0x000000a9, 0x0007004f, 0x00000007, 0x000000bf, 0x000000be,
    0x000000be, 0x00000000, 0x00000000, 0x0004003d, 0x00000013, 0x000000c0, 0x000000b2, 0x0007004f,
    0x00000007, 0x000000c1, 0x000000c0, 0x000000c0, 0x00000000, 0x00000001, 0x00050081, 0x00000007,
    0x000000c2, 0x000000bf, 0x000000c1, 0x0003003e, 0x000000bd, 0x000000c2, 0x0004003d, 0x00000007,
    0x000000c4, 0x000000a9, 0x0007004f, 0x00000007, 0x000000c5, 0x000000c4, 0x000000c4, 0x00000001,
    0x00000001, 0x0004003d, 0x00000013, 0x000000c6, 0x000000b2, 0x0007004f, 0x00000007, 0x000000c7,
    0x000000c6, 0x000000c6, 0x00000002, 0x00000003, 0x00050081, 0x00000007, 0x000000c8, 0x000000c5,
    0x000000c7, 0x0003003e, 0x000000c3, 0x000000c8, 0x0004003d, 0x00000007, 0x000000ca, 0x000000bd,
    0x0004003d, 0x00000007, 0x000000cb, 0x000000c3, 0x0004003d, 0x00000006, 0x000000cc, 0x0000009a,
    0x00050083, 0x00000006, 0x000000cd, 0x00000077, 0x000000cc, 0x0004003d, 0x00000006, 0x000000ce,
    0x000000a0, 0x0006000c, 0x00000006, 0x000000cf, 0x00000001, 0x0000000a, 0x000000ce, 0x00050085,
    0x00000006, 0x000000d1, 0x000000cf, 0x000000d0, 0x0006000c, 0x00000006, 0x000000d2, 0x00000001,
    0x0000000e, 0x000000d1, 0x00050085, 0x00000006, 0x000000d4, 0x000000d2, 0x000000d3, 0x00050081,
    0x00000006, 0x000000d5, 0x000000d4, 0x0000009d, 0x0008000c, 0x00000006, 0x000000d6, 0x00000001,
    0x00000031, 0x000000cd, 0x00000077, 0x000000d5, 0x00050050, 0x00000007, 0x000000d7, 0x000000d6,
    0x000000d6, 0x0008000c, 0x00000007, 0x000000d8, 0x00000001, 0x0000002e, 0x000000ca, 0x000000cb,
    0x000000d7, 0x0003003e, 0x000000c9, 0x000000d8, 0x0004003d, 0x00000006, 0x000000d9, 0x000000a0,
    0x0006000c, 0x00000006, 0x000000da, 0x00000001, 0x0000000d, 0x000000d9, 0x0004003d, 0x00000006,
    0x000000db, 0x0000009c, 0x00050085, 0x00000006, 0x000000dc, 0x000000da, 0x000000db, 0x00050041,
    0x00000081, 0x000000dd, 0x000000c9, 0x0000002c, 0x0004003d, 0x00000006, 0x000000de, 0x000000dd,
    0x00050081, 0x00000006, 0x000000df, 0x000000de, 0x000000dc, 0x00050041, 0x00000081, 0x000000e0,
    0x000000c9, 0x0000002c, 0x0003003e, 0x000000e0, 0x000000df, 0x0004003d, 0x00000006, 0x000000e1,
    0x000000a0, 0x0006000c, 0x00000006, 0x000000e2, 0x00000001, 0x0000000e, 0x000000e1, 0x0004003d,
    0x00000006, 0x000000e3, 0x0000009c, 0x00050085, 0x00000006, 0x000000e4, 0x000000e2, 0x000000e3,
    0x00050041, 0x00000081, 0x000000e5, 0x000000c9, 0x00000020, 0x0004003d, 0x00000006, 0x000000e6,
    0x000000e5, 0x00050081, 0x00000006, 0x000000e7, 0x000000e6, 0x000000e4, 0x00050041, 0x00000081,
    0x000000e8, 0x000000c9, 0x00000020, 0x0003003e, 0x000000e8, 0x000000e7, 0x0004003d, 0x00000007,
    0x000000e9, 0x00000097, 0x0005008e, 0x00000007, 0x000000eb, 0x000000e9, 0x000000ea, 0x0004003d,
    0x00000007, 0x000000ec, 0x000000c9, 0x00050085, 0x00000007, 0x000000ed, 0x000000ec, 0x000000eb,
    0x0003003e, 0x000000c9, 0x000000ed, 0x0004003d, 0x00000007, 0x000000ee, 0x000000c9, 0x000200fe,
    0x000000ee, 0x00010038, 0x00050036, 0x00000013, 0x00000015, 0x00000000, 0x00000014, 0x000200f8,
    0x00000016, 0x0004003b, 0x00000081, 0x000000fc, 0x00000007, 0x0004003b, 0x00000008, 0x000000ff,
    0x00000007, 0x0004003b, 0x00000008, 0x00000102, 0x00000007, 0x0004003b, 0x00000081, 0x00000119,
    0x00000007, 0x0004003b, 0x00000008, 0x0000011a, 0x00000007, 0x0004003b, 0x00000008, 0x0000011d,
    0x00000007, 0x0004003b, 0x000000b1, 0x00000121, 0x00000007, 0x0004003b, 0x000000b1, 0x00000131,
    0x00000007, 0x0004003b, 0x00000008, 0x00000135, 0x00000007, 0x00050041, 0x0000001d, 0x000000f1,
    0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x000000f2, 0x000000f1, 0x000500ab, 0x00000021,
    0x000000f3, 0x000000f2, 0x0000007c, 0x000300f7, 0x000000f5, 0x00000000, 0x000400fa, 0x000000f3,
    0x000000f4, 0x000000f5, 0x000200f8, 0x000000f4, 0x00050041, 0x0000001d, 0x000000f6, 0x0000001a,
    0x00000086, 0x0004003d, 0x00000017, 0x000000f7, 0x000000f6, 0x000500aa, 0x00000021, 0x000000f8,
    0x000000f7, 0x00000031, 0x000200f9, 0x000000f5, 0x000200f8, 0x000000f5, 0x000700f5, 0x00000021,
    0x000000f9, 0x000000f3, 0x00000016, 0x000000f8, 0x000000f4, 0x000300f7, 0x000000fb, 0x00000000,
    0x000400fa, 0x000000f9, 0x000000fa, 0x000000fb, 0x000200f8, 0x000000fa, 0x0004003d, 0x00000007,
    0x00000100, 0x000000fe, 0x0003003e, 0x000000ff, 0x00000100, 0x00050039, 0x00000006, 0x00000101,
    0x0000000b, 0x000000ff, 0x0003003e, 0x000000fc, 0x00000101, 0x00040039, 0x00000007, 0x00000103,
    0x00000011, 0x0004003d, 0x00000006, 0x00000104, 0x000000fc, 0x0005008e, 0x00000007, 0x00000105,
    0x00000103, 0x00000104, 0x0003003e, 0x00000102, 0x00000105, 0x0004003d, 0x00000026, 0x00000107,
    0x00000106, 0x0004003d, 0x00000007, 0x00000109, 0x00000108, 0x0004003d, 0x00000007, 0x0000010a,
    0x00000102, 0x00050081, 0x00000007, 0x0000010b, 0x00000109, 0x0000010a, 0x00050057, 0x00000013,
    0x0000010c, 0x00000107, 0x0000010b, 0x000200fe, 0x0000010c, 0x000200f8, 0x000000fb, 0x00050041,
    0x0000001d, 0x0000010e, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000010f, 0x0000010e,
    0x000500ab, 0x00000021, 0x00000110, 0x0000010f, 0x0000007c, 0x000300f7, 0x00000112, 0x00000000,
    0x000400fa, 0x00000110, 0x00000111, 0x00000112, 0x000200f8, 0x00000111, 0x00050041, 0x0000001d,
    0x00000113, 0x0000001a, 0x00000086, 0x0004003d, 0x00000017, 0x00000114, 0x00000113, 0x000500aa,
    0x00000021, 0x00000115, 0x00000114, 0x00000049, 0x000200f9, 0x00000112, 0x000200f8, 0x00000112,
    0x000700f5, 0x00000021, 0x00000116, 0x00000110, 0x000000fb, 0x00000115, 0x00000111, 0x000300f7,
    0x00000118, 0x00000000, 0x000400fa, 0x00000116, 0x00000117, 0x00000118, 0x000200f8, 0x00000117,
    0x0004003d, 0x00000007, 0x0000011b, 0x000000fe, 0x0003003e, 0x0000011a, 0x0000011b, 0x00050039,
    0x00000006, 0x0000011c, 0x0000000b, 0x0000011a, 0x0003003e, 0x00000119, 0x0000011c, 0x00040039,
    0x00000007, 0x0000011e, 0x00000011, 0x0004003d, 0x00000006, 0x0000011f, 0x00000119, 0x0005008e,
    0x00000007, 0x00000120, 0x0000011e, 0x0000011f, 0x0003003e, 0x0000011d, 0x00000120, 0x0004003d,
    0x00000026, 0x00000122, 0x00000106, 0x0004003d, 0x00000007, 0x00000123, 0x00000108, 0x0004003d,
    0x00000007, 0x00000124, 0x0000011d, 0x00050081, 0x00000007, 0x00000125, 0x00000123, 0x00000124,
    0x00050057, 0x00000013, 0x00000126, 0x00000122, 0x00000125, 0x0003003e, 0x00000121, 0x00000126,
    0x0004003d, 0x00000026, 0x00000127, 0x00000035, 0x0004003d, 0x00000007, 0x00000128, 0x000000fe,
    0x00050057, 0x00000013, 0x00000129, 0x00000127, 0x00000128, 0x00050051, 0x00000006, 0x0000012a,
    0x00000129, 0x00000000, 0x00050041, 0x00000081, 0x0000012b, 0x00000121, 0x0000003d, 0x0004003d,
    0x00000006, 0x0000012c, 0x0000012b, 0x00050085, 0x00000006, 0x0000012d, 0x0000012c, 0x0000012a,
    0x00050041, 0x00000081, 0x0000012e, 0x00000121, 0x0000003d, 0x0003003e, 0x0000012e, 0x0000012d,
    0x0004003d, 0x00000013, 0x0000012f, 0x00000121, 0x000200fe, 0x0000012f, 0x000200f8, 0x00000118,
    0x0004003d, 0x00000026, 0x00000132, 0x00000106, 0x0004003d, 0x00000007, 0x00000133, 0x00000108,
    0x00050057, 0x00000013, 0x00000134, 0x00000132, 0x00000133, 0x0003003e, 0x00000131, 0x00000134,
    0x0004003d, 0x00000007, 0x00000136, 0x000000fe, 0x0003003e, 0x00000135, 0x00000136, 0x00050039,
    0x00000006, 0x00000137, 0x0000000e, 0x00000135, 0x00050041, 0x00000081, 0x00000138, 0x00000131,
    0x0000003d, 0x0004003d, 0x00000006, 0x00000139, 0x00000138, 0x00050085, 0x00000006, 0x0000013a,
    0x00000139, 0x00000137, 0x00050041, 0x00000081, 0x0000013b, 0x00000131, 0x0000003d, 0x0003003e,
    0x0000013b, 0x0000013a, 0x0004003d, 0x00000013, 0x0000013c, 0x00000131, 0x000200fe, 0x0000013c,
    0x00010038,
];

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERRIPPLE_FRAGMENT_SPIRV: [u32; 1693] =
    include!("shaders/sampled_image_waterripple.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERWAVES_FRAGMENT_SPIRV: [u32; 2895] =
    include!("shaders/sampled_image_waterwaves.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERFLOW_FRAGMENT_SPIRV: [u32; 1765] =
    include!("shaders/sampled_image_waterflow.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERCAUSTICS_FRAGMENT_SPIRV: [u32; 5037] =
    include!("shaders/sampled_image_watercaustics.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FOLIAGE_SWAY_FRAGMENT_SPIRV: [u32; 2230] =
    include!("shaders/sampled_image_foliagesway.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_AUTO_SWAY_FRAGMENT_SPIRV: [u32; 4870] =
    include!("shaders/sampled_image_autosway.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SCROLL_FRAGMENT_SPIRV: [u32; 945] =
    include!("shaders/sampled_image_scroll.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SKEW_FRAGMENT_SPIRV: [u32; 1148] =
    include!("shaders/sampled_image_skew.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_IRIS_FRAGMENT_SPIRV: [u32; 1436] =
    include!("shaders/sampled_image_iris.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_OPACITY_FRAGMENT_SPIRV: [u32; 821] =
    include!("shaders/sampled_image_opacity.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_TECHCIRCLE_FRAGMENT_SPIRV: [u32; 2909] =
    include!("shaders/sampled_image_techcircle.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_AUDIOBARS_FRAGMENT_SPIRV: [u32; 4074] =
    include!("shaders/sampled_image_audiobars.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PASSTHROUGHBLEND_FRAGMENT_SPIRV: [u32;
    9663] = include!("shaders/sampled_image_passthroughblend.frag.spv.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::SceneRenderAlphaTextureMode;

    fn blend_state(
        mode: SceneBlendMode,
    ) -> super::super::present::NativeVulkanVulkanaliaSceneBlendState {
        super::super::present::NativeVulkanVulkanaliaSceneBlendState::from_mode(mode)
    }

    fn sampled_image_material(
        blend_mode: SceneBlendMode,
    ) -> super::super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial {
        super::super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial::sampled_image(
            blend_mode,
            None,
            SceneRenderAlphaTextureMode::Multiply,
            1,
        )
    }

    fn effect_vec2_uniform(
        name: &str,
        values: [f32; 2],
    ) -> super::super::present::NativeVulkanVulkanaliaSceneEffectUniform {
        super::super::present::NativeVulkanVulkanaliaSceneEffectUniform {
            name: name.to_owned(),
            value_kind: "vec2",
            component_count: 2,
            float_bits: [values[0].to_bits(), values[1].to_bits(), 0, 0],
            int_values: [0; 4],
        }
    }

    fn push_f32(bytes: &[u8], offset: usize) -> f32 {
        f32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn push_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn texture_slot_bindings(
        resources: &[u32],
    ) -> Vec<super::super::present::NativeVulkanVulkanaliaSceneTextureSlotResourceBinding> {
        resources
            .iter()
            .copied()
            .enumerate()
            .map(|(slot, resource_index)| {
                super::super::present::NativeVulkanVulkanaliaSceneTextureSlotResourceBinding {
                    slot: slot.min(u32::MAX as usize) as u32,
                    resource_index,
                }
            })
            .collect()
    }

    fn input() -> NativeVulkanVulkanaliaSceneDrawPassInput {
        NativeVulkanVulkanaliaSceneDrawPassInput {
            plan_ready: true,
            native_draw_ready: true,
            draw_op_count: 1,
            backend_status: "solid-quad-recording-ready",
            blocking_reason: None,
            fast_clear_color_ready: false,
            clear_background_op_count: 0,
            quad_recording_ready: true,
            quad_recording_step_count: 1,
            quad_vertex_buffer_bytes: 96,
            quad_index_buffer_bytes: 24,
            sampled_image_recording_ready: false,
            sampled_image_implicit_full_extent_ready: false,
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
    fn solid_quad_scene_path_is_dynamic_rendering_recordable() {
        let snapshot = native_vulkan_vulkanalia_scene_draw_pass_snapshot(input());

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "solid-quad-dynamic-rendering-recording-ready"
        );
        assert_eq!(
            snapshot.pipeline_labels,
            vec![
                "scene-solid-quad-alpha-blend",
                "scene-solid-quad-normal-blend",
                "scene-solid-quad-additive-blend",
                "scene-solid-quad-multiply-blend",
                "scene-solid-quad-screen-blend",
                "scene-solid-quad-max-blend",
                "scene-solid-quad-modulate-blend",
                "scene-solid-quad-hsl-color-blend",
                "scene-solid-quad-alpha-to-coverage"
            ]
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
    fn sampled_image_scene_path_is_dynamic_rendering_recordable() {
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
        input.sampled_image_vertex_buffer_bytes = 176;
        input.sampled_image_index_buffer_bytes = 24;
        input.vector_shape_op_count = 0;

        let snapshot = native_vulkan_vulkanalia_scene_draw_pass_snapshot(input);

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "sampled-image-dynamic-rendering-recording-ready"
        );
        assert_eq!(snapshot.blocking_reason, None);
        assert_eq!(
            snapshot.pipeline_labels,
            vec![
                "scene-sampled-image-alpha-blend",
                "scene-sampled-image-normal-blend",
                "scene-sampled-image-additive-blend",
                "scene-sampled-image-multiply-blend",
                "scene-sampled-image-screen-blend",
                "scene-sampled-image-max-blend",
                "scene-sampled-image-modulate-blend",
                "scene-sampled-image-hsl-color-blend",
                "scene-sampled-image-alpha-to-coverage"
            ]
        );
        assert_eq!(snapshot.descriptor_set_count, 0);
        assert_eq!(
            snapshot.vertex_stride_bytes,
            SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES
        );
        assert_eq!(snapshot.draw_indexed_count, 1);
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_bind_scene_descriptor_heap")
        );
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_draw_indexed_per_image_quad")
        );
    }

    #[test]
    fn sampled_image_implicit_full_extent_path_is_present_ready() {
        let mut input = input();
        input.draw_op_count = 1;
        input.backend_status = "sampled-image-implicit-full-extent-ready";
        input.quad_recording_ready = false;
        input.quad_recording_step_count = 0;
        input.quad_vertex_buffer_bytes = 0;
        input.quad_index_buffer_bytes = 0;
        input.sampled_image_implicit_full_extent_ready = true;
        input.sampled_image_op_count = 1;
        input.vector_shape_op_count = 0;

        let snapshot = native_vulkan_vulkanalia_scene_draw_pass_snapshot(input);

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "sampled-image-implicit-full-extent-present-ready"
        );
        assert_eq!(snapshot.blocking_reason, None);
        assert_eq!(snapshot.sampled_image_quad_count, 1);
        assert_eq!(
            snapshot.pipeline_labels,
            vec![
                "scene-sampled-image-alpha-blend",
                "scene-sampled-image-normal-blend",
                "scene-sampled-image-additive-blend",
                "scene-sampled-image-multiply-blend",
                "scene-sampled-image-screen-blend",
                "scene-sampled-image-max-blend",
                "scene-sampled-image-modulate-blend",
                "scene-sampled-image-hsl-color-blend",
                "scene-sampled-image-alpha-to-coverage"
            ]
        );
        assert_eq!(
            snapshot.vertex_stride_bytes,
            SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES
        );
        assert_eq!(snapshot.draw_indexed_count, 1);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_draw_indexed_per_image_quad")
        );
    }

    #[test]
    fn mixed_full_extent_sampled_image_path_is_present_ready() {
        let mut input = input();
        input.draw_op_count = 2;
        input.backend_status = "mixed-quad-sampled-image-implicit-full-extent-ready";
        input.sampled_image_implicit_full_extent_ready = true;
        input.sampled_image_op_count = 1;

        let snapshot = native_vulkan_vulkanalia_scene_draw_pass_snapshot(input);

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "mixed-quad-sampled-image-implicit-full-extent-present-ready"
        );
        assert_eq!(snapshot.blocking_reason, None);
        assert_eq!(snapshot.solid_quad_count, 1);
        assert_eq!(snapshot.sampled_image_quad_count, 1);
        assert_eq!(
            snapshot.pipeline_labels,
            vec![
                "scene-solid-quad-alpha-blend",
                "scene-solid-quad-normal-blend",
                "scene-solid-quad-additive-blend",
                "scene-solid-quad-multiply-blend",
                "scene-solid-quad-screen-blend",
                "scene-solid-quad-max-blend",
                "scene-solid-quad-modulate-blend",
                "scene-solid-quad-hsl-color-blend",
                "scene-solid-quad-alpha-to-coverage",
                "scene-sampled-image-alpha-blend",
                "scene-sampled-image-normal-blend",
                "scene-sampled-image-additive-blend",
                "scene-sampled-image-multiply-blend",
                "scene-sampled-image-screen-blend",
                "scene-sampled-image-max-blend",
                "scene-sampled-image-modulate-blend",
                "scene-sampled-image-hsl-color-blend",
                "scene-sampled-image-alpha-to-coverage"
            ]
        );
        assert_eq!(snapshot.draw_indexed_count, 2);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_draw_indexed_in_scene_layer_order")
        );
    }

    #[test]
    fn draw_pass_snapshot_uses_recordable_backend_status_for_effect_chain_steps() {
        let mut input = input();
        input.plan_ready = false;
        input.native_draw_ready = false;
        input.draw_op_count = 76;
        input.backend_status = "mixed-quad-sampled-image-recording-ready";
        input.quad_recording_ready = false;
        input.quad_recording_step_count = 10;
        input.quad_vertex_buffer_bytes = 960;
        input.quad_index_buffer_bytes = 240;
        input.sampled_image_recording_ready = true;
        input.sampled_image_op_count = 66;
        input.sampled_image_recording_step_count = 67;
        input.sampled_image_vertex_buffer_bytes = 1_234_024;
        input.sampled_image_index_buffer_bytes = 746_328;
        input.vector_shape_op_count = 10;

        let snapshot = native_vulkan_vulkanalia_scene_draw_pass_snapshot(input);

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "mixed-quad-sampled-image-dynamic-rendering-recording-ready"
        );
        assert_eq!(snapshot.blocking_reason, None);
        assert_eq!(snapshot.solid_quad_count, 10);
        assert_eq!(snapshot.sampled_image_quad_count, 67);
        assert_eq!(snapshot.draw_indexed_count, 77);
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_draw_indexed_in_scene_layer_order")
        );
    }

    #[test]
    fn solid_quad_pipeline_template_uses_dynamic_rendering_and_push_constants() {
        let snapshot = native_vulkan_vulkanalia_scene_solid_quad_pipeline_snapshot(
            vk::Format::B8G8R8A8_SRGB,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            vk::SampleCountFlags::_1,
        );

        assert_eq!(snapshot.target_format, "B8G8R8A8_SRGB");
        assert_eq!(snapshot.extent, (3840, 2160));
        assert_eq!(
            snapshot.render_pass_compatibility,
            "dynamic-rendering-no-render-pass"
        );
        assert_eq!(snapshot.vertex_input_binding_count, 1);
        assert_eq!(snapshot.vertex_input_attribute_count, 2);
        assert_eq!(snapshot.vertex_stride_bytes, 24);
        assert_eq!(snapshot.push_constant_bytes, 8);
        assert_eq!(
            snapshot.push_constant_model,
            "scene-space pixel extent -> NDC conversion in vertex shader"
        );
        assert!(snapshot.uses_pipeline_rendering_create_info);
        assert!(snapshot.uses_dynamic_rendering);
    }

    #[test]
    fn sampled_image_pipeline_template_uses_descriptor_heap_and_dynamic_rendering() {
        let snapshot = native_vulkan_vulkanalia_scene_sampled_image_pipeline_snapshot(
            vk::Format::B8G8R8A8_SRGB,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            vk::SampleCountFlags::_1,
        );

        assert_eq!(snapshot.target_format, "B8G8R8A8_SRGB");
        assert_eq!(snapshot.extent, (3840, 2160));
        assert!(!snapshot.descriptor_set_layout_created);
        assert_eq!(snapshot.descriptor_type, "combined-image-sampler");
        assert_eq!(snapshot.descriptor_binding, 0);
        assert_eq!(snapshot.vertex_input_attribute_count, 5);
        assert_eq!(
            snapshot.vertex_stride_bytes,
            SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES
        );
        assert_eq!(snapshot.vertex_uv_format, "R32G32_SFLOAT");
        assert_eq!(snapshot.vertex_effect_uv_format, "R32G32_SFLOAT");
        assert_eq!(snapshot.vertex_opacity_format, "R32_SFLOAT");
        assert_eq!(snapshot.vertex_tint_format, "R32G32B32A32_SFLOAT");
        assert_eq!(
            snapshot.sampled_image_model,
            "retained native sampled image -> VK_EXT_descriptor_heap constant-offset mapping -> generic, framebuffer-passthrough, or pass-specific fragment shader"
        );
        assert_eq!(snapshot.pass_specific_fragment_pipeline_count, 108);
        assert!(snapshot.uses_pipeline_rendering_create_info);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert_eq!(snapshot.descriptor_set_count, 0);
        assert_eq!(snapshot.descriptor_model, "VK_EXT_descriptor_heap");
        assert_eq!(snapshot.push_constant_bytes, 256);
        assert_eq!(
            snapshot.push_constant_model,
            "scene-space pixel extent, alpha/mask state, elapsed time, WE g_TextureNResolution rows, and pass-specific effect parameter rows"
        );
        assert!(snapshot.descriptor_heap_mapping_enabled);
        assert!(snapshot.descriptor_heap_pipeline_flag_enabled);
        assert_eq!(
            snapshot.blend_model,
            "sampled rgba with opacity; alpha/normal/additive/multiply/screen/max/modulate/hsl-color blend pipeline selected per draw command; WE passthroughblend uses shader framebuffer sampling plus normal replace output"
        );
        assert!(snapshot.descriptor_set_layout_create_flags.is_empty());
        assert!(!snapshot.uses_push_descriptor_fast_path);
    }

    #[test]
    fn sampled_image_push_constants_encode_we_texture_resolution_rows() {
        let mut material = sampled_image_material(SceneBlendMode::Alpha);
        material.alpha_texture_slot = Some(1);
        material.alpha_texture_mode = SceneRenderAlphaTextureMode::Coverage;
        material.system_shader_uniforms = vec![
            effect_vec2_uniform("g_Texture0Resolution", [663.0, 230.0]),
            effect_vec2_uniform("g_Texture2Resolution", [512.0, 256.0]),
            effect_vec2_uniform("not-a-texture-resolution", [1.0, 1.0]),
        ];
        material.constant_shader_uniforms = vec![
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "strength",
                &serde_json::json!(0.1),
            )
            .expect("float constant uniform"),
        ];

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            1234,
        );

        assert_eq!(push_f32(&bytes, 0), 3840.0);
        assert_eq!(push_f32(&bytes, 4), 2160.0);
        assert_eq!(push_u32(&bytes, 8), 1);
        assert_eq!(
            push_u32(&bytes, 12),
            SceneRenderAlphaTextureMode::Coverage.shader_code()
        );
        assert_eq!(push_f32(&bytes, 16), 1.2340001);
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_MASK_OFFSET_BYTES
            ),
            0b0000_0101
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SYSTEM_UNIFORM_COUNT_OFFSET_BYTES
            ),
            3
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_UNIFORM_COUNT_OFFSET_BYTES
            ),
            1
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_BASE_OFFSET_BYTES
            ),
            663.0
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_BASE_OFFSET_BYTES + 4
            ),
            230.0
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_OUTPUT_FLAGS_OFFSET_BYTES
            ),
            0
        );
        let slot_2_offset = SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_BASE_OFFSET_BYTES
            + 2 * SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_STRIDE_BYTES;
        assert_eq!(push_f32(&bytes, slot_2_offset), 512.0);
        assert_eq!(push_f32(&bytes, slot_2_offset + 4), 256.0);
    }

    #[test]
    fn sampled_image_push_constants_mark_premultiplied_output_blends() {
        let alpha = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &sampled_image_material(SceneBlendMode::Alpha),
            0,
        );
        assert_eq!(
            push_u32(
                &alpha,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_OUTPUT_FLAGS_OFFSET_BYTES
            ),
            0
        );

        for blend in [
            SceneBlendMode::Multiply,
            SceneBlendMode::Screen,
            SceneBlendMode::Max,
            SceneBlendMode::Modulate,
        ] {
            let bytes = scene_sampled_image_push_constant_bytes(
                vk::Extent2D {
                    width: 3840,
                    height: 2160,
                },
                &sampled_image_material(blend),
                0,
            );
            assert_eq!(
                push_u32(
                    &bytes,
                    SCENE_FULL_SAMPLED_IMAGE_PUSH_OUTPUT_FLAGS_OFFSET_BYTES
                ),
                SCENE_SAMPLED_IMAGE_OUTPUT_FLAG_PREMULTIPLY_RGB
            );
        }
    }

    #[test]
    fn sampled_image_push_constants_encode_passthrough_blend_mode() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        material.shader = Some("util/effectpassthrough".to_owned());
        material.texture_slot_count = 2;
        material.combo_values.insert("BLENDMODE".to_owned(), 28);

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 1280,
                height: 720,
            },
            &material,
            0,
        );

        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::PassthroughBlend
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_PASSTHROUGH_BLEND_MODE_OFFSET_BYTES
            ),
            28
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_PASSTHROUGHBLEND
        );
    }

    #[test]
    fn passthroughblend_fragments_cover_we_apply_blending_modes() {
        let sampled_source = include_str!("shaders/sampled_image_passthroughblend.frag");
        let solid_source = include_str!("shaders/solid_quad_passthroughblend.frag");

        for mode in 1..=32 {
            assert!(
                sampled_source.contains(&format!("mode == {mode}u")) || matches!(mode, 20 | 22),
                "sampled passthroughblend should cover WE ApplyBlending mode {mode}"
            );
        }
        assert!(sampled_source.contains("mode == 4u || mode == 20u"));
        assert!(sampled_source.contains("mode == 22u"));
        assert!(sampled_source.contains("out_color.a = screen.a;"));
        assert!(solid_source.contains("apply_blending(28u, screen.rgb, v_color.rgb, v_color.a)"));
        assert!(solid_source.contains("out_color.a = screen.a;"));
    }

    #[test]
    fn sampled_image_shader_program_selects_pass_specific_single_effect_kinds() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::Generic
        );

        material.shader = Some("util/effectpassthrough".to_owned());
        material.combo_values.insert("BLENDMODE".to_owned(), 28);
        material.texture_slot_count = 2;
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::PassthroughBlend
        );
        material.shader = None;
        material.combo_values.clear();
        material.texture_slot_count = 1;

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterRipple];
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::WaterRipple
        );

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterWaves];
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::WaterWaves
        );

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterFlow];
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::WaterFlow
        );

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterCaustics];
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::WaterCaustics
        );

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::FoliageSway];
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::FoliageSway
        );

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::AutoSway];
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::AutoSway
        );

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::Scroll];
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::Scroll
        );

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::Skew];
        material.combo_values.clear();
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::Skew
        );
        material.combo_values.insert("MODE".to_owned(), 1);
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::Skew
        );
        material.combo_values.clear();

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::Iris];
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::Iris
        );

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::OpacityMask];
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::Opacity
        );

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::TechCircle];
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::TechCircle
        );

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::AudioBars];
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::AudioBars
        );

        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::Iris];
        material
            .effect_kinds
            .push(super::super::present::NativeVulkanVulkanaliaSceneEffectKind::OpacityMask);
        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::Generic
        );
    }

    #[test]
    fn sampled_image_push_constants_encode_audiobars_shape_constants() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::AudioBars];
        material.combo_values.insert("SHAPE".to_owned(), 7);
        material.system_shader_uniforms = vec![effect_vec2_uniform(
            "g_Texture0Resolution",
            [1000.0, 1000.0],
        )];
        material.constant_shader_values = serde_json::from_value(serde_json::json!({
            "Bar Count": 12.0,
            "Bar Spacing": 0.31,
            "Lower/Upper Bar Bounds": "0.1 0.1",
            "Minimum Height (Will be multiplied by the bar width) ": 1.0,
            "Radius": 1.0,
            "Volume Factor": 0.5,
            "Anti-alias blurring ": "0.01 0.04"
        }))
        .expect("audio bars constant shader values");

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 1280,
                height: 720,
            },
            &material,
            5000,
        );

        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::AudioBars
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_AUDIOBARS
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_FLAGS_OFFSET_BYTES
            ),
            7
        );
        assert!(
            (push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_BAR_COUNT_OFFSET_BYTES
            ) - 12.0)
                .abs()
                < f32::EPSILON
        );
        assert!(
            (push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_BAR_SPACING_OFFSET_BYTES
            ) - 0.31)
                .abs()
                < 0.00001
        );
        assert!(
            (push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_BOUNDS_LOW_OFFSET_BYTES
            ) - 0.1)
                .abs()
                < 0.00001
        );
        assert!(
            (push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_BOUNDS_HIGH_OFFSET_BYTES
            ) - 0.1)
                .abs()
                < 0.00001
        );
        assert!(
            (push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_AA_X_OFFSET_BYTES
            ) - 0.01)
                .abs()
                < 0.00001
        );
        assert!(
            (push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_AA_Y_OFFSET_BYTES
            ) - 0.04)
                .abs()
                < 0.00001
        );
        assert!(
            (push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_RADIUS_OFFSET_BYTES
            ) - 1.0)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn sampled_image_push_constants_encode_iris_motion_constants() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::Iris];
        material.alpha_texture_slot = Some(1);
        material.alpha_texture_mode = SceneRenderAlphaTextureMode::Iris;
        material.constant_shader_values = serde_json::from_value(serde_json::json!({
            "scale": "2 3",
            "speed": 1.25,
            "rough": 0.4,
            "noiseamount": 0.75,
            "phase": -0.5
        }))
        .expect("constant shader values");
        material.system_shader_uniforms =
            vec![effect_vec2_uniform("g_Texture1Resolution", [331.0, 115.0])];

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            2500,
        );

        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::Iris
        );
        assert_eq!(push_f32(&bytes, 16), 2.5);
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_SCALE_X_OFFSET_BYTES
            ),
            2.0
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_SCALE_Y_OFFSET_BYTES
            ),
            3.0
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_SPEED_OFFSET_BYTES
            ),
            1.25
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_ROUGH_OFFSET_BYTES
            ),
            0.4
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_NOISE_AMOUNT_OFFSET_BYTES
            ),
            0.75
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_IRIS_PHASE_OFFSET_BYTES
            ),
            -0.5
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_MASK_OFFSET_BYTES
            ),
            0b0000_0010
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_IRIS
        );
    }

    #[test]
    fn sampled_image_push_constants_encode_opacity_alpha_constant() {
        let mut material = sampled_image_material(SceneBlendMode::Alpha);
        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::OpacityMask];
        material.alpha_texture_slot = Some(1);
        material.alpha_texture_mode = SceneRenderAlphaTextureMode::Coverage;
        material.constant_shader_values = serde_json::from_value(serde_json::json!({
            "alpha": 0.42
        }))
        .expect("constant shader values");
        material.system_shader_uniforms =
            vec![effect_vec2_uniform("g_Texture1Resolution", [331.0, 115.0])];

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            2500,
        );

        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::Opacity
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_OPACITY_ALPHA_OFFSET_BYTES
            ),
            0.42
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_MASK_OFFSET_BYTES
            ),
            0b0000_0010
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_OPACITY
        );
    }

    #[test]
    fn sampled_image_push_constants_encode_waterripple_constants() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterRipple];
        material.constant_shader_uniforms = vec![
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "ripplestrength",
                &serde_json::json!(0.42),
            )
            .expect("ripplestrength"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "animationspeed",
                &serde_json::json!(0.25),
            )
            .expect("animationspeed"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "scale",
                &serde_json::json!(2.0),
            )
            .expect("scale"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "scrollspeed",
                &serde_json::json!(0.33),
            )
            .expect("scrollspeed"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "scrolldirection",
                &serde_json::json!(1.57),
            )
            .expect("scrolldirection"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "ratio",
                &serde_json::json!(0.75),
            )
            .expect("ratio"),
        ];

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            500,
        );

        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_WATERRIPPLE
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_STRENGTH_OFFSET_BYTES
            ),
            0.42
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_ANIMATION_SPEED_OFFSET_BYTES
            ),
            0.25
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCALE_OFFSET_BYTES
            ),
            2.0
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_SCROLL_SPEED_OFFSET_BYTES
            ),
            0.33
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_DIRECTION_OFFSET_BYTES
            ),
            1.57
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERRIPPLE_RATIO_OFFSET_BYTES
            ),
            0.75
        );
    }

    #[test]
    fn sampled_image_push_constants_encode_waterwaves_constants() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterWaves];
        material.combo_keys = vec!["DUALWAVES".to_owned(), "TIMEOFFSET".to_owned()];
        material.system_shader_uniforms = vec![
            effect_vec2_uniform("g_Texture0Resolution", [663.0, 230.0]),
            effect_vec2_uniform("g_Texture1Resolution", [331.0, 115.0]),
            effect_vec2_uniform("g_Texture2Resolution", [64.0, 64.0]),
        ];
        material.constant_shader_uniforms = vec![
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "strength",
                &serde_json::json!(0.42),
            )
            .expect("strength"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "speed",
                &serde_json::json!(5.5),
            )
            .expect("speed"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "scale",
                &serde_json::json!(222.0),
            )
            .expect("scale"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "exponent",
                &serde_json::json!(1.5),
            )
            .expect("exponent"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "direction",
                &serde_json::json!(0.75),
            )
            .expect("direction"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "speed2",
                &serde_json::json!(3.5),
            )
            .expect("speed2"),
        ];

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            500,
        );

        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_WATERWAVES
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_STRENGTH_OFFSET_BYTES
            ),
            0.42
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_SPEED_OFFSET_BYTES
            ),
            5.5
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_SCALE_OFFSET_BYTES
            ),
            222.0
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_EXPONENT_OFFSET_BYTES
            ),
            1.5
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_DIRECTION_OFFSET_BYTES
            ),
            0.75
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_SPEED2_OFFSET_BYTES
            ),
            3.5
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_FLAGS_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_WATERWAVES_FLAG_MASK
                | SCENE_SAMPLED_IMAGE_WATERWAVES_FLAG_DUAL
                | SCENE_SAMPLED_IMAGE_WATERWAVES_FLAG_TIMEOFFSET
        );
    }

    #[test]
    fn sampled_image_waterwaves_combo_zero_disables_optional_flags() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterWaves];
        material.combo_keys = vec!["DUALWAVES".to_owned(), "TIMEOFFSET".to_owned()];
        material.combo_values = std::collections::BTreeMap::from([
            ("DUALWAVES".to_owned(), 0),
            ("TIMEOFFSET".to_owned(), 0),
        ]);
        material.system_shader_uniforms = vec![
            effect_vec2_uniform("g_Texture1Resolution", [331.0, 115.0]),
            effect_vec2_uniform("g_Texture2Resolution", [64.0, 64.0]),
        ];
        material.constant_shader_uniforms = vec![
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "speed2",
                &serde_json::json!(3.5),
            )
            .expect("speed2"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "direction2",
                &serde_json::json!(1.25),
            )
            .expect("direction2"),
        ];

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            500,
        );

        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERWAVES_FLAGS_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_WATERWAVES_FLAG_MASK
        );
    }

    #[test]
    fn sampled_image_push_constants_encode_waterflow_constants() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterFlow];
        material.constant_shader_uniforms = vec![
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "strength",
                &serde_json::json!(1.25),
            )
            .expect("strength"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "speed",
                &serde_json::json!(0.75),
            )
            .expect("speed"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "feather",
                &serde_json::json!(0.35),
            )
            .expect("feather"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "phasescale",
                &serde_json::json!(2.5),
            )
            .expect("phasescale"),
        ];

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            500,
        );

        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_WATERFLOW
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERFLOW_STRENGTH_OFFSET_BYTES
            ),
            1.25
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERFLOW_SPEED_OFFSET_BYTES
            ),
            0.75
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERFLOW_FEATHER_OFFSET_BYTES
            ),
            0.35
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_WATERFLOW_PHASE_SCALE_OFFSET_BYTES
            ),
            2.5
        );
    }

    #[test]
    fn sampled_image_push_constants_encode_watercaustics_constants() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterCaustics];
        material.combo_values =
            std::collections::BTreeMap::from([("BLENDMODE".to_owned(), 6), ("MODE".to_owned(), 1)]);
        material.constant_shader_values = serde_json::from_value(serde_json::json!({
            "ui_editor_properties_brightness": 2.48,
            "ui_editor_properties_glow": 0.5,
            "ui_editor_properties_granularity": 1.91,
            "ui_editor_properties_speed": 0.3,
            "ui_editor_properties_time_offset": -0.25,
            "ui_editor_properties_distortion": 1.0,
            "ui_editor_properties_chromatic_aberration": 0.0,
            "ui_editor_properties_blur": 0.2,
            "ui_editor_properties_color_start": "0.7 0.9 1.0",
            "ui_editor_properties_color_end": [0.4, 0.6, 1.0]
        }))
        .expect("watercaustics constants");

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            500,
        );

        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_WATERCAUSTICS
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_BRIGHTNESS_OFFSET_BYTES
            ),
            2.48
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_SCALE_OFFSET_BYTES
            ),
            1.91
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_SPEED_OFFSET_BYTES
            ),
            0.3
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_TIME_OFFSET_OFFSET_BYTES
            ),
            -0.25
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_CHROMATIC_OFFSET_BYTES
            ),
            0.0
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_BLUR_OFFSET_BYTES
            ),
            0.2
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_COLOR1_OFFSET_BYTES
            ),
            0.7
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_COLOR1_OFFSET_BYTES + 4
            ),
            0.9
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_COLOR2_OFFSET_BYTES
            ),
            0.4
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_CAUSTICS_FLAGS_OFFSET_BYTES
            ),
            6 | (1 << 8)
        );
    }

    #[test]
    fn sampled_image_push_constants_encode_foliagesway_constants() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::FoliageSway];
        material.system_shader_uniforms =
            vec![effect_vec2_uniform("g_Texture1Resolution", [512.0, 256.0])];
        material.constant_shader_uniforms = vec![
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "strength",
                &serde_json::json!(0.5),
            )
            .expect("strength"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "speeduv",
                &serde_json::json!(5.0),
            )
            .expect("speeduv"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "phase",
                &serde_json::json!(2.0),
            )
            .expect("phase"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "power",
                &serde_json::json!(2.0),
            )
            .expect("power"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "scale",
                &serde_json::json!(0.05),
            )
            .expect("scale"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "ratio",
                &serde_json::json!(2.11),
            )
            .expect("ratio"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "scrolldirection",
                &serde_json::json!(1.5),
            )
            .expect("scrolldirection"),
        ];

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            500,
        );

        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_FOLIAGE_SWAY
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_STRENGTH_OFFSET_BYTES
            ),
            0.5
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_SPEED_OFFSET_BYTES
            ),
            5.0
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_PHASE_OFFSET_BYTES
            ),
            2.0
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_POWER_OFFSET_BYTES
            ),
            2.0
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_NOISE_SCALE_OFFSET_BYTES
            ),
            0.05
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_RATIO_OFFSET_BYTES
            ),
            2.11
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_DIRECTION_OFFSET_BYTES
            ),
            1.5
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_FOLIAGE_SWAY_FLAGS_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_FOLIAGE_SWAY_FLAG_MASK
        );
    }

    #[test]
    fn sampled_image_push_constants_encode_autosway_constants() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::AutoSway];
        material.constant_shader_values = serde_json::from_value(serde_json::json!({
            "strength": 0.05,
            "末端阻尼": 0.15,
            "xFeather": 0.2,
            "speed": 0.2,
            "inertia": 0.55,
            "sigment": 1.0,
            "timeoffset": 0.53,
            "windDirectionOffset": 0.0,
            "weightCenterOffset": 0.0,
            "smoothDistance": 1.0,
            "directionalCompensation": 0.0,
            "center1": "1.05477 0.36680",
            "center2": "0.38628 0.23530",
            "center3": "0.66675 0.20303",
            "center4": "1.01683 0.11451",
            "size1": 0.1,
            "size2": 0.11560908,
            "size3": 0.10066229,
            "size4": 0.10451362,
            "angle2": -0.031899612,
            "angle3": -0.018805601,
            "angle4": -0.29432327,
            "angle5": -0.012552562
        }))
        .expect("auto sway constants");

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            500,
        );

        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_AUTO_SWAY
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_STRENGTH_OFFSET_BYTES
            ),
            0.05
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_DAMPING_OFFSET_BYTES
            ),
            0.15
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_GLOBAL_TIME_OFFSET_BYTES
            ),
            0.53
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_CENTERS_OFFSET_BYTES
            ),
            1.05477
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_CENTERS_OFFSET_BYTES + 8
            ),
            0.38628
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_SIZES_OFFSET_BYTES + 4
            ),
            0.11560908
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUTO_SWAY_ANGLES_OFFSET_BYTES + 8
            ),
            -0.29432327
        );
    }

    #[test]
    fn sampled_image_push_constants_encode_scroll_constants() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::Scroll];
        material.constant_shader_uniforms = vec![
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "speedx",
                &serde_json::json!(-0.5),
            )
            .expect("speedx"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "speedy",
                &serde_json::json!(0.25),
            )
            .expect("speedy"),
            super::super::present::NativeVulkanVulkanaliaSceneEffectUniform::from_constant_shader_value(
                "repeat",
                &serde_json::json!([2.0, 3.0]),
            )
            .expect("repeat"),
        ];

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            500,
        );

        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_SCROLL
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SCROLL_SPEED_X_OFFSET_BYTES
            ),
            -0.5
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SCROLL_SPEED_Y_OFFSET_BYTES
            ),
            0.25
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SCROLL_REPEAT_X_OFFSET_BYTES
            ),
            2.0
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SCROLL_REPEAT_Y_OFFSET_BYTES
            ),
            3.0
        );
    }

    #[test]
    fn sampled_image_push_constants_encode_skew_constants() {
        let mut material = sampled_image_material(SceneBlendMode::Normal);
        material.effect_kinds =
            vec![super::super::present::NativeVulkanVulkanaliaSceneEffectKind::Skew];
        material.combo_values =
            std::collections::BTreeMap::from([("REPEAT".to_owned(), 0), ("MODE".to_owned(), 0)]);
        material.constant_shader_values = serde_json::from_value(serde_json::json!({
            "top": 0.1,
            "bottom": -0.39,
            "left": 0.2,
            "right": -0.3
        }))
        .expect("skew constants");

        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            500,
        );

        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_EFFECT_SHADER_CODE_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_EFFECT_SHADER_CODE_SKEW
        );
        assert_eq!(
            push_f32(&bytes, SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_TOP_OFFSET_BYTES),
            0.1
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_BOTTOM_OFFSET_BYTES
            ),
            -0.39
        );
        assert_eq!(
            push_f32(&bytes, SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_LEFT_OFFSET_BYTES),
            0.2
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_RIGHT_OFFSET_BYTES
            ),
            -0.3
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_FLAGS_OFFSET_BYTES
            ),
            0
        );

        material.combo_values.remove("MODE");
        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            &material,
            500,
        );
        assert_eq!(
            push_u32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_SKEW_FLAGS_OFFSET_BYTES
            ),
            SCENE_SAMPLED_IMAGE_SKEW_FLAG_VERTEX_MODE
        );
    }

    #[test]
    fn scene_blend_attachments_cover_alpha_normal_additive_multiply_screen_max_modulate_hsl_and_alpha_to_coverage_modes()
     {
        let alpha = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Alpha);
        let normal = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Normal);
        let additive = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Additive);
        let multiply = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Multiply);
        let screen = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Screen);
        let max = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Max);
        let modulate = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Modulate);
        let hsl = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::HslColor);
        let alpha_to_coverage =
            native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::AlphaToCoverage);

        assert_eq!(alpha.src_color_blend_factor, vk::BlendFactor::SRC_ALPHA);
        assert_eq!(
            alpha.dst_color_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
        assert_eq!(alpha.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(alpha.src_alpha_blend_factor, vk::BlendFactor::SRC_ALPHA);
        assert_eq!(
            alpha.dst_alpha_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
        assert_eq!(normal.src_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(normal.dst_color_blend_factor, vk::BlendFactor::ZERO);
        assert_eq!(normal.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(normal.src_alpha_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(normal.dst_alpha_blend_factor, vk::BlendFactor::ZERO);
        assert_eq!(normal.alpha_blend_op, vk::BlendOp::ADD);
        assert_eq!(additive.src_color_blend_factor, vk::BlendFactor::SRC_ALPHA);
        assert_eq!(additive.dst_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(additive.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(multiply.src_color_blend_factor, vk::BlendFactor::DST_COLOR);
        assert_eq!(
            multiply.dst_color_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
        assert_eq!(multiply.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(
            screen.src_color_blend_factor,
            vk::BlendFactor::ONE_MINUS_DST_COLOR
        );
        assert_eq!(screen.dst_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(screen.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(max.src_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(max.dst_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(max.color_blend_op, vk::BlendOp::MAX);
        assert_eq!(
            max.dst_alpha_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
        assert_eq!(max.alpha_blend_op, vk::BlendOp::ADD);
        assert_eq!(modulate.src_color_blend_factor, vk::BlendFactor::DST_COLOR);
        assert_eq!(modulate.dst_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(modulate.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(modulate.src_alpha_blend_factor, vk::BlendFactor::ZERO);
        assert_eq!(modulate.dst_alpha_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(modulate.alpha_blend_op, vk::BlendOp::ADD);
        assert_eq!(hsl.src_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(hsl.dst_color_blend_factor, vk::BlendFactor::ZERO);
        assert_eq!(hsl.color_blend_op, vk::BlendOp::HSL_COLOR_EXT);
        assert_eq!(hsl.src_alpha_blend_factor, vk::BlendFactor::ZERO);
        assert_eq!(hsl.dst_alpha_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(hsl.alpha_blend_op, vk::BlendOp::ADD);
        assert_eq!(alpha_to_coverage.blend_enable, vk::FALSE);
        assert!(alpha_to_coverage.color_write_mask.contains(
            vk::ColorComponentFlags::R | vk::ColorComponentFlags::G | vk::ColorComponentFlags::B
        ));
        assert!(
            !alpha_to_coverage
                .color_write_mask
                .contains(vk::ColorComponentFlags::A)
        );
        assert!(
            native_vulkan_vulkanalia_scene_advanced_color_blend_state(SceneBlendMode::HslColor)
                .is_some()
        );
    }

    #[test]
    fn premultiplied_fragment_shader_is_only_used_by_dst_color_blend_modes() {
        let straight = vk::ShaderModule::from_raw(1);
        let premultiplied = vk::ShaderModule::from_raw(2);

        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::Alpha,
                straight,
                premultiplied
            ),
            straight
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::Normal,
                straight,
                premultiplied
            ),
            straight
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::Additive,
                straight,
                premultiplied
            ),
            straight
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::AlphaToCoverage,
                straight,
                premultiplied
            ),
            straight
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::Multiply,
                straight,
                premultiplied
            ),
            premultiplied
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::Screen,
                straight,
                premultiplied
            ),
            premultiplied
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::Max,
                straight,
                premultiplied
            ),
            premultiplied
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::Modulate,
                straight,
                premultiplied
            ),
            premultiplied
        );
    }

    #[test]
    fn sampled_image_pipeline_template_can_use_descriptor_heap_mapping() {
        let snapshot = native_vulkan_vulkanalia_scene_sampled_image_pipeline_snapshot(
            vk::Format::B8G8R8A8_SRGB,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            vk::SampleCountFlags::_1,
        );

        assert_eq!(snapshot.descriptor_set_count, 0);
        assert_eq!(snapshot.descriptor_model, "VK_EXT_descriptor_heap");
        assert!(snapshot.descriptor_heap_mapping_enabled);
        assert!(snapshot.descriptor_heap_pipeline_flag_enabled);
        assert!(
            snapshot
                .sampled_image_model
                .contains("VK_EXT_descriptor_heap")
        );
        assert!(!snapshot.uses_push_descriptor_fast_path);
    }

    #[test]
    fn mixed_ordered_draw_steps_follow_scene_layer_order() {
        let solid_commands = [VulkanaliaSceneSolidQuadDrawCommand {
            layer_index: 2,
            last_layer_index: 2,
            blend: blend_state(SceneBlendMode::Alpha),
            first_index: 0,
            index_count: 6,
        }];
        let sampled_commands = [
            VulkanaliaSceneSampledImageDrawCommand {
                layer_index: 1,
                last_layer_index: 1,
                material: sampled_image_material(SceneBlendMode::Alpha),
                descriptor_binding: VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                    descriptor_group_base_index: 0,
                    texture_slot_bindings: texture_slot_bindings(&[0]),
                },
                render_target: VulkanaliaSceneSampledImageRenderTarget::Swapchain,
                first_index: 0,
                index_count: 6,
            },
            VulkanaliaSceneSampledImageDrawCommand {
                layer_index: 3,
                last_layer_index: 3,
                material: sampled_image_material(SceneBlendMode::Alpha),
                descriptor_binding: VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                    descriptor_group_base_index: SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT
                        as u32,
                    texture_slot_bindings: texture_slot_bindings(&[1]),
                },
                render_target: VulkanaliaSceneSampledImageRenderTarget::Swapchain,
                first_index: 6,
                index_count: 6,
            },
        ];

        let ordered =
            native_vulkan_vulkanalia_scene_ordered_draw_steps(&solid_commands, &sampled_commands);
        let order = ordered
            .iter()
            .map(|step| (step.layer_index, step.pipeline.label(), step.command_index))
            .collect::<Vec<_>>();

        assert_eq!(
            order,
            vec![
                (1, "sampled-image", 0),
                (2, "solid-quad", 0),
                (3, "sampled-image", 1)
            ]
        );
    }

    #[test]
    fn solid_quad_shader_bytecode_is_inline_spirv() {
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_VERTEX_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_PREMULTIPLIED_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_VERTEX_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PREMULTIPLIED_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERRIPPLE_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERWAVES_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_AUTO_SWAY_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_IRIS_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SCROLL_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SKEW_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_OPACITY_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_VERTEX_SPIRV
            ),
            1516
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_FRAGMENT_SPIRV
            ),
            376
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_PREMULTIPLIED_FRAGMENT_SPIRV
            ),
            656
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_VERTEX_SPIRV
            ),
            2024
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV
            ),
            7776
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PREMULTIPLIED_FRAGMENT_SPIRV
            ),
            8004
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERRIPPLE_FRAGMENT_SPIRV
            ),
            6772
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERWAVES_FRAGMENT_SPIRV
            ),
            11580
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERFLOW_FRAGMENT_SPIRV
            ),
            7060
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FOLIAGE_SWAY_FRAGMENT_SPIRV
            ),
            8920
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_AUTO_SWAY_FRAGMENT_SPIRV
            ),
            19480
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SCROLL_FRAGMENT_SPIRV
            ),
            3780
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SKEW_FRAGMENT_SPIRV
            ),
            5572
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_IRIS_FRAGMENT_SPIRV
            ),
            5744
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_OPACITY_FRAGMENT_SPIRV
            ),
            3284
        );
    }

    #[test]
    fn sampled_image_iris_fragment_matches_we_masked_mix_semantics() {
        let source = include_str!("shaders/sampled_image_iris.frag");
        assert!(source.contains("vec4 albedo = texture(g_Texture0, v_uv);"));
        assert!(
            source.contains(
                "out_color = finalize_output(apply_vertex_color(mix(albedo, iris, mask)));"
            )
        );
    }

    #[test]
    fn sampled_image_autosway_fragment_matches_reveng_endpoint_basis() {
        let source = include_str!("shaders/sampled_image_autosway.frag");
        assert!(
            source.contains(
                "float aspect = max(base_resolution.x, 1.0) / max(base_resolution.y, 1.0);"
            )
        );
        assert!(source.contains("endpoint_center.x *= aspect;"));
        assert!(!source.contains("endpoint_center.x *= aspect;\n    this_center.x *= aspect;"));
        assert!(source.contains("float width_mix = node.pos_x / max(node.len, 0.000001);"));
        assert!(!source.contains("clamp(node.pos_x / max(node.len, 0.000001)"));
    }

    #[test]
    fn sampled_image_foliagesway_fragment_matches_reveng_aspect_basis() {
        let source = include_str!("shaders/sampled_image_foliagesway.frag");
        assert!(source.contains(
            "float aspect = max(base_resolution.x, 1.0) / max(base_resolution.y, 1.0) * pc.foliage_ratio;"
        ));
        assert!(!source.contains("base_resolution.y, 1.0) / max(base_resolution.x"));
    }

    #[test]
    fn sampled_image_skew_fragment_matches_reveng_uv_mode() {
        let source = include_str!("shaders/sampled_image_skew.frag");
        assert!(source.contains("SKEW_FLAG_VERTEX_MODE"));
        assert!(source.contains("texture(g_Texture0, v_uv)"));
        assert!(!source.contains("float source_y = 1.0 - layer_uv.y;"));
        assert!(!source.contains("mix(pc.skew_top, pc.skew_bottom, source_y)"));
        assert!(!source.contains("out_color = vec4(0.0);"));
        assert!(source.contains("vec2 pass_uv = vec2(v_uv.x, 1.0 - v_uv.y);"));
        assert!(source.contains("uv.x -= step(pass_uv.y, 0.5) * pc.skew_top"));
        assert!(source.contains("uv.y += step(pass_uv.x, 0.5) * pc.skew_left"));
        assert!(source.contains("vec2 sample_uv = vec2(uv.x, 1.0 - uv.y);"));
        assert!(source.contains("uv = fract(uv);"));
        assert!(!source.contains("time_seconds *"));
    }

    #[test]
    fn sampled_image_waterwaves_uses_layer_uv_basis_for_expanded_targets() {
        let source = include_str!("shaders/sampled_image_waterwaves.frag");
        assert!(source.contains("bool effect_uv_inside(vec2 uv)"));
        assert!(source.contains("vec2 source_coord = v_uv;"));
        assert!(source.contains("vec2 tex_coord_motion = v_effect_uv;"));
        assert!(source.contains("vec2 mask_uv = v_effect_uv;"));
        assert!(source.contains("float waterwaves_mask_sample("));
        assert!(source.contains("float waterwaves_timeoffset_sample("));
        assert!(source.contains("source_alpha_at(source_coord, cached_source_alpha) <= 0.001"));
        assert!(source.contains("texture(g_Texture1, clamp(uv, vec2(0.0), vec2(1.0))).r"));
        assert!(source.contains("vec2 target_uv_per_layer_uv()"));
        assert!(source.contains("dFdx(v_effect_uv.x)"));
        assert!(source.contains(
            "mask = waterwaves_mask_sample(mask_uv, source_coord, cached_source_alpha);"
        ));
        assert!(source.contains(
            "waterwaves_timeoffset_sample(mask_uv, source_coord, cached_source_alpha) * M_PI_2"
        ));
        assert!(source.contains("vec2 layer_uv_offset = val * offset * strength * mask;"));
        assert!(source.contains("source_coord += layer_uv_offset * target_uv_per_layer_uv();"));
    }

    #[test]
    fn sampled_image_fragment_shader_samples_alpha_mask_from_effect_uv() {
        assert!(spirv_function_argument_loads_named_input(
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV,
            "alpha_mask",
            "v_effect_uv"
        ));
        assert!(spirv_function_argument_loads_named_input(
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PREMULTIPLIED_FRAGMENT_SPIRV,
            "alpha_mask",
            "v_effect_uv"
        ));
    }

    #[test]
    fn sampled_image_pass_specific_fragments_apply_vertex_tint_and_opacity() {
        for (label, words) in [
            (
                "waterripple",
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERRIPPLE_FRAGMENT_SPIRV
                    as &[u32],
            ),
            (
                "waterwaves",
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERWAVES_FRAGMENT_SPIRV
                    as &[u32],
            ),
            (
                "waterflow",
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERFLOW_FRAGMENT_SPIRV
                    as &[u32],
            ),
            (
                "foliagesway",
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FOLIAGE_SWAY_FRAGMENT_SPIRV
                    as &[u32],
            ),
            (
                "autosway",
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_AUTO_SWAY_FRAGMENT_SPIRV
                    as &[u32],
            ),
            (
                "scroll",
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SCROLL_FRAGMENT_SPIRV
                    as &[u32],
            ),
            (
                "skew",
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SKEW_FRAGMENT_SPIRV as &[u32],
            ),
            (
                "iris",
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_IRIS_FRAGMENT_SPIRV as &[u32],
            ),
            (
                "opacity",
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_OPACITY_FRAGMENT_SPIRV
                    as &[u32],
            ),
        ] {
            assert!(
                spirv_function_loads_named_input(words, "apply_vertex_color", "v_tint"),
                "{label} fragment must keep WE solid/shadow tint"
            );
            assert!(
                spirv_function_loads_named_input(words, "apply_vertex_color", "v_opacity"),
                "{label} fragment must keep layer opacity"
            );
        }
    }

    fn spirv_function_argument_loads_named_input(
        words: &[u32],
        function_name_prefix: &str,
        input_name: &str,
    ) -> bool {
        let Some(function_id) = spirv_named_id(words, function_name_prefix, true) else {
            return false;
        };
        let Some(input_id) = spirv_named_id(words, input_name, false) else {
            return false;
        };
        let offsets = spirv_instruction_offsets(words);
        for (call_position, offset) in offsets.iter().enumerate() {
            let word_count = spirv_word_count(words[*offset]) as usize;
            if spirv_opcode(words[*offset]) != 57
                || word_count < 5
                || words[*offset + 3] != function_id
            {
                continue;
            }
            let argument_id = words[*offset + 4];
            let Some(loaded_id) =
                spirv_latest_store_object(words, &offsets[..call_position], argument_id)
            else {
                continue;
            };
            if spirv_loads_input_before(words, &offsets[..call_position], loaded_id, input_id) {
                return true;
            }
        }
        false
    }

    fn spirv_function_loads_named_input(
        words: &[u32],
        function_name_prefix: &str,
        input_name: &str,
    ) -> bool {
        let Some(function_id) = spirv_named_id(words, function_name_prefix, true) else {
            return false;
        };
        let Some(input_id) = spirv_named_id(words, input_name, false) else {
            return false;
        };
        let mut in_function = false;
        for offset in spirv_instruction_offsets(words) {
            match spirv_opcode(words[offset]) {
                54 if spirv_word_count(words[offset]) >= 5 && words[offset + 2] == function_id => {
                    in_function = true;
                }
                56 if in_function => break,
                61 if in_function
                    && spirv_word_count(words[offset]) >= 4
                    && words[offset + 3] == input_id =>
                {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    fn spirv_latest_store_object(words: &[u32], offsets: &[usize], pointer_id: u32) -> Option<u32> {
        offsets.iter().rev().find_map(|offset| {
            (spirv_opcode(words[*offset]) == 62
                && spirv_word_count(words[*offset]) == 3
                && words[*offset + 1] == pointer_id)
                .then_some(words[*offset + 2])
        })
    }

    fn spirv_loads_input_before(
        words: &[u32],
        offsets: &[usize],
        loaded_id: u32,
        input_id: u32,
    ) -> bool {
        offsets.iter().rev().any(|offset| {
            spirv_opcode(words[*offset]) == 61
                && spirv_word_count(words[*offset]) >= 4
                && words[*offset + 2] == loaded_id
                && words[*offset + 3] == input_id
        })
    }

    fn spirv_named_id(words: &[u32], name: &str, prefix: bool) -> Option<u32> {
        for offset in spirv_instruction_offsets(words) {
            let word_count = spirv_word_count(words[offset]) as usize;
            if spirv_opcode(words[offset]) != 5 || word_count < 3 {
                continue;
            }
            let decoded = spirv_string(&words[offset + 2..offset + word_count]);
            if decoded == name || (prefix && decoded.starts_with(name)) {
                return Some(words[offset + 1]);
            }
        }
        None
    }

    fn spirv_instruction_offsets(words: &[u32]) -> Vec<usize> {
        let mut offsets = Vec::new();
        let mut offset = 5usize;
        while offset < words.len() {
            let word_count = spirv_word_count(words[offset]) as usize;
            if word_count == 0 || offset.saturating_add(word_count) > words.len() {
                break;
            }
            offsets.push(offset);
            offset += word_count;
        }
        offsets
    }

    fn spirv_word_count(word: u32) -> u16 {
        (word >> 16) as u16
    }

    fn spirv_opcode(word: u32) -> u16 {
        (word & 0xffff) as u16
    }

    fn spirv_string(words: &[u32]) -> String {
        let mut bytes = Vec::with_capacity(words.len().saturating_mul(4));
        for word in words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    }

    #[test]
    fn solid_quad_command_order_records_dynamic_rendering_draw_indexed() {
        let order =
            native_vulkan_vulkanalia_scene_draw_pass_command_order(true, false, false, false);

        assert_eq!(order[0], "cmd_pipeline_barrier2_swapchain_attachment");
        assert!(order.contains(&"cmd_begin_rendering"));
        assert!(order.contains(&"cmd_bind_scene_solid_quad_pipeline"));
        assert!(order.contains(&"cmd_bind_scene_vertex_buffer"));
        assert!(order.contains(&"cmd_bind_scene_index_buffer"));
        assert!(order.contains(&"cmd_draw_indexed_per_quad"));
        assert!(order.contains(&"queue_submit2_present"));
        assert!(order.contains(&"queue_present_khr"));
    }

    #[test]
    fn mixed_scene_command_order_records_layer_ordered_draws() {
        let order =
            native_vulkan_vulkanalia_scene_draw_pass_command_order(false, true, false, true);

        assert!(order.contains(&"cmd_bind_scene_solid_quad_pipeline_as_needed"));
        assert!(order.contains(&"cmd_bind_scene_sampled_image_pipeline_as_needed"));
        assert!(order.contains(&"cmd_bind_scene_descriptor_heap_when_needed"));
        assert!(order.contains(&"cmd_draw_indexed_in_scene_layer_order"));
        assert!(order.contains(&"queue_submit2_present"));
    }

    #[test]
    fn mixed_scene_command_order_can_use_descriptor_heap() {
        let order =
            native_vulkan_vulkanalia_scene_draw_pass_command_order(false, true, false, true);

        assert!(order.contains(&"cmd_bind_scene_descriptor_heap_when_needed"));
        assert!(order.contains(&"cmd_draw_indexed_in_scene_layer_order"));
    }
}
