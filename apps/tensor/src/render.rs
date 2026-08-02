#[cfg(feature = "tty")]
mod completion;
#[cfg(feature = "tty")]
mod cursor;
mod device;
#[cfg(feature = "tty")]
mod dmabuf;
mod format;
#[cfg(feature = "tty")]
mod frame;
mod interop;
mod target;
mod vulkan;

#[cfg(feature = "tty")]
pub(crate) use completion::{
    GpuFenceEvent, GpuFenceRuntime, GpuFenceRuntimeError, GpuFenceSubmitter, MAX_PENDING_GPU_FENCES,
};
#[cfg(feature = "tty")]
pub(crate) use cursor::{CursorOverlay, CursorOverlays, CursorTexture};
pub use device::{
    DescriptorHeapProperties, DeviceCandidate, DeviceSelectionError, DrmDeviceIdentity,
    DrmNodeError, DrmNodeId, GpuPreference, ParseGpuPreferenceError,
};
#[cfg(feature = "tty")]
pub(crate) use dmabuf::{Dmabuf, DmabufPlane, ExportedDmabuf};
#[cfg(not(feature = "tty"))]
pub(crate) use format::VulkanFormatCapability;
#[cfg(feature = "tty")]
pub(crate) use format::{
    GbmFormatCapability, OutputFormat, VulkanFormatCapability, negotiate_output_formats,
};
#[cfg(feature = "tty")]
pub(crate) use frame::{
    FrameScheduler, FrameSubmission, NativeCursorTarget, NativeOutputTarget, RenderOutputId,
};
pub use interop::NativeInteropCapabilities;
pub use target::RendererTarget;
#[cfg(feature = "tty")]
pub(crate) use vulkan::ClientReleaseFence;
#[cfg(feature = "tty")]
pub(crate) use vulkan::{NativeCursorBuffer, NativeOutputBuffer, NativeOutputBuffers};
pub(crate) use vulkan::{RendererError, VulkanRenderer};
