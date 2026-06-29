//! Vulkanalia-backed native Vulkan implementation.
//!
//! Vulkanalia is the only Vulkan binding used by the native renderer. This
//! facade keeps the public native-vulkan API stable while implementation files
//! are grouped by core device setup, present, scene, and video responsibilities.

mod core;
mod present;
mod scene;
mod video;

use self::core::descriptor_heap;
use self::core::device_probe;
use self::core::features;
use self::core::plan;
use self::core::profiles;
use self::present::clear as present_clear;
use self::present::render as render_present;
use self::present::render_descriptors as render_present_descriptors;
use self::present::swapchain;
use self::scene::draw_pass as scene_draw_pass;
use self::scene::present as scene_present;
use self::scene::sampled_image as scene_sampled_image;
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

pub use descriptor_heap::NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot;
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
pub(crate) use scene_draw_pass::{
    NativeVulkanVulkanaliaSceneDrawPassInput, native_vulkan_vulkanalia_scene_draw_pass_snapshot,
};
pub use scene_draw_pass::{
    NativeVulkanVulkanaliaSceneDrawPassSnapshot,
    NativeVulkanVulkanaliaSceneSampledImageCommandSnapshot,
    NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot,
    NativeVulkanVulkanaliaSceneSolidQuadCommandSnapshot,
    NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot,
};
pub(in crate::renderer::native_vulkan) use scene_present::NativeVulkanVulkanaliaSceneVideoOverlayInput;
pub(in crate::renderer::native_vulkan) use scene_present::native_vulkan_vulkanalia_take_scene_sampled_image_vertex_vec;
pub use scene_present::{
    NativeVulkanVulkanaliaSceneMixedSolidQuadDynamicGeometry,
    NativeVulkanVulkanaliaSceneSampledImageDrawStep,
    NativeVulkanVulkanaliaSceneSampledImageDynamicGeometry,
    NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
    NativeVulkanVulkanaliaSceneSampledImageGeometrySnapshot,
    NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
    NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot,
    NativeVulkanVulkanaliaSceneSampledImageVertex, NativeVulkanVulkanaliaSceneSolidQuadDrawStep,
    NativeVulkanVulkanaliaSceneSolidQuadDynamicGeometry,
    NativeVulkanVulkanaliaSceneSolidQuadGeometryInput,
    NativeVulkanVulkanaliaSceneSolidQuadGeometrySnapshot,
    NativeVulkanVulkanaliaSceneSolidQuadPresentOptions,
    NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot,
    NativeVulkanVulkanaliaSceneSolidQuadVertex, NativeVulkanVulkanaliaSceneVideoLayerDrawStep,
    NativeVulkanVulkanaliaSceneVideoLayerGeometryInput,
    run_native_vulkan_vulkanalia_scene_sampled_image_present,
    run_native_vulkan_vulkanalia_scene_solid_quad_present,
};
pub use scene_sampled_image::{
    NativeVulkanVulkanaliaSceneSampledImageDescriptorStrategySnapshot,
    NativeVulkanVulkanaliaSceneSampledImagePlanSnapshot,
};
pub(crate) use scene_sampled_image::{
    NativeVulkanVulkanaliaSceneSampledImagePlanInput,
    native_vulkan_vulkanalia_configure_scene_sampled_image_allocator,
    native_vulkan_vulkanalia_scene_sampled_image_plan,
    native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap,
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
pub use video_present_runtime::{
    NativeVulkanVulkanaliaAv1RetainedVideoPresentDecodeSnapshot,
    NativeVulkanVulkanaliaH264RetainedVideoPresentDecodeSnapshot,
    NativeVulkanVulkanaliaH265RetainedVideoPresentDecodeSnapshot,
};
#[cfg(feature = "native-vulkan-video")]
pub use video_present_runtime::{
    NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSnapshot,
    NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions,
    run_native_vulkan_vulkanalia_av1_streaming_video_present_decode,
    run_native_vulkan_vulkanalia_h264_streaming_video_present_decode,
    run_native_vulkan_vulkanalia_h265_streaming_video_present_decode,
};
#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) use video_present_runtime::{
    run_native_vulkan_vulkanalia_av1_streaming_video_present_decode_with_scene_video_overlay,
    run_native_vulkan_vulkanalia_h264_streaming_video_present_decode_with_scene_video_overlay,
    run_native_vulkan_vulkanalia_h265_streaming_video_present_decode_with_scene_video_overlay,
    run_native_vulkan_vulkanalia_multi_streaming_video_present_decode_with_scene_video_overlay,
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
