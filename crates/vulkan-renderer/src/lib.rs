#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_debug_implementations)]

//! A backend-neutral Vulkan 1.4 renderer foundation built directly on
//! [`vulkanalia`].
//!
//! The crate owns Vulkan loader, instance, device, queue, command-pool and
//! timeline-semaphore lifetimes. Higher layers provide render passes and
//! resources without depending on a window system or `wgpu`.

mod backend;
mod capabilities;
mod descriptor_heap;
mod error;
mod frame;
mod present;
mod queue;
mod render_graph;
mod roadmap_2026;
mod standard;

pub use backend::{
    Backend, BackendConfig, DeviceInfo, DevicePreference, DeviceQueues, Queue, SemaphoreWait,
};
pub use capabilities::{
    BackendProfile, CoreFeatures, DescriptorHeapLimits, Features, Limits, ROADMAP_2026_API_VERSION,
    ROADMAP_2026_PROFILE_NAME, ROADMAP_2026_PROFILE_REVISION,
    ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS, ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS,
};
pub use descriptor_heap::{DescriptorAllocation, DescriptorHeapAllocator, DescriptorHeapError};
pub use error::{Error, Result};
pub use frame::{FrameClock, FrameToken, RetirementQueue};
pub use present::{PresentMode, SurfacePresentCapabilities};
pub use queue::{QueueFamilyInfo, QueuePlan};
pub use render_graph::{
    AccessKind, Barrier, CompiledGraph, PassId, RenderGraph, RenderGraphError, RenderPass,
    ResourceId, ResourceKind, ResourceState, ResourceUse,
};
pub use standard::{
    Adapter, Device, DeviceDescriptor, Instance, InstanceDescriptor, PowerPreference,
    RequestAdapterOptions,
};
pub use vulkanalia::{Version, vk};
