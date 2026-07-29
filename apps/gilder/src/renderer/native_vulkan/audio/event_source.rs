//! Adapter from process audio publishers to one typed scene-event snapshot.

use std::sync::{Arc, Mutex, OnceLock};

use crate::engine::scene::{SceneAudioSource, SceneAudioState, StereoSpectrum64};

use super::clock::native_vulkan_audio_spectrum64;
use super::spectrum::SpectrumNormalizer;
use super::system_monitor::system_audio_monitor_spectrum_status;

static DIAGNOSTIC_SPECTRUM64: OnceLock<Option<StereoSpectrum64>> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
struct NativeVulkanAudioEventChannelSample {
    generation: u64,
    sample_time_ns: u64,
    spectrum: StereoSpectrum64,
}

#[derive(Debug, Clone, Default)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAudioEventChannel {
    state: Arc<Mutex<NativeVulkanAudioEventChannelState>>,
}

#[derive(Debug, Default)]
struct NativeVulkanAudioEventChannelState {
    latest: Option<NativeVulkanAudioEventChannelSample>,
    normalizer: SpectrumNormalizer,
    last_capture_time_ns: Option<u64>,
}

impl NativeVulkanAudioEventChannel {
    pub(in crate::renderer::native_vulkan) fn publish(
        &self,
        generation: u64,
        sample_time_ns: u64,
        spectrum: StereoSpectrum64,
    ) {
        if let Ok(mut state) = self.state.lock() {
            state.latest = Some(NativeVulkanAudioEventChannelSample {
                generation,
                sample_time_ns,
                spectrum,
            });
        }
    }

    pub(in crate::renderer::native_vulkan) fn capture(
        &self,
        generation: u64,
        fallback_sample_time_ns: u64,
    ) -> SceneAudioState {
        let Ok(mut state) = self.state.lock() else {
            return SceneAudioState {
                source: SceneAudioSource::MediaSession,
                media_generation: crate::engine::scene::SceneMediaGeneration(generation),
                sample_time_ns: fallback_sample_time_ns,
                ..SceneAudioState::default()
            };
        };
        let sample = state
            .latest
            .filter(|sample| sample.generation == generation);
        let spectrum = sample
            .map(|sample| {
                let dt =
                    frame_delta_seconds(&mut state.last_capture_time_ns, fallback_sample_time_ns);
                state.normalizer.normalize(sample.spectrum, dt)
            })
            .unwrap_or_default();
        SceneAudioState {
            source: SceneAudioSource::MediaSession,
            media_generation: crate::engine::scene::SceneMediaGeneration(generation),
            sample_time_ns: sample
                .map(|sample| sample.sample_time_ns)
                .unwrap_or(fallback_sample_time_ns),
            spectrum,
            ready: sample.is_some(),
            ..SceneAudioState::default()
        }
    }
}

#[derive(Debug, Default)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAudioEventSource {
    normalizer: SpectrumNormalizer,
    last_capture_time_ns: Option<u64>,
}

impl NativeVulkanAudioEventSource {
    pub(in crate::renderer::native_vulkan) fn capture(
        &mut self,
        sample_time_ns: u64,
    ) -> SceneAudioState {
        if let Some(spectrum) = diagnostic_spectrum64() {
            return SceneAudioState {
                source: SceneAudioSource::Diagnostic,
                sample_time_ns,
                spectrum,
                ready: true,
                ..SceneAudioState::default()
            };
        }
        let raw_spectrum = native_vulkan_audio_spectrum64();
        let source = if system_audio_monitor_spectrum_status().is_some() {
            SceneAudioSource::SystemOutput
        } else if raw_spectrum.is_some() {
            SceneAudioSource::MediaSession
        } else {
            SceneAudioSource::None
        };
        let spectrum = raw_spectrum
            .map(|raw| {
                let dt = frame_delta_seconds(&mut self.last_capture_time_ns, sample_time_ns);
                self.normalizer.normalize(raw, dt)
            })
            .unwrap_or_default();
        SceneAudioState {
            source,
            sample_time_ns,
            spectrum,
            ready: raw_spectrum.is_some(),
            ..SceneAudioState::default()
        }
    }

