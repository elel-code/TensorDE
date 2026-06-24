use std::ffi::CString;

use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::loader::LibloadingLoader;
use vulkanalia::prelude::v1_4::*;

use super::queue_probe::native_vulkan_vulkanalia_has_video_decode_queue_family;

const LOADER_CANDIDATES: &[&str] = &["libvulkan.so.1", "libvulkan.so"];
const REQUIRED_INSTANCE_EXTENSIONS: &[&str] = &["VK_KHR_surface", "VK_KHR_wayland_surface"];
const REQUIRED_VIDEO_DEVICE_EXTENSIONS: &[&str] = &[
    "VK_KHR_video_queue",
    "VK_KHR_video_decode_queue",
    "VK_KHR_video_decode_h264",
    "VK_KHR_video_decode_h265",
    "VK_KHR_video_decode_av1",
];
const REQUIRED_EXTERNAL_MEMORY_DEVICE_EXTENSIONS: &[&str] = &[
    "VK_KHR_external_memory_fd",
    "VK_KHR_external_semaphore_fd",
    "VK_KHR_timeline_semaphore",
    "VK_EXT_external_memory_dma_buf",
    "VK_EXT_image_drm_format_modifier",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDeviceProbeTemplate {
    pub binding: &'static str,
    pub loader_candidates: &'static [&'static str],
    pub requested_api_version: String,
    pub required_instance_extensions: &'static [&'static str],
    pub required_video_device_extensions: &'static [&'static str],
    pub required_external_memory_device_extensions: &'static [&'static str],
    pub probe_scope: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDeviceProbeSnapshot {
    pub binding: &'static str,
    pub loader: String,
    pub entry_version: String,
    pub requested_api_version: String,
    pub available_instance_extensions: Vec<String>,
    pub enabled_instance_extensions: Vec<&'static str>,
    pub missing_instance_extensions: Vec<&'static str>,
    pub physical_device_count: usize,
    pub devices: Vec<NativeVulkanVulkanaliaPhysicalDeviceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaPhysicalDeviceSnapshot {
    pub physical_device_index: usize,
    pub physical_device_name: String,
    pub physical_device_type: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub api_version: String,
    pub driver_version: u32,
    pub device_extensions: Vec<String>,
    pub has_required_video_device_extensions: bool,
    pub has_required_external_memory_device_extensions: bool,
    pub has_video_decode_queue_family: bool,
    pub vulkan_1_4_features: NativeVulkanVulkanaliaVulkan14FeatureSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVulkan14FeatureSnapshot {
    pub dynamic_rendering_local_read: bool,
    pub maintenance5: bool,
    pub maintenance6: bool,
    pub push_descriptor: bool,
}

pub fn native_vulkan_vulkanalia_device_probe_template() -> NativeVulkanVulkanaliaDeviceProbeTemplate
{
    NativeVulkanVulkanaliaDeviceProbeTemplate {
        binding: "vulkanalia",
        loader_candidates: LOADER_CANDIDATES,
        requested_api_version: Version::V1_4_0.to_string(),
        required_instance_extensions: REQUIRED_INSTANCE_EXTENSIONS,
        required_video_device_extensions: REQUIRED_VIDEO_DEVICE_EXTENSIONS,
        required_external_memory_device_extensions: REQUIRED_EXTERNAL_MEMORY_DEVICE_EXTENSIONS,
        probe_scope: "entry/instance/physical-device capability enumeration only; no logical device, surface, swapchain or submit work",
    }
}

pub fn probe_native_vulkan_vulkanalia_devices()
-> Result<NativeVulkanVulkanaliaDeviceProbeSnapshot, String> {
    let (loader, loader_name) = load_vulkanalia_loader()?;
    let entry = unsafe { Entry::new(loader) }
        .map_err(|err| format!("vulkanalia Entry::new({loader_name}): {err}"))?;
    let entry_version = entry
        .version()
        .map_err(|err| format!("vkEnumerateInstanceVersion: {err:?}"))?;

    let available_instance_extensions =
        unsafe { entry.enumerate_instance_extension_properties(None) }
            .map_err(|err| format!("vkEnumerateInstanceExtensionProperties: {err:?}"))?
            .into_iter()
            .map(|property| property.extension_name.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
    let enabled_instance_extensions = REQUIRED_INSTANCE_EXTENSIONS
        .iter()
        .copied()
        .filter(|extension| {
            available_instance_extensions
                .iter()
                .any(|name| name == extension)
        })
        .collect::<Vec<_>>();
    let missing_instance_extensions = REQUIRED_INSTANCE_EXTENSIONS
        .iter()
        .copied()
        .filter(|extension| {
            !available_instance_extensions
                .iter()
                .any(|name| name == extension)
        })
        .collect::<Vec<_>>();

    let extension_names = enabled_instance_extensions
        .iter()
        .map(|extension| CString::new(*extension).expect("static extension name has no nul"))
        .collect::<Vec<_>>();
    let extension_name_ptrs = extension_names
        .iter()
        .map(|extension| extension.as_ptr())
        .collect::<Vec<_>>();
    let app_info = vk::ApplicationInfo::builder()
        .application_name(b"gilder-native-vulkan\0")
        .application_version(1)
        .engine_name(b"gilder\0")
        .engine_version(1)
        .api_version(u32::from(Version::V1_4_0));
    let create_info = vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_extension_names(&extension_name_ptrs);
    let instance = unsafe { entry.create_instance(&create_info, None) }
        .map_err(|err| format!("vkCreateInstance(vulkanalia): {err:?}"))?;

    let devices = probe_vulkanalia_instance_devices(&instance);
    unsafe {
        instance.destroy_instance(None);
    }

    let devices = devices?;
    Ok(NativeVulkanVulkanaliaDeviceProbeSnapshot {
        binding: "vulkanalia",
        loader: loader_name.to_owned(),
        entry_version: entry_version.to_string(),
        requested_api_version: Version::V1_4_0.to_string(),
        available_instance_extensions: sorted_strings(available_instance_extensions),
        enabled_instance_extensions,
        missing_instance_extensions,
        physical_device_count: devices.len(),
        devices,
    })
}

fn load_vulkanalia_loader() -> Result<(LibloadingLoader, &'static str), String> {
    let mut errors = Vec::new();
    for candidate in LOADER_CANDIDATES {
        match unsafe { LibloadingLoader::new(candidate) } {
            Ok(loader) => return Ok((loader, candidate)),
            Err(err) => errors.push(format!("{candidate}: {err}")),
        }
    }
    Err(format!(
        "failed to load Vulkan loader via vulkanalia: {}",
        errors.join("; ")
    ))
}

fn probe_vulkanalia_instance_devices(
    instance: &Instance,
) -> Result<Vec<NativeVulkanVulkanaliaPhysicalDeviceSnapshot>, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|err| format!("vkEnumeratePhysicalDevices(vulkanalia): {err:?}"))?;
    physical_devices
        .iter()
        .copied()
        .enumerate()
        .map(|(physical_device_index, physical_device)| {
            let properties = unsafe { instance.get_physical_device_properties(physical_device) };
            let mut vulkan14_features = vk::PhysicalDeviceVulkan14Features::default();
            let mut features2 = vk::PhysicalDeviceFeatures2::builder()
                .push_next(&mut vulkan14_features)
                .build();
            unsafe {
                instance.get_physical_device_features2(physical_device, &mut features2);
            }
            let device_extensions =
                unsafe { instance.enumerate_device_extension_properties(physical_device, None) }
                    .map_err(|err| {
                        format!("vkEnumerateDeviceExtensionProperties(vulkanalia): {err:?}")
                    })?
                    .into_iter()
                    .map(|property| property.extension_name.to_string_lossy().into_owned())
                    .collect::<Vec<_>>();

            Ok(NativeVulkanVulkanaliaPhysicalDeviceSnapshot {
                physical_device_index,
                physical_device_name: properties.device_name.to_string_lossy().into_owned(),
                physical_device_type: format!("{:?}", properties.device_type),
                vendor_id: properties.vendor_id,
                device_id: properties.device_id,
                api_version: Version::from(properties.api_version).to_string(),
                driver_version: properties.driver_version,
                has_required_video_device_extensions: has_all_extensions(
                    &device_extensions,
                    REQUIRED_VIDEO_DEVICE_EXTENSIONS,
                ),
                has_required_external_memory_device_extensions: has_all_extensions(
                    &device_extensions,
                    REQUIRED_EXTERNAL_MEMORY_DEVICE_EXTENSIONS,
                ),
                has_video_decode_queue_family:
                    native_vulkan_vulkanalia_has_video_decode_queue_family(
                        instance,
                        physical_device,
                    ),
                device_extensions: sorted_strings(device_extensions),
                vulkan_1_4_features: NativeVulkanVulkanaliaVulkan14FeatureSnapshot {
                    dynamic_rendering_local_read: vulkan14_features.dynamic_rendering_local_read
                        != 0,
                    maintenance5: vulkan14_features.maintenance5 != 0,
                    maintenance6: vulkan14_features.maintenance6 != 0,
                    push_descriptor: vulkan14_features.push_descriptor != 0,
                },
            })
        })
        .collect()
}

fn has_all_extensions(available: &[String], required: &[&str]) -> bool {
    required
        .iter()
        .all(|required| available.iter().any(|available| available == required))
}

fn sorted_strings(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_probe_template_tracks_vulkan_1_4_and_video_gates() {
        let template = native_vulkan_vulkanalia_device_probe_template();
        assert_eq!(template.binding, "vulkanalia");
        assert_eq!(template.requested_api_version, "1.4.0");
        assert!(
            template
                .required_instance_extensions
                .contains(&"VK_KHR_wayland_surface")
        );
        assert!(
            template
                .required_video_device_extensions
                .contains(&"VK_KHR_video_decode_h265")
        );
        assert!(
            template
                .required_external_memory_device_extensions
                .contains(&"VK_EXT_image_drm_format_modifier")
        );
        assert!(template.probe_scope.contains("no logical device"));
    }

    #[test]
    fn extension_gate_requires_every_extension() {
        let available = vec![
            "VK_KHR_video_queue".to_owned(),
            "VK_KHR_video_decode_queue".to_owned(),
        ];
        assert!(has_all_extensions(
            &available,
            &["VK_KHR_video_queue", "VK_KHR_video_decode_queue"]
        ));
        assert!(!has_all_extensions(
            &available,
            &["VK_KHR_video_queue", "VK_KHR_video_decode_h265"]
        ));
    }
}
