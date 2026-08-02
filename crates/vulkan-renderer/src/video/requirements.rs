use std::fmt;
use std::ops::{BitOr, BitOrAssign};

use crate::{Error, Result};

/// Exact decode profiles that a logical device must support.
#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct VideoDecodeCodecs(u8);

impl VideoDecodeCodecs {
    pub const H264_HIGH_8: Self = Self(1 << 0);
    pub const H265_MAIN_8: Self = Self(1 << 1);
    pub const H265_MAIN_10: Self = Self(1 << 2);
    pub const AV1_MAIN_8: Self = Self(1 << 3);
    pub const AV1_MAIN_10: Self = Self(1 << 4);

    const ALL: Self = Self(
        Self::H264_HIGH_8.0
            | Self::H265_MAIN_8.0
            | Self::H265_MAIN_10.0
            | Self::AV1_MAIN_8.0
            | Self::AV1_MAIN_10.0,
    );

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub(crate) const fn union(self, other: Self) -> Self {
        Self((self.0 | other.0) & Self::ALL.0)
    }

    pub(crate) const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0 & Self::ALL.0)
    }

    pub(crate) fn labels(self) -> Vec<&'static str> {
        [
            (Self::H264_HIGH_8, "h264-high-8"),
            (Self::H265_MAIN_8, "h265-main-8"),
            (Self::H265_MAIN_10, "h265-main-10"),
            (Self::AV1_MAIN_8, "av1-main-8"),
            (Self::AV1_MAIN_10, "av1-main-10"),
        ]
        .into_iter()
        .filter_map(|(codec, label)| self.contains(codec).then_some(label))
        .collect()
    }
}

impl fmt::Debug for VideoDecodeCodecs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(self.labels()).finish()
    }
}

impl BitOr for VideoDecodeCodecs {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl BitOrAssign for VideoDecodeCodecs {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

/// Vulkan Video requirements owned by a logical-device request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoDecodeRequirements {
    codecs: VideoDecodeCodecs,
}

impl VideoDecodeRequirements {
    pub fn new(codecs: VideoDecodeCodecs) -> Result<Self> {
        if codecs.is_empty() {
            return Err(Error::Validation(
                "video decode requirements must contain at least one profile".into(),
            ));
        }
        Ok(Self { codecs })
    }

    pub const fn codecs(self) -> VideoDecodeCodecs {
        self.codecs
    }

    pub(crate) fn required_extensions(self) -> Vec<&'static str> {
        let mut extensions = vec!["VK_KHR_video_queue", "VK_KHR_video_decode_queue"];
        if self.codecs.intersects(VideoDecodeCodecs::H264_HIGH_8) {
            extensions.push("VK_KHR_video_decode_h264");
        }
        if self
            .codecs
            .intersects(VideoDecodeCodecs::H265_MAIN_8 | VideoDecodeCodecs::H265_MAIN_10)
        {
            extensions.push("VK_KHR_video_decode_h265");
        }
        if self
            .codecs
            .intersects(VideoDecodeCodecs::AV1_MAIN_8 | VideoDecodeCodecs::AV1_MAIN_10)
        {
            extensions.push("VK_KHR_video_decode_av1");
        }
        extensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_profiles_lower_to_exact_codec_extension_families() {
        let requirements = VideoDecodeRequirements::new(
            VideoDecodeCodecs::H264_HIGH_8
                | VideoDecodeCodecs::H265_MAIN_10
                | VideoDecodeCodecs::AV1_MAIN_8,
        )
        .unwrap();
        assert_eq!(
            requirements.required_extensions(),
            vec![
                "VK_KHR_video_queue",
                "VK_KHR_video_decode_queue",
                "VK_KHR_video_decode_h264",
                "VK_KHR_video_decode_h265",
                "VK_KHR_video_decode_av1",
            ]
        );
    }

    #[test]
    fn empty_profile_request_is_rejected() {
        assert!(VideoDecodeRequirements::new(VideoDecodeCodecs::empty()).is_err());
    }
}
