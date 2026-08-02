use std::fmt;
use std::sync::Arc;

use crate::backend::DeviceOwner;
use vulkanalia::vk;

use super::{VideoDecodeOperations, VideoDecodeRequirements};

/// Opaque renderer-owned Vulkan Video decode endpoint.
///
/// It retains the logical device and selected decode queue without exposing
/// their Vulkan handles. Decoder integrations are constructed from this owner,
/// not from application-provided instance/device/queue handles.
#[derive(Clone)]
pub struct VideoDecodeDevice {
    #[cfg(feature = "ffmpeg-vulkan-decode")]
    pub(super) owner: Arc<DeviceOwner>,
    #[cfg(not(feature = "ffmpeg-vulkan-decode"))]
    _owner: Arc<DeviceOwner>,
    requirements: VideoDecodeRequirements,
    #[cfg(feature = "ffmpeg-vulkan-decode")]
    pub(super) queue_family: u32,
    #[cfg(not(feature = "ffmpeg-vulkan-decode"))]
    _queue_family: u32,
    #[cfg(feature = "ffmpeg-vulkan-decode")]
    pub(super) queue_flags: vk::QueueFlags,
    #[cfg(not(feature = "ffmpeg-vulkan-decode"))]
    _queue_flags: vk::QueueFlags,
    #[cfg(feature = "ffmpeg-vulkan-decode")]
    pub(super) operations: VideoDecodeOperations,
    #[cfg(not(feature = "ffmpeg-vulkan-decode"))]
    _operations: VideoDecodeOperations,
}

impl VideoDecodeDevice {
    pub(crate) fn new(
        owner: Arc<DeviceOwner>,
        requirements: VideoDecodeRequirements,
        queue_family: u32,
        queue_flags: vk::QueueFlags,
        operations: VideoDecodeOperations,
    ) -> Self {
        Self {
            #[cfg(feature = "ffmpeg-vulkan-decode")]
            owner,
            #[cfg(not(feature = "ffmpeg-vulkan-decode"))]
            _owner: owner,
            requirements,
            #[cfg(feature = "ffmpeg-vulkan-decode")]
            queue_family,
            #[cfg(not(feature = "ffmpeg-vulkan-decode"))]
            _queue_family: queue_family,
            #[cfg(feature = "ffmpeg-vulkan-decode")]
            queue_flags,
            #[cfg(not(feature = "ffmpeg-vulkan-decode"))]
            _queue_flags: queue_flags,
            #[cfg(feature = "ffmpeg-vulkan-decode")]
            operations,
            #[cfg(not(feature = "ffmpeg-vulkan-decode"))]
            _operations: operations,
        }
    }

    pub const fn requirements(&self) -> VideoDecodeRequirements {
        self.requirements
    }
}

impl fmt::Debug for VideoDecodeDevice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VideoDecodeDevice")
            .field("requirements", &self.requirements)
            .finish_non_exhaustive()
    }
}
