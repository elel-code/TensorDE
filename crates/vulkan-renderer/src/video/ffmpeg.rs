//! Renderer-owned FFmpeg Vulkan decode boundary.
//!
//! Applications submit a media path and exact codec profile. FFmpeg and raw
//! Vulkan objects remain private; decoded output crosses this boundary only as
//! retained typed plane images plus renderer-owned synchronization metadata.

mod decoder;
mod device;
mod ffi;
mod frame;
mod resources;

pub use decoder::FfmpegVulkanDecoder;
pub(crate) use frame::decoded_video_submission_parts;
pub use frame::{DecodedVideoFormat, DecodedVideoFrame, DecodedVideoPlanes};

use crate::video::VideoDecodeCodecs;

/// Exact codec profile selected for an FFmpeg Vulkan decoder.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FfmpegVideoCodec {
    H264High8,
    H265Main8,
    H265Main10,
    Av1Main8,
    Av1Main10,
}

impl FfmpegVideoCodec {
    pub(super) const fn requirement(self) -> VideoDecodeCodecs {
        match self {
            Self::H264High8 => VideoDecodeCodecs::H264_HIGH_8,
            Self::H265Main8 => VideoDecodeCodecs::H265_MAIN_8,
            Self::H265Main10 => VideoDecodeCodecs::H265_MAIN_10,
            Self::Av1Main8 => VideoDecodeCodecs::AV1_MAIN_8,
            Self::Av1Main10 => VideoDecodeCodecs::AV1_MAIN_10,
        }
    }
}

/// Stream time base used to interpret raw frame timestamps.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FfmpegTimeBase {
    numerator: i32,
    denominator: i32,
}

impl FfmpegTimeBase {
    pub(super) fn new(numerator: i32, denominator: i32) -> crate::Result<Self> {
        if numerator <= 0 || denominator <= 0 {
            return Err(crate::Error::VideoDecode(format!(
                "invalid FFmpeg stream time base {numerator}/{denominator}"
            )));
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub const fn numerator(self) -> i32 {
        self.numerator
    }

    pub const fn denominator(self) -> i32 {
        self.denominator
    }

    pub fn timestamp_ns(self, value: Option<i64>) -> Option<u64> {
        let value = u128::try_from(value?).ok()?;
        let numerator = u128::try_from(self.numerator).ok()?;
        let denominator = u128::try_from(self.denominator).ok()?;
        let nanos = value
            .saturating_mul(numerator)
            .saturating_mul(1_000_000_000)
            / denominator;
        Some(nanos.min(u128::from(u64::MAX)) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_base_converts_non_negative_timestamps_without_float_rounding() {
        let time_base = FfmpegTimeBase::new(1, 60).unwrap();
        assert_eq!(time_base.timestamp_ns(Some(3)), Some(50_000_000));
        assert_eq!(time_base.timestamp_ns(Some(-1)), None);
    }
}
