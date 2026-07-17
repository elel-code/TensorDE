//! Adapter from process audio publishers to one typed scene-event snapshot.

use std::sync::OnceLock;

use crate::engine::scene::{SceneAudioSource, SceneAudioState};

use super::clock::native_vulkan_audio_spectrum32_packed;
use super::system_monitor::system_audio_monitor_spectrum_status;

static DIAGNOSTIC_SPECTRUM32: OnceLock<Option<[f32; 32]>> = OnceLock::new();

#[derive(Debug, Default)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAudioEventSource;

impl NativeVulkanAudioEventSource {
    pub(in crate::renderer::native_vulkan) fn capture(&self, sample_time_ns: u64) -> SceneAudioState {
        if let Some(spectrum32) = diagnostic_spectrum32() {
            return SceneAudioState {
                source: SceneAudioSource::Diagnostic,
                sample_time_ns,
                spectrum32,
                ready: true,
                ..SceneAudioState::default()
            };
        }
        let spectrum32 = native_vulkan_audio_spectrum32_packed().map(unpack_spectrum32);
        let source = if system_audio_monitor_spectrum_status().is_some() {
            SceneAudioSource::SystemOutput
        } else if spectrum32.is_some() {
            SceneAudioSource::MediaSession
        } else {
            SceneAudioSource::None
        };
        SceneAudioState {
            source,
            sample_time_ns,
            spectrum32: spectrum32.unwrap_or([0.0; 32]),
            ready: spectrum32.is_some(),
            ..SceneAudioState::default()
        }
    }

    pub(in crate::renderer::native_vulkan) fn status(&self) -> (&'static str, bool) {
        if diagnostic_spectrum32().is_some() {
            ("diagnostic-spectrum32-override", true)
        } else if let Some(status) = system_audio_monitor_spectrum_status() {
            status
        } else if native_vulkan_audio_spectrum32_packed().is_some() {
            ("decoded-audio-goertzel32-mono-duplicated-stereo", true)
        } else {
            ("zero-spectrum-no-publisher", false)
        }
    }
}

pub(in crate::renderer::native_vulkan) fn audio_state_summary(state: &SceneAudioState) -> (f32, u32) {
    let peak = state.spectrum32.iter().copied().fold(0.0f32, f32::max);
    let active_bands = state
        .spectrum32
        .iter()
        .filter(|value| **value > 1.0 / 65535.0)
        .count()
        .min(u32::MAX as usize) as u32;
    (peak, active_bands)
}

fn diagnostic_spectrum32() -> Option<[f32; 32]> {
    *DIAGNOSTIC_SPECTRUM32.get_or_init(|| {
        std::env::var("GILDER_SCENE_AUDIO_SPECTRUM32")
            .ok()
            .and_then(|value| parse_spectrum32(&value))
    })
}

fn unpack_spectrum32(packed: [u32; 16]) -> [f32; 32] {
    std::array::from_fn(|band| {
        let shift = (band & 1) * 16;
        ((packed[band / 2] >> shift) & 0xffff) as f32 / 65535.0
    })
}

fn parse_spectrum32(value: &str) -> Option<[f32; 32]> {
    if let Some(value) = value.trim().strip_prefix("flat:") {
        let value = value.parse::<f32>().ok()?;
        return (value.is_finite() && (0.0..=1.0).contains(&value)).then_some([value; 32]);
    }
    let values = value
        .split([',', ' '])
        .filter(|value| !value.is_empty())
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() != 32
        || values
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return None;
    }
    values.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_spectrum_is_bounded_and_explicit() {
        assert_eq!(parse_spectrum32("flat:0.75"), Some([0.75; 32]));
        assert!(parse_spectrum32("flat:1.1").is_none());
        let bands = (0..32)
            .map(|band| (band as f32 / 31.0).to_string())
            .collect::<Vec<_>>()
            .join(",");
        let parsed = parse_spectrum32(&bands).expect("32 bands");
        assert_eq!(parsed[0], 0.0);
        assert_eq!(parsed[31], 1.0);
        assert!(parse_spectrum32("0,1").is_none());
    }
}
