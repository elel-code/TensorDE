#![allow(dead_code)]

use std::ffi::CString;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::renderer::native_vulkan::NativeVulkanVideoSessionCodec;

use super::features::{
    DESCRIPTOR_HEAP_EXTENSION_NAME, NativeVulkanVulkanaliaCoreFeatureSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    native_vulkan_vulkanalia_core_feature_snapshot,
    native_vulkan_vulkanalia_descriptor_heap_device_features,
    native_vulkan_vulkanalia_vulkan12_device_features,
    native_vulkan_vulkanalia_vulkan13_device_features,
    native_vulkan_vulkanalia_vulkan14_device_features,
};
use super::queue_probe::native_vulkan_vulkanalia_video_decode_queue_family_indices;
use super::video_codec::native_vulkan_vulkanalia_video_decode_codec_label;

pub(in crate::renderer::native_vulkan::vulkan) const VIDEO_MAINTENANCE1_EXTENSION_NAME: &str =
    "VK_KHR_video_maintenance1";
pub(in crate::renderer::native_vulkan::vulkan) const VIDEO_MAINTENANCE2_EXTENSION_NAME: &str =
    "VK_KHR_video_maintenance2";

const VIDEO_QUEUE_EXTENSION_NAME: &str = "VK_KHR_video_queue";
const VIDEO_DECODE_QUEUE_EXTENSION_NAME: &str = "VK_KHR_video_decode_queue";
const VIDEO_DECODE_H264_EXTENSION_NAME: &str = "VK_KHR_video_decode_h264";
const VIDEO_DECODE_H265_EXTENSION_NAME: &str = "VK_KHR_video_decode_h265";
const VIDEO_DECODE_AV1_EXTENSION_NAME: &str = "VK_KHR_video_decode_av1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaVideoDeviceFeatureSelection
{
    pub core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
    pub descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    pub synchronization2_enabled: bool,
    pub dynamic_rendering_enabled: bool,
    pub video_maintenance1_enabled: bool,
    pub video_maintenance2_enabled: bool,
    pub inline_session_parameters_enabled: bool,
}

