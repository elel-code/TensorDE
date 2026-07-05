#![allow(dead_code)]

use std::sync::atomic::AtomicUsize;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder};

use crate::core::SceneBlendMode;
use crate::renderer::native_vulkan::audio::clock::{
    native_vulkan_audio_signal_level, native_vulkan_audio_spectrum32_packed,
};
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
const SCENE_FULL_SOLID_QUAD_DRAW_INSTANCE_STRIDE_BYTES: u32 = 32;
const SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES: u32 = 44;
const SCENE_PUPPET_GPU_VERTEX_STRIDE_BYTES: u32 = 64;
const SCENE_PARTICLE_GPU_VERTEX_STRIDE_BYTES: u32 = 48;
pub(in crate::renderer::native_vulkan::vulkan) const SCENE_FULL_SAMPLED_IMAGE_DRAW_INSTANCE_STRIDE_BYTES: u32 = 144;
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
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_SIGNAL_OFFSET_BYTES: usize = 156;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_SPECTRUM32_OFFSET_BYTES: usize = 160;
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
const SCENE_FULL_SAMPLED_IMAGE_PUSH_VERTEX_EXTENT_OFFSET_BYTES: usize = 232;
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSampledImageViewportState {
    pub(in crate::renderer::native_vulkan::vulkan) vertex_extent_width: f32,
    pub(in crate::renderer::native_vulkan::vulkan) vertex_extent_height: f32,
    pub(in crate::renderer::native_vulkan::vulkan) viewport_x: f32,
    pub(in crate::renderer::native_vulkan::vulkan) viewport_y: f32,
    pub(in crate::renderer::native_vulkan::vulkan) viewport_width: f32,
    pub(in crate::renderer::native_vulkan::vulkan) viewport_height: f32,
}

impl VulkanaliaSceneSampledImageViewportState {
    pub(in crate::renderer::native_vulkan::vulkan) fn full_extent(extent: vk::Extent2D) -> Self {
        Self {
            vertex_extent_width: extent.width.max(1) as f32,
            vertex_extent_height: extent.height.max(1) as f32,
            viewport_x: 0.0,
            viewport_y: 0.0,
            viewport_width: extent.width as f32,
            viewport_height: extent.height as f32,
        }
    }

    fn vertex_extent(self) -> [f32; 2] {
        [self.vertex_extent_width, self.vertex_extent_height]
    }

