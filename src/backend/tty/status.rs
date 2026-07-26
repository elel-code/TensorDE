use std::path::PathBuf;

use super::{TtyBackend, node_path};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BackendStatus {
    pub(crate) seat: String,
    pub(crate) primary_node: PathBuf,
    pub(crate) render_node: PathBuf,
    pub(crate) drm_devices: usize,
    pub(crate) primary_gbm_ready: bool,
    pub(crate) session_active: bool,
    pub(crate) topology_generation: u64,
    pub(crate) outputs: usize,
    pub(crate) native_format_candidates: usize,
}

impl TtyBackend {
    pub(super) fn status(&self) -> BackendStatus {
        let primary_gbm_ready =
            self.devices
                .get(&self.primary_node.dev_id())
                .is_some_and(|device| {
                    let _ = &device.gbm;
                    true
                });
        BackendStatus {
            seat: self.session.seat(),
            primary_node: node_path(self.primary_node),
            render_node: node_path(self.render_node),
            drm_devices: self.devices.len(),
            outputs: self.outputs.len(),
            native_format_candidates: self
                .devices
                .values()
                .flat_map(|device| device.output_formats.values())
                .map(Vec::len)
                .sum(),
            primary_gbm_ready,
            session_active: self.session.is_active(),
            topology_generation: self.topology_generation,
        }
    }
}
