//! Value-only head snapshots for `wlr-output-management` (topology rate).

use super::TtyBackend;
use tensor_host::ConnectorState;

impl TtyBackend {
    /// Connected connectors for `wlr-output-management` (includes policy-disabled).
    ///
    /// Topology-rate only; value-only snapshots, no DRM FDs.
    /// Returns `(name, mode_w, mode_h, refresh_mHz, enabled_by_policy)`.
    pub(crate) fn management_connector_heads(&self) -> Vec<(String, i32, i32, i32, bool)> {
        let mut heads = Vec::new();
        for device in self.devices.values() {
            for connector in device.connectors.values() {
                if connector.state != ConnectorState::Connected {
                    continue;
                }
                let enabled = self
                    .output_policy
                    .rules()
                    .get(&connector.name)
                    .is_none_or(|rule| rule.enabled);
                let mode = connector
                    .preferred_mode
                    .or_else(|| connector.modes.first().copied());
                let Some(mode) = mode else {
                    continue;
                };
                heads.push((
                    connector.name.clone(),
                    mode.width,
                    mode.height,
                    mode.refresh_millihertz,
                    enabled,
                ));
            }
        }
        heads.sort_by(|a, b| a.0.cmp(&b.0));
        heads
    }
}
