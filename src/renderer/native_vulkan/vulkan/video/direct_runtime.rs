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
        route_name: "direct-video",
        owner_module: "src/renderer/native_vulkan/vulkan/video/direct_runtime.rs",
        primary_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
        ffmpeg_reference_files: &[
            FFMPEG_VULKAN_EXEC_REFERENCE,
            FFMPEG_VULKAN_DECODE_REFERENCE,
            FFMPEG_VULKAN_H264_REFERENCE,
            FFMPEG_VULKAN_H265_REFERENCE,
            FFMPEG_VULKAN_AV1_REFERENCE,
        ],
        resource_owner: "Vulkanalia-owned instance/device/session/images/bitstream-command resources; the old ash comparison backend is removed",
        command_submit_model: "FFmpeg-style command lifecycle: start command buffer, CmdPipelineBarrier2, Begin/Decode/End video coding, QueueSubmit2, fence/timeline completion",
        present_handoff_model: "decoded image handoff stays codec-neutral; zero-copy is claimed only when imported/direct image telemetry proves no CPU frame copy",
        audio_sync_boundary: "audio remains a separate runtime clock; video direct runtime publishes PTS/present timing for audio clock synchronization",
        required_submit_order: &[
            "vkBeginCommandBuffer",
            "cmd_pipeline_barrier2",
            "cmd_begin_video_coding_khr",
            "cmd_decode_video_khr",
            "cmd_end_video_coding_khr",
            "vkEndCommandBuffer",
            "queue_submit2",
        ],
        required_backend_modules: &[
            "video_session.rs",
            "video_session_bind.rs",
            "video_session_images.rs",
            "video_bitstream_buffer.rs",
            "video_command_pool.rs",
            "video_decode_commands.rs",
            "video_decode_submit.rs",
            "video_decode_submit_h264.rs",
            "video_decode_submit_h265.rs",
            "video_decode_submit_av1.rs",
        ],
        vulkanalia_inline_session_parameter_type_evidence: vec![
            std::any::type_name::<vulkanalia::vk::PhysicalDeviceVideoMaintenance2FeaturesKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeH264InlineSessionParametersInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeH265InlineSessionParametersInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeAV1InlineSessionParametersInfoKHR>(),
        ],
        codec_plans: native_vulkan_vulkanalia_direct_codec_runtime_plans(),
        runtime_policy: "do not reintroduce ash direct-video ownership; codec-neutral runtime resources stay Vulkanalia-owned and must be validated by continuous real-source smokes",
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
            direct_runtime_gate: "H.264 continuous direct runtime consumes the Vulkanalia submit plan and records decode work through Vulkanalia",
            session_parameter_strategy: "VK_KHR_video_maintenance2 inline SPS/PPS on VideoDecodeInfoKHR; no streaming VkVideoSessionParametersKHR object",
            display_handoff_target: "decoded DPB/output image -> codec-neutral direct display handoff",
        },
        NativeVulkanVulkanaliaDirectCodecRuntimePlan {
            codec: NativeVulkanVideoSessionCodec::H265Main8,
            codec_reference: FFMPEG_VULKAN_H265_REFERENCE,
            submit_plan_module: "video_decode_submit_h265.rs",
            ready_prefix_smoke_gate: "H.265 main8 Vulkanalia ready-prefix decode smoke records and submits real access units with queue_submit2",
            direct_runtime_gate: "H.265 main8 continuous direct runtime consumes the Vulkanalia submit plan and records decode work through Vulkanalia",
            session_parameter_strategy: "VK_KHR_video_maintenance2 inline VPS/SPS/PPS on VideoDecodeInfoKHR; no streaming VkVideoSessionParametersKHR object",
            display_handoff_target: "decoded DPB/output image -> codec-neutral direct display handoff",
        },
        NativeVulkanVulkanaliaDirectCodecRuntimePlan {
            codec: NativeVulkanVideoSessionCodec::H265Main10,
            codec_reference: FFMPEG_VULKAN_H265_REFERENCE,
            submit_plan_module: "video_decode_submit_h265.rs",
            ready_prefix_smoke_gate: "H.265 main10 Vulkanalia ready-prefix decode smoke records and submits real access units with queue_submit2",
            direct_runtime_gate: "H.265 main10 continuous direct runtime consumes the Vulkanalia submit plan and records decode work through Vulkanalia",
            session_parameter_strategy: "VK_KHR_video_maintenance2 inline VPS/SPS/PPS on VideoDecodeInfoKHR; no streaming VkVideoSessionParametersKHR object",
            display_handoff_target: "decoded DPB/output image -> codec-neutral direct display handoff",
        },
        NativeVulkanVulkanaliaDirectCodecRuntimePlan {
            codec: NativeVulkanVideoSessionCodec::Av1Main8,
            codec_reference: FFMPEG_VULKAN_AV1_REFERENCE,
            submit_plan_module: "video_decode_submit_av1.rs",
            ready_prefix_smoke_gate: "AV1 main8 Vulkanalia decode-frame submit lowering records real temporal units with queue_submit2",
            direct_runtime_gate: "AV1 main8 continuous direct runtime consumes the Vulkanalia submit plan and records decode work through Vulkanalia",
            session_parameter_strategy: "VK_KHR_video_maintenance2 inline sequence header on VideoDecodeInfoKHR; no streaming VkVideoSessionParametersKHR object",
            display_handoff_target: "decoded DPB/output image -> codec-neutral direct display handoff, including show-existing/display-only reuse",
        },
        NativeVulkanVulkanaliaDirectCodecRuntimePlan {
            codec: NativeVulkanVideoSessionCodec::Av1Main10,
            codec_reference: FFMPEG_VULKAN_AV1_REFERENCE,
            submit_plan_module: "video_decode_submit_av1.rs",
            ready_prefix_smoke_gate: "AV1 main10 Vulkanalia decode-frame submit lowering records real temporal units with queue_submit2",
            direct_runtime_gate: "AV1 main10 continuous direct runtime consumes the Vulkanalia submit plan and records decode work through Vulkanalia",
            session_parameter_strategy: "VK_KHR_video_maintenance2 inline sequence header on VideoDecodeInfoKHR; no streaming VkVideoSessionParametersKHR object",
            display_handoff_target: "decoded DPB/output image -> codec-neutral direct display handoff, including show-existing/display-only reuse",
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
        assert_eq!(contract.route_name, "direct-video");
        assert!(
            contract
                .ffmpeg_reference_files
                .contains(&"references/ffmpeg/libavutil/vulkan.c")
        );
        assert!(
            contract
                .required_submit_order
                .contains(&"cmd_pipeline_barrier2")
        );
        assert!(contract.required_submit_order.contains(&"queue_submit2"));
        assert!(
            contract
                .resource_owner
                .contains("Vulkanalia-owned instance/device/session")
        );
        assert!(contract.runtime_policy.contains("do not reintroduce ash"));
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
                .all(|plan| plan.direct_runtime_gate.contains("Vulkanalia submit plan"))
        );
    }
}
