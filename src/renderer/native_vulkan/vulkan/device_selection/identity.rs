//! Vulkan identity queries used by physical-device selection.

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::policy::NativeVulkanPciAddress;

#[derive(Debug, Clone, Copy)]
pub(super) struct NativeVulkanPhysicalDeviceIdentity {
    pub original_index: usize,
    pub physical_device: vk::PhysicalDevice,
    pub properties: vk::PhysicalDeviceProperties,
    pub device_uuid: [u8; vk::UUID_SIZE],
    pub pci_address: Option<NativeVulkanPciAddress>,
}

pub(super) fn query_physical_device_identity(
    instance: &Instance,
    original_index: usize,
    physical_device: vk::PhysicalDevice,
) -> Result<NativeVulkanPhysicalDeviceIdentity, String> {
    let extensions = unsafe { instance.enumerate_device_extension_properties(physical_device, None) }
        .map_err(|err| {
            format!(
                "vkEnumerateDeviceExtensionProperties(Vulkan device selection): {err:?}"
            )
        })?;
    let pci_bus_info_available = extensions.iter().any(|extension| {
        extension.extension_name.to_string_lossy() == "VK_EXT_pci_bus_info"
    });
    let mut id_properties = vk::PhysicalDeviceIDProperties::default();
    let mut pci_properties = vk::PhysicalDevicePCIBusInfoPropertiesEXT::default();
    let mut properties_builder =
        vk::PhysicalDeviceProperties2::builder().push_next(&mut id_properties);
    if pci_bus_info_available {
        properties_builder = properties_builder.push_next(&mut pci_properties);
    }
    let mut properties = properties_builder.build();
    unsafe {
        instance.get_physical_device_properties2(physical_device, &mut properties);
    }
    Ok(NativeVulkanPhysicalDeviceIdentity {
        original_index,
        physical_device,
        properties: properties.properties,
        device_uuid: id_properties.device_uuid.0,
        pci_address: pci_bus_info_available.then_some(NativeVulkanPciAddress {
            domain: pci_properties.pci_domain,
            bus: pci_properties.pci_bus,
            device: pci_properties.pci_device,
            function: pci_properties.pci_function,
        }),
    })
}
