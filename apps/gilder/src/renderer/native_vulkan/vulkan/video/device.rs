#![allow(dead_code)]

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::renderer::native_vulkan::NativeVulkanVideoSessionCodec;

use super::features::{
    DESCRIPTOR_HEAP_EXTENSION_NAME, NativeVulkanVulkanaliaCoreFeatureSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    native_vulkan_vulkanalia_core_feature_snapshot,
};

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
pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaVideoDecodeDeviceExtensionPlan
{
    pub required_device_extensions: Vec<&'static str>,
    pub enabled_device_extensions: Vec<&'static str>,
}

impl NativeVulkanVulkanaliaVideoDecodeDeviceExtensionPlan {
    fn for_codecs(
        codecs: &[NativeVulkanVideoSessionCodec],
        feature_selection: NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
    ) -> Result<Self, String> {
        let required_device_extensions =
            native_vulkan_vulkanalia_video_decode_required_device_extensions_for_codecs(codecs)?;
        let mut enabled_device_extensions = required_device_extensions.clone();
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
        Ok(Self {
            required_device_extensions,
            enabled_device_extensions,
        })
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_video_decode_required_device_extensions(
    codec: NativeVulkanVideoSessionCodec,
) -> Vec<&'static str> {
    native_vulkan_vulkanalia_video_decode_required_device_extensions_for_codecs(&[codec])
        .expect("single codec extension set is non-empty")
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_video_decode_required_device_extensions_for_codecs(
    codecs: &[NativeVulkanVideoSessionCodec],
) -> Result<Vec<&'static str>, String> {
    if codecs.is_empty() {
        return Err("Vulkan Video decode device requires at least one codec".to_owned());
    }
    let mut extensions = vec![
        VIDEO_QUEUE_EXTENSION_NAME,
        VIDEO_DECODE_QUEUE_EXTENSION_NAME,
    ];
    for codec in codecs {
        let codec_extension = match codec {
            NativeVulkanVideoSessionCodec::H264High8 => VIDEO_DECODE_H264_EXTENSION_NAME,
            NativeVulkanVideoSessionCodec::H265Main8
            | NativeVulkanVideoSessionCodec::H265Main10 => VIDEO_DECODE_H265_EXTENSION_NAME,
            NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
                VIDEO_DECODE_AV1_EXTENSION_NAME
            }
        };
        if !extensions.contains(&codec_extension) {
            extensions.push(codec_extension);
        }
    }
    Ok(extensions)
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_video_decode_device_extensions(
    codec: NativeVulkanVideoSessionCodec,
    feature_selection: NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
) -> Vec<&'static str> {
    native_vulkan_vulkanalia_video_decode_device_extensions_for_codecs(&[codec], feature_selection)
        .expect("single codec extension set is non-empty")
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_video_decode_device_extension_plan_for_codecs(
    codecs: &[NativeVulkanVideoSessionCodec],
    feature_selection: NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
) -> Result<NativeVulkanVulkanaliaVideoDecodeDeviceExtensionPlan, String> {
    NativeVulkanVulkanaliaVideoDecodeDeviceExtensionPlan::for_codecs(codecs, feature_selection)
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_video_decode_device_extensions_for_codecs(
    codecs: &[NativeVulkanVideoSessionCodec],
    feature_selection: NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
) -> Result<Vec<&'static str>, String> {
    native_vulkan_vulkanalia_video_decode_device_extension_plan_for_codecs(
        codecs,
        feature_selection,
    )
    .map(|plan| plan.enabled_device_extensions)
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
    fn required_video_decode_extensions_support_mixed_codec_sets() {
        assert_eq!(
            native_vulkan_vulkanalia_video_decode_required_device_extensions_for_codecs(&[
                NativeVulkanVideoSessionCodec::H264High8,
                NativeVulkanVideoSessionCodec::H265Main10,
                NativeVulkanVideoSessionCodec::Av1Main10,
                NativeVulkanVideoSessionCodec::H265Main8,
            ])
            .unwrap(),
            vec![
                "VK_KHR_video_queue",
                "VK_KHR_video_decode_queue",
                "VK_KHR_video_decode_h264",
                "VK_KHR_video_decode_h265",
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
        assert!(
            native_vulkan_vulkanalia_video_decode_device_extensions_for_codecs(
                &[
                    NativeVulkanVideoSessionCodec::H264High8,
                    NativeVulkanVideoSessionCodec::Av1Main8,
                ],
                descriptor_heap_enabled,
            )
            .unwrap()
            .contains(&DESCRIPTOR_HEAP_EXTENSION_NAME)
        );
        let mixed_plan = native_vulkan_vulkanalia_video_decode_device_extension_plan_for_codecs(
            &[
                NativeVulkanVideoSessionCodec::H264High8,
                NativeVulkanVideoSessionCodec::H265Main8,
                NativeVulkanVideoSessionCodec::Av1Main8,
            ],
            descriptor_heap_enabled,
        )
        .unwrap();
        assert_eq!(
            mixed_plan.required_device_extensions,
            vec![
                "VK_KHR_video_queue",
                "VK_KHR_video_decode_queue",
                "VK_KHR_video_decode_h264",
                "VK_KHR_video_decode_h265",
                "VK_KHR_video_decode_av1",
            ]
        );
        assert!(
            mixed_plan
                .enabled_device_extensions
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
