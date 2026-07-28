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

use super::core::roadmap_2026::{
    ROADMAP_2026_API_VERSION, ROADMAP_2026_PROFILE_NAME, Roadmap2026DeviceRequirementProbe,
    query_roadmap_2026_device_requirements,
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
    let positions = ordered_candidate_positions(&policy, &candidates)?;
    let mut eligible = Vec::with_capacity(positions.len());
    let mut rejected = Vec::new();
    for position in positions {
        let identity = identities[position];
        let device_extensions = unsafe {
            instance.enumerate_device_extension_properties(identity.physical_device, None)
        }
        .map_err(|err| {
            format!(
                "vkEnumerateDeviceExtensionProperties({ROADMAP_2026_PROFILE_NAME} gate): {err:?}"
            )
        })?
        .into_iter()
        .map(|property| property.extension_name.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
        let requirements = query_roadmap_2026_device_requirements(
            instance,
            identity.physical_device,
            identity.properties.api_version,
            &device_extensions,
        );
        if !requirements.ready() {
            let rejection = roadmap_2026_rejection(&identity.properties, &requirements);
            if policy.has_explicit_selector() {
                return Err(format!(
                    "explicit Vulkan device selection rejected by the mandatory {ROADMAP_2026_PROFILE_NAME} contract: {rejection}"
                ));
            }
            rejected.push(rejection);
            continue;
        }
        eligible.push(NativeVulkanRankedPhysicalDevice {
            original_index: identity.original_index,
            physical_device: identity.physical_device,
            properties: identity.properties,
        });
    }
    if eligible.is_empty() {
        return Err(format!(
            "no Vulkan physical device satisfies mandatory {ROADMAP_2026_PROFILE_NAME} revision 11 / Vulkan {ROADMAP_2026_API_VERSION}: {}",
            if rejected.is_empty() {
                "no physical devices were enumerated".to_owned()
            } else {
                rejected.join("; ")
            }
        ));
    }
    Ok(eligible)
}

fn roadmap_2026_rejection(
    properties: &vk::PhysicalDeviceProperties,
    requirements: &Roadmap2026DeviceRequirementProbe,
) -> String {
    let name = properties.device_name.to_string_lossy();
    let mut failures = Vec::new();
    if !requirements.api_version_ready {
        failures.push(format!(
            "apiVersion={} below {}",
            vulkanalia::Version::from(properties.api_version),
            ROADMAP_2026_API_VERSION
        ));
    }
    for (label, missing) in [
        (
            "device extensions",
            requirements.missing_device_extensions.as_slice(),
        ),
        (
            "core features",
            requirements.missing_core_features.as_slice(),
        ),
        ("properties", requirements.missing_properties.as_slice()),
        (
            "extension features",
            requirements.missing_extension_features.as_slice(),
        ),
    ] {
        if !missing.is_empty() {
            failures.push(format!("missing {label}: {}", missing.join(", ")));
        }
    }
    format!("{name}: {}", failures.join(" | "))
}
