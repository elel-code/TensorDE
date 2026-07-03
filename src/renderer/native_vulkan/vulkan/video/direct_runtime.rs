use serde::Serialize;

use crate::renderer::native_vulkan::NativeVulkanVideoSessionCodec;

use super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE;

const FFMPEG_VULKAN_EXEC_REFERENCE: &str = "references/ffmpeg/libavutil/vulkan.c";
const FFMPEG_VULKAN_H264_REFERENCE: &str = "references/ffmpeg/libavcodec/vulkan_h264.c";
const FFMPEG_VULKAN_H265_REFERENCE: &str = "references/ffmpeg/libavcodec/vulkan_hevc.c";
const FFMPEG_VULKAN_AV1_REFERENCE: &str = "references/ffmpeg/libavcodec/vulkan_av1.c";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDirectRuntimeContract {
    pub binding: &'static str,
    pub route_name: &'static str,
    pub owner_module: &'static str,
    pub primary_reference: &'static str,
    pub ffmpeg_reference_files: &'static [&'static str],
    pub resource_owner: &'static str,
    pub command_submit_model: &'static str,
    pub present_handoff_model: &'static str,
    pub audio_sync_boundary: &'static str,
    pub required_submit_order: &'static [&'static str],
    pub required_backend_modules: &'static [&'static str],
    pub vulkanalia_inline_session_parameter_type_evidence: Vec<&'static str>,
    pub codec_plans: Vec<NativeVulkanVulkanaliaDirectCodecRuntimePlan>,
    pub runtime_policy: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDirectCodecRuntimePlan {
    pub codec: NativeVulkanVideoSessionCodec,
    pub codec_reference: &'static str,
    pub submit_plan_module: &'static str,
    pub ready_prefix_smoke_gate: &'static str,
    pub direct_runtime_gate: &'static str,
    pub session_parameter_strategy: &'static str,
    pub display_handoff_target: &'static str,
}

pub fn native_vulkan_vulkanalia_direct_runtime_contract()
-> NativeVulkanVulkanaliaDirectRuntimeContract {
    NativeVulkanVulkanaliaDirectRuntimeContract {
        binding: "vulkanalia",
        route_name: "ffmpeg-vulkan-hwdecode-mainline",
        owner_module: "src/renderer/native_vulkan/vulkan/video/direct_runtime.rs",
        primary_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
        ffmpeg_reference_files: &[
            FFMPEG_VULKAN_EXEC_REFERENCE,
            FFMPEG_VULKAN_DECODE_REFERENCE,
            FFMPEG_VULKAN_H264_REFERENCE,
            FFMPEG_VULKAN_H265_REFERENCE,
            FFMPEG_VULKAN_AV1_REFERENCE,
        ],
        resource_owner: "Gilder owns the Vulkanalia instance/device/queues, descriptor heaps and present resources; FFmpeg owns codec/session/frame-pool decode state on the provided Vulkan device",
        command_submit_model: "FFmpeg avcodec owns Vulkan hwaccel command recording/submission; Gilder waits AVVkFrame timeline semaphore values before descriptor-heap sampling",
        present_handoff_model: "AVFrame(format=AV_PIX_FMT_VULKAN) -> AVVkFrame VkImage/layout/timeline/queue-family -> descriptor-heap Y/UV plane sampling; zero-copy is claimed only with no av_hwframe_transfer_data",
        audio_sync_boundary: "audio remains a separate runtime clock; video direct runtime publishes PTS/present timing for audio clock synchronization",
        required_submit_order: &[
            "avcodec_send_packet",
            "avcodec_receive_frame",
            "validate_AV_PIX_FMT_VULKAN",
            "retain_AVFrame_until_present_fence",
            "wait_AVVkFrame_timeline_semaphore",
            "write_descriptor_heap_plane_descriptors",
            "draw_dynamic_rendering_present",
        ],
        required_backend_modules: &[
            "src/renderer/native_vulkan/video/ffmpeg_hw.rs",
            "src/renderer/native_vulkan/video/demux_ffmpeg.rs",
            "src/renderer/native_vulkan/vulkan/present/render_descriptors.rs",
            "src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs",
            "src/renderer/native_vulkan/vulkan/video/present_runtime.rs",
        ],
        vulkanalia_inline_session_parameter_type_evidence: vec![
            std::any::type_name::<vulkanalia::vk::PhysicalDeviceVideoMaintenance2FeaturesKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeH264InlineSessionParametersInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeH265InlineSessionParametersInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeAV1InlineSessionParametersInfoKHR>(),
        ],
        codec_plans: native_vulkan_vulkanalia_direct_codec_runtime_plans(),
        runtime_policy: "FFmpeg Vulkan hwaccel is the mainline decoder; the old Gilder Vulkan Video submit path is compatibility-only until removed, and software decode fallback is rejected",
    }
}

