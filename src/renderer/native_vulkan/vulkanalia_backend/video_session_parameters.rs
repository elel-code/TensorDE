use crate::renderer::native_vulkan::NativeVulkanVideoSessionCodec;
use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder, KhrVideoQueueExtensionDeviceCommands};

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

pub(super) fn native_vulkan_vulkanalia_smoke_create_empty_video_session_parameters(
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
    )
}

fn native_vulkan_vulkanalia_smoke_create_video_session_parameters(
    device: &Device,
    create_info: &vk::VideoSessionParametersCreateInfoKHR,
    parameters_snapshot: NativeVulkanVulkanaliaVideoSessionParametersSnapshot,
) -> NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
    match unsafe { device.create_video_session_parameters_khr(create_info, None) } {
        Ok(parameters) => {
            unsafe {
                device.destroy_video_session_parameters_khr(parameters, None);
            }
            NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
                requested: true,
                supported: true,
                created: true,
                destroyed: true,
                error: None,
                parameters: parameters_snapshot,
            }
        }
        Err(err) => NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
            requested: true,
            supported: true,
            created: false,
            destroyed: false,
            error: Some(format!(
                "vkCreateVideoSessionParametersKHR(vulkanalia empty): {err:?}"
            )),
            parameters: parameters_snapshot,
        },
    }
}

fn vulkanalia_session_parameters_codec_label(codec: NativeVulkanVideoSessionCodec) -> &'static str {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => "h264-high-8",
        NativeVulkanVideoSessionCodec::H265Main8 => "h265-main-8",
        NativeVulkanVideoSessionCodec::H265Main10 => "h265-main-10",
        NativeVulkanVideoSessionCodec::Av1Main8 => "av1-main-8",
        NativeVulkanVideoSessionCodec::Av1Main10 => "av1-main-10",
    }
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
