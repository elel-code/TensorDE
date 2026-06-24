use std::path::PathBuf;

use serde::Serialize;

use super::NativeVulkanError;
use super::audio_clock::{
    NativeVulkanAudioClockRuntimeProbe, NativeVulkanAudioClockRuntimeSnapshot,
    NativeVulkanAudioClockRuntimeTelemetry,
};
use super::audio_policy::NativeVulkanAudioOutputMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum NativeVulkanAudioClockProvider {
    Gstreamer,
}

impl NativeVulkanAudioClockProvider {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Gstreamer => "gstreamer",
        }
    }
}

pub(super) enum NativeVulkanAudioClockRuntimeFrontend {
    Gstreamer(NativeVulkanAudioClockRuntimeProbe),
}

impl NativeVulkanAudioClockRuntimeFrontend {
    pub(super) fn start(
        source: &PathBuf,
        output_mode: NativeVulkanAudioOutputMode,
    ) -> Result<Self, NativeVulkanError> {
        Ok(Self::Gstreamer(NativeVulkanAudioClockRuntimeProbe::start(
            source,
            output_mode,
        )?))
    }

    pub(super) fn provider(&self) -> NativeVulkanAudioClockProvider {
        match self {
            Self::Gstreamer(_) => NativeVulkanAudioClockProvider::Gstreamer,
        }
    }

    pub(super) fn telemetry(&self) -> NativeVulkanAudioClockRuntimeTelemetry {
        match self {
            Self::Gstreamer(frontend) => frontend.telemetry(),
        }
    }

    pub(super) fn sample_video_pts_ms(
        &mut self,
        video_pts_ms: Option<u64>,
        video_clock_ns: Option<u64>,
    ) -> Result<(), NativeVulkanError> {
        match self {
            Self::Gstreamer(frontend) => frontend.sample_video_pts_ms(video_pts_ms, video_clock_ns),
        }
    }

    pub(super) fn seek_for_video_loop(
        &mut self,
        position_ms: u64,
    ) -> Result<(), NativeVulkanError> {
        match self {
            Self::Gstreamer(frontend) => frontend.seek_for_video_loop(position_ms),
        }
    }

    pub(super) fn snapshot(
        &mut self,
    ) -> Result<NativeVulkanAudioClockRuntimeSnapshot, NativeVulkanError> {
        match self {
            Self::Gstreamer(frontend) => frontend.snapshot(),
        }
    }

    pub(super) fn audio_master_clock_estimate_ns(&self) -> Option<u64> {
        match self {
            Self::Gstreamer(frontend) => frontend.audio_master_clock_estimate_ns(),
        }
    }
}
