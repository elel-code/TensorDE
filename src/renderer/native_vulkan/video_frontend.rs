use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(not(feature = "native-vulkan-gst-video"), allow(dead_code))]
#[serde(rename_all = "kebab-case")]
pub(super) enum NativeVulkanVideoFrontendProvider {
    Gstreamer,
}

impl NativeVulkanVideoFrontendProvider {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Gstreamer => "gstreamer",
        }
    }

    pub(super) fn active_frontend_label(self) -> &'static str {
        match self {
            Self::Gstreamer => "gstreamer-appsink",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoCapsSnapshot {
    pub element: String,
    pub pad: String,
    pub direction: String,
    pub caps: String,
    pub source: String,
    pub memory_features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeVulkanVideoFrontendSnapshot {
    pub(super) provider: NativeVulkanVideoFrontendProvider,
    pub(super) provider_state: Option<String>,
    pub(super) eos_messages: u64,
    pub(super) segment_done_messages: u64,
    pub(super) frames_received: u64,
    pub(super) last_sample_caps: Option<String>,
    pub(super) last_sample_format: Option<String>,
    pub(super) last_sample_size: Option<(u32, u32)>,
    pub(super) last_sample_pts_ms: Option<u64>,
    pub(super) last_sample_duration_ms: Option<u64>,
    pub(super) last_sample_pts_delta_ms: Option<u64>,
    pub(super) last_sample_memory_types: Vec<String>,
    pub(super) actual_decoders: Vec<String>,
    pub(super) decoder_policy_status: Option<String>,
    pub(super) caps_report_count: usize,
    pub(super) caps_memory_features: Vec<String>,
    pub(super) caps_reports: Vec<NativeVulkanVideoCapsSnapshot>,
    pub(super) last_error: Option<String>,
}
