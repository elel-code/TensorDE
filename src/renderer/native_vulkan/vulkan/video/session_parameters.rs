use crate::renderer::native_vulkan::NativeVulkanVideoSessionCodec;
use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder, KhrVideoQueueExtensionDeviceCommands};

use super::video_codec::native_vulkan_vulkanalia_video_session_label;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
    pub requested: bool,
    pub supported: bool,
    pub created: bool,
    pub destroyed: bool,
    pub error: Option<String>,
    pub parameters: NativeVulkanVulkanaliaVideoSessionParametersSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionParametersSnapshot {
    pub codec: &'static str,
    pub source: &'static str,
    pub max_std_vps_count: u32,
    pub max_std_sps_count: u32,
    pub max_std_pps_count: u32,
    pub std_vps_count: u32,
    pub std_sps_count: u32,
    pub std_pps_count: u32,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaVideoSessionParameters {
    pub(in crate::renderer::native_vulkan::vulkan) parameters: vk::VideoSessionParametersKHR,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaVideoSessionParametersSnapshot,
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_smoke_create_empty_video_session_parameters(
    device: &Device,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
) -> NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => {
            native_vulkan_vulkanalia_smoke_create_empty_h264_video_session_parameters(
                device, session,
            )
        }
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            native_vulkan_vulkanalia_smoke_create_empty_h265_video_session_parameters(
                device, session, codec,
            )
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            native_vulkan_vulkanalia_unsupported_av1_empty_video_session_parameters(codec)
        }
    }
}

fn native_vulkan_vulkanalia_unsupported_av1_empty_video_session_parameters(
    codec: NativeVulkanVideoSessionCodec,
) -> NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
    NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
        requested: true,
        supported: false,
        created: false,
        destroyed: false,
        error: Some("AV1 Vulkanalia session parameters require a real sequence header".to_owned()),
        parameters: NativeVulkanVulkanaliaVideoSessionParametersSnapshot {
            codec: vulkanalia_session_parameters_codec_label(codec),
            source: "av1-sequence-header-required",
            max_std_vps_count: 0,
            max_std_sps_count: 0,
            max_std_pps_count: 0,
            std_vps_count: 0,
            std_sps_count: 0,
            std_pps_count: 0,
        },
    }
}

fn native_vulkan_vulkanalia_smoke_create_empty_h264_video_session_parameters(
    device: &Device,
    session: vk::VideoSessionKHR,
) -> NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
    let max_std_sps_count = 32;
    let max_std_pps_count = 32;
    let mut h264_create_info = vk::VideoDecodeH264SessionParametersCreateInfoKHR::builder()
        .max_std_sps_count(max_std_sps_count)
        .max_std_pps_count(max_std_pps_count)
        .build();
    let create_info = vk::VideoSessionParametersCreateInfoKHR::builder()
        .video_session(session)
        .push_next(&mut h264_create_info)
        .build();
    native_vulkan_vulkanalia_smoke_create_video_session_parameters(
        device,
        &create_info,
        NativeVulkanVulkanaliaVideoSessionParametersSnapshot {
            codec: "h264-high-8",
            source: "empty-h264-session-parameter-capacity-smoke",
            max_std_vps_count: 0,
            max_std_sps_count,
            max_std_pps_count,
            std_vps_count: 0,
            std_sps_count: 0,
            std_pps_count: 0,
        },
        "vulkanalia empty h264 session parameters",
    )
}

