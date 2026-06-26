//! Early vulkanalia backend spike boundary.
//!
//! Keep this module as a facade. Backend implementation pieces live under
//! `native_vulkan/vulkanalia_backend/` so the Vulkanalia main path does not
//! grow a second monolithic Vulkan file.

#[path = "vulkanalia_backend/descriptor_heap.rs"]
mod descriptor_heap;
#[path = "vulkanalia_backend/device_probe.rs"]
mod device_probe;
#[path = "vulkanalia_backend/features.rs"]
mod features;
#[path = "vulkanalia_backend/instance.rs"]
mod instance;
#[path = "vulkanalia_backend/migration.rs"]
mod migration;
#[path = "vulkanalia_backend/plan.rs"]
mod plan;
#[path = "vulkanalia_backend/present_clear.rs"]
mod present_clear;
#[path = "vulkanalia_backend/present_timing.rs"]
mod present_timing;
#[path = "vulkanalia_backend/profiles.rs"]
mod profiles;
#[path = "vulkanalia_backend/queue_probe.rs"]
mod queue_probe;
#[path = "vulkanalia_backend/render_present.rs"]
mod render_present;
#[path = "vulkanalia_backend/render_present_descriptors.rs"]
mod render_present_descriptors;
#[path = "vulkanalia_backend/scene_lite_draw_pass.rs"]
mod scene_lite_draw_pass;
#[path = "vulkanalia_backend/scene_lite_present.rs"]
mod scene_lite_present;
#[path = "vulkanalia_backend/scene_lite_sampled_image.rs"]
mod scene_lite_sampled_image;
#[path = "vulkanalia_backend/swapchain.rs"]
mod swapchain;
#[path = "vulkanalia_backend/video_bitstream_buffer.rs"]
mod video_bitstream_buffer;
#[path = "vulkanalia_backend/video_codec.rs"]
mod video_codec;
#[path = "vulkanalia_backend/video_command_pool.rs"]
mod video_command_pool;
#[path = "vulkanalia_backend/video_decode_commands.rs"]
mod video_decode_commands;
#[path = "vulkanalia_backend/video_decode_submit.rs"]
mod video_decode_submit;
#[path = "vulkanalia_backend/video_decode_submit_av1.rs"]
mod video_decode_submit_av1;
#[path = "vulkanalia_backend/video_decode_submit_h264.rs"]
mod video_decode_submit_h264;
#[path = "vulkanalia_backend/video_decode_submit_h265.rs"]
mod video_decode_submit_h265;
#[path = "vulkanalia_backend/video_device.rs"]
mod video_device;
#[path = "vulkanalia_backend/video_direct_runtime.rs"]
mod video_direct_runtime;
#[path = "vulkanalia_backend/video_format_probe.rs"]
mod video_format_probe;
#[path = "vulkanalia_backend/video_present_device.rs"]
mod video_present_device;
#[path = "vulkanalia_backend/video_present_handoff.rs"]
mod video_present_handoff;
#[path = "vulkanalia_backend/video_present_runtime.rs"]
mod video_present_runtime;
#[path = "vulkanalia_backend/video_profile_gate.rs"]
mod video_profile_gate;
#[path = "vulkanalia_backend/video_profile_info.rs"]
mod video_profile_info;
#[path = "vulkanalia_backend/video_profile_labels.rs"]
mod video_profile_labels;
#[path = "vulkanalia_backend/video_profile_probe.rs"]
mod video_profile_probe;
#[path = "vulkanalia_backend/video_session.rs"]
mod video_session;
#[path = "vulkanalia_backend/video_session_bind.rs"]
mod video_session_bind;
#[path = "vulkanalia_backend/video_session_capabilities.rs"]
mod video_session_capabilities;
#[path = "vulkanalia_backend/video_session_images.rs"]
mod video_session_images;
#[path = "vulkanalia_backend/video_session_parameters.rs"]
mod video_session_parameters;
#[path = "vulkanalia_backend/video_session_parameters_av1.rs"]
mod video_session_parameters_av1;
#[path = "vulkanalia_backend/video_session_parameters_h264.rs"]
mod video_session_parameters_h264;
#[path = "vulkanalia_backend/video_session_parameters_h265.rs"]
mod video_session_parameters_h265;

#[cfg(test)]
pub(super) fn native_vulkan_vulkanalia_h265_std_long_term_ref_pics_sps(
    ref_pics: &[super::NativeVulkanH265LongTermRefPicSpsSnapshot],
) -> Result<Option<vulkanalia::vk::video::StdVideoH265LongTermRefPicsSps>, String> {
    video_session_parameters_h265::native_vulkan_vulkanalia_h265_std_long_term_ref_pics_sps(
        ref_pics,
    )
}