    fn dynamic_viewport(self, scissor_extent: vk::Extent2D) -> SceneDynamicViewport {
        SceneDynamicViewport {
            x: self.viewport_x,
            y: self.viewport_y,
            width: self.viewport_width,
            height: self.viewport_height,
            scissor_extent,
        }
    }
}

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
    pub sample_shading_enabled: bool,
    pub min_sample_shading: &'static str,
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
    pub rasterization_samples: &'static str,
    pub uses_msaa_color_target: bool,
    pub resolve_mode: &'static str,
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
    pub sample_shading_enabled: bool,
    pub min_sample_shading: &'static str,
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
    pub rasterization_samples: &'static str,
    pub uses_msaa_color_target: bool,
    pub effect_msaa_target_count: u32,
    pub resolve_mode: &'static str,
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
    pub(in crate::renderer::native_vulkan::vulkan) sample_count: vk::SampleCountFlags,
    pub(in crate::renderer::native_vulkan::vulkan) sample_shading_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot,
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneMsaaColorTarget {
    pub(in crate::renderer::native_vulkan::vulkan) image: vk::Image,
    pub(in crate::renderer::native_vulkan::vulkan) image_view: vk::ImageView,
    pub(in crate::renderer::native_vulkan::vulkan) extent: vk::Extent2D,
    pub(in crate::renderer::native_vulkan::vulkan) sample_count: vk::SampleCountFlags,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSampledImagePipelineResources {
    pub(in crate::renderer::native_vulkan::vulkan) pipeline_layout: vk::PipelineLayout,
    pub(in crate::renderer::native_vulkan::vulkan) generic_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) puppet_generic_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) particle_generic_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) puppet_water_ripple_pipelines:
        VulkanaliaSceneSampledImagePipelineSet,
    pub(in crate::renderer::native_vulkan::vulkan) puppet_water_waves_pipelines:
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
    pub(in crate::renderer::native_vulkan::vulkan) sample_count: vk::SampleCountFlags,
    pub(in crate::renderer::native_vulkan::vulkan) sample_shading_enabled: bool,
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
    pub(in crate::renderer::native_vulkan::vulkan) draw_instance_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) vertex_program:
        VulkanaliaSceneSampledImageVertexProgram,
    pub(in crate::renderer::native_vulkan::vulkan) vertex_offset: i32,
    pub(in crate::renderer::native_vulkan::vulkan) first_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) index_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) enum VulkanaliaSceneSampledImageVertexProgram {
    Sampled,
    PuppetGpu,
    ParticleGpu,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSampledImageDrawInstance {
    pub(in crate::renderer::native_vulkan::vulkan) position_transform_x: [f32; 4],
    pub(in crate::renderer::native_vulkan::vulkan) position_transform_y: [f32; 4],
    pub(in crate::renderer::native_vulkan::vulkan) frame_constants: [f32; 4],
    pub(in crate::renderer::native_vulkan::vulkan) effect_uv_x: [f32; 4],
    pub(in crate::renderer::native_vulkan::vulkan) effect_uv_y: [f32; 4],
    pub(in crate::renderer::native_vulkan::vulkan) puppet_pose_ref: [u32; 4],
    pub(in crate::renderer::native_vulkan::vulkan) tint: [f32; 4],
    pub(in crate::renderer::native_vulkan::vulkan) frame_time_ref: [u32; 4],
    pub(in crate::renderer::native_vulkan::vulkan) layer_pose_ref: [u32; 4],
}

impl VulkanaliaSceneSampledImageDrawInstance {
    pub(in crate::renderer::native_vulkan::vulkan) const fn identity() -> Self {
        Self {
            position_transform_x: [1.0, 0.0, 0.0, 0.0],
            position_transform_y: [0.0, 1.0, 0.0, 0.0],
            frame_constants: [0.0, 0.0, 0.0, 0.0],
            effect_uv_x: [0.0, 1.0, 0.0, 0.0],
            effect_uv_y: [1.0, 0.0, 0.0, 0.0],
            puppet_pose_ref: [0, 0, 0, 0],
            tint: [1.0, 1.0, 1.0, 1.0],
            frame_time_ref: [0, 0, 0, 0],
            layer_pose_ref: [0, 0, 0, 0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSolidQuadDrawCommand {
    pub(in crate::renderer::native_vulkan::vulkan) layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) last_layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) blend:
        super::present::NativeVulkanVulkanaliaSceneBlendState,
    pub(in crate::renderer::native_vulkan::vulkan) draw_instance_index: u32,
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
        vertex_program: VulkanaliaSceneSampledImageVertexProgram,
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
    pub(in crate::renderer::native_vulkan::vulkan) vertex_offset_bytes: u64,
    pub(in crate::renderer::native_vulkan::vulkan) draw_instance_buffer: vk::Buffer,
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
                vertex_program: sampled_commands[draw.command_index].vertex_program,
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
    vertex_program: VulkanaliaSceneSampledImageVertexProgram,
) -> vk::Pipeline {
    let shader_program = scene_sampled_image_shader_program(material);
    let pipelines = scene_sampled_image_pipeline_set(resources, shader_program, vertex_program);
    native_vulkan_vulkanalia_scene_sampled_image_pipeline_from_set(
        pipelines,
        material.render_state.blend.mode,
    )
}

fn scene_sampled_image_pipeline_set(
    resources: &VulkanaliaSceneSampledImagePipelineResources,
    shader_program: VulkanaliaSceneSampledImageShaderProgram,
    vertex_program: VulkanaliaSceneSampledImageVertexProgram,
) -> &VulkanaliaSceneSampledImagePipelineSet {
    if vertex_program == VulkanaliaSceneSampledImageVertexProgram::PuppetGpu {
        return match shader_program {
            VulkanaliaSceneSampledImageShaderProgram::WaterRipple => {
                &resources.puppet_water_ripple_pipelines
            }
            VulkanaliaSceneSampledImageShaderProgram::WaterWaves => {
                &resources.puppet_water_waves_pipelines
            }
            _ => &resources.puppet_generic_pipelines,
        };
    }
    if vertex_program == VulkanaliaSceneSampledImageVertexProgram::ParticleGpu {
        return &resources.particle_generic_pipelines;
    }
    match shader_program {
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
    }
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
    sample_count: vk::SampleCountFlags,
    sample_shading_enabled: bool,
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
                    sample_count,
                    sample_shading_enabled,
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
    sample_count: vk::SampleCountFlags,
    sample_shading_enabled: bool,
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
        let vertex_binding = vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build();
        let draw_instance_binding = vk::VertexInputBindingDescription::builder()
            .binding(1)
            .stride(SCENE_FULL_SOLID_QUAD_DRAW_INSTANCE_STRIDE_BYTES)
            .input_rate(vk::VertexInputRate::INSTANCE)
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
            vk::VertexInputAttributeDescription::builder()
                .location(2)
                .binding(1)
                .format(vk::Format::R32G32B32A32_UINT)
                .offset(0)
                .build(),
            vk::VertexInputAttributeDescription::builder()
                .location(3)
                .binding(1)
                .format(vk::Format::R32G32B32A32_UINT)
                .offset(16)
                .build(),
        ];
        let bindings = [vertex_binding, draw_instance_binding];
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
        let sample_shading = scene_sample_shading_enabled(sample_count, sample_shading_enabled);
        let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
            .rasterization_samples(sample_count)
            .sample_shading_enable(sample_shading)
            .min_sample_shading(scene_min_sample_shading_value(sample_shading))
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
                        sample_count,
                        sample_shading_enabled,
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
            sample_count,
            sample_shading_enabled: scene_sample_shading_enabled(
                sample_count,
                sample_shading_enabled,
            ),
            snapshot: native_vulkan_vulkanalia_scene_solid_quad_pipeline_snapshot(
                target_format,
                extent,
                sample_count,
                sample_shading_enabled,
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
    sample_count: vk::SampleCountFlags,
    sample_shading_enabled: bool,
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
    let vertex_binding = vk::VertexInputBindingDescription::builder()
        .binding(0)
        .stride(SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES)
        .input_rate(vk::VertexInputRate::VERTEX)
        .build();
    let draw_instance_binding = vk::VertexInputBindingDescription::builder()
        .binding(1)
        .stride(SCENE_FULL_SOLID_QUAD_DRAW_INSTANCE_STRIDE_BYTES)
        .input_rate(vk::VertexInputRate::INSTANCE)
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
        vk::VertexInputAttributeDescription::builder()
            .location(2)
            .binding(1)
            .format(vk::Format::R32G32B32A32_UINT)
            .offset(0)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(3)
            .binding(1)
            .format(vk::Format::R32G32B32A32_UINT)
            .offset(16)
            .build(),
    ];
    let bindings = [vertex_binding, draw_instance_binding];
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
    let sample_shading = scene_sample_shading_enabled(sample_count, sample_shading_enabled);
    let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(sample_count)
        .sample_shading_enable(sample_shading)
        .min_sample_shading(scene_min_sample_shading_value(sample_shading))
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
    sample_shading_enabled: bool,
) -> NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot {
    let sample_shading = scene_sample_shading_enabled(sample_count, sample_shading_enabled);
    NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot {
        binding: "vulkanalia",
        route: "scene-solid-quad-dynamic-rendering-pipeline",
        target_format: format!("{target_format:?}"),
        extent: (extent.width, extent.height),
        shader_modules_created: true,
        pipeline_layout_created: true,
        pipeline_created: true,
        rasterization_samples: scene_sample_count_label(sample_count),
        sample_shading_enabled: sample_shading,
        min_sample_shading: scene_min_sample_shading_label(sample_shading),
        render_pass_compatibility: "dynamic-rendering-no-render-pass",
        primitive_topology: "triangle-list-indexed-quad",
        vertex_input_binding_count: 2,
        vertex_input_attribute_count: 4,
        vertex_stride_bytes: SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES,
        vertex_position_format: "R32G32_SFLOAT",
        vertex_color_format: "R32G32B32A32_SFLOAT",
        push_constant_bytes: SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES,
        push_constant_model: "scene-space pixel extent -> NDC conversion; retained layer pose timelines read frame time and transform/state payloads on GPU",
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

fn scene_sample_shading_enabled(
    sample_count: vk::SampleCountFlags,
    sample_rate_shading_available: bool,
) -> bool {
    sample_rate_shading_available && sample_count != vk::SampleCountFlags::_1
}

fn scene_min_sample_shading_value(sample_shading_enabled: bool) -> f32 {
    if sample_shading_enabled { 1.0 } else { 0.0 }
}

fn scene_min_sample_shading_label(sample_shading_enabled: bool) -> &'static str {
    if sample_shading_enabled { "1.0" } else { "0.0" }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_resources(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    sample_count: vk::SampleCountFlags,
    sample_shading_enabled: bool,
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
        let puppet_vertex_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PUPPET_VERTEX_SPIRV,
            "scene sampled image puppet vertex",
        )?;
        let particle_vertex_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PARTICLE_VERTEX_SPIRV,
            "scene sampled image particle vertex",
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
                    sample_count,
                    sample_shading_enabled,
                    pipeline_layout,
                    vertex_module,
                    fragment_module,
                    premultiplied_fragment_module,
                )?;
            let puppet_generic_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set_for_vertex_program(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    sample_count,
                    sample_shading_enabled,
                    pipeline_layout,
                    puppet_vertex_module,
                    fragment_module,
                    premultiplied_fragment_module,
                    VulkanaliaSceneSampledImageVertexProgram::PuppetGpu,
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
            let particle_generic_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set_for_vertex_program(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    sample_count,
                    sample_shading_enabled,
                    pipeline_layout,
                    particle_vertex_module,
                    fragment_module,
                    premultiplied_fragment_module,
                    VulkanaliaSceneSampledImageVertexProgram::ParticleGpu,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            puppet_generic_pipelines,
                        );
                        return Err(err);
                    }
                };
            let puppet_water_ripple_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set_for_vertex_program(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    sample_count,
                    sample_shading_enabled,
                    pipeline_layout,
                    puppet_vertex_module,
                    water_ripple_fragment_module,
                    water_ripple_fragment_module,
                    VulkanaliaSceneSampledImageVertexProgram::PuppetGpu,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            puppet_generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            particle_generic_pipelines,
                        );
                        return Err(err);
                    }
                };
            let puppet_water_waves_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set_for_vertex_program(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    sample_count,
                    sample_shading_enabled,
                    pipeline_layout,
                    puppet_vertex_module,
                    water_waves_fragment_module,
                    water_waves_fragment_module,
                    VulkanaliaSceneSampledImageVertexProgram::PuppetGpu,
                ) {
                    Ok(pipelines) => pipelines,
                    Err(err) => {
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            puppet_generic_pipelines,
                        );
                        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
                            device,
                            puppet_water_ripple_pipelines,
                        );
                        return Err(err);
                    }
                };
            let water_ripple_pipelines =
                match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
                    device,
                    target_format,
                    extent,
                    descriptor_heap_plan,
                    sample_count,
                    sample_shading_enabled,
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
                    sample_count,
                    sample_shading_enabled,
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
                    sample_count,
                    sample_shading_enabled,
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
                    sample_count,
                    sample_shading_enabled,
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
                    sample_count,
                    sample_shading_enabled,
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
                    sample_count,
                    sample_shading_enabled,
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
                    sample_count,
                    sample_shading_enabled,
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
                    sample_count,
                    sample_shading_enabled,
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
                    sample_count,
                    sample_shading_enabled,
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
                    sample_count,
                    sample_shading_enabled,
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
                    sample_count,
                    sample_shading_enabled,
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
                    sample_count,
                    sample_shading_enabled,
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
                    sample_count,
                    sample_shading_enabled,
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
                puppet_generic_pipelines,
                particle_generic_pipelines,
                puppet_water_ripple_pipelines,
                puppet_water_waves_pipelines,
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
                sample_count,
                sample_shading_enabled: scene_sample_shading_enabled(
                    sample_count,
                    sample_shading_enabled,
                ),
                snapshot: native_vulkan_vulkanalia_scene_sampled_image_pipeline_snapshot(
                    target_format,
                    extent,
                    sample_count,
                    sample_shading_enabled,
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
            device.destroy_shader_module(particle_vertex_module, None);
            device.destroy_shader_module(puppet_vertex_module, None);
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
        resources.puppet_generic_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.particle_generic_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.puppet_water_ripple_pipelines,
    );
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_set(
        device,
        resources.puppet_water_waves_pipelines,
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
#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    sample_count: vk::SampleCountFlags,
    sample_shading_enabled: bool,
    pipeline_layout: vk::PipelineLayout,
    vertex_module: vk::ShaderModule,
    fragment_module: vk::ShaderModule,
    premultiplied_fragment_module: vk::ShaderModule,
) -> Result<VulkanaliaSceneSampledImagePipelineSet, String> {
    native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set_for_vertex_program(
        device,
        target_format,
        extent,
        descriptor_heap_plan,
        sample_count,
        sample_shading_enabled,
        pipeline_layout,
        vertex_module,
        fragment_module,
        premultiplied_fragment_module,
        VulkanaliaSceneSampledImageVertexProgram::Sampled,
    )
}

