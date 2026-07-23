use smithay::backend::allocator::Fourcc;
use thiserror::Error;
use vulkanalia::{Version, loader::LIBRARY, vk};

use super::super::DeviceSelectionError;

#[derive(Debug, Error)]
pub enum RendererError {
    #[error("failed to load Vulkan library {LIBRARY}: {0}")]
    LoadLibrary(String),
    #[error("failed to load the Vulkan entry points: {0}")]
    LoadEntry(String),
    #[error("failed to query the Vulkan loader version: {0:?}")]
    LoaderVersion(vk::ErrorCode),
    #[error("Vulkan {required} is required but the loader exposes {found}")]
    UnsupportedLoaderVersion { required: Version, found: Version },
    #[error("failed to create the Vulkan instance: {0:?}")]
    CreateInstance(vk::ErrorCode),
    #[error("failed to enumerate Vulkan physical devices: {0:?}")]
    EnumerateDevices(vk::ErrorCode),
    #[error("failed to enumerate Vulkan device extensions: {0:?}")]
    EnumerateExtensions(vk::ErrorCode),
    #[error("failed to probe Vulkan dma-buf format {format} modifier {modifier:#x}: {source:?}")]
    ProbeFormat {
        format: Fourcc,
        modifier: u64,
        source: vk::ErrorCode,
    },
    #[error(transparent)]
    Selection(#[from] DeviceSelectionError),
    #[error("failed to create the Vulkan descriptor-heap dma-buf device: {0:?}")]
    CreateDevice(vk::ErrorCode),
    #[error("failed to create Vulkan frame resources: {0:?}")]
    #[cfg(feature = "tty")]
    CreateFrameResources(String),
    #[error("failed to create a native Vulkan output target: {0}")]
    #[cfg(feature = "tty")]
    NativeTarget(String),
    #[error(
        "native output target {format} modifier {modifier:#x} with {plane_count} planes is not exportable by the selected Vulkan device"
    )]
    #[cfg(feature = "tty")]
    UnsupportedOutputTarget {
        format: Fourcc,
        modifier: u64,
        plane_count: u32,
    },
    #[error("failed to query the renderer timeline semaphore: {0:?}")]
    #[cfg(feature = "tty")]
    QueryTimeline(vk::ErrorCode),
    #[error("failed to submit a renderer frame: {0}")]
    #[cfg(feature = "tty")]
    SubmitFrame(String),
    #[error("renderer frame could not be prepared: {0}")]
    #[cfg(feature = "tty")]
    Frame(String),
    #[error("failed to import a client linux dma-buf: {0}")]
    #[cfg(feature = "tty")]
    ClientImport(String),
}
