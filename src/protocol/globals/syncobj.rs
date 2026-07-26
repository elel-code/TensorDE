use smithay::{
    backend::drm::DrmDeviceFd,
    wayland::drm_syncobj::{DrmSyncobjState, supports_syncobj_eventfd},
};
use wayland_server::DisplayHandle;

use super::super::state::RuntimeState;

/// Owns the optional `wp_linux_drm_syncobj_v1` global and follows the
/// Smithay DRM device selected by Vulkan.  The global is advertised only once
/// an eventfd-capable primary DRM device is available; device loss closes the
/// import fd while preserving the global for already-bound clients.
#[derive(Debug)]
pub(crate) struct DrmSyncobjProtocol {
    pub(crate) state: Option<DrmSyncobjState>,
    device: Option<DrmDeviceFd>,
    active: bool,
}

impl DrmSyncobjProtocol {
    pub(crate) fn new() -> Self {
        Self {
            state: None,
            device: None,
            active: false,
        }
    }

    pub(crate) fn update(&mut self, display: &DisplayHandle, device: Option<DrmDeviceFd>) {
        let Some(device) = device else {
            self.close_device();
            return;
        };
        if !supports_syncobj_eventfd(&device) {
            self.close_device();
            return;
        }
        if self.device.as_ref() == Some(&device) {
            self.active = true;
            return;
        }
        if let Some(state) = self.state.as_mut() {
            state.update_device(device.clone());
        } else {
            self.state = Some(DrmSyncobjState::new::<RuntimeState>(
                display,
                device.clone(),
            ));
        }
        self.device = Some(device);
        self.active = true;
    }

    pub(crate) fn close_device(&mut self) {
        if let Some(state) = self.state.as_mut() {
            let guard = state.close_device();
            drop(guard);
        }
        self.device = None;
        self.active = false;
    }

    pub(crate) fn advertised(&self) -> bool {
        self.state.is_some()
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }
}

impl Default for DrmSyncobjProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_starts_unadvertised_and_inactive() {
        let protocol = DrmSyncobjProtocol::new();
        assert!(!protocol.advertised());
        assert!(!protocol.active());
    }
}
