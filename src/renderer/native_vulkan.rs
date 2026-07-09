//! Native Wayland/Vulkan renderer.
//!
//! This module owns the native Wayland/Vulkan renderer path. The backend
//! contract covers native Wayland layer-shell ownership, Vulkan
//! surface/swapchain ownership, and direct video texture interop.

#![allow(unsafe_code)]
#![allow(dead_code)]

include!("native_vulkan/backend_entry.rs");
include!("native_vulkan/h264_h265_parameter_sets.rs");
include!("native_vulkan/h264_h265_slice_parsers.rs");
include!("native_vulkan/av1_frame_submit.rs");
include!("native_vulkan/av1_sequence_contract.rs");