impl NativeVulkanVulkanaliaVideoDeviceFeatureSelection {
    pub(in crate::renderer::native_vulkan::vulkan) fn inline_session_parameter_codecs(
        self,
    ) -> Vec<&'static str> {
        if self.inline_session_parameters_enabled {
            vec!["h264", "h265", "av1"]
        } else {
            Vec::new()
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaVideoPhysicalDeviceSelection
{
    pub(in crate::renderer::native_vulkan::vulkan) physical_device_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) physical_device: vk::PhysicalDevice,
    pub(in crate::renderer::native_vulkan::vulkan) properties: vk::PhysicalDeviceProperties,
    pub(in crate::renderer::native_vulkan::vulkan) queue_family_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) queue_count: u32,
    pub(in crate::renderer::native_vulkan::vulkan) queue_flags: vk::QueueFlags,
    pub(in crate::renderer::native_vulkan::vulkan) device_extensions: Vec<String>,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaVideoDecodeDevice {
    pub(in crate::renderer::native_vulkan::vulkan) device: Device,
    pub(in crate::renderer::native_vulkan::vulkan) queue: vk::Queue,
    pub(in crate::renderer::native_vulkan::vulkan) enabled_device_extensions: Vec<&'static str>,
    pub(in crate::renderer::native_vulkan::vulkan) feature_selection:
        NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_video_decode_device(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    queue_family_index: u32,
    codec: NativeVulkanVideoSessionCodec,
    device_extensions: &[String],
    require_decode_submit: bool,
) -> Result<VulkanaliaVideoDecodeDevice, String> {
    let priorities = [1.0_f32];
    let queue_create_info = vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(queue_family_index)
        .queue_priorities(&priorities)
        .build();
    let queue_create_infos = [queue_create_info];
    let feature_selection = native_vulkan_vulkanalia_video_device_feature_selection(
        instance,
        physical_device,
        device_extensions,
    );
    if require_decode_submit && !feature_selection.synchronization2_enabled {
        return Err(
            "Vulkanalia ready-prefix decode submit requires synchronization2 for CmdPipelineBarrier2/QueueSubmit2"
                .to_owned(),
        );
    }

    let enabled_device_extensions =
        native_vulkan_vulkanalia_video_decode_device_extensions(codec, feature_selection);
    let extension_names = enabled_device_extensions
        .iter()
        .map(|extension| CString::new(*extension).expect("static extension name has no nul"))
        .collect::<Vec<_>>();
    let extension_name_ptrs = extension_names
        .iter()
        .map(|extension| extension.as_ptr())
        .collect::<Vec<_>>();
    let mut vulkan12_features =
        native_vulkan_vulkanalia_vulkan12_device_features(feature_selection.core_features);
    let mut vulkan13_features =
        native_vulkan_vulkanalia_vulkan13_device_features(feature_selection.core_features);
    let mut vulkan14_features =
        native_vulkan_vulkanalia_vulkan14_device_features(feature_selection.core_features);
    let mut descriptor_heap_features =
        native_vulkan_vulkanalia_descriptor_heap_device_features(feature_selection.core_features);
    let mut video_maintenance1_features = vk::PhysicalDeviceVideoMaintenance1FeaturesKHR::builder()
        .video_maintenance1(true)
        .build();
    let mut video_maintenance2_features = vk::PhysicalDeviceVideoMaintenance2FeaturesKHR::builder()
        .video_maintenance2(true)
        .build();
    let mut device_create_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_name_ptrs);
    if feature_selection
        .core_features
        .enables_vulkan_1_2_features()
    {
        device_create_info = device_create_info.push_next(&mut vulkan12_features);
    }
    if feature_selection
        .core_features
        .enables_vulkan_1_3_features()
    {
        device_create_info = device_create_info.push_next(&mut vulkan13_features);
    }
    if feature_selection
        .core_features
        .enables_vulkan_1_4_features()
    {
        device_create_info = device_create_info.push_next(&mut vulkan14_features);
    }
    if feature_selection
        .core_features
        .enables_descriptor_heap_features()
    {
        device_create_info = device_create_info.push_next(&mut descriptor_heap_features);
    }
    if feature_selection.video_maintenance1_enabled {
        device_create_info = device_create_info.push_next(&mut video_maintenance1_features);
    }
    if feature_selection.video_maintenance2_enabled {
        device_create_info = device_create_info.push_next(&mut video_maintenance2_features);
    }

    let device = unsafe { instance.create_device(physical_device, &device_create_info, None) }
        .map_err(|err| format!("vkCreateDevice(vulkanalia video decode device): {err:?}"))?;
    let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

    Ok(VulkanaliaVideoDecodeDevice {
        device,
        queue,
        enabled_device_extensions,
        feature_selection,
    })
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_video_decode_device(
    device: VulkanaliaVideoDecodeDevice,
) {
    unsafe {
        device.device.destroy_device(None);
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_select_video_decode_physical_device(
    instance: &Instance,
    codec: NativeVulkanVideoSessionCodec,
) -> Result<NativeVulkanVulkanaliaVideoPhysicalDeviceSelection, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|err| format!("vkEnumeratePhysicalDevices(vulkanalia video decode): {err:?}"))?;
    let required_extensions =
        native_vulkan_vulkanalia_video_decode_required_device_extensions(codec);
    let mut rejected = Vec::new();

    for (physical_device_index, physical_device) in physical_devices.iter().copied().enumerate() {
        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let device_extensions =
            unsafe { instance.enumerate_device_extension_properties(physical_device, None) }
                .map_err(|err| {
                    format!(
                        "vkEnumerateDeviceExtensionProperties(vulkanalia video decode): {err:?}"
                    )
                })?
                .into_iter()
                .map(|property| property.extension_name.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
        let missing_extensions = required_extensions
            .iter()
            .copied()
            .filter(|required| {
                !native_vulkan_vulkanalia_video_device_extension_available(
                    &device_extensions,
                    required,
                )
            })
            .collect::<Vec<_>>();
        if !missing_extensions.is_empty() {
            rejected.push(format!(
                "{} missing {}",
                properties.device_name.to_string_lossy(),
                missing_extensions.join(", ")
            ));
            continue;
        }

        let queue_family_indices =
            native_vulkan_vulkanalia_video_decode_queue_family_indices(instance, physical_device);
        let Some(queue_family_index) = queue_family_indices.first().copied() else {
            rejected.push(format!(
                "{} has no VIDEO_DECODE_KHR queue family",
                properties.device_name.to_string_lossy()
            ));
            continue;
        };
        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let queue_family = queue_families
            .get(queue_family_index as usize)
            .ok_or_else(|| format!("selected invalid queue family index {queue_family_index}"))?;

        return Ok(NativeVulkanVulkanaliaVideoPhysicalDeviceSelection {
            physical_device_index,
            physical_device,
            properties,
            queue_family_index,
            queue_count: queue_family.queue_count,
            queue_flags: queue_family.queue_flags,
            device_extensions,
        });
    }

    Err(format!(
        "no Vulkanalia physical device can create {} video decode session{}",
        native_vulkan_vulkanalia_video_decode_codec_label(codec),
        if rejected.is_empty() {
            String::new()
        } else {
            format!(": {}", rejected.join("; "))
        }
    ))
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_video_decode_required_device_extensions(
    codec: NativeVulkanVideoSessionCodec,
) -> Vec<&'static str> {
    let mut extensions = vec![
        VIDEO_QUEUE_EXTENSION_NAME,
        VIDEO_DECODE_QUEUE_EXTENSION_NAME,
    ];
    extensions.push(match codec {
        NativeVulkanVideoSessionCodec::H264High8 => VIDEO_DECODE_H264_EXTENSION_NAME,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            VIDEO_DECODE_H265_EXTENSION_NAME
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            VIDEO_DECODE_AV1_EXTENSION_NAME
        }
    });
    extensions
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_video_decode_device_extensions(
    codec: NativeVulkanVideoSessionCodec,
    feature_selection: NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
) -> Vec<&'static str> {
    let mut enabled_device_extensions =
        native_vulkan_vulkanalia_video_decode_required_device_extensions(codec);
    if feature_selection.video_maintenance1_enabled
        && !enabled_device_extensions.contains(&VIDEO_MAINTENANCE1_EXTENSION_NAME)
    {
        enabled_device_extensions.push(VIDEO_MAINTENANCE1_EXTENSION_NAME);
    }
    if feature_selection.video_maintenance2_enabled
        && !enabled_device_extensions.contains(&VIDEO_MAINTENANCE2_EXTENSION_NAME)
    {
        enabled_device_extensions.push(VIDEO_MAINTENANCE2_EXTENSION_NAME);
    }
    if feature_selection.core_features.descriptor_heap
        && !enabled_device_extensions.contains(&DESCRIPTOR_HEAP_EXTENSION_NAME)
    {
        enabled_device_extensions.push(DESCRIPTOR_HEAP_EXTENSION_NAME);
    }
    enabled_device_extensions
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_video_device_feature_selection(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
) -> NativeVulkanVulkanaliaVideoDeviceFeatureSelection {
    let (mut core_features, _, descriptor_heap_properties) =
        native_vulkan_vulkanalia_core_feature_snapshot(instance, physical_device);
    if !native_vulkan_vulkanalia_video_device_extension_available(
        device_extensions,
        DESCRIPTOR_HEAP_EXTENSION_NAME,
    ) {
        core_features.descriptor_heap = false;
        core_features.descriptor_heap_capture_replay = false;
    }
    let synchronization2_enabled = core_features.synchronization2;
    let dynamic_rendering_enabled = core_features.dynamic_rendering;
    let video_maintenance1_enabled =
        native_vulkan_vulkanalia_video_device_extension_available(
            device_extensions,
            VIDEO_MAINTENANCE1_EXTENSION_NAME,
        ) && query_vulkanalia_video_maintenance1_feature(instance, physical_device);
    let video_maintenance2_enabled = video_maintenance1_enabled
        && native_vulkan_vulkanalia_video_device_extension_available(
            device_extensions,
            VIDEO_MAINTENANCE2_EXTENSION_NAME,
        )
        && query_vulkanalia_video_maintenance2_feature(instance, physical_device);

    NativeVulkanVulkanaliaVideoDeviceFeatureSelection {
        core_features,
        descriptor_heap_properties,
        synchronization2_enabled,
        dynamic_rendering_enabled,
        video_maintenance1_enabled,
        video_maintenance2_enabled,
        inline_session_parameters_enabled: video_maintenance2_enabled,
    }
}

fn query_vulkanalia_video_maintenance1_feature(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> bool {
    let mut feature = vk::PhysicalDeviceVideoMaintenance1FeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.video_maintenance1 != 0
}

fn query_vulkanalia_video_maintenance2_feature(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> bool {
    let mut feature = vk::PhysicalDeviceVideoMaintenance2FeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.video_maintenance2 != 0
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_video_device_extension_available(
    device_extensions: &[String],
    extension: &str,
) -> bool {
    device_extensions
        .iter()
        .any(|available| available == extension)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_video_decode_extensions_follow_codec_family() {
        assert_eq!(
            native_vulkan_vulkanalia_video_decode_required_device_extensions(
                NativeVulkanVideoSessionCodec::H264High8,
            ),
            vec![
                "VK_KHR_video_queue",
                "VK_KHR_video_decode_queue",
                "VK_KHR_video_decode_h264",
            ]
        );
        assert_eq!(
            native_vulkan_vulkanalia_video_decode_required_device_extensions(
                NativeVulkanVideoSessionCodec::H265Main10,
            ),
            vec![
                "VK_KHR_video_queue",
                "VK_KHR_video_decode_queue",
                "VK_KHR_video_decode_h265",
            ]
        );
        assert_eq!(
            native_vulkan_vulkanalia_video_decode_required_device_extensions(
                NativeVulkanVideoSessionCodec::Av1Main10,
            ),
            vec![
                "VK_KHR_video_queue",
                "VK_KHR_video_decode_queue",
                "VK_KHR_video_decode_av1",
            ]
        );
    }

    #[test]
    fn enabled_extensions_add_video_maintenance_when_features_are_selected() {
        let disabled = NativeVulkanVulkanaliaVideoDeviceFeatureSelection {
            core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot {
                synchronization2: true,
                ..NativeVulkanVulkanaliaCoreFeatureSnapshot::default()
            },
            descriptor_heap_properties:
                NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default(),
            synchronization2_enabled: true,
            dynamic_rendering_enabled: false,
            video_maintenance1_enabled: false,
            video_maintenance2_enabled: false,
            inline_session_parameters_enabled: false,
        };
        let enabled = NativeVulkanVulkanaliaVideoDeviceFeatureSelection {
            video_maintenance1_enabled: true,
            video_maintenance2_enabled: true,
            inline_session_parameters_enabled: true,
            ..disabled
        };
        let descriptor_heap_enabled = NativeVulkanVulkanaliaVideoDeviceFeatureSelection {
            core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot {
                synchronization2: true,
                descriptor_heap: true,
                ..NativeVulkanVulkanaliaCoreFeatureSnapshot::default()
            },
            ..disabled
        };

        assert!(
            !native_vulkan_vulkanalia_video_decode_device_extensions(
                NativeVulkanVideoSessionCodec::H265Main8,
                disabled,
            )
            .contains(&VIDEO_MAINTENANCE2_EXTENSION_NAME)
        );
        assert!(
            native_vulkan_vulkanalia_video_decode_device_extensions(
                NativeVulkanVideoSessionCodec::H265Main8,
                enabled,
            )
            .contains(&VIDEO_MAINTENANCE2_EXTENSION_NAME)
        );
        assert!(
            native_vulkan_vulkanalia_video_decode_device_extensions(
                NativeVulkanVideoSessionCodec::H265Main8,
                descriptor_heap_enabled,
            )
            .contains(&DESCRIPTOR_HEAP_EXTENSION_NAME)
        );
        assert_eq!(
            enabled.inline_session_parameter_codecs(),
            vec!["h264", "h265", "av1"]
        );
    }

    #[test]
    fn video_device_extension_lookup_uses_exact_names() {
        let extensions = vec![
            VIDEO_MAINTENANCE1_EXTENSION_NAME.to_owned(),
            VIDEO_MAINTENANCE2_EXTENSION_NAME.to_owned(),
        ];

        assert!(native_vulkan_vulkanalia_video_device_extension_available(
            &extensions,
            VIDEO_MAINTENANCE1_EXTENSION_NAME
        ));
        assert!(!native_vulkan_vulkanalia_video_device_extension_available(
            &extensions,
            "VK_KHR_video_maintenance"
        ));
    }
}
