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
mod types;
mod upload;
mod video;

pub use allocator::{
    Buffer, Image, ImageView, ImageViewDescriptor, MemoryAllocator, MemoryAllocatorConfig,
};
pub use backend::{
    Backend, BackendConfig, DeviceInfo, DevicePreference, DeviceQueues, NativeDevice, PciAddress,
    Queue, SemaphoreWait,
};
pub use capabilities::{
    BackendProfile, CoreFeatures, DescriptorHeapLimits, DeviceProperties, Features, Limits,
    PipelineBinaryProperties, ROADMAP_2026_API_VERSION, ROADMAP_2026_PROFILE_NAME,
    ROADMAP_2026_PROFILE_REVISION, ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS,
    ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS, STANDARD_REQUIRED_INSTANCE_EXTENSIONS,
};
pub use command::{
    AttachmentView, BufferCopy, BufferImageCopy, BufferState, ColorAttachment,
    ColorBufferImageCopy, ColorImageCopy, CommandBuffer, CommandEncoder, CommandEncoderDescriptor,
    ComputeEncoder, ComputePassDescriptor, DepthAttachment, ImageBlit, ImageBlitFilter, ImageCopy,
    IndexFormat, LoadOp, RenderingDescriptor, RenderingEncoder, RenderingLocalReadMapping,
    RenderingLocalReadMappingDescriptor, RenderingLocalReadMappingKind, ResolveMode,
    StencilAttachment, StoreOp, TextureState,
};
pub use descriptor_heap::{
    BufferDescriptorBinding, BufferDescriptorKind, DescriptorAllocation, DescriptorHeap,
    DescriptorHeapAllocator, DescriptorHeapDescriptor, DescriptorHeapError, DescriptorHeapKind,
    DescriptorHeapMemory, DescriptorHeapUploadBatch, DescriptorHeapUploadRange, DescriptorSlotKind,
    DynamicExternalImageDescriptorBinding, HeapDescriptorType, ImageDescriptorBinding,
    ImageDescriptorKind, ReservedDescriptorBinding, SampledImageBinding, SampledImageDescriptor,
    SampledImageDescriptorWriteBatch, SampledTextureBinding, SampledTextureHeapIndices,
    SampledTextureHeapOffsets, SampledTextureShaderBindings, SamplerAddressMode, SamplerBinding,
    SamplerBorderColor, SamplerCompareFunction, SamplerDescriptor, SamplerFilterMode,
    descriptor_heap_element_index,
};
pub use dynamic_buffer::{DynamicBuffer, DynamicBufferDescriptor, DynamicBufferUpload};
pub use error::{Error, Result, VulkanFailure};
pub use external_image::{
    ExternalImageViewDescriptor, RetainedExternalImage, RetainedExternalImageView,
};
pub use external_memory::{
    DmaBufExportDescriptor, DmaBufExportPlane, DmaBufImageDescriptor, DmaBufPlaneLayout,
    DrmDeviceIdentity, DrmFormatModifierCapability, DrmNodeIdentity, ExportedDmaBufImage,
    ImportedDmaBufImage, LinuxDmaBufCapabilities,
};
pub use frame::{FrameClock, FrameToken, RetirementQueue, SubmissionLease, SubmissionResource};
pub use memory::{
    AllocationRequirements, BufferDescriptor, ImageDescriptor, MemoryLocation, MemoryPlan,
    MemoryPlanError, MemoryTypeInfo, MemoryTypeSelector, ResourceDescriptorError,
};
pub use pipeline::{
    AdvancedBlendState, BlendComponent, BlendFactor, BlendOperation, BlendOverlap, BlendState,
    ColorTargetState, ColorWrites, ComputePipeline, ComputePipelineDescriptor,
    ConstantOffsetMapping, CullMode, DepthBiasState, DepthState, DepthStencilState, FragmentState,
    FrontFace, GraphicsPipeline, GraphicsPipelineDescriptor, IndirectIndexMapping,
    MachineCodeComputePipeline, MachineCodeComputePipelineDescriptor, MachineCodeGraphicsPipeline,
    MachineCodeGraphicsPipelineDescriptor, MachineCodePipeline, MultisampleState,
    PipelineBinaryArchive, PipelineBinaryArchiveCache, PipelineBinaryBlob,
    PipelineBinaryCacheIdentity, PipelineBinaryCreation, PipelineCache, PipelineCacheDescriptor,
    PolygonMode, PrimitiveState, PrimitiveTopology, ProgrammableStage, PushIndexMapping,
    ShaderBindingMap, ShaderBindingMapError, ShaderBindingMapping, ShaderBindingSource,
    StencilState, VertexAttribute, VertexBufferLayout, VertexFormat, VertexState, VertexStepMode,
    compute_pipeline_binary_key, create_compute_pipeline_machine_code,
    create_graphics_pipeline_machine_code, graphics_pipeline_binary_key,
};
pub use present::{
    AcquiredSurfaceTexture, DirectSurfaceBlocker, FrameTargetPreference,
    FullscreenSampledSurfaceTerminal, FullscreenSampledSurfaceTerminalDescriptor,
    FullscreenSampledSurfaceTerminalProgram, OffscreenColorTarget, OffscreenColorTargets,
    OffscreenColorTargetsDescriptor, OffscreenSampledBindings, OffscreenSamplerTopology,
    PresentMode, PresentStatus, PresentTransactionOutcome, PresentationAdapterRequest,
    PresentationBootstrap, PresentationBootstrapDescriptor, PresentationExtentPolicy,
    PresentationImageCount, PresentationPathDescriptor, PresentationPathPlan,
    PresentationRequirements, PresentationSurfaceConfigurationDescriptor, PresentationTarget,
    PresentationTransaction, PresentationTransactionDescriptor, PresentationTransactionPhase,
    PresentationTransactionSchedule, PresentationTransactionStep, Surface, SurfaceAcquireStrategy,
    SurfaceCapabilities, SurfaceConfiguration, SurfaceConfigurationRequest,
    SurfacePresentCapabilities, Swapchain, SwapchainDescriptor, TerminalAlphaMode,
    TerminalCompositeDescriptor, TerminalSampling,
};
#[cfg(feature = "ffmpeg-vulkan-decode")]
pub use present::{PresentationDependencyScope, PresentationFrameDependencies};
pub use queue::{QueueFamilyInfo, QueuePlan};
pub use render_graph::{
    AccessKind, Barrier, CompiledGraph, ForeignImageState, PassId, RenderGraph, RenderGraphError,
    RenderGraphImageState, RenderPass, ResourceId, ResourceKind, ResourceState, ResourceUse,
};
pub use shader::{
    ShaderModule, ShaderModuleDescriptor, ShaderModuleError, SpirvValidationError, validate_spirv,
};
pub use standard::{
    Adapter, AdapterSelector, Device, DeviceDescriptor, Instance, InstanceDescriptor,
    PowerPreference, RequestAdapterOptions,
};
pub use sync::{
    BarrierBatch, BinarySemaphore, BinarySemaphoreDescriptor, ExternalTimelineSemaphoreDescriptor,
    RenderGraphSyncError, ResourceBinding, RetainedExternalTimelineSemaphore,
};
pub use types::{
    ApiVersion, BufferUsages, ColorSpace, ComponentMapping, ComponentSwizzle, CompositeAlphaMode,
    CompositeAlphaModes, DeviceType, Extent2D, Extent3D, ImageDimension, ImageTiling,
    ImageViewDimension, Origin2D, Origin3D, PipelineStages, Rect2D, SampleCount, SampleCounts,
    SurfaceFormat, SurfaceTransform, SurfaceTransforms, TextureAspects, TextureFormat,
    TextureFormatFeatures, TextureLayout, TextureSubresourceLayers, TextureSubresourceRange,
    TextureUsages, Viewport,
};
pub use upload::{
    ImageDataLayout, ImageUpload, TexelBlockLayout, UploadBatch, UploadBelt, UploadBeltDescriptor,
    UploadBeltStats, UploadSlice,
};
#[cfg(feature = "ffmpeg-vulkan-decode")]
pub use video::{
    DecodedVideoFormat, DecodedVideoFrame, DecodedVideoPlanes, DecodedVideoSurfaceTerminal,
    DecodedVideoSurfaceTerminalDescriptor, DecodedVideoSurfaceTerminalProgram, FfmpegTimeBase,
    FfmpegVideoCodec, FfmpegVulkanDecoder,
};
pub use video::{
    VideoDecodeCodecs, VideoDecodeDevice, VideoDecodeOperations, VideoDecodeRequirements,
};
/// Includes a little-endian, four-byte-aligned SPIR-V asset as `&[u32]`.
///
/// This is the standard shader-asset inclusion path for consumers. It keeps
/// Vulkanalia's alignment and byte-length validation available without making
/// applications depend on Vulkanalia directly.
pub use vulkanalia::include_shader_code as include_spirv;
