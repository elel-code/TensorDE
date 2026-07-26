#[cfg(feature = "tty")]
mod host_map;
#[cfg(feature = "tty")]
mod output;
#[cfg(feature = "tty")]
mod tty;

#[cfg(feature = "tty")]
pub(crate) use host_map::{
    host_drm_format, physical_mode_from_smithay, smithay_drm_format, smithay_mode, smithay_subpixel,
};
#[cfg(feature = "tty")]
pub(crate) use output::{BackendOutputEvent, BackendOutputId, OutputDescriptor};
#[cfg(feature = "tty")]
pub(crate) use tty::TtyBackend;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BackendConfig {
    pub(crate) drm_node: crate::render::DrmNodeId,
    pub(crate) renderer_formats: Vec<crate::render::VulkanFormatCapability>,
    pub(crate) output_rules: std::collections::BTreeMap<String, crate::config::OutputRule>,
}
