use std::fmt;

use vulkanalia::vk;

use crate::ApiVersion;

pub type Result<T> = std::result::Result<T, Error>;

/// Stable classification for a Vulkan operation failure.
///
/// The shared renderer keeps Vulkanalia's error value internally. Products
/// can make recovery decisions without importing raw Vulkan error codes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VulkanFailure {
    DeviceLost,
    SurfaceOutOfDate,
    Other(String),
}

impl VulkanFailure {
    fn from_vk(source: vk::ErrorCode) -> Self {
        match source {
            vk::ErrorCode::DEVICE_LOST => Self::DeviceLost,
            vk::ErrorCode::OUT_OF_DATE_KHR => Self::SurfaceOutOfDate,
            _ => Self::Other(format!("{source:?}")),
        }
    }
}

impl fmt::Display for VulkanFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceLost => formatter.write_str("device lost"),
            Self::SurfaceOutOfDate => formatter.write_str("surface out of date"),
            Self::Other(source) => formatter.write_str(source),
        }
    }
}

/// The bounded upload-belt limit that prevented a staging allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadBeltLimit {
    ChunkCount(usize),
    RetainedBytes(u64),
}

#[derive(Debug)]
pub enum Error {
    LoadLibrary(String),
    LoadEntry(String),
    LoaderVersion {
        required: ApiVersion,
        found: ApiVersion,
    },
    Vulkan {
        operation: &'static str,
        source: VulkanFailure,
    },
    NoPhysicalDevice,
    NoCompatibleDevice(Vec<String>),
    UploadBeltExhausted {
        limit: UploadBeltLimit,
    },
    Validation(String),
    VideoDecode(String),
    TimelineExhausted,
}

impl Error {
    pub(crate) fn vulkan(operation: &'static str, source: vk::ErrorCode) -> Self {
        Self::Vulkan {
            operation,
            source: VulkanFailure::from_vk(source),
        }
    }

    /// True when the operation failed because the device is no longer usable.
    pub const fn is_device_lost(&self) -> bool {
        matches!(
            self,
            Self::Vulkan {
                source: VulkanFailure::DeviceLost,
                ..
            }
        )
    }

    /// True when the WSI surface must be recreated before retrying.
    pub const fn is_surface_out_of_date(&self) -> bool {
        matches!(
            self,
            Self::Vulkan {
                source: VulkanFailure::SurfaceOutOfDate,
                ..
            }
        )
    }

    /// True when a bounded staging allocation needs an explicit streamed
    /// submission before it can be retried.
    pub const fn is_upload_belt_exhausted(&self) -> bool {
        matches!(self, Self::UploadBeltExhausted { .. })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadLibrary(error) => write!(formatter, "load Vulkan library: {error}"),
            Self::LoadEntry(error) => write!(formatter, "load Vulkan entry: {error}"),
            Self::LoaderVersion { required, found } => write!(
                formatter,
                "Vulkan loader {found} is below required version {required}"
            ),
            Self::Vulkan { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::NoPhysicalDevice => formatter.write_str("no Vulkan physical device found"),
            Self::NoCompatibleDevice(rejections) => write!(
                formatter,
                "no compatible Vulkan device: {}",
                rejections.join("; ")
            ),
            Self::UploadBeltExhausted {
                limit: UploadBeltLimit::ChunkCount(max_chunks),
            } => write!(
                formatter,
                "upload belt exhausted its {max_chunks}-chunk memory bound"
            ),
            Self::UploadBeltExhausted {
                limit: UploadBeltLimit::RetainedBytes(max_bytes),
            } => write!(
                formatter,
                "upload belt exhausted its {max_bytes}-byte memory bound"
            ),
            Self::Validation(message) => formatter.write_str(message),
            Self::VideoDecode(message) => write!(formatter, "video decode: {message}"),
            Self::TimelineExhausted => formatter.write_str("timeline value space exhausted"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_belt_exhaustion_is_machine_classified() {
        let error = Error::UploadBeltExhausted {
            limit: UploadBeltLimit::ChunkCount(8),
        };

        assert!(error.is_upload_belt_exhausted());
        assert_eq!(
            error.to_string(),
            "upload belt exhausted its 8-chunk memory bound"
        );
    }
}
