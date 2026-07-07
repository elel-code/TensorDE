//! Vulkanalia-backed native Vulkan implementation.
//!
//! Vulkanalia is the only Vulkan binding used by the native renderer. This
//! facade keeps the public native-vulkan API stable while implementation files
//! are grouped by core device setup, present, scene, and video responsibilities.

mod core;
mod present;
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
use self::present::scene as scene_present;
use self::present::scene_prepare;
use self::present::swapchain;
use self::video::bitstream_buffer as video_bitstream_buffer;
use self::video::decode_submit as video_decode_submit;
use self::video::decode_submit_av1 as video_decode_submit_av1;
use self::video::decode_submit_h264 as video_decode_submit_h264;
use self::video::decode_submit_h265 as video_decode_submit_h265;
use self::video::direct_runtime as video_direct_runtime;
use self::video::format_probe as video_format_probe;
use self::video::present_device as video_present_device;
use self::video::present_handoff as video_present_handoff;
use self::video::present_runtime as video_present_runtime;
use self::video::profile_probe as video_profile_probe;
use self::video::session as video_session;
use self::video::session_bind as video_session_bind;
use self::video::session_images as video_session_images;
use self::video::session_parameters as video_session_parameters;
use self::video::surface_host as video_surface_host;

#[cfg(test)]
pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_h265_std_long_term_ref_pics_sps(
    ref_pics: &[super::NativeVulkanH265LongTermRefPicSpsSnapshot],
) -> Result<Option<vulkanalia::vk::video::StdVideoH265LongTermRefPicsSps>, String> {
    video::session_parameters_h265::native_vulkan_vulkanalia_h265_std_long_term_ref_pics_sps(
        ref_pics,
    )
}

#[cfg(test)]
pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_h265_std_short_term_ref_pic_set(
    ref_pic_set: &super::NativeVulkanH265ShortTermRefPicSetSnapshot,
) -> Result<vulkanalia::vk::video::StdVideoH265ShortTermRefPicSet, String> {
    video::session_parameters_h265::native_vulkan_vulkanalia_h265_std_short_term_ref_pic_set(
        ref_pic_set,
    )
}