pub fn native_vulkan_vulkanalia_direct_codec_runtime_plans()
-> Vec<NativeVulkanVulkanaliaDirectCodecRuntimePlan> {
    vec![
        NativeVulkanVulkanaliaDirectCodecRuntimePlan {
            codec: NativeVulkanVideoSessionCodec::H264High8,
            codec_reference: FFMPEG_VULKAN_H264_REFERENCE,
            submit_plan_module: "video_decode_submit_h264.rs",
            ready_prefix_smoke_gate: "H.264 Vulkanalia ready-prefix decode smoke records and submits real access units with queue_submit2",
            direct_runtime_gate: "H.264 mainline runtime consumes AV_PIX_FMT_VULKAN frames from FFmpeg h264_vulkan",
            session_parameter_strategy: "FFmpeg owns parser, DPB and Vulkan session parameters",
            display_handoff_target: "AVVkFrame VkImage -> descriptor-heap Y/UV plane sampling",
        },
        NativeVulkanVulkanaliaDirectCodecRuntimePlan {
            codec: NativeVulkanVideoSessionCodec::H265Main8,
            codec_reference: FFMPEG_VULKAN_H265_REFERENCE,
            submit_plan_module: "video_decode_submit_h265.rs",
            ready_prefix_smoke_gate: "H.265 main8 Vulkanalia ready-prefix decode smoke records and submits real access units with queue_submit2",
            direct_runtime_gate: "H.265 main8 mainline runtime consumes AV_PIX_FMT_VULKAN frames from FFmpeg hevc_vulkan",
            session_parameter_strategy: "FFmpeg owns parser, DPB and Vulkan session parameters",
            display_handoff_target: "AVVkFrame VkImage -> descriptor-heap Y/UV plane sampling",
        },
        NativeVulkanVulkanaliaDirectCodecRuntimePlan {
            codec: NativeVulkanVideoSessionCodec::H265Main10,
            codec_reference: FFMPEG_VULKAN_H265_REFERENCE,
            submit_plan_module: "video_decode_submit_h265.rs",
            ready_prefix_smoke_gate: "H.265 main10 Vulkanalia ready-prefix decode smoke records and submits real access units with queue_submit2",
            direct_runtime_gate: "H.265 main10 mainline runtime consumes AV_PIX_FMT_VULKAN frames from FFmpeg hevc_vulkan",
            session_parameter_strategy: "FFmpeg owns parser, DPB and Vulkan session parameters",
            display_handoff_target: "AVVkFrame VkImage -> descriptor-heap Y/UV plane sampling",
        },
        NativeVulkanVulkanaliaDirectCodecRuntimePlan {
            codec: NativeVulkanVideoSessionCodec::Av1Main8,
            codec_reference: FFMPEG_VULKAN_AV1_REFERENCE,
            submit_plan_module: "video_decode_submit_av1.rs",
            ready_prefix_smoke_gate: "AV1 main8 Vulkanalia decode-frame submit lowering records real temporal units with queue_submit2",
            direct_runtime_gate: "AV1 main8 mainline runtime consumes AV_PIX_FMT_VULKAN frames from FFmpeg av1_vulkan",
            session_parameter_strategy: "FFmpeg owns parser, DPB and Vulkan session parameters, including show-existing/display-only reuse",
            display_handoff_target: "AVVkFrame VkImage -> descriptor-heap Y/UV plane sampling",
        },
        NativeVulkanVulkanaliaDirectCodecRuntimePlan {
            codec: NativeVulkanVideoSessionCodec::Av1Main10,
            codec_reference: FFMPEG_VULKAN_AV1_REFERENCE,
            submit_plan_module: "video_decode_submit_av1.rs",
            ready_prefix_smoke_gate: "AV1 main10 Vulkanalia decode-frame submit lowering records real temporal units with queue_submit2",
            direct_runtime_gate: "AV1 main10 mainline runtime consumes AV_PIX_FMT_VULKAN frames from FFmpeg av1_vulkan",
            session_parameter_strategy: "FFmpeg owns parser, DPB and Vulkan session parameters, including show-existing/display-only reuse",
            display_handoff_target: "AVVkFrame VkImage -> descriptor-heap Y/UV plane sampling",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_runtime_contract_is_vulkanalia_owned_and_ffmpeg_aligned() {
        let contract = native_vulkan_vulkanalia_direct_runtime_contract();

        assert_eq!(contract.binding, "vulkanalia");
        assert_eq!(contract.route_name, "ffmpeg-vulkan-hwdecode-mainline");
        assert!(
            contract
                .ffmpeg_reference_files
                .contains(&"references/ffmpeg/libavutil/vulkan.c")
        );
        assert!(
            contract
                .required_submit_order
                .contains(&"avcodec_receive_frame")
        );
        assert!(
            contract
                .required_submit_order
                .contains(&"write_descriptor_heap_plane_descriptors")
        );
        assert!(
            contract
                .resource_owner
                .contains("FFmpeg owns codec/session/frame-pool")
        );
        assert!(
            contract
                .runtime_policy
                .contains("FFmpeg Vulkan hwaccel is the mainline decoder")
        );
        assert!(
            contract
                .vulkanalia_inline_session_parameter_type_evidence
                .iter()
                .any(|name| name.ends_with("VideoDecodeH265InlineSessionParametersInfoKHR"))
        );
    }

    #[test]
    fn direct_runtime_contract_covers_all_current_video_codecs() {
        let plans = native_vulkan_vulkanalia_direct_codec_runtime_plans();
        let codecs = plans.iter().map(|plan| plan.codec).collect::<Vec<_>>();

        assert_eq!(plans.len(), 5);
        assert!(codecs.contains(&NativeVulkanVideoSessionCodec::H264High8));
        assert!(codecs.contains(&NativeVulkanVideoSessionCodec::H265Main8));
        assert!(codecs.contains(&NativeVulkanVideoSessionCodec::H265Main10));
        assert!(codecs.contains(&NativeVulkanVideoSessionCodec::Av1Main8));
        assert!(codecs.contains(&NativeVulkanVideoSessionCodec::Av1Main10));
        assert!(
            plans
                .iter()
                .all(|plan| plan.direct_runtime_gate.contains("AV_PIX_FMT_VULKAN"))
        );
    }
}