#[cfg(test)]
pub(super) fn native_vulkan_vulkanalia_h265_std_short_term_ref_pic_set(
    ref_pic_set: &super::NativeVulkanH265ShortTermRefPicSetSnapshot,
) -> Result<vulkanalia::vk::video::StdVideoH265ShortTermRefPicSet, String> {
    video_session_parameters_h265::native_vulkan_vulkanalia_h265_std_short_term_ref_pic_set(
        ref_pic_set,
    )
}

pub use descriptor_heap::NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot;
pub use device_probe::{
    NativeVulkanVulkanaliaDeviceProbeSnapshot, NativeVulkanVulkanaliaDeviceProbeTemplate,
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
pub use migration::{
    NativeVulkanVulkanaliaMigrationContract, NativeVulkanVulkanaliaMigrationStage,
    NativeVulkanVulkanaliaMigrationStageKind, native_vulkan_vulkanalia_migration_contract,
};
pub use plan::{NativeVulkanVulkanaliaBackendPlan, native_vulkan_vulkanalia_backend_plan};
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
pub(crate) use scene_lite_draw_pass::{
    NativeVulkanVulkanaliaSceneLiteDrawPassInput,
    native_vulkan_vulkanalia_scene_lite_draw_pass_snapshot,
};
pub use scene_lite_draw_pass::{
    NativeVulkanVulkanaliaSceneLiteDrawPassSnapshot,
    NativeVulkanVulkanaliaSceneLiteSampledImageCommandSnapshot,
    NativeVulkanVulkanaliaSceneLiteSampledImagePipelineSnapshot,
    NativeVulkanVulkanaliaSceneLiteSolidQuadCommandSnapshot,
    NativeVulkanVulkanaliaSceneLiteSolidQuadPipelineSnapshot,
};
pub use scene_lite_present::{
    NativeVulkanVulkanaliaSceneLiteSampledImageDrawStep,
    NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput,
    NativeVulkanVulkanaliaSceneLiteSampledImageGeometrySnapshot,
    NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions,
    NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot,
    NativeVulkanVulkanaliaSceneLiteSampledImageVertex,
    NativeVulkanVulkanaliaSceneLiteSolidQuadDrawStep,
    NativeVulkanVulkanaliaSceneLiteSolidQuadGeometryInput,
    NativeVulkanVulkanaliaSceneLiteSolidQuadGeometrySnapshot,
    NativeVulkanVulkanaliaSceneLiteSolidQuadPresentOptions,
    NativeVulkanVulkanaliaSceneLiteSolidQuadPresentSnapshot,
    NativeVulkanVulkanaliaSceneLiteSolidQuadVertex,
    run_native_vulkan_vulkanalia_scene_lite_sampled_image_present,
    run_native_vulkan_vulkanalia_scene_lite_solid_quad_present,
};
pub use scene_lite_sampled_image::{
    NativeVulkanVulkanaliaSceneLiteSampledImageDescriptorStrategySnapshot,
    NativeVulkanVulkanaliaSceneLiteSampledImagePlanSnapshot,
};
pub(crate) use scene_lite_sampled_image::{
    NativeVulkanVulkanaliaSceneLiteSampledImagePlanInput,
    native_vulkan_vulkanalia_scene_lite_sampled_image_plan,
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
    NativeVulkanVulkanaliaAv1CommandSmokeSnapshot, NativeVulkanVulkanaliaAv1DecodeFrameBatchInput,
    NativeVulkanVulkanaliaAv1DecodeFrameInput, NativeVulkanVulkanaliaAv1FrameSubmitInput,
    NativeVulkanVulkanaliaAv1GlobalMotionPlan, NativeVulkanVulkanaliaAv1LoopFilterPlan,
    NativeVulkanVulkanaliaAv1LoopRestorationPlan, NativeVulkanVulkanaliaAv1QuantizationPlan,
    NativeVulkanVulkanaliaAv1ReferenceInfoPlan, NativeVulkanVulkanaliaAv1SegmentationPlan,
    NativeVulkanVulkanaliaAv1TileInfoPlan,
};
pub use video_decode_submit_h264::{
    NativeVulkanVulkanaliaH264ReadyPrefixCommandFrameSnapshot,
    NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput,
    NativeVulkanVulkanaliaH264ReadyPrefixFrameInput,
};
pub use video_decode_submit_h265::{
    NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput,
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
#[cfg(feature = "native-vulkan-gst-video")]
pub use video_present_runtime::{
    NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions,
    NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions,
    run_native_vulkan_vulkanalia_av1_streaming_video_present_decode,
    run_native_vulkan_vulkanalia_h264_streaming_video_present_decode,
    run_native_vulkan_vulkanalia_h265_streaming_video_present_decode,
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