#[allow(unused_imports)]
pub(in crate::renderer::native_vulkan) use buffer::{
    NativeVulkanVulkanaliaBuffer, NativeVulkanVulkanaliaBufferMemoryPreference,
    NativeVulkanVulkanaliaBufferSnapshot, NativeVulkanVulkanaliaRecordedBufferUpload,
    native_vulkan_vulkanalia_create_buffer,
    native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload,
    native_vulkan_vulkanalia_destroy_buffer,
};
pub use descriptor_heap::NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot;
#[allow(unused_imports)]
pub(in crate::renderer::native_vulkan) use descriptor_heap::{
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
    native_vulkan_vulkanalia_descriptor_heap_resource_plan,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_sampled_image_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_uniform_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info_for_image,
    native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_plan,
    native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_resource_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_resource_bind_info_for_buffer,
    native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources,
    native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources,
    native_vulkan_vulkanalia_destroy_descriptor_heap_uniform_buffer_resources,
    native_vulkan_vulkanalia_write_descriptor_heap_image_sampler,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_image_sampler,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_uniform_buffer,
    native_vulkan_vulkanalia_write_descriptor_heap_uniform_buffer,
};
pub use device_probe::{
    NativeVulkanVulkanaliaDeviceProbeSnapshot, NativeVulkanVulkanaliaDeviceProbeTemplate,
    NativeVulkanVulkanaliaRoadmap2026FeatureProbeSnapshot,
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
pub use scene_prepare::{
    NativeVulkanVulkanaliaScenePipelinePrepareSnapshot, NativeVulkanVulkanaliaScenePrepareSnapshot,
    NativeVulkanVulkanaliaScenePrepareSubmitSnapshot,
    NativeVulkanVulkanaliaSceneResourcePrepareSnapshot,
};
pub use scene_present::{
    NativeVulkanVulkanaliaScenePresentOptions, NativeVulkanVulkanaliaScenePresentSnapshot,
    run_native_vulkan_vulkanalia_scene_present,
};
#[allow(unused_imports)]
pub use swapchain::{
    NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    NativeVulkanVulkanaliaPresentQueueSnapshot, NativeVulkanVulkanaliaSurfaceCapabilitiesSnapshot,
    NativeVulkanVulkanaliaSurfaceFormatSnapshot, NativeVulkanVulkanaliaSurfaceSnapshot,
    NativeVulkanVulkanaliaSurfaceSwapchainProbeOptions,
    NativeVulkanVulkanaliaSurfaceSwapchainProbeSnapshot, NativeVulkanVulkanaliaSwapchainSnapshot,
    probe_native_vulkan_vulkanalia_surface_swapchain,
};
#[allow(unused_imports)]
pub use video_bitstream_buffer::{
    NativeVulkanVulkanaliaVideoSessionBitstreamBufferSmokeSnapshot,
    NativeVulkanVulkanaliaVideoSessionBitstreamBufferSnapshot,
};
pub use video_decode_submit::NativeVulkanVulkanaliaStreamingDecodeTimingSnapshot;
pub use video_decode_submit_av1::{
    NativeVulkanVulkanaliaAv1CdefPlan, NativeVulkanVulkanaliaAv1CommandFrameSnapshot,
    NativeVulkanVulkanaliaAv1CommandSmokeSnapshot, NativeVulkanVulkanaliaAv1FrameSubmitInput,
    NativeVulkanVulkanaliaAv1GlobalMotionPlan, NativeVulkanVulkanaliaAv1LoopFilterPlan,
    NativeVulkanVulkanaliaAv1LoopRestorationPlan, NativeVulkanVulkanaliaAv1QuantizationPlan,
    NativeVulkanVulkanaliaAv1ReferenceInfoPlan, NativeVulkanVulkanaliaAv1SegmentationPlan,
    NativeVulkanVulkanaliaAv1TileInfoPlan,
};
pub use video_decode_submit_h264::{
    NativeVulkanVulkanaliaH264ReadyPrefixCommandFrameSnapshot,
    NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaH264ReadyPrefixFrameInput,
};
pub use video_decode_submit_h265::{
    NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaH265ReadyPrefixFrameInput,
};
pub use video_direct_runtime::{
    NativeVulkanVulkanaliaDirectCodecRuntimePlan, NativeVulkanVulkanaliaDirectRuntimeContract,
    native_vulkan_vulkanalia_direct_codec_runtime_plans,
    native_vulkan_vulkanalia_direct_runtime_contract,
};
#[allow(unused_imports)]
pub use video_format_probe::{
    NativeVulkanVulkanaliaVideoFormatProbeSnapshot,
    NativeVulkanVulkanaliaVideoFormatPropertySnapshot,
    NativeVulkanVulkanaliaVideoFormatQuerySnapshot,
};
pub use video_present_device::{
    NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    NativeVulkanVulkanaliaVideoPresentDeviceProbeOptions,
    NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot,
    NativeVulkanVulkanaliaVideoPresentFeatureSnapshot,
    NativeVulkanVulkanaliaVideoPresentQueueSnapshot,
    NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
    NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot,
    probe_native_vulkan_vulkanalia_video_present_device,
    probe_native_vulkan_vulkanalia_video_present_session,
};
pub use video_present_handoff::NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot;
#[cfg(feature = "native-vulkan-video")]
pub use video_present_runtime::{
    NativeVulkanFfmpegVulkanHwSceneVideoPresentSnapshot,
    NativeVulkanFfmpegVulkanHwSceneVideoPresentSourceSnapshot,
    NativeVulkanFfmpegVulkanHwVideoPresentOptions, NativeVulkanFfmpegVulkanHwVideoPresentSnapshot,
    NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSnapshot,
    NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions,
    run_native_vulkan_ffmpeg_vulkan_hw_video_present,
    run_native_vulkan_vulkanalia_av1_streaming_video_present_decode,
    run_native_vulkan_vulkanalia_h264_streaming_video_present_decode,
    run_native_vulkan_vulkanalia_h265_streaming_video_present_decode,
};
pub use video_present_runtime::{
    NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot,
    NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeSnapshot,
    NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot,
};
#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) use video_present_runtime::{
    NativeVulkanVulkanaliaSceneVideoOverlayInput,
    run_native_vulkan_vulkanalia_av1_streaming_video_present_decode_with_scene_video_overlay,
    run_native_vulkan_vulkanalia_h264_streaming_video_present_decode_with_scene_video_overlay,
    run_native_vulkan_vulkanalia_h265_streaming_video_present_decode_with_scene_video_overlay,
};
#[allow(unused_imports)]
pub use video_profile_probe::{
    NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot,
    NativeVulkanVulkanaliaVideoProfileProbeSnapshot,
};
#[allow(unused_imports)]
pub use video_session::{
    NativeVulkanVulkanaliaMemoryTypeCandidate, NativeVulkanVulkanaliaVideoSessionMemoryBindPlan,
    NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot,
    NativeVulkanVulkanaliaVideoSessionMemoryRequirementSnapshot,
    NativeVulkanVulkanaliaVideoSessionResourceProbePlan,
    NativeVulkanVulkanaliaVideoSessionResourceStep,
    NativeVulkanVulkanaliaVideoSessionResourceStepKind, NativeVulkanVulkanaliaVideoSessionTemplate,
    native_vulkan_vulkanalia_memory_type_candidates,
    native_vulkan_vulkanalia_video_session_memory_bind_plans,
    native_vulkan_vulkanalia_video_session_memory_requirement_snapshots,
    native_vulkan_vulkanalia_video_session_resource_plans_from_format_probe,
    native_vulkan_vulkanalia_video_session_template,
};
#[allow(unused_imports)]
pub use video_session_bind::{
    NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
    NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot,
    probe_native_vulkan_vulkanalia_video_session_bind,
};
#[allow(unused_imports)]
pub use video_session_images::{
    NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot,
    NativeVulkanVulkanaliaVideoSessionResourceImageSnapshot,
};
#[allow(unused_imports)]
pub use video_session_parameters::{
    NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot,
    NativeVulkanVulkanaliaVideoSessionParametersSnapshot,
};
pub use video_surface_host::NativeVulkanVideoSurfaceHostSnapshot;
