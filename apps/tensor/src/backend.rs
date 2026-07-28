#[cfg(feature = "tty")]
mod host_map;
#[cfg(feature = "tty")]
mod output;
#[cfg(feature = "tty")]
mod tty;

#[cfg(feature = "tty")]
pub(crate) use host_map::physical_mode_from_drm;
#[cfg(feature = "tty")]
pub(crate) use output::{BackendOutputEvent, BackendOutputId, OutputDescriptor};
#[cfg(feature = "tty")]
pub(crate) use tty::{DrmDeviceFd, LibinputEvent, TtyBackend, UdevEvent, WeakDrmDeviceFd};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BackendConfig {
    pub(crate) drm_node: crate::render::DrmNodeId,
    pub(crate) renderer_formats: Vec<crate::render::VulkanFormatCapability>,
    pub(crate) output_rules: std::collections::BTreeMap<String, crate::config::OutputRule>,
}
