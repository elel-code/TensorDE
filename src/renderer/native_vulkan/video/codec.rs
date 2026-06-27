use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanVideoSessionCodec {
    #[serde(rename = "h264-high-8")]
    H264High8,
    #[serde(rename = "h265-main-8")]
    H265Main8,
    #[serde(rename = "h265-main-10")]
    H265Main10,
    #[serde(rename = "av1-main-8")]
    Av1Main8,
    #[serde(rename = "av1-main-10")]
    Av1Main10,
}

impl NativeVulkanVideoSessionCodec {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::H264High8 => "h264-high-8",
            Self::H265Main8 => "h265-main-8",
            Self::H265Main10 => "h265-main-10",
            Self::Av1Main8 => "av1-main-8",
            Self::Av1Main10 => "av1-main-10",
        }
    }

    pub(crate) fn profile_label(self) -> &'static str {
        match self {
            Self::H264High8 => "high-8",
            Self::H265Main8 | Self::Av1Main8 => "main-8",
            Self::H265Main10 | Self::Av1Main10 => "main-10",
        }
    }
}

impl std::str::FromStr for NativeVulkanVideoSessionCodec {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "h264" | "avc" | "h264-high" | "h264-high-8" | "avc-high-8" => Ok(Self::H264High8),
            "h265" | "hevc" | "h265-main-8" | "hevc-main-8" => Ok(Self::H265Main8),
            "h265-main-10" | "hevc-main-10" => Ok(Self::H265Main10),
            "av1" | "av1-main-8" => Ok(Self::Av1Main8),
            "av1-main-10" => Ok(Self::Av1Main10),
            other => Err(format!("unsupported Vulkan Video session codec: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_video_session_codec_aliases() {
        assert_eq!(
            "h264".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::H264High8)
        );
        assert_eq!(
            "h264-high-8".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::H264High8)
        );
        assert_eq!(
            "hevc-main-10".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::H265Main10)
        );
        assert_eq!(
            "av1-main-10".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::Av1Main10)
        );
    }

    #[test]
    fn exposes_codec_and_profile_labels_without_vulkan_binding_types() {
        assert_eq!(
            NativeVulkanVideoSessionCodec::H264High8.label(),
            "h264-high-8"
        );
        assert_eq!(
            NativeVulkanVideoSessionCodec::H264High8.profile_label(),
            "high-8"
        );
        assert_eq!(
            NativeVulkanVideoSessionCodec::H265Main10.profile_label(),
            "main-10"
        );
        assert_eq!(
            NativeVulkanVideoSessionCodec::Av1Main8.profile_label(),
            "main-8"
        );
    }
}
