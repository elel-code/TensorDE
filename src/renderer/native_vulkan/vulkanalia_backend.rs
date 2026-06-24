//! Early vulkanalia backend spike boundary.
//!
//! Keep this module as a facade. Backend implementation pieces live under
//! `native_vulkan/vulkanalia_backend/` so the ash replacement does not grow a
//! second monolithic Vulkan file.

#[path = "vulkanalia_backend/device_probe.rs"]
mod device_probe;
#[path = "vulkanalia_backend/features.rs"]
mod features;
#[path = "vulkanalia_backend/migration.rs"]
mod migration;
#[path = "vulkanalia_backend/plan.rs"]
mod plan;
#[path = "vulkanalia_backend/profiles.rs"]
mod profiles;
#[path = "vulkanalia_backend/queue_probe.rs"]
mod queue_probe;
#[path = "vulkanalia_backend/video_format_probe.rs"]
mod video_format_probe;
#[path = "vulkanalia_backend/video_profile_gate.rs"]
mod video_profile_gate;
#[path = "vulkanalia_backend/video_profile_labels.rs"]
mod video_profile_labels;
#[path = "vulkanalia_backend/video_profile_probe.rs"]
mod video_profile_probe;
#[path = "vulkanalia_backend/video_session.rs"]
mod video_session;

pub use device_probe::{
    NativeVulkanVulkanaliaDeviceProbeSnapshot, NativeVulkanVulkanaliaDeviceProbeTemplate,
    native_vulkan_vulkanalia_device_probe_template, probe_native_vulkan_vulkanalia_devices,
};
pub use features::{
    NativeVulkanVulkanaliaFeatureChainTemplate, native_vulkan_vulkanalia_feature_chain_template,
};
#[allow(unused_imports)]
pub use migration::{
    NativeVulkanVulkanaliaMigrationContract, NativeVulkanVulkanaliaMigrationStage,
    NativeVulkanVulkanaliaMigrationStageKind, native_vulkan_vulkanalia_migration_contract,
};
pub use plan::{NativeVulkanVulkanaliaBackendPlan, native_vulkan_vulkanalia_backend_plan};
pub use profiles::{
    NativeVulkanVulkanaliaVideoProfileTemplate, native_vulkan_vulkanalia_video_profile_templates,
};
#[allow(unused_imports)]
pub use video_format_probe::{
    NativeVulkanVulkanaliaVideoFormatProbeSnapshot,
    NativeVulkanVulkanaliaVideoFormatPropertySnapshot,
    NativeVulkanVulkanaliaVideoFormatQuerySnapshot,
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
