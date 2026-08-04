//! Process-owned PipeWire sink monitor for scene audio-reactive uniforms.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicU8, Ordering};

use super::event_source::{clear_published_audio_spectrum64, publish_audio_spectrum64};
use super::spectrum::{DEFAULT_INPUT_VOLUME, PcmSpectrumProducer};

const MONITOR_SAMPLE_RATE: u32 = 48_000;
const MONITOR_CHANNELS: u32 = 2;
const MONITOR_PCM_CAPACITY_SAMPLES: usize = 4_096 * MONITOR_CHANNELS as usize;

const MONITOR_NOT_REQUESTED: u8 = 0;
const MONITOR_STARTING: u8 = 1;
const MONITOR_READY: u8 = 2;
const MONITOR_UNAVAILABLE: u8 = 3;
const MONITOR_DISABLED: u8 = 4;
static SYSTEM_AUDIO_MONITOR_STATE: AtomicU8 = AtomicU8::new(MONITOR_NOT_REQUESTED);

#[repr(C)]
struct TensorWallpaperSystemAudioMonitor {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn tensor_wallpaper_system_audio_monitor_alloc() -> *mut TensorWallpaperSystemAudioMonitor;
    fn tensor_wallpaper_system_audio_monitor_free(handle: *mut *mut TensorWallpaperSystemAudioMonitor);
    fn tensor_wallpaper_system_audio_monitor_snapshot(
        monitor: *mut TensorWallpaperSystemAudioMonitor,
        pcm: *mut f32,
        pcm_capacity: u32,
        sample_count: *mut u32,
        stream_state: *mut i32,
        process_callbacks: *mut u64,
    ) -> i32;
}

pub(in crate::renderer::rendering_device) struct RenderingDeviceSystemAudioMonitor {
    handle: Option<NonNull<TensorWallpaperSystemAudioMonitor>>,
    producer: Option<PcmSpectrumProducer>,
    pcm: Vec<f32>,
    published_spectrum: bool,
}

impl RenderingDeviceSystemAudioMonitor {
    pub(in crate::renderer::rendering_device) fn start_if_needed(required: bool) -> Self {
        if !required {
            SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_NOT_REQUESTED, Ordering::Release);
            return Self {
                handle: None,
                producer: None,
                pcm: Vec::new(),
                published_spectrum: false,
            };
        }
        if system_audio_monitor_disabled()
            || std::env::var_os("TENSOR_WALLPAPER_SCENE_AUDIO_SPECTRUM64").is_some()
        {
            SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_DISABLED, Ordering::Release);
            return Self {
                handle: None,
                producer: None,
                pcm: Vec::new(),
                published_spectrum: false,
            };
        }
        let handle = NonNull::new(unsafe { tensor_wallpaper_system_audio_monitor_alloc() });
        SYSTEM_AUDIO_MONITOR_STATE.store(
            if handle.is_some() {
                MONITOR_STARTING
            } else {
                MONITOR_UNAVAILABLE
            },
            Ordering::Release,
        );
        Self {
            handle,
            producer: handle.map(|_| {
                PcmSpectrumProducer::new(
                    MONITOR_SAMPLE_RATE,
                    MONITOR_CHANNELS,
                    DEFAULT_INPUT_VOLUME,
                    0.0,
                )
                .expect("fixed PipeWire spectrum configuration is valid")
            }),
            pcm: handle
                .map(|_| vec![0.0; MONITOR_PCM_CAPACITY_SAMPLES])
                .unwrap_or_default(),
            published_spectrum: false,
        }
    }

    pub(in crate::renderer::rendering_device) fn publish_latest(&mut self) {
        let Some(handle) = self.handle else {
            return;
        };
        let mut sample_count = 0u32;
        let mut stream_state = 0;
        let mut process_callbacks = 0;
        let ready = unsafe {
            tensor_wallpaper_system_audio_monitor_snapshot(
                handle.as_ptr(),
                self.pcm.as_mut_ptr(),
                self.pcm.len() as u32,
                &mut sample_count,
                &mut stream_state,
                &mut process_callbacks,
            )
        };
        if ready > 0 {
            if let Some(spectrum) = self
                .producer
                .as_mut()
                .and_then(|producer| producer.push_interleaved(&self.pcm[..sample_count as usize]))
            {
                publish_audio_spectrum64(spectrum);
                self.published_spectrum = true;
                SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_READY, Ordering::Release);
            }
        } else if ready < 0 {
            SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_UNAVAILABLE, Ordering::Release);
        }
    }
}

impl Drop for RenderingDeviceSystemAudioMonitor {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let mut raw = handle.as_ptr();
            unsafe { tensor_wallpaper_system_audio_monitor_free(&mut raw) };
        }
        if self.published_spectrum {
            clear_published_audio_spectrum64();
        }
        SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_NOT_REQUESTED, Ordering::Release);
    }
}

pub(in crate::renderer::rendering_device) fn system_audio_monitor_spectrum_status()
-> Option<(&'static str, bool)> {
    match SYSTEM_AUDIO_MONITOR_STATE.load(Ordering::Acquire) {
        MONITOR_STARTING => Some(("pipewire-system-output-monitor-starting", false)),
        MONITOR_READY => Some(("pipewire-system-output-canonical-stereo64", true)),
        MONITOR_UNAVAILABLE => Some(("zero-spectrum-pipewire-monitor-unavailable", false)),
        MONITOR_DISABLED => Some(("zero-spectrum-pipewire-monitor-disabled", false)),
        _ => None,
    }
}

fn system_audio_monitor_disabled() -> bool {
    std::env::var("TENSOR_WALLPAPER_SCENE_SYSTEM_AUDIO")
        .ok()
        .is_some_and(|value| matches!(value.trim(), "0" | "off" | "false" | "disabled"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_state_labels_distinguish_lifecycle_boundaries() {
        SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_STARTING, Ordering::Release);
        assert_eq!(
            system_audio_monitor_spectrum_status(),
            Some(("pipewire-system-output-monitor-starting", false))
        );
        SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_READY, Ordering::Release);
        assert_eq!(
            system_audio_monitor_spectrum_status(),
            Some(("pipewire-system-output-canonical-stereo64", true))
        );
        SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_NOT_REQUESTED, Ordering::Release);
    }
}