    pub(in crate::renderer::native_vulkan) fn status(&self) -> (&'static str, bool) {
        if diagnostic_spectrum64().is_some() {
            ("diagnostic-stereo64-override", true)
        } else if let Some(status) = system_audio_monitor_spectrum_status() {
            status
        } else if native_vulkan_audio_spectrum64().is_some() {
            ("decoded-audio-canonical-stereo64", true)
        } else {
            ("zero-spectrum-no-publisher", false)
        }
    }
}

fn frame_delta_seconds(last: &mut Option<u64>, current: u64) -> f32 {
    let delta = last
        .replace(current)
        .map(|previous| current.saturating_sub(previous) as f64 / 1_000_000_000.0)
        .unwrap_or(0.0);
    delta as f32
}

pub(in crate::renderer::native_vulkan) fn audio_state_summary(
    state: &SceneAudioState,
) -> (f32, u32) {
    let peak = state
        .spectrum
        .left
        .iter()
        .chain(&state.spectrum.right)
        .copied()
        .fold(0.0f32, f32::max);
    let active_bands = state
        .spectrum
        .left
        .iter()
        .zip(&state.spectrum.right)
        .filter(|(left, right)| left.max(**right) > 1.0 / 65535.0)
        .count()
        .min(u32::MAX as usize) as u32;
    (peak, active_bands)
}

fn diagnostic_spectrum64() -> Option<StereoSpectrum64> {
    *DIAGNOSTIC_SPECTRUM64.get_or_init(|| {
        std::env::var("GILDER_SCENE_AUDIO_SPECTRUM64")
            .ok()
            .and_then(|value| parse_spectrum64(&value))
    })
}

fn parse_spectrum64(value: &str) -> Option<StereoSpectrum64> {
    if let Some(value) = value.trim().strip_prefix("stereo-flat:") {
        let channels = value
            .split(',')
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        if channels.len() != 2
            || channels
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return None;
        }
        return Some(StereoSpectrum64 {
            left: [channels[0]; 64],
            right: [channels[1]; 64],
        });
    }
    let values = value
        .split([',', ' '])
        .filter(|value| !value.is_empty())
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() != 128
        || values
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return None;
    }
    Some(StereoSpectrum64 {
        left: values[..64].try_into().ok()?,
        right: values[64..].try_into().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_stereo_spectrum_is_bounded_and_explicit() {
        assert_eq!(
            parse_spectrum64("stereo-flat:0.25,0.75"),
            Some(StereoSpectrum64 {
                left: [0.25; 64],
                right: [0.75; 64],
            })
        );
        assert!(parse_spectrum64("stereo-flat:0.25,1.1").is_none());
        let bands = (0..128)
            .map(|band| (band as f32 / 127.0).to_string())
            .collect::<Vec<_>>()
            .join(",");
        let parsed = parse_spectrum64(&bands).expect("stereo 64 bands");
        assert_eq!(parsed.left[0], 0.0);
        assert_eq!(parsed.right[63], 1.0);
        assert!(parse_spectrum64("0,1").is_none());
    }

    #[test]
    fn media_channel_rejects_spectrum_from_another_generation() {
        let channel = NativeVulkanAudioEventChannel::default();
        channel.publish(
            2,
            10,
            StereoSpectrum64 {
                left: [0.25; 64],
                right: [0.75; 64],
            },
        );

        let current = channel.capture(2, 20);
        assert!(current.ready);
        assert_eq!(current.sample_time_ns, 10);
        assert!(current.spectrum.left[0] > 0.0);
        assert!(current.spectrum.right[0] > current.spectrum.left[0]);

        let stale = channel.capture(3, 30);
        assert!(!stale.ready);
        assert_eq!(stale.sample_time_ns, 30);
        assert_eq!(stale.spectrum, StereoSpectrum64::ZERO);
    }
}
