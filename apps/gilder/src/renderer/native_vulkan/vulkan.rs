//! Vulkanalia-backed native Vulkan implementation.
//!
//! Vulkanalia is the only Vulkan binding used by the native renderer. This
//! facade keeps the public native-vulkan API stable while implementation files
//! are grouped by core device setup, present, scene, and video responsibilities.

mod core;
mod device_selection;
mod present;
mod scene;
mod video;

use self::core::buffer;
use self::core::descriptor_heap;
use self::core::device_probe;
use self::core::features;
use self::core::image;
use self::core::plan;
use self::core::profiles;
use self::present::clear as present_clear;
use self::present::render as render_present;
use self::present::render_descriptors as render_present_descriptors;
use self::present::swapchain;
use self::scene as vulkan_scene;
use self::video::present_device as video_present_device;
use self::video::present_handoff as video_present_handoff;
#[cfg(feature = "native-vulkan-video")]
use self::video::present_runtime as video_present_runtime;
use self::video::surface_host as video_surface_host;

#[allow(unused_imports)]
pub(in crate::renderer::native_vulkan) use buffer::{
    NativeVulkanVulkanaliaBuffer, NativeVulkanVulkanaliaBufferMemoryPreference,
    NativeVulkanVulkanaliaBufferSnapshot, NativeVulkanVulkanaliaRecordedBufferUpload,
    native_vulkan_vulkanalia_create_buffer,
    native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload,
    native_vulkan_vulkanalia_destroy_buffer, native_vulkan_vulkanalia_read_host_buffer,
    native_vulkan_vulkanalia_write_host_buffer,
};
pub use descriptor_heap::NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot;
#[allow(unused_imports)]
pub(in crate::renderer::native_vulkan) use descriptor_heap::{
    NativeVulkanDescriptorHeapShaderBindingMapping,
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput,
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanInput,
    NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanSnapshot,
    VulkanaliaDescriptorHeapImageSamplerResources, VulkanaliaDescriptorHeapResourceResources,
    VulkanaliaDescriptorHeapUniformBufferResources,
    native_vulkan_vulkanalia_create_descriptor_heap_image_sampler_resources,
    native_vulkan_vulkanalia_create_descriptor_heap_resource_resources,
    native_vulkan_vulkanalia_create_descriptor_heap_uniform_buffer_resources,
    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_mapping,
    native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan,
    native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor,
    native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info_for_descriptor,
    native_vulkan_vulkanalia_descriptor_heap_resource_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_resource_bind_info_for_image,
    native_vulkan_vulkanalia_descriptor_heap_resource_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_input_attachment_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_plan,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_input_attachment_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_mixed_input_attachment_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_sampled_image_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_storage_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_uniform_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info_for_image,
    native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info,
    native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_plan,
    native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_resource_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_resource_bind_info_for_buffer,
    native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources,
    native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources,
    native_vulkan_vulkanalia_destroy_descriptor_heap_uniform_buffer_resources,
    native_vulkan_vulkanalia_write_descriptor_heap_image_sampler,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_image_sampler,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_input_attachment,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_storage_buffer,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_uniform_buffer,
    native_vulkan_vulkanalia_write_descriptor_heap_uniform_buffer,
};
pub use device_probe::{
    NativeVulkanVulkanaliaDeviceProbeSnapshot, NativeVulkanVulkanaliaDeviceProbeTemplate,
    NativeVulkanVulkanaliaRoadmap2026ProbeSnapshot,
    NativeVulkanVulkanaliaVideoMaintenanceFeatureSnapshot,
    native_vulkan_vulkanalia_device_probe_template, probe_native_vulkan_vulkanalia_devices,
};
pub use features::{
    NativeVulkanVulkanaliaCoreFeatureSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaFeatureChainTemplate, NativeVulkanVulkanaliaVulkan14PropertySnapshot,
    native_vulkan_vulkanalia_feature_chain_template,
};
#[allow(unused_imports)]
pub(in crate::renderer::native_vulkan) use image::{
    NativeVulkanVulkanaliaImage, NativeVulkanVulkanaliaImageMipUpload,
    NativeVulkanVulkanaliaImageSnapshot, NativeVulkanVulkanaliaRecordedImageUpload,
    native_vulkan_vulkanalia_create_color_attachment_sampled_image,
    native_vulkan_vulkanalia_create_color_attachment_sampled_image_with_usage,
    native_vulkan_vulkanalia_create_multisampled_color_attachment_image,
    native_vulkan_vulkanalia_create_sampled_image_with_recorded_staging_upload,
    native_vulkan_vulkanalia_destroy_image,
};
pub use plan::{NativeVulkanBackendPlan, native_vulkan_backend_plan};
pub use present_clear::{
    NativeVulkanVulkanaliaClearPresentOptions, NativeVulkanVulkanaliaClearPresentSnapshot,
    run_native_vulkan_vulkanalia_clear_present,
};
pub use profiles::{
    NativeVulkanVulkanaliaVideoProfileTemplate, native_vulkan_vulkanalia_video_profile_templates,
};
#[allow(unused_imports)]
pub use render_present::{
    NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot,
    NativeVulkanVulkanaliaDecodedImagePresentPipelineSnapshot,
    NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot,
};
pub use render_present_descriptors::NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot;
#[allow(unused_imports)]
pub use swapchain::{
    NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    NativeVulkanVulkanaliaPresentQueueSnapshot, NativeVulkanVulkanaliaSurfaceCapabilitiesSnapshot,
    NativeVulkanVulkanaliaSurfaceFormatSnapshot, NativeVulkanVulkanaliaSurfaceSnapshot,
    NativeVulkanVulkanaliaSurfaceSwapchainProbeOptions,
    NativeVulkanVulkanaliaSurfaceSwapchainProbeSnapshot, NativeVulkanVulkanaliaSwapchainSnapshot,
    probe_native_vulkan_vulkanalia_surface_swapchain,
};
pub use video_present_device::{
    NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    NativeVulkanVulkanaliaVideoPresentDeviceProbeOptions,
    NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot,
    NativeVulkanVulkanaliaVideoPresentFeatureSnapshot,
    NativeVulkanVulkanaliaVideoPresentQueueSnapshot,
    probe_native_vulkan_vulkanalia_video_present_device,
};
pub use video_present_handoff::NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot;
#[cfg(feature = "native-vulkan-video")]
#[allow(unused_imports)]
pub(in crate::renderer::native_vulkan) use video_present_runtime::{
    NativeVulkanFfmpegVulkanHwSceneVideoPresentOptions,
    NativeVulkanFfmpegVulkanHwSceneVideoPresentSourceOptions,
    run_native_vulkan_ffmpeg_vulkan_hw_scene_video_present,
};
#[cfg(feature = "native-vulkan-video")]
pub use video_present_runtime::{
    NativeVulkanFfmpegVulkanHwSceneVideoPresentSnapshot,
    NativeVulkanFfmpegVulkanHwSceneVideoPresentSourceSnapshot,
    NativeVulkanFfmpegVulkanHwVideoPresentOptions, NativeVulkanFfmpegVulkanHwVideoPresentSnapshot,
    run_native_vulkan_ffmpeg_vulkan_hw_video_present,
};
pub use video_surface_host::NativeVulkanVideoSurfaceHostSnapshot;
pub use vulkan_scene::{
    NativeVulkanSceneOwnedUniformArenaPlanSnapshot, NativeVulkanSceneOwnedUniformSliceSnapshot,
    native_vulkan_scene_owned_uniform_arena_plan,
};
pub(in crate::renderer::native_vulkan) use vulkan_scene::{
    NativeVulkanVulkanaliaScenePresentOptions, NativeVulkanVulkanaliaScenePresentSnapshot,
    run_native_vulkan_vulkanalia_scene_present,
};