#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_set_for_vertex_program(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    sample_count: vk::SampleCountFlags,
    sample_shading_enabled: bool,
    pipeline_layout: vk::PipelineLayout,
    vertex_module: vk::ShaderModule,
    fragment_module: vk::ShaderModule,
    premultiplied_fragment_module: vk::ShaderModule,
    vertex_program: VulkanaliaSceneSampledImageVertexProgram,
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
            sample_count,
            sample_shading_enabled,
            pipeline_layout,
            vertex_module,
            selected_fragment_module,
            blend_mode,
            vertex_program,
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
    sample_count: vk::SampleCountFlags,
    sample_shading_enabled: bool,
    pipeline_layout: vk::PipelineLayout,
    vertex_module: vk::ShaderModule,
    fragment_module: vk::ShaderModule,
    blend_mode: SceneBlendMode,
    vertex_program: VulkanaliaSceneSampledImageVertexProgram,
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
    let vertex_binding = vk::VertexInputBindingDescription::builder()
        .binding(0)
        .stride(match vertex_program {
            VulkanaliaSceneSampledImageVertexProgram::Sampled => {
                SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES
            }
            VulkanaliaSceneSampledImageVertexProgram::PuppetGpu => {
                SCENE_PUPPET_GPU_VERTEX_STRIDE_BYTES
            }
            VulkanaliaSceneSampledImageVertexProgram::ParticleGpu => {
                SCENE_PARTICLE_GPU_VERTEX_STRIDE_BYTES
            }
        })
        .input_rate(vk::VertexInputRate::VERTEX)
        .build();
    let instance_binding = vk::VertexInputBindingDescription::builder()
        .binding(1)
        .stride(SCENE_FULL_SAMPLED_IMAGE_DRAW_INSTANCE_STRIDE_BYTES)
        .input_rate(vk::VertexInputRate::INSTANCE)
        .build();
    let vertex_attributes = match vertex_program {
        VulkanaliaSceneSampledImageVertexProgram::Sampled => {
            vec![
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
            ]
        }
        VulkanaliaSceneSampledImageVertexProgram::PuppetGpu => {
            vec![
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
                    .format(vk::Format::R32G32B32A32_SFLOAT)
                    .offset(16)
                    .build(),
                vk::VertexInputAttributeDescription::builder()
                    .location(3)
                    .binding(0)
                    .format(vk::Format::R32G32B32A32_UINT)
                    .offset(32)
                    .build(),
                vk::VertexInputAttributeDescription::builder()
                    .location(4)
                    .binding(0)
                    .format(vk::Format::R32G32B32A32_SFLOAT)
                    .offset(48)
                    .build(),
            ]
        }
        VulkanaliaSceneSampledImageVertexProgram::ParticleGpu => {
            vec![
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
                    .format(vk::Format::R32G32_SFLOAT)
                    .offset(24)
                    .build(),
                vk::VertexInputAttributeDescription::builder()
                    .location(4)
                    .binding(0)
                    .format(vk::Format::R32G32B32A32_SFLOAT)
                    .offset(32)
                    .build(),
            ]
        }
    };
    let instance_attributes = [
        vk::VertexInputAttributeDescription::builder()
            .location(5)
            .binding(1)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(0)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(6)
            .binding(1)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(16)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(7)
            .binding(1)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(32)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(8)
            .binding(1)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(48)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(9)
            .binding(1)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(64)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(10)
            .binding(1)
            .format(vk::Format::R32G32B32A32_UINT)
            .offset(80)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(11)
            .binding(1)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(96)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(12)
            .binding(1)
            .format(vk::Format::R32G32B32A32_UINT)
            .offset(112)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(13)
            .binding(1)
            .format(vk::Format::R32G32B32A32_UINT)
            .offset(128)
            .build(),
    ];
    let mut attributes = vertex_attributes;
    attributes.extend_from_slice(&instance_attributes);
    let bindings = [vertex_binding, instance_binding];
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
    let sample_shading = scene_sample_shading_enabled(sample_count, sample_shading_enabled);
    let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(sample_count)
        .sample_shading_enable(sample_shading)
        .min_sample_shading(scene_min_sample_shading_value(sample_shading))
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
    sample_shading_enabled: bool,
) -> NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot {
    let sample_shading = scene_sample_shading_enabled(sample_count, sample_shading_enabled);
    NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot {
        binding: "vulkanalia",
        route: "scene-sampled-image-dynamic-rendering-pipeline",
        target_format: format!("{target_format:?}"),
        extent: (extent.width, extent.height),
        shader_modules_created: true,
        descriptor_set_layout_created: false,
        pipeline_layout_created: true,
        pipeline_created: true,
        pass_specific_fragment_pipeline_count: 153,
        rasterization_samples: scene_sample_count_label(sample_count),
        sample_shading_enabled: sample_shading,
        min_sample_shading: scene_min_sample_shading_label(sample_shading),
        render_pass_compatibility: "dynamic-rendering-no-render-pass",
        primitive_topology: "triangle-list-indexed-image-quad",
        vertex_input_binding_count: 2,
        vertex_input_attribute_count: 14,
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
        push_constant_model: "scene-space pixel extent, fixed-function viewport vertex extent, alpha/mask state, WE g_TextureNResolution rows, and pass-specific effect parameter rows; elapsed time is read from a retained GPU frame-time buffer",
        blend_model: "sampled rgba with opacity; alpha/normal/additive/multiply/screen/max/modulate/hsl-color blend pipeline selected per draw command; WE passthroughblend uses shader framebuffer sampling plus normal replace output",
        sampled_image_model: "retained native sampled image + GPU draw-instance constants -> VK_EXT_descriptor_heap constant-offset mapping -> generic, framebuffer-passthrough, or pass-specific fragment shader",
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
    swapchain_msaa_target: Option<&VulkanaliaSceneMsaaColorTarget>,
    extent: vk::Extent2D,
    pipeline_resources: &VulkanaliaSceneSolidQuadPipelineResources,
    vertex_buffer: vk::Buffer,
    vertex_offset_bytes: u64,
    draw_instance_buffer: vk::Buffer,
    index_buffer: vk::Buffer,
    draw_commands: &[VulkanaliaSceneSolidQuadDrawCommand],
    clear_color: [f32; 4],
) -> Result<NativeVulkanVulkanaliaSceneSolidQuadCommandSnapshot, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene solid quad command requires non-zero extent".to_owned());
    }
    if draw_commands.is_empty() {
        return Err("scene solid quad command requires at least one draw".to_owned());
    }
    for draw in draw_commands {
        if draw.index_count == 0 {
            return Err("scene solid quad command requires at least one index".to_owned());
        }
    }
    let index_count = draw_commands
        .iter()
        .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count));
    validate_scene_msaa_color_target(
        "solid quad swapchain",
        swapchain_msaa_target,
        extent,
        pipeline_resources.sample_count,
    )?;

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
        if let Some(msaa_target) = swapchain_msaa_target {
            scene_color_image_transition(
                device,
                command_buffer,
                msaa_target.image,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::PipelineStageFlags2::TOP_OF_PIPE,
                vk::AccessFlags2::empty(),
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            );
        }

        begin_scene_color_rendering(
            device,
            command_buffer,
            swapchain_view,
            swapchain_msaa_target,
            extent,
            vk::AttachmentLoadOp::CLEAR,
            clear_color,
        );
        let vertex_buffers = [vertex_buffer, draw_instance_buffer];
        let vertex_offsets = [vertex_offset_bytes, 0];
        let push_constants = [extent.width as f32, extent.height as f32];
        let push_constant_bytes = std::slice::from_raw_parts(
            push_constants.as_ptr().cast::<u8>(),
            SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES as usize,
        );
        let mut bound_pipeline = None;
        for draw in draw_commands {
            if bound_pipeline != Some(draw.blend) {
                device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    native_vulkan_vulkanalia_scene_solid_quad_pipeline(
                        pipeline_resources,
                        draw.blend.mode,
                    ),
                );
                device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
                device.cmd_bind_index_buffer(
                    command_buffer,
                    index_buffer,
                    0,
                    vk::IndexType::UINT32,
                );
                device.cmd_push_constants(
                    command_buffer,
                    pipeline_resources.pipeline_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    push_constant_bytes,
                );
                bound_pipeline = Some(draw.blend);
            }
            device.cmd_draw_indexed(
                command_buffer,
                draw.index_count,
                1,
                draw.first_index,
                0,
                draw.draw_instance_index,
            );
        }
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
        rasterization_samples: scene_sample_count_label(pipeline_resources.sample_count),
        uses_msaa_color_target: swapchain_msaa_target.is_some(),
        resolve_mode: if swapchain_msaa_target.is_some() {
            "average"
        } else {
            "none"
        },
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
                let vertex_buffers = [
                    solid_quad_draw.vertex_buffer,
                    solid_quad_draw.draw_instance_buffer,
                ];
                let vertex_offsets = [solid_quad_draw.vertex_offset_bytes, 0];
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
                solid_draw.draw_instance_index,
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
    swapchain_viewport_state: VulkanaliaSceneSampledImageViewportState,
    solid_quad_draw: Option<VulkanaliaSceneSolidQuadDrawResources<'_>>,
    descriptor_heap_draw: Option<VulkanaliaSceneDescriptorHeapDrawResources<'_>>,
    pipeline_resources: &VulkanaliaSceneSampledImagePipelineResources,
    draw_commands: &[VulkanaliaSceneSampledImageDrawCommand],
    vertex_buffer: vk::Buffer,
    puppet_gpu_payload_buffer: Option<vk::Buffer>,
    particle_gpu_payload_buffer: Option<vk::Buffer>,
    draw_instance_buffer: vk::Buffer,
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
        if draw.vertex_program == VulkanaliaSceneSampledImageVertexProgram::PuppetGpu
            && puppet_gpu_payload_buffer.is_none()
        {
            return Err(
                "scene sampled-image puppet GPU draw requires a retained payload buffer".to_owned(),
            );
        }
        if draw.vertex_program == VulkanaliaSceneSampledImageVertexProgram::ParticleGpu
            && particle_gpu_payload_buffer.is_none()
        {
            return Err(
                "scene sampled-image particle GPU draw requires a retained payload buffer"
                    .to_owned(),
            );
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
        let full_viewport = SceneDynamicViewport::full_extent(extent);
        let sampled_viewport = swapchain_viewport_state.dynamic_viewport(extent);
        set_scene_dynamic_viewport_and_scissor(device, command_buffer, full_viewport);
        let mut active_viewport = full_viewport;
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
                    if active_viewport != full_viewport {
                        set_scene_dynamic_viewport_and_scissor(
                            device,
                            command_buffer,
                            full_viewport,
                        );
                        active_viewport = full_viewport;
                    }
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
                        let vertex_buffers = [
                            solid_resources.vertex_buffer,
                            solid_resources.draw_instance_buffer,
                        ];
                        let vertex_offsets = [solid_resources.vertex_offset_bytes, 0];
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
                        solid_draw.draw_instance_index,
                    );
                }
                VulkanaliaSceneOrderedDrawPipeline::SampledImage => {
                    let sampled_draw = &draw_commands[draw.command_index];
                    if active_viewport != sampled_viewport {
                        set_scene_dynamic_viewport_and_scissor(
                            device,
                            command_buffer,
                            sampled_viewport,
                        );
                        active_viewport = sampled_viewport;
                    }
                    let pipeline_key = VulkanaliaSceneBoundDrawPipeline::SampledImage {
                        blend: sampled_draw.material.render_state.blend,
                        shader_program: scene_sampled_image_shader_program(&sampled_draw.material),
                        vertex_program: sampled_draw.vertex_program,
                    };
                    if bound_pipeline != Some(pipeline_key) {
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            native_vulkan_vulkanalia_scene_sampled_image_pipeline_for_material(
                                pipeline_resources,
                                &sampled_draw.material,
                                sampled_draw.vertex_program,
                            ),
                        );
                        let source_vertex_buffer = match sampled_draw.vertex_program {
                            VulkanaliaSceneSampledImageVertexProgram::Sampled => vertex_buffer,
                            VulkanaliaSceneSampledImageVertexProgram::PuppetGpu => {
                                puppet_gpu_payload_buffer.expect("puppet payload buffer checked")
                            }
                            VulkanaliaSceneSampledImageVertexProgram::ParticleGpu => {
                                particle_gpu_payload_buffer
                                    .expect("particle payload buffer checked")
                            }
                        };
                        let vertex_buffers = [source_vertex_buffer, draw_instance_buffer];
                        let vertex_offsets = [0u64, 0u64];
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
                        swapchain_viewport_state,
                        &sampled_draw.material,
                    );
                    device.cmd_draw_indexed(
                        command_buffer,
                        sampled_draw.index_count,
                        1,
                        sampled_draw.first_index,
                        sampled_draw.vertex_offset,
                        sampled_draw.draw_instance_index,
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
    viewport_state: VulkanaliaSceneSampledImageViewportState,
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
) {
    let push_constant_bytes =
        scene_sampled_image_push_constant_bytes(extent, viewport_state, material);
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
                "extent={}x{} alpha_slot={:?} mode={} shader_code={} texture_resolution_mask=0x{texture_resolution_mask:02x}",
                extent.width,
                extent.height,
                material.alpha_texture_slot,
                material.alpha_texture_mode.as_str(),
                material.alpha_texture_mode.shader_code(),
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
    viewport_state: VulkanaliaSceneSampledImageViewportState,
    material: &super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
) -> [u8; SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES as usize] {
    let alpha_texture_slot = material
        .alpha_texture_slot
        .unwrap_or(SCENE_SAMPLED_IMAGE_ALPHA_TEXTURE_SLOT_DISABLED);
    let mut push_constant_bytes = [0u8; SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES as usize];
    push_constant_bytes[0..4].copy_from_slice(&(extent.width as f32).to_ne_bytes());
    push_constant_bytes[4..8].copy_from_slice(&(extent.height as f32).to_ne_bytes());
    push_constant_bytes[8..12].copy_from_slice(&alpha_texture_slot.to_ne_bytes());
    push_constant_bytes[12..16]
        .copy_from_slice(&material.alpha_texture_mode.shader_code().to_ne_bytes());
    let output_flags = scene_sampled_image_output_flags(material.render_state.blend.mode);
    push_constant_bytes[SCENE_FULL_SAMPLED_IMAGE_PUSH_OUTPUT_FLAGS_OFFSET_BYTES
        ..SCENE_FULL_SAMPLED_IMAGE_PUSH_OUTPUT_FLAGS_OFFSET_BYTES + 4]
        .copy_from_slice(&output_flags.to_ne_bytes());
    let vertex_extent = viewport_state.vertex_extent();
    for (index, value) in vertex_extent.into_iter().enumerate() {
        let offset = SCENE_FULL_SAMPLED_IMAGE_PUSH_VERTEX_EXTENT_OFFSET_BYTES + index * 4;
        push_constant_bytes[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
    }
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
            let mut audio_flags = shape;
            if let Some(spectrum32_packed) = native_vulkan_audio_spectrum32_packed() {
                audio_flags |= 1u32 << 17;
                for (index, packed) in spectrum32_packed.iter().enumerate() {
                    scene_sampled_image_write_push_constant_u32(
                        &mut push_constant_bytes,
                        SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_SPECTRUM32_OFFSET_BYTES
                            + index * std::mem::size_of::<u32>(),
                        *packed,
                    );
                }
            }
            if let Some(signal_level) =
                native_vulkan_audio_signal_level().filter(|level| *level > f32::EPSILON)
            {
                audio_flags |= 1u32 << 16;
                scene_sampled_image_push_constant_f32(
                    &mut push_constant_bytes,
                    SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_SIGNAL_OFFSET_BYTES,
                    signal_level.clamp(0.0, 1.0),
                );
            }
            scene_sampled_image_write_push_constant_u32(
                &mut push_constant_bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_AUDIO_FLAGS_OFFSET_BYTES,
                audio_flags,
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

#[derive(Debug, Clone, Copy, PartialEq)]
struct SceneDynamicViewport {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    scissor_extent: vk::Extent2D,
}

impl SceneDynamicViewport {
    fn full_extent(extent: vk::Extent2D) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            scissor_extent: extent,
        }
    }
}

fn set_scene_dynamic_viewport_and_scissor(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    viewport: SceneDynamicViewport,
) {
    let vk_viewport = vk::Viewport::builder()
        .x(viewport.x)
        .y(viewport.y)
        .width(viewport.width)
        .height(viewport.height)
        .min_depth(0.0)
        .max_depth(1.0)
        .build();
    let scissor = vk::Rect2D::builder()
        .offset(vk::Offset2D { x: 0, y: 0 })
        .extent(viewport.scissor_extent)
        .build();
    unsafe {
        device.cmd_set_viewport(command_buffer, 0, &[vk_viewport]);
        device.cmd_set_scissor(command_buffer, 0, &[scissor]);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneSampledImageActiveRenderingTarget {
    Swapchain,
    EffectTarget(u32),
}

fn validate_scene_msaa_color_target(
    label: &'static str,
    target: Option<&VulkanaliaSceneMsaaColorTarget>,
    extent: vk::Extent2D,
    sample_count: vk::SampleCountFlags,
) -> Result<(), String> {
    if sample_count == vk::SampleCountFlags::_1 {
        return Ok(());
    }
    let Some(target) = target else {
        return Err(format!(
            "scene {label} render uses {} pipelines but has no MSAA color target",
            scene_sample_count_label(sample_count)
        ));
    };
    if target.sample_count != sample_count {
        return Err(format!(
            "scene {label} MSAA target sample count {} does not match pipeline sample count {}",
            scene_sample_count_label(target.sample_count),
            scene_sample_count_label(sample_count)
        ));
    }
    if target.extent.width != extent.width || target.extent.height != extent.height {
        return Err(format!(
            "scene {label} MSAA target extent {}x{} does not match render extent {}x{}",
            target.extent.width, target.extent.height, extent.width, extent.height
        ));
    }
    Ok(())
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
    msaa_target: Option<&VulkanaliaSceneMsaaColorTarget>,
    extent: vk::Extent2D,
    load_op: vk::AttachmentLoadOp,
    clear_color: [f32; 4],
) {
    let clear_value = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: clear_color,
        },
    };
    let mut color_attachment = vk::RenderingAttachmentInfo::builder()
        .image_view(msaa_target.map_or(image_view, |target| target.image_view))
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(load_op)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear_value);
    if msaa_target.is_some() {
        color_attachment = color_attachment
            .resolve_mode(vk::ResolveModeFlags::AVERAGE)
            .resolve_image_view(image_view)
            .resolve_image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    }
    let color_attachment = color_attachment.build();
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
    set_scene_dynamic_viewport_and_scissor(
        device,
        command_buffer,
        SceneDynamicViewport::full_extent(extent),
    );
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
    swapchain_msaa_target: Option<&VulkanaliaSceneMsaaColorTarget>,
    extent: vk::Extent2D,
    swapchain_viewport_state: VulkanaliaSceneSampledImageViewportState,
    solid_quad_draw: Option<VulkanaliaSceneSolidQuadDrawResources<'_>>,
    descriptor_heap_draw: Option<VulkanaliaSceneDescriptorHeapDrawResources<'_>>,
    pipeline_resources: &VulkanaliaSceneSampledImagePipelineResources,
    draw_commands: &[VulkanaliaSceneSampledImageDrawCommand],
    effect_target_resources: &[VulkanaliaSceneSampledImageResources],
    effect_msaa_targets: &[VulkanaliaSceneMsaaColorTarget],
    framebuffer_snapshot_resource: Option<&VulkanaliaSceneSampledImageResources>,
    framebuffer_snapshot_initial_layout: vk::ImageLayout,
    vertex_buffer: vk::Buffer,
    puppet_gpu_payload_buffer: Option<vk::Buffer>,
    particle_gpu_payload_buffer: Option<vk::Buffer>,
    draw_instance_buffer: vk::Buffer,
    index_buffer: vk::Buffer,
    clear_color: [f32; 4],
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
        if draw.vertex_program == VulkanaliaSceneSampledImageVertexProgram::PuppetGpu
            && puppet_gpu_payload_buffer.is_none()
        {
            return Err(
                "scene sampled-image puppet GPU draw requires a retained payload buffer".to_owned(),
            );
        }
        if draw.vertex_program == VulkanaliaSceneSampledImageVertexProgram::ParticleGpu
            && particle_gpu_payload_buffer.is_none()
        {
            return Err(
                "scene sampled-image particle GPU draw requires a retained payload buffer"
                    .to_owned(),
            );
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
    validate_scene_msaa_color_target(
        "sampled-image swapchain",
        swapchain_msaa_target,
        extent,
        pipeline_resources.sample_count,
    )?;
    if pipeline_resources.sample_count != vk::SampleCountFlags::_1 {
        if effect_msaa_targets.len() < effect_target_resources.len() {
            return Err(format!(
                "scene sampled-image MSAA effect target count {} is smaller than effect target resource count {}",
                effect_msaa_targets.len(),
                effect_target_resources.len()
            ));
        }
        for (index, target) in effect_target_resources.iter().enumerate() {
            let extent = vk::Extent2D {
                width: target.snapshot.extent.0,
                height: target.snapshot.extent.1,
            };
            validate_scene_msaa_color_target(
                "sampled-image effect target",
                effect_msaa_targets.get(index),
                extent,
                pipeline_resources.sample_count,
            )?;
        }
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
        let mut active_viewport: Option<SceneDynamicViewport> = None;
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
                            if let Some(msaa_target) = swapchain_msaa_target {
                                scene_color_image_transition(
                                    device,
                                    command_buffer,
                                    msaa_target.image,
                                    vk::ImageLayout::UNDEFINED,
                                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                                    vk::PipelineStageFlags2::TOP_OF_PIPE,
                                    vk::AccessFlags2::empty(),
                                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                                );
                            }
                            swapchain_started = true;
                            vk::AttachmentLoadOp::CLEAR
                        };
                        active_extent = extent;
                        begin_scene_color_rendering(
                            device,
                            command_buffer,
                            swapchain_view,
                            swapchain_msaa_target,
                            active_extent,
                            load_op,
                            clear_color,
                        );
                        active_viewport = Some(SceneDynamicViewport::full_extent(active_extent));
                    }
                    SceneSampledImageActiveRenderingTarget::EffectTarget(target_index) => {
                        let sampled_draw = &draw_commands[draw.command_index];
                        let VulkanaliaSceneSampledImageRenderTarget::EffectTarget { clear, .. } =
                            sampled_draw.render_target
                        else {
                            unreachable!("desired effect target came from sampled draw target");
                        };
                        let target = &effect_target_resources[target_index as usize];
                        let effect_msaa_target = (pipeline_resources.sample_count
                            != vk::SampleCountFlags::_1)
                            .then(|| &effect_msaa_targets[target_index as usize]);
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
                        if let Some(msaa_target) = effect_msaa_target {
                            scene_color_image_transition(
                                device,
                                command_buffer,
                                msaa_target.image,
                                if clear {
                                    vk::ImageLayout::UNDEFINED
                                } else {
                                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                                },
                                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                                if clear {
                                    vk::PipelineStageFlags2::TOP_OF_PIPE
                                } else {
                                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                                },
                                if clear {
                                    vk::AccessFlags2::empty()
                                } else {
                                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                                },
                                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                            );
                        }
                        begin_scene_color_rendering(
                            device,
                            command_buffer,
                            target.image_view,
                            effect_msaa_target,
                            active_extent,
                            if clear {
                                vk::AttachmentLoadOp::CLEAR
                            } else {
                                vk::AttachmentLoadOp::LOAD
                            },
                            [0.0, 0.0, 0.0, 0.0],
                        );
                        active_viewport = Some(SceneDynamicViewport::full_extent(active_extent));
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
                            swapchain_msaa_target,
                            extent,
                            vk::AttachmentLoadOp::LOAD,
                            clear_color,
                        );
                        active_target = Some(SceneSampledImageActiveRenderingTarget::Swapchain);
                        active_extent = extent;
                        active_viewport = Some(SceneDynamicViewport::full_extent(active_extent));
                        bound_pipeline = None;
                        bound_descriptor_heap_group = None;
                    }
                    let solid_viewport = SceneDynamicViewport::full_extent(active_extent);
                    if active_viewport != Some(solid_viewport) {
                        set_scene_dynamic_viewport_and_scissor(
                            device,
                            command_buffer,
                            solid_viewport,
                        );
                        active_viewport = Some(solid_viewport);
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
                        let vertex_buffers = [
                            solid_resources.vertex_buffer,
                            solid_resources.draw_instance_buffer,
                        ];
                        let vertex_offsets = [solid_resources.vertex_offset_bytes, 0];
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
                            swapchain_msaa_target,
                            extent,
                            vk::AttachmentLoadOp::LOAD,
                            clear_color,
                        );
                        active_target = Some(SceneSampledImageActiveRenderingTarget::Swapchain);
                        active_extent = extent;
                        active_viewport = Some(SceneDynamicViewport::full_extent(active_extent));
                        bound_pipeline = None;
                        bound_descriptor_heap_group = None;
                    }
                    let sampled_viewport = match active_target {
                        Some(SceneSampledImageActiveRenderingTarget::Swapchain) => {
                            swapchain_viewport_state.dynamic_viewport(extent)
                        }
                        Some(SceneSampledImageActiveRenderingTarget::EffectTarget(_)) | None => {
                            SceneDynamicViewport::full_extent(active_extent)
                        }
                    };
                    if active_viewport != Some(sampled_viewport) {
                        set_scene_dynamic_viewport_and_scissor(
                            device,
                            command_buffer,
                            sampled_viewport,
                        );
                        active_viewport = Some(sampled_viewport);
                    }
                    let pipeline_key = VulkanaliaSceneBoundDrawPipeline::SampledImage {
                        blend: sampled_draw.material.render_state.blend,
                        shader_program: scene_sampled_image_shader_program(&sampled_draw.material),
                        vertex_program: sampled_draw.vertex_program,
                    };
                    if bound_pipeline != Some(pipeline_key) {
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            native_vulkan_vulkanalia_scene_sampled_image_pipeline_for_material(
                                pipeline_resources,
                                &sampled_draw.material,
                                sampled_draw.vertex_program,
                            ),
                        );
                        let source_vertex_buffer = match sampled_draw.vertex_program {
                            VulkanaliaSceneSampledImageVertexProgram::Sampled => vertex_buffer,
                            VulkanaliaSceneSampledImageVertexProgram::PuppetGpu => {
                                puppet_gpu_payload_buffer.expect("puppet payload buffer checked")
                            }
                            VulkanaliaSceneSampledImageVertexProgram::ParticleGpu => {
                                particle_gpu_payload_buffer
                                    .expect("particle payload buffer checked")
                            }
                        };
                        let vertex_buffers = [source_vertex_buffer, draw_instance_buffer];
                        let vertex_offsets = [0u64, 0u64];
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
                        match active_target {
                            Some(SceneSampledImageActiveRenderingTarget::Swapchain) => {
                                swapchain_viewport_state
                            }
                            Some(SceneSampledImageActiveRenderingTarget::EffectTarget(_))
                            | None => {
                                VulkanaliaSceneSampledImageViewportState::full_extent(active_extent)
                            }
                        },
                        &sampled_draw.material,
                    );
                    device.cmd_draw_indexed(
                        command_buffer,
                        sampled_draw.index_count,
                        1,
                        sampled_draw.first_index,
                        sampled_draw.vertex_offset,
                        sampled_draw.draw_instance_index,
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
        rasterization_samples: scene_sample_count_label(pipeline_resources.sample_count),
        uses_msaa_color_target: swapchain_msaa_target.is_some() || !effect_msaa_targets.is_empty(),
        effect_msaa_target_count: saturating_u32(effect_msaa_targets.len()),
        resolve_mode: if pipeline_resources.sample_count != vk::SampleCountFlags::_1 {
            "average"
        } else {
            "none"
        },
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

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_VERTEX_SPIRV: [u32; 3227] =
    include!("shaders/solid_quad.vert.spv.rs");

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

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_VERTEX_SPIRV: [u32; 8866] =
    include!("shaders/sampled_image.vert.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PUPPET_VERTEX_SPIRV: [u32; 9195] =
    include!("shaders/sampled_image_puppet.vert.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PARTICLE_VERTEX_SPIRV: [u32; 2927] =
    include!("shaders/sampled_image_particle.vert.spv.rs");

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV: [u32; 1383] =
    include!("shaders/sampled_image.frag.spv.rs");

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PREMULTIPLIED_FRAGMENT_SPIRV: [u32; 1383] =
    include!("shaders/sampled_image.frag.spv.rs");

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERRIPPLE_FRAGMENT_SPIRV: [u32; 1688] =
    include!("shaders/sampled_image_waterripple.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERWAVES_FRAGMENT_SPIRV: [u32; 2890] =
    include!("shaders/sampled_image_waterwaves.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERFLOW_FRAGMENT_SPIRV: [u32; 1765] =
    include!("shaders/sampled_image_waterflow.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERCAUSTICS_FRAGMENT_SPIRV: [u32; 5040] =
    include!("shaders/sampled_image_watercaustics.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FOLIAGE_SWAY_FRAGMENT_SPIRV: [u32; 2225] =
    include!("shaders/sampled_image_foliagesway.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_AUTO_SWAY_FRAGMENT_SPIRV: [u32; 4869] =
    include!("shaders/sampled_image_autosway.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SCROLL_FRAGMENT_SPIRV: [u32; 943] =
    include!("shaders/sampled_image_scroll.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SKEW_FRAGMENT_SPIRV: [u32; 1155] =
    include!("shaders/sampled_image_skew.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_IRIS_FRAGMENT_SPIRV: [u32; 1435] =
    include!("shaders/sampled_image_iris.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_OPACITY_FRAGMENT_SPIRV: [u32; 932] =
    include!("shaders/sampled_image_opacity.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_TECHCIRCLE_FRAGMENT_SPIRV: [u32; 2925] =
    include!("shaders/sampled_image_techcircle.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_AUDIOBARS_FRAGMENT_SPIRV: [u32; 4686] =
    include!("shaders/sampled_image_audiobars.frag.spv.rs");
const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PASSTHROUGHBLEND_FRAGMENT_SPIRV: [u32;
    7365] = include!("shaders/sampled_image_passthroughblend.frag.spv.rs");

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

    fn full_sampled_viewport(width: u32, height: u32) -> VulkanaliaSceneSampledImageViewportState {
        VulkanaliaSceneSampledImageViewportState::full_extent(vk::Extent2D { width, height })
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
            false,
        );

        assert_eq!(snapshot.target_format, "B8G8R8A8_SRGB");
        assert_eq!(snapshot.extent, (3840, 2160));
        assert_eq!(snapshot.rasterization_samples, "1x");
        assert!(!snapshot.sample_shading_enabled);
        assert_eq!(snapshot.min_sample_shading, "0.0");
        assert_eq!(
            snapshot.render_pass_compatibility,
            "dynamic-rendering-no-render-pass"
        );
        assert_eq!(snapshot.vertex_input_binding_count, 2);
        assert_eq!(snapshot.vertex_input_attribute_count, 4);
        assert_eq!(snapshot.vertex_stride_bytes, 24);
        assert_eq!(snapshot.push_constant_bytes, 8);
        assert_eq!(
            snapshot.push_constant_model,
            "scene-space pixel extent -> NDC conversion; retained layer pose timelines read frame time and transform/state payloads on GPU"
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
            false,
        );

        assert_eq!(snapshot.target_format, "B8G8R8A8_SRGB");
        assert_eq!(snapshot.extent, (3840, 2160));
        assert_eq!(snapshot.rasterization_samples, "1x");
        assert!(!snapshot.sample_shading_enabled);
        assert_eq!(snapshot.min_sample_shading, "0.0");
        assert!(!snapshot.descriptor_set_layout_created);
        assert_eq!(snapshot.descriptor_type, "combined-image-sampler");
        assert_eq!(snapshot.descriptor_binding, 0);
        assert_eq!(snapshot.vertex_input_attribute_count, 14);
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
            "retained native sampled image + GPU draw-instance constants -> VK_EXT_descriptor_heap constant-offset mapping -> generic, framebuffer-passthrough, or pass-specific fragment shader"
        );
        assert_eq!(snapshot.pass_specific_fragment_pipeline_count, 153);
        assert!(snapshot.uses_pipeline_rendering_create_info);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert_eq!(snapshot.descriptor_set_count, 0);
        assert_eq!(snapshot.descriptor_model, "VK_EXT_descriptor_heap");
        assert_eq!(snapshot.push_constant_bytes, 256);
        assert_eq!(
            snapshot.push_constant_model,
            "scene-space pixel extent, fixed-function viewport vertex extent, alpha/mask state, WE g_TextureNResolution rows, and pass-specific effect parameter rows; elapsed time is read from a retained GPU frame-time buffer"
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
    fn scene_pipeline_snapshots_record_msaa_sample_count() {
        let extent = vk::Extent2D {
            width: 1920,
            height: 1080,
        };
        let solid = native_vulkan_vulkanalia_scene_solid_quad_pipeline_snapshot(
            vk::Format::B8G8R8A8_UNORM,
            extent,
            vk::SampleCountFlags::_4,
            true,
        );
        let sampled = native_vulkan_vulkanalia_scene_sampled_image_pipeline_snapshot(
            vk::Format::B8G8R8A8_UNORM,
            extent,
            vk::SampleCountFlags::_4,
            true,
        );

        assert_eq!(solid.rasterization_samples, "4x");
        assert_eq!(sampled.rasterization_samples, "4x");
        assert!(solid.sample_shading_enabled);
        assert_eq!(solid.min_sample_shading, "1.0");
        assert!(sampled.sample_shading_enabled);
        assert_eq!(sampled.min_sample_shading, "1.0");
        assert!(solid.uses_dynamic_rendering);
        assert!(sampled.uses_dynamic_rendering);
    }

    #[test]
    fn scene_msaa_target_validation_requires_matching_multisample_target() {
        let extent = vk::Extent2D {
            width: 1280,
            height: 720,
        };
        assert!(
            validate_scene_msaa_color_target("unit-test", None, extent, vk::SampleCountFlags::_1,)
                .is_ok()
        );

        let err =
            validate_scene_msaa_color_target("unit-test", None, extent, vk::SampleCountFlags::_4)
                .unwrap_err();
        assert!(err.contains("uses 4x pipelines but has no MSAA color target"));

        let target = VulkanaliaSceneMsaaColorTarget {
            image: vk::Image::null(),
            image_view: vk::ImageView::null(),
            extent,
            sample_count: vk::SampleCountFlags::_2,
        };
        let err = validate_scene_msaa_color_target(
            "unit-test",
            Some(&target),
            extent,
            vk::SampleCountFlags::_4,
        )
        .unwrap_err();
        assert!(err.contains("sample count 2x does not match pipeline sample count 4x"));

        let target = VulkanaliaSceneMsaaColorTarget {
            sample_count: vk::SampleCountFlags::_4,
            extent: vk::Extent2D {
                width: 640,
                height: 720,
            },
            ..target
        };
        let err = validate_scene_msaa_color_target(
            "unit-test",
            Some(&target),
            extent,
            vk::SampleCountFlags::_4,
        )
        .unwrap_err();
        assert!(err.contains("MSAA target extent 640x720 does not match render extent 1280x720"));

        let target = VulkanaliaSceneMsaaColorTarget { extent, ..target };
        assert!(
            validate_scene_msaa_color_target(
                "unit-test",
                Some(&target),
                extent,
                vk::SampleCountFlags::_4,
            )
            .is_ok()
        );
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
            full_sampled_viewport(3840, 2160),
            &material,
        );

        assert_eq!(push_f32(&bytes, 0), 3840.0);
        assert_eq!(push_f32(&bytes, 4), 2160.0);
        assert_eq!(push_u32(&bytes, 8), 1);
        assert_eq!(
            push_u32(&bytes, 12),
            SceneRenderAlphaTextureMode::Coverage.shader_code()
        );
        assert_eq!(push_f32(&bytes, 16), 0.0);
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
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_VERTEX_EXTENT_OFFSET_BYTES
            ),
            3840.0
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_VERTEX_EXTENT_OFFSET_BYTES + 4
            ),
            2160.0
        );
        let slot_2_offset = SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_BASE_OFFSET_BYTES
            + 2 * SCENE_FULL_SAMPLED_IMAGE_PUSH_TEXTURE_RESOLUTION_STRIDE_BYTES;
        assert_eq!(push_f32(&bytes, slot_2_offset), 512.0);
        assert_eq!(push_f32(&bytes, slot_2_offset + 4), 256.0);
    }

    #[test]
    fn sampled_image_push_constants_encode_sampled_vertex_extent() {
        let bytes = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            VulkanaliaSceneSampledImageViewportState {
                vertex_extent_width: 2160.0,
                vertex_extent_height: 1440.0,
                viewport_x: 0.0,
                viewport_y: -53.166668,
                viewport_width: 2561.0,
                viewport_height: 1707.3334,
            },
            &sampled_image_material(SceneBlendMode::Alpha),
        );

        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_VERTEX_EXTENT_OFFSET_BYTES
            ),
            2160.0
        );
        assert_eq!(
            push_f32(
                &bytes,
                SCENE_FULL_SAMPLED_IMAGE_PUSH_VERTEX_EXTENT_OFFSET_BYTES + 4
            ),
            1440.0
        );
    }

    #[test]
    fn sampled_image_push_constants_mark_premultiplied_output_blends() {
        let alpha = scene_sampled_image_push_constant_bytes(
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            full_sampled_viewport(3840, 2160),
            &sampled_image_material(SceneBlendMode::Alpha),
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
                full_sampled_viewport(3840, 2160),
                &sampled_image_material(blend),
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
            full_sampled_viewport(1280, 720),
            &material,
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
            full_sampled_viewport(1280, 720),
            &material,
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
            full_sampled_viewport(3840, 2160),
            &material,
        );

        assert_eq!(
            scene_sampled_image_shader_program(&material),
            VulkanaliaSceneSampledImageShaderProgram::Iris
        );
        assert_eq!(push_f32(&bytes, 16), 0.0);
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
            full_sampled_viewport(3840, 2160),
            &material,
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
            full_sampled_viewport(3840, 2160),
            &material,
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
            full_sampled_viewport(3840, 2160),
            &material,
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
            full_sampled_viewport(3840, 2160),
            &material,
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
            full_sampled_viewport(3840, 2160),
            &material,
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
            full_sampled_viewport(3840, 2160),
            &material,
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
            full_sampled_viewport(3840, 2160),
            &material,
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
            full_sampled_viewport(3840, 2160),
            &material,
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
            full_sampled_viewport(3840, 2160),
            &material,
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
            full_sampled_viewport(3840, 2160),
            &material,
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
            full_sampled_viewport(3840, 2160),
            &material,
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
            false,
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
            draw_instance_index: 0,
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
                draw_instance_index: 0,
                vertex_program: VulkanaliaSceneSampledImageVertexProgram::Sampled,
                vertex_offset: 0,
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
                draw_instance_index: 0,
                vertex_program: VulkanaliaSceneSampledImageVertexProgram::Sampled,
                vertex_offset: 0,
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
            12908
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
            35464
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV
            ),
            5532
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PREMULTIPLIED_FRAGMENT_SPIRV
            ),
            5532
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERRIPPLE_FRAGMENT_SPIRV
            ),
            6752
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_WATERWAVES_FRAGMENT_SPIRV
            ),
            11560
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
            8900
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_AUTO_SWAY_FRAGMENT_SPIRV
            ),
            19476
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SCROLL_FRAGMENT_SPIRV
            ),
            3772
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_SKEW_FRAGMENT_SPIRV
            ),
            4620
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_IRIS_FRAGMENT_SPIRV
            ),
            5740
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_OPACITY_FRAGMENT_SPIRV
            ),
            3728
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
