//! Minimal DRM connector/CRTC scanner owned by the tty adapter.
//!
//! Smithay's drm-extras scanner also constructs connect/disconnect/change
//! event vectors. Tensor only needs the resulting connector table and CRTC
//! assignment, so keeping those values here avoids an unused event layer and
//! removes the extra Smithay dependency.

use std::collections::HashMap;

use drm::control::{Device as ControlDevice, connector, crtc};

#[derive(Debug, Default)]
pub(super) struct DrmScanner {
    connectors: HashMap<connector::Handle, connector::Info>,
    crtcs: HashMap<connector::Handle, crtc::Handle>,
}

impl DrmScanner {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn scan_connectors(&mut self, drm: &impl ControlDevice) -> std::io::Result<()> {
        let resources = drm.resource_handles()?;
        self.connectors
            .retain(|handle, _| resources.connectors().contains(handle));
        for &handle in resources.connectors() {
            // A transient forced-probe failure retains the prior snapshot;
            // only disappearance from resource_handles means removal.
            if let Ok(info) = drm.get_connector(handle, true) {
                self.connectors.insert(handle, info);
            }
        }

        self.crtcs.retain(|handle, _| {
            self.connectors
                .get(handle)
                .is_some_and(|info| info.state() == connector::State::Connected)
        });
        self.assign_crtcs(drm, &resources);
        Ok(())
    }

    pub(super) fn connectors(&self) -> &HashMap<connector::Handle, connector::Info> {
        &self.connectors
    }

    pub(super) fn crtc_for_connector(&self, connector: &connector::Handle) -> Option<crtc::Handle> {
        self.crtcs.get(connector).copied()
    }

    fn assign_crtcs(
        &mut self,
        drm: &impl ControlDevice,
        resources: &drm::control::ResourceHandles,
    ) {
        let connectors = &self.connectors;
        let crtcs = &mut self.crtcs;

        // Preserve the kernel's current assignment when it is still available.
        for &handle in resources.connectors() {
            let Some(info) = connectors.get(&handle) else {
                continue;
            };
            if info.state() != connector::State::Connected || crtcs.contains_key(&handle) {
                continue;
            }
            let Some(current) = info.current_encoder() else {
                continue;
            };
            let Some(crtc) = drm
                .get_encoder(current)
                .ok()
                .and_then(|encoder| encoder.crtc())
            else {
                continue;
            };
            if !crtc_is_taken(crtcs, crtc) {
                crtcs.insert(handle, crtc);
            }
        }

        // Assign remaining connectors from their encoder compatibility masks.
        for &handle in resources.connectors() {
            let Some(info) = connectors.get(&handle) else {
                continue;
            };
            if info.state() != connector::State::Connected || crtcs.contains_key(&handle) {
                continue;
            }
            let crtc = info.encoders().iter().find_map(|encoder| {
                let encoder = drm.get_encoder(*encoder).ok()?;
                resources
                    .filter_crtcs(encoder.possible_crtcs())
                    .into_iter()
                    .find(|candidate| !crtc_is_taken(crtcs, *candidate))
            });
            if let Some(crtc) = crtc {
                crtcs.insert(handle, crtc);
            }
        }
    }
}

fn crtc_is_taken(
    assignments: &HashMap<connector::Handle, crtc::Handle>,
    candidate: crtc::Handle,
) -> bool {
    assignments.values().any(|assigned| *assigned == candidate)
}
