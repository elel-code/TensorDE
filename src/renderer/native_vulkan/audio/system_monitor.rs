//! Process-owned PipeWire sink monitor for scene audio-reactive uniforms.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicU8, Ordering};

use super::clock::{
    native_vulkan_audio_clear_spectrum32, native_vulkan_audio_publish_spectrum32_packed,
};

const MONITOR_NOT_REQUESTED: u8 = 0;
const MONITOR_STARTING: u8 = 1;
const MONITOR_READY: u8 = 2;
const MONITOR_UNAVAILABLE: u8 = 3;
const MONITOR_DISABLED: u8 = 4;
static SYSTEM_AUDIO_MONITOR_STATE: AtomicU8 = AtomicU8::new(MONITOR_NOT_REQUESTED);

#[repr(C)]
struct GilderSystemAudioMonitor {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn gilder_system_audio_monitor_alloc() -> *mut GilderSystemAudioMonitor;
    fn gilder_system_audio_monitor_free(handle: *mut *mut GilderSystemAudioMonitor);
    fn gilder_system_audio_monitor_snapshot(
        monitor: *const GilderSystemAudioMonitor,
        spectrum32_packed: *mut u32,
        stream_state: *mut i32,
        process_callbacks: *mut u64,
    ) -> i32;
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanSystemAudioMonitor {
    handle: Option<NonNull<GilderSystemAudioMonitor>>,
    published_spectrum: bool,
}

impl NativeVulkanSystemAudioMonitor {
    pub(in crate::renderer::native_vulkan) fn start_if_needed(required: bool) -> Self {
        if !required {
            SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_NOT_REQUESTED, Ordering::Release);
            return Self {
                handle: None,
                published_spectrum: false,
            };
        }
        if system_audio_monitor_disabled() || std::env::var_os("GILDER_SCENE_AUDIO_SPECTRUM32").is_some() {
            SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_DISABLED, Ordering::Release);
            return Self {
                handle: None,
                published_spectrum: false,
            };
        }
        let handle = NonNull::new(unsafe { gilder_system_audio_monitor_alloc() });
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
            published_spectrum: false,
        }
    }

    pub(in crate::renderer::native_vulkan) fn publish_latest(&mut self) {
        let Some(handle) = self.handle else {
            return;
        };
        let mut spectrum = [0u32; 16];
        let mut stream_state = 0;
        let mut process_callbacks = 0;
        let ready = unsafe {
            gilder_system_audio_monitor_snapshot(
                handle.as_ptr(),
                spectrum.as_mut_ptr(),
                &mut stream_state,
                &mut process_callbacks,
            )
        };
        if ready > 0 {
            native_vulkan_audio_publish_spectrum32_packed(spectrum);
            self.published_spectrum = true;
            SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_READY, Ordering::Release);
        } else if ready < 0 {
            SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_UNAVAILABLE, Ordering::Release);
        }
    }
}

impl Drop for NativeVulkanSystemAudioMonitor {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let mut raw = handle.as_ptr();
            unsafe { gilder_system_audio_monitor_free(&mut raw) };
        }
        if self.published_spectrum {
            native_vulkan_audio_clear_spectrum32();
        }
        SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_NOT_REQUESTED, Ordering::Release);
    }
}

pub(in crate::renderer::native_vulkan) fn system_audio_monitor_spectrum_status(
) -> Option<(&'static str, bool)> {
    match SYSTEM_AUDIO_MONITOR_STATE.load(Ordering::Acquire) {
        MONITOR_STARTING => Some(("pipewire-system-output-monitor-starting", false)),
        MONITOR_READY => Some(("pipewire-system-output-monitor-we-log-goertzel32-mono", true)),
        MONITOR_UNAVAILABLE => Some(("zero-spectrum-pipewire-monitor-unavailable", false)),
        MONITOR_DISABLED => Some(("zero-spectrum-pipewire-monitor-disabled", false)),
        _ => None,
    }
}

fn system_audio_monitor_disabled() -> bool {
    std::env::var("GILDER_SCENE_SYSTEM_AUDIO")
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
            Some(("pipewire-system-output-monitor-we-log-goertzel32-mono", true))
        );
        SYSTEM_AUDIO_MONITOR_STATE.store(MONITOR_NOT_REQUESTED, Ordering::Release);
    }
}
