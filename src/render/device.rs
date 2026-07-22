use std::str::FromStr;

use thiserror::Error;
use vulkanalia::vk;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GpuPreference {
    #[default]
    Discrete,
    Integrated,
    Any,
}

impl GpuPreference {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Discrete => "discrete",
            Self::Integrated => "integrated",
            Self::Any => "any",
        }
    }
}

impl FromStr for GpuPreference {
    type Err = ParseGpuPreferenceError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "discrete" => Ok(Self::Discrete),
            "integrated" => Ok(Self::Integrated),
            "any" => Ok(Self::Any),
            _ => Err(ParseGpuPreferenceError(value.to_owned())),
        }
    }
}

#[derive(Debug, Error)]
#[error("unknown GPU preference '{0}'; expected discrete, integrated, or any")]
pub struct ParseGpuPreferenceError(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceCandidate {
    pub ordinal: usize,
    pub name: String,
    pub device_type: vk::PhysicalDeviceType,
    pub descriptor_heap_supported: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceSelector {
    preference: GpuPreference,
}

impl DeviceSelector {
    pub const fn new(preference: GpuPreference) -> Self {
        Self { preference }
    }

    pub const fn preference(self) -> GpuPreference {
        self.preference
    }

    pub fn select<'a>(
        self,
        candidates: impl IntoIterator<Item = &'a DeviceCandidate>,
    ) -> Result<&'a DeviceCandidate, DeviceSelectionError> {
        candidates
            .into_iter()
            .filter(|candidate| candidate.descriptor_heap_supported)
            .min_by_key(|candidate| (self.rank(candidate.device_type), candidate.ordinal))
            .ok_or(DeviceSelectionError::NoDescriptorHeapDevice)
    }

    fn rank(self, device_type: vk::PhysicalDeviceType) -> u8 {
        match self.preference {
            GpuPreference::Discrete => match device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => 0,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
                vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                vk::PhysicalDeviceType::OTHER => 3,
                vk::PhysicalDeviceType::CPU => 4,
                _ => 5,
            },
            GpuPreference::Integrated => match device_type {
                vk::PhysicalDeviceType::INTEGRATED_GPU => 0,
                vk::PhysicalDeviceType::DISCRETE_GPU => 1,
                vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                vk::PhysicalDeviceType::OTHER => 3,
                vk::PhysicalDeviceType::CPU => 4,
                _ => 5,
            },
            GpuPreference::Any => match device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU | vk::PhysicalDeviceType::INTEGRATED_GPU => 0,
                vk::PhysicalDeviceType::VIRTUAL_GPU => 1,
                vk::PhysicalDeviceType::OTHER => 2,
                vk::PhysicalDeviceType::CPU => 3,
                _ => 4,
            },
        }
    }
}

#[derive(Debug, Error)]
pub enum DeviceSelectionError {
    #[error("no Vulkan device supports the required VK_EXT_descriptor_heap feature")]
    NoDescriptorHeapDevice,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        ordinal: usize,
        device_type: vk::PhysicalDeviceType,
        heap: bool,
    ) -> DeviceCandidate {
        DeviceCandidate {
            ordinal,
            name: format!("device-{ordinal}"),
            device_type,
            descriptor_heap_supported: heap,
        }
    }

    #[test]
    fn default_prefers_discrete_gpu_with_heap() {
        let candidates = [
            candidate(0, vk::PhysicalDeviceType::CPU, true),
            candidate(1, vk::PhysicalDeviceType::DISCRETE_GPU, true),
            candidate(2, vk::PhysicalDeviceType::INTEGRATED_GPU, true),
        ];

        assert_eq!(
            DeviceSelector::new(GpuPreference::Discrete)
                .select(&candidates)
                .unwrap()
                .ordinal,
            1
        );
    }

    #[test]
    fn unsupported_heap_devices_are_never_selected() {
        let candidates = [
            candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, false),
            candidate(1, vk::PhysicalDeviceType::CPU, false),
        ];

        assert!(matches!(
            DeviceSelector::new(GpuPreference::Any).select(&candidates),
            Err(DeviceSelectionError::NoDescriptorHeapDevice)
        ));
    }
}
