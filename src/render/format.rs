//! Output format negotiation — thin re-export of `tensor-host` pure types.
//!
//! DRM/GBM fourcc conversion lives only in backend adapters (`host_map`).

pub(crate) use tensor_host::FormatCapability as VulkanFormatCapability;
#[cfg(feature = "tty")]
pub(crate) use tensor_host::{
    GbmCapability as GbmFormatCapability, OutputFormat, negotiate_output_formats,
};