fn native_vulkan_vulkanalia_smoke_create_empty_h265_video_session_parameters(
    device: &Device,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
) -> NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
    let max_std_vps_count = 32;
    let max_std_sps_count = 32;
    let max_std_pps_count = 64;
    let mut h265_create_info = vk::VideoDecodeH265SessionParametersCreateInfoKHR::builder()
        .max_std_vps_count(max_std_vps_count)
        .max_std_sps_count(max_std_sps_count)
        .max_std_pps_count(max_std_pps_count)
        .build();
    let create_info = vk::VideoSessionParametersCreateInfoKHR::builder()
        .video_session(session)
        .push_next(&mut h265_create_info)
        .build();
    native_vulkan_vulkanalia_smoke_create_video_session_parameters(
        device,
        &create_info,
        NativeVulkanVulkanaliaVideoSessionParametersSnapshot {
            codec: vulkanalia_session_parameters_codec_label(codec),
            source: "empty-h265-session-parameter-capacity-smoke",
            max_std_vps_count,
            max_std_sps_count,
            max_std_pps_count,
            std_vps_count: 0,
            std_sps_count: 0,
            std_pps_count: 0,
        },
        "vulkanalia empty h265 session parameters",
    )
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_smoke_create_video_session_parameters(
    device: &Device,
    create_info: &vk::VideoSessionParametersCreateInfoKHR,
    parameters_snapshot: NativeVulkanVulkanaliaVideoSessionParametersSnapshot,
    operation: &'static str,
) -> NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
    match native_vulkan_vulkanalia_create_video_session_parameters(
        device,
        create_info,
        parameters_snapshot,
        operation,
    ) {
        Ok(parameters) => {
            let snapshot = parameters.snapshot.clone();
            native_vulkan_vulkanalia_destroy_video_session_parameters(device, parameters);
            NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
                requested: true,
                supported: true,
                created: true,
                destroyed: true,
                error: None,
                parameters: snapshot,
            }
        }
        Err(err) => {
            let error = err.error;
            NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
                requested: true,
                supported: true,
                created: false,
                destroyed: false,
                error: Some(error),
                parameters: err.snapshot,
            }
        }
    }
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaVideoSessionParametersCreateError {
    pub(in crate::renderer::native_vulkan::vulkan) error: String,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaVideoSessionParametersSnapshot,
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_video_session_parameters(
    device: &Device,
    create_info: &vk::VideoSessionParametersCreateInfoKHR,
    parameters_snapshot: NativeVulkanVulkanaliaVideoSessionParametersSnapshot,
    operation: &'static str,
) -> Result<VulkanaliaVideoSessionParameters, VulkanaliaVideoSessionParametersCreateError> {
    match unsafe { device.create_video_session_parameters_khr(create_info, None) } {
        Ok(parameters) => Ok(VulkanaliaVideoSessionParameters {
            parameters,
            snapshot: parameters_snapshot,
        }),
        Err(err) => Err(VulkanaliaVideoSessionParametersCreateError {
            error: format!("vkCreateVideoSessionParametersKHR({operation}): {err:?}"),
            snapshot: parameters_snapshot,
        }),
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_video_session_parameters(
    device: &Device,
    parameters: VulkanaliaVideoSessionParameters,
) {
    unsafe {
        device.destroy_video_session_parameters_khr(parameters.parameters, None);
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn vulkanalia_session_parameters_codec_label(
    codec: NativeVulkanVideoSessionCodec,
) -> &'static str {
    native_vulkan_vulkanalia_video_session_label(codec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_parameter_smoke_keeps_av1_explicitly_blocked_until_sequence_header() {
        let snapshot = native_vulkan_vulkanalia_unsupported_av1_empty_video_session_parameters(
            NativeVulkanVideoSessionCodec::Av1Main10,
        );

        assert!(!snapshot.supported);
        assert!(!snapshot.created);
        assert_eq!(snapshot.parameters.source, "av1-sequence-header-required");
    }

    #[test]
    fn codec_labels_cover_current_video_session_codecs() {
        assert_eq!(
            vulkanalia_session_parameters_codec_label(NativeVulkanVideoSessionCodec::H264High8),
            "h264-high-8"
        );
        assert_eq!(
            vulkanalia_session_parameters_codec_label(NativeVulkanVideoSessionCodec::H265Main10),
            "h265-main-10"
        );
        assert_eq!(
            vulkanalia_session_parameters_codec_label(NativeVulkanVideoSessionCodec::Av1Main8),
            "av1-main-8"
        );
    }
}
