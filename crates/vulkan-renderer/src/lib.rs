#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_debug_implementations)]

//! A backend-neutral Vulkan 1.4 renderer foundation built directly on
//! [`vulkanalia`].
//!
//! The crate owns Vulkan loader, instance, device, queue, command-pool and
//! timeline-semaphore lifetimes. Higher layers provide render passes and
//! resources without depending on a window system or `wgpu`.

mod allocator;
mod backend;
mod capabilities;
mod command;
mod descriptor_heap;
mod dynamic_buffer;
mod error;
mod external_image;
mod external_memory;
mod frame;
mod memory;
mod pipeline;
mod present;
mod queue;
mod render_graph;
mod roadmap_2026;
mod shader;
mod standard;
mod sync;
mod upload;

pub use allocator::{
    Buffer, Image, ImageView, ImageViewDescriptor, MemoryAllocator, MemoryAllocatorConfig,
};
pub use backend::{
    Backend, BackendConfig, DeviceInfo, DevicePreference, DeviceQueues, Queue, SemaphoreWait,
};
pub use capabilities::{
    BackendProfile, CoreFeatures, DescriptorHeapLimits, Features, Limits, ROADMAP_2026_API_VERSION,
    ROADMAP_2026_PROFILE_NAME, ROADMAP_2026_PROFILE_REVISION,
    ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS, ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS,
    STANDARD_REQUIRED_INSTANCE_EXTENSIONS,
};
pub use command::{
    AttachmentView, BufferCopy, BufferImageCopy, ColorAttachment, CommandBuffer, CommandEncoder,
    CommandEncoderDescriptor, ComputeEncoder, ComputePassDescriptor, DepthAttachment, ImageBlit,
    ImageBlitFilter, ImageCopy, IndexFormat, LoadOp, RenderingDescriptor, RenderingEncoder,
    StencilAttachment, StoreOp,
};
pub use descriptor_heap::{
    DescriptorAllocation, DescriptorHeap, DescriptorHeapAllocator, DescriptorHeapDescriptor,
    DescriptorHeapError, DescriptorHeapKind, HeapDescriptorType, SampledImageBinding,
    SampledTextureBinding, SampledTextureHeapOffsets, SampledTextureShaderBindings,
    SamplerAddressMode, SamplerBinding, SamplerBorderColor, SamplerCompareFunction,
    SamplerDescriptor, SamplerFilterMode,
};
pub use dynamic_buffer::{DynamicBuffer, DynamicBufferDescriptor, DynamicBufferUpload};
pub use error::{Error, Result};
pub use external_image::{
    ExternalImageViewDescriptor, RetainedExternalImage, RetainedExternalImageView,
};
pub use external_memory::{
    DmaBufExportDescriptor, DmaBufExportPlane, DmaBufImageDescriptor, DmaBufPlaneLayout,
    DrmFormatModifierCapability, ExportedDmaBufImage, ImportedDmaBufImage,
};
pub use frame::{FrameClock, FrameToken, RetirementQueue, SubmissionLease, SubmissionResource};
pub use memory::{
    AllocationRequirements, BufferDescriptor, ImageDescriptor, MemoryLocation, MemoryPlan,
    MemoryPlanError, MemoryTypeInfo, MemoryTypeSelector, ResourceDescriptorError,
};
pub use pipeline::{
    BlendComponent, BlendState, ColorTargetState, ComputePipeline, ComputePipelineDescriptor,
    ConstantOffsetMapping, DepthBiasState, DepthState, DepthStencilState, FragmentState,
    GraphicsPipeline, GraphicsPipelineDescriptor, IndirectIndexMapping, MultisampleState,
    PipelineCache, PipelineCacheDescriptor, PrimitiveState, ProgrammableStage, PushIndexMapping,
    ShaderBindingMap, ShaderBindingMapError, ShaderBindingMapping, ShaderBindingSource,
    StencilState, VertexAttribute, VertexBufferLayout, VertexState, VertexStepMode,
};
pub use present::{
    AcquiredSurfaceTexture, PresentMode, PresentStatus, Surface, SurfaceCapabilities,
    SurfaceConfiguration, SurfaceConfigurationRequest, SurfacePresentCapabilities, Swapchain,
    SwapchainDescriptor,
};
pub use queue::{QueueFamilyInfo, QueuePlan};
pub use render_graph::{
    AccessKind, Barrier, CompiledGraph, PassId, RenderGraph, RenderGraphError, RenderPass,
    ResourceId, ResourceKind, ResourceState, ResourceUse,
};
pub use shader::{ShaderModule, ShaderModuleDescriptor, ShaderModuleError, SpirvValidationError};
pub use standard::{
    Adapter, Device, DeviceDescriptor, Instance, InstanceDescriptor, PowerPreference,
    RequestAdapterOptions,
};
pub use sync::{
    BarrierBatch, BinarySemaphore, BinarySemaphoreDescriptor, ExternalTimelineSemaphoreDescriptor,
    RenderGraphSyncError, ResourceBinding, RetainedExternalTimelineSemaphore,
};
pub use upload::{
    ImageDataLayout, ImageUpload, TexelBlockLayout, UploadBatch, UploadBelt, UploadBeltDescriptor,
    UploadBeltStats, UploadSlice,
};
/// Includes a little-endian, four-byte-aligned SPIR-V asset as `&[u32]`.
///
/// This is the standard shader-asset inclusion path for consumers. It keeps
/// Vulkanalia's alignment and byte-length validation available without making
/// applications depend on Vulkanalia directly.
pub use vulkanalia::include_shader_code as include_spirv;
pub use vulkanalia::{Version, vk};
