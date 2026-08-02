//! Stable version and adapter-class values at the renderer boundary.

use std::fmt;

use vulkanalia::{Version, vk};

/// Vulkan API version without exposing the binding crate to renderer users.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ApiVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl ApiVersion {
    pub const V1_0_0: Self = Self::new(1, 0, 0);
    pub const V1_1_0: Self = Self::new(1, 1, 0);
    pub const V1_2_0: Self = Self::new(1, 2, 0);
    pub const V1_3_0: Self = Self::new(1, 3, 0);
    pub const V1_4_0: Self = Self::new(1, 4, 0);

    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub(crate) const fn from_vk(version: Version) -> Self {
        Self::new(version.major, version.minor, version.patch)
    }

    pub(crate) const fn from_raw(version: u32) -> Self {
        Self::new(
            vk::version_major(version),
            vk::version_minor(version),
            vk::version_patch(version),
        )
    }

    pub(crate) const fn to_raw(self) -> u32 {
        vk::make_version(self.major, self.minor, self.patch)
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Physical-device class used for deterministic adapter ranking.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeviceType {
    Discrete,
    Integrated,
    Virtual,
    Other,
    Cpu,
    Unknown,
}

impl DeviceType {
    pub(crate) const fn from_vk(device_type: vk::PhysicalDeviceType) -> Self {
        match device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => Self::Discrete,
            vk::PhysicalDeviceType::INTEGRATED_GPU => Self::Integrated,
            vk::PhysicalDeviceType::VIRTUAL_GPU => Self::Virtual,
            vk::PhysicalDeviceType::OTHER => Self::Other,
            vk::PhysicalDeviceType::CPU => Self::Cpu,
            _ => Self::Unknown,
        }
    }
}
