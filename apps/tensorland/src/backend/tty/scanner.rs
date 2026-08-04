//! Minimal DRM connector/CRTC scanner owned by the tty adapter.
//!
//! Smithay's drm-extras scanner also constructs connect/disconnect/change
//! event vectors. Tensor only needs the resulting connector table and CRTC
//! assignment, so keeping those values here avoids an unused event layer and
//! removes the extra Smithay dependency.

use std::collections::HashMap;

use drm::control::{Device as ControlDevice, connector, crtc, property};

#[derive(Debug, Default)]
pub(super) struct DrmScanner {
    connectors: HashMap<connector::Handle, connector::Info>,
    non_desktop: HashMap<connector::Handle, bool>,
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
                self.non_desktop
                    .insert(handle, connector_is_non_desktop(drm, handle));
            }
        }
        self.non_desktop
            .retain(|handle, _| self.connectors.contains_key(handle));
        self.assign_crtcs(drm, &resources);
        Ok(())
    }

    pub(super) fn connectors(&self) -> &HashMap<connector::Handle, connector::Info> {
        &self.connectors
    }

    pub(super) fn crtc_for_connector(&self, connector: &connector::Handle) -> Option<crtc::Handle> {
        self.crtcs.get(connector).copied()
    }

    pub(super) fn is_non_desktop(&self, connector: &connector::Handle) -> bool {
        self.non_desktop.get(connector).copied().unwrap_or(false)
    }

    fn assign_crtcs(
        &mut self,
        drm: &impl ControlDevice,
        resources: &drm::control::ResourceHandles,
    ) {
        let connectors = &self.connectors;
        let non_desktop = &self.non_desktop;
        let crtcs = &mut self.crtcs;
        crtcs.clear();

        // Desktop heads always claim compatible resources before lease-only
        // heads, both when retaining kernel assignments and when selecting a
        // fresh CRTC. This prevents an HMD from starving an ordinary output.
        for lease_only in [false, true] {
            for &handle in resources.connectors() {
                let Some(info) = connectors.get(&handle) else {
                    continue;
                };
                if info.state() != connector::State::Connected
                    || non_desktop.get(&handle).copied().unwrap_or(false) != lease_only
                {
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
        }

        for lease_only in [false, true] {
            for &handle in resources.connectors() {
                let Some(info) = connectors.get(&handle) else {
                    continue;
                };
                if info.state() != connector::State::Connected
                    || crtcs.contains_key(&handle)
                    || non_desktop.get(&handle).copied().unwrap_or(false) != lease_only
                {
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
}

fn connector_is_non_desktop(drm: &impl ControlDevice, connector: connector::Handle) -> bool {
    let Ok(properties) = drm.get_properties(connector) else {
        return false;
    };
    properties.into_iter().any(|(handle, value)| {
        drm.get_property(handle).ok().is_some_and(|info| {
            info.name().to_bytes() == b"non-desktop"
                && matches!(
                    info.value_type().convert_value(value),
                    property::Value::Boolean(true)
                )
        })
    })
}

fn crtc_is_taken(
    assignments: &HashMap<connector::Handle, crtc::Handle>,
    candidate: crtc::Handle,
) -> bool {
    assignments.values().any(|assigned| *assigned == candidate)
}
