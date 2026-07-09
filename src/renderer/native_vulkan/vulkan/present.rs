pub(in crate::renderer::native_vulkan::vulkan) mod clear;
pub(in crate::renderer::native_vulkan::vulkan) mod render;
pub(in crate::renderer::native_vulkan::vulkan) mod render_descriptors;
pub(in crate::renderer::native_vulkan::vulkan) mod swapchain;
pub(in crate::renderer::native_vulkan::vulkan) mod timing;

use self::render_descriptors as render_present_descriptors;
use self::timing as present_timing;
use super::core::{descriptor_heap, features, instance};
use super::video::{
    decode_submit as video_decode_submit, present_handoff as video_present_handoff,
    session_images as video_session_images,
};
