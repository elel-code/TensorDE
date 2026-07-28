use std::fmt;

use vulkanalia::{Version, vk};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    LoadLibrary(String),
    LoadEntry(String),
    LoaderVersion {
        required: Version,
        found: Version,
    },
    Vulkan {
        operation: &'static str,
        source: vk::ErrorCode,
    },
    NoPhysicalDevice,
    NoCompatibleDevice(Vec<String>),
    Validation(String),
    TimelineExhausted,
}

impl Error {
    pub(crate) const fn vulkan(operation: &'static str, source: vk::ErrorCode) -> Self {
        Self::Vulkan { operation, source }
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
            Self::Vulkan { operation, source } => write!(formatter, "{operation}: {source:?}"),
            Self::NoPhysicalDevice => formatter.write_str("no Vulkan physical device found"),
            Self::NoCompatibleDevice(rejections) => write!(
                formatter,
                "no compatible Vulkan device: {}",
                rejections.join("; ")
            ),
            Self::Validation(message) => formatter.write_str(message),
            Self::TimelineExhausted => formatter.write_str("timeline value space exhausted"),
        }
    }
}

impl std::error::Error for Error {}
