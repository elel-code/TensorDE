#[cfg(feature = "tty")]
mod output;
#[cfg(feature = "tty")]
mod tty;

#[cfg(feature = "tty")]
pub(crate) use output::{BackendOutputEvent, BackendOutputId, OutputDescriptor};
#[cfg(feature = "tty")]
pub(crate) use tty::TtyBackend;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BackendConfig {
    pub(crate) drm_node: crate::render::DrmNodeId,
    pub(crate) renderer_formats: Vec<crate::render::VulkanFormatCapability>,
}
