//! Typed Vulkan Video capability and device-request contracts.

mod device;
#[cfg(feature = "ffmpeg-vulkan-decode")]
mod ffmpeg;
mod probe;
mod requirements;
#[cfg(feature = "ffmpeg-vulkan-decode")]
mod surface;

pub use device::VideoDecodeDevice;
#[cfg(feature = "ffmpeg-vulkan-decode")]
pub(crate) use ffmpeg::decoded_video_submission_parts;
#[cfg(feature = "ffmpeg-vulkan-decode")]
pub use ffmpeg::{
    DecodedVideoFormat, DecodedVideoFrame, DecodedVideoPlanes, FfmpegTimeBase, FfmpegVideoCodec,
    FfmpegVulkanDecoder,
};
pub(crate) use probe::{query_supported_decode_profiles, query_video_queue_operations};
pub use requirements::{VideoDecodeCodecs, VideoDecodeRequirements};
#[cfg(feature = "ffmpeg-vulkan-decode")]
pub use surface::{
    DecodedVideoSurfaceTerminal, DecodedVideoSurfaceTerminalDescriptor,
    DecodedVideoSurfaceTerminalProgram,
};

/// Codec operations advertised by one Vulkan queue family.
///
/// This deliberately omits profiles and bit depths: those are independently
/// validated through [`VideoDecodeCodecs`] before logical-device creation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VideoDecodeOperations(u8);

impl VideoDecodeOperations {
    pub const H264: Self = Self(1 << 0);
    pub const H265: Self = Self(1 << 1);
    pub const AV1: Self = Self(1 << 2);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub(crate) const fn from_codecs(codecs: VideoDecodeCodecs) -> Self {
        let mut operations = Self::empty();
        if codecs.intersects(VideoDecodeCodecs::H264_HIGH_8) {
            operations = operations.union(Self::H264);
        }
        if codecs.intersects(VideoDecodeCodecs::H265_MAIN_8)
            || codecs.intersects(VideoDecodeCodecs::H265_MAIN_10)
        {
            operations = operations.union(Self::H265);
        }
        if codecs.intersects(VideoDecodeCodecs::AV1_MAIN_8)
            || codecs.intersects(VideoDecodeCodecs::AV1_MAIN_10)
        {
            operations = operations.union(Self::AV1);
        }
        operations
    }

    pub(crate) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[cfg(feature = "ffmpeg-vulkan-decode")]
    pub(crate) const fn to_vk(self) -> vulkanalia::vk::VideoCodecOperationFlagsKHR {
        let mut operations = vulkanalia::vk::VideoCodecOperationFlagsKHR::empty();
        if self.contains(Self::H264) {
            operations = operations.union(vulkanalia::vk::VideoCodecOperationFlagsKHR::DECODE_H264);
        }
        if self.contains(Self::H265) {
            operations = operations.union(vulkanalia::vk::VideoCodecOperationFlagsKHR::DECODE_H265);
        }
        if self.contains(Self::AV1) {
            operations = operations.union(vulkanalia::vk::VideoCodecOperationFlagsKHR::DECODE_AV1);
        }
        operations
    }
}
