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
            Self::Validation(message) => formatter.write_str(message),
            Self::VideoDecode(message) => write!(formatter, "video decode: {message}"),
            Self::TimelineExhausted => formatter.write_str("timeline value space exhausted"),
        }
    }
}

impl std::error::Error for Error {}
