use std::path::PathBuf;

#[cfg(feature = "tty")]
mod output;
#[cfg(feature = "tty")]
mod tty;

#[cfg(feature = "tty")]
pub(crate) use output::{BackendOutputEvent, BackendOutputId, OutputDescriptor};
#[cfg(feature = "tty")]
pub(crate) use tty::TtyBackend;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct BackendConfig {
    pub(crate) render_device: Option<PathBuf>,
}
