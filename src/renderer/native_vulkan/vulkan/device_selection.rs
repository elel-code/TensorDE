//! Shared Vulkan physical-device policy for present, scene, and video routes.

mod identity;
mod policy;

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use identity::query_physical_device_identity;
use policy::{
    NativeVulkanDeviceCandidate, NativeVulkanDeviceSelectionPolicy,
    ordered_candidate_positions,
};

pub(super) struct NativeVulkanRankedPhysicalDevice {
    pub original_index: usize,
    pub physical_device: vk::PhysicalDevice,
    pub properties: vk::PhysicalDeviceProperties,
}

pub(super) fn ranked_physical_devices(
    instance: &Instance,
    physical_devices: &[vk::PhysicalDevice],
) -> Result<Vec<NativeVulkanRankedPhysicalDevice>, String> {
    let policy = NativeVulkanDeviceSelectionPolicy::from_environment()?;
    let mut identities = Vec::with_capacity(physical_devices.len());
    for (original_index, physical_device) in physical_devices.iter().copied().enumerate() {
        identities.push(query_physical_device_identity(
            instance,
            original_index,
            physical_device,
        )?);
    }
    let candidates = identities
        .iter()
        .map(|identity| NativeVulkanDeviceCandidate {
            original_index: identity.original_index,
            name: identity
                .properties
                .device_name
                .to_string_lossy()
                .into_owned(),
            device_type: identity.properties.device_type,
            device_uuid: identity.device_uuid,
            pci_address: identity.pci_address,
        })
        .collect::<Vec<_>>();
    ordered_candidate_positions(&policy, &candidates).map(|positions| {
        positions
            .into_iter()
            .map(|position| {
                let identity = identities[position];
                NativeVulkanRankedPhysicalDevice {
                    original_index: identity.original_index,
                    physical_device: identity.physical_device,
                    properties: identity.properties,
                }
            })
            .collect()
    })
}
