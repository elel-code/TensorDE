//! Early vulkanalia backend spike boundary.
//!
//! Keep this module as a facade. Backend implementation pieces live under
//! `native_vulkan/vulkanalia_backend/` so the ash replacement does not grow a
//! second monolithic Vulkan file.

#[path = "vulkanalia_backend/features.rs"]
mod features;
#[path = "vulkanalia_backend/plan.rs"]
mod plan;
#[path = "vulkanalia_backend/profiles.rs"]
mod profiles;

pub use features::{
    NativeVulkanVulkanaliaFeatureChainTemplate, native_vulkan_vulkanalia_feature_chain_template,
};
pub use plan::{NativeVulkanVulkanaliaBackendPlan, native_vulkan_vulkanalia_backend_plan};
pub use profiles::{
    NativeVulkanVulkanaliaVideoProfileTemplate, native_vulkan_vulkanalia_video_profile_templates,
};
