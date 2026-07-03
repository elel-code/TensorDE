use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanVideoPipelineStageKind {
    Source,
    Demux,
    BitstreamFilter,
    PacketQueue,
    CodecState,
    Decode,
    DisplayHandoff,
    Render,
    Present,
    AudioClock,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoPipelineStageContract {
    pub order: u8,
    pub kind: NativeVulkanVideoPipelineStageKind,
    pub owner: &'static str,
    pub boundary: &'static str,
    pub ffmpeg_reference: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanVideoPipelineRouteKind {
    FfmpegVulkanHwDecode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoPipelineRouteContract {
    pub kind: NativeVulkanVideoPipelineRouteKind,
    pub frontend_role: &'static str,
    pub decode_owner: &'static str,
    pub gilder_role: &'static str,
    pub handoff_contract: &'static str,
    pub compressed_payload_copy_scope: &'static str,
    pub decoded_frame_copy_scope: &'static str,
    pub zero_copy_claim: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoPipelineContract {
    pub reference: &'static str,
    pub routes: Vec<NativeVulkanVideoPipelineRouteContract>,
    pub stages: Vec<NativeVulkanVideoPipelineStageContract>,
    pub invariants: &'static [&'static str],
}

pub fn native_vulkan_video_pipeline_contract() -> NativeVulkanVideoPipelineContract {
    NativeVulkanVideoPipelineContract {
        reference: "FFmpeg packet/frame/clock model",
        routes: vec![NativeVulkanVideoPipelineRouteContract {
            kind: NativeVulkanVideoPipelineRouteKind::FfmpegVulkanHwDecode,
            frontend_role: "FFmpeg demux/parser/bitstream-filter and avcodec send/receive runtime",
            decode_owner: "ffmpeg-vulkan-hwdecode",
            gilder_role: "provide the Vulkanalia device to FFmpeg, then own AVVkFrame descriptor-heap sampling, render and present",
            handoff_contract: "AVFrame(format=AV_PIX_FMT_VULKAN) carrying AVVkFrame VkImage/layout/timeline/queue-family state",
            compressed_payload_copy_scope: "AVPacket payload remains FFmpeg-owned until avcodec consumes it; Gilder does not upload a Vulkan Video bitstream ring on the mainline",
            decoded_frame_copy_scope: "decoded pixels stay in FFmpeg-produced Vulkan images and are never downloaded or CPU-uploaded",
            zero_copy_claim: "decoded-image render/present only; descriptor heap writes copy metadata, not frame pixels",
        }],
        stages: vec![
            NativeVulkanVideoPipelineStageContract {
                order: 0,
                kind: NativeVulkanVideoPipelineStageKind::Source,
                owner: "render-plan",
                boundary: "manifest/render sync selects source, fit, loop, mute, target fps and decoder policy",
                ffmpeg_reference: "AVFormatContext input URL/options and stream selection",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 1,
                kind: NativeVulkanVideoPipelineStageKind::Demux,
                owner: "ffmpeg-frontend",
                boundary: "container packets are selected by codec stream without decoded-frame handoff",
                ffmpeg_reference: "av_read_frame packet ownership and stream_index filtering",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 2,
                kind: NativeVulkanVideoPipelineStageKind::BitstreamFilter,
                owner: "ffmpeg-vulkan-hwdecode",
                boundary: "FFmpeg owns parser/BSF requirements needed by the selected Vulkan hwaccel",
                ffmpeg_reference: "avcodec parser/BSF and Vulkan hwaccel packet consumption",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 3,
                kind: NativeVulkanVideoPipelineStageKind::PacketQueue,
                owner: "ffmpeg-decoder-boundary",
                boundary: "bounded AVPacket refs feed avcodec_send_packet with serial/timestamp ownership",
                ffmpeg_reference: "ffplay PacketQueue av_packet_move_ref and decoder send/receive flow",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 4,
                kind: NativeVulkanVideoPipelineStageKind::CodecState,
                owner: "ffmpeg-vulkan-hwdecode",
                boundary: "parameter sets, DPB/reference maps, reorder and recovery points stay inside FFmpeg",
                ffmpeg_reference: "libavcodec parser state plus h264_vulkan/hevc_vulkan/av1_vulkan hwaccel state",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 5,
                kind: NativeVulkanVideoPipelineStageKind::Decode,
                owner: "ffmpeg-vulkan-hwdecode",
                boundary: "avcodec_receive_frame emits AV_PIX_FMT_VULKAN frames from the Vulkanalia-provided device",
                ffmpeg_reference: "libavcodec/vulkan_decode.c and codec-specific Vulkan hwaccels",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 6,
                kind: NativeVulkanVideoPipelineStageKind::DisplayHandoff,
                owner: "ffmpeg-gpu-frame-render-boundary",
                boundary: "AVVkFrame VkImage/layout/timeline semaphore/queue family are adapted into a descriptor-heap sampled frame",
                ffmpeg_reference: "AVFrame hw_frames_ctx and AVVkFrame lifetime/refcount handoff",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 7,
                kind: NativeVulkanVideoPipelineStageKind::Render,
                owner: "native-vulkan-render",
                boundary: "YUV planes are sampled directly into the swapchain-sized composition pass with fit handling",
                ffmpeg_reference: "filter/display stage consumes frames without mutating decoder state",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 8,
                kind: NativeVulkanVideoPipelineStageKind::Present,
                owner: "native-vulkan-present",
                boundary: "Wayland surface and Vulkan swapchain present are paced by target fps and compositor feedback",
                ffmpeg_reference: "ffplay video refresh delay and master-clock comparison",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 9,
                kind: NativeVulkanVideoPipelineStageKind::AudioClock,
                owner: "separate-audio-pipeline",
                boundary: "audio decode/clock stays separate from video texture ownership and advances serial on loop/seek",
                ffmpeg_reference: "ffplay audio master clock, packet serial and stale sample rejection",
            },
        ],
        invariants: &[
            "FFmpeg is the only frontend reference for codec packet/frame/clock semantics",
            "FFmpeg Vulkan hwaccel is the native video decode mainline",
            "Gilder must provide the Vulkanalia device to FFmpeg instead of accepting a private FFmpeg Vulkan device on the mainline",
            "software decoded frames are rejected rather than uploaded behind a zero-copy label",
            "compressed payload retention must stay bounded by FFmpeg packet queue/send-receive ownership",
            "zero-copy claims must name scope: packet borrow, bitstream upload, decoded-image handoff, render or compositor present",
            "decode, render and present telemetry must be independently attributable",
            "audio clock serial changes must invalidate stale video/audio samples across loop or seek",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_pipeline_contract_keeps_ffmpeg_as_frontend_reference() {
        let contract = native_vulkan_video_pipeline_contract();

        assert!(contract.reference.contains("FFmpeg"));
        assert_eq!(contract.stages.len(), 10);
        assert_eq!(contract.routes.len(), 1);
        assert!(contract.routes.iter().any(|route| {
            route.kind == NativeVulkanVideoPipelineRouteKind::FfmpegVulkanHwDecode
                && route.decode_owner == "ffmpeg-vulkan-hwdecode"
                && route.frontend_role.contains("FFmpeg")
                && route.handoff_contract.contains("AV_PIX_FMT_VULKAN")
                && route
                    .compressed_payload_copy_scope
                    .contains("avcodec consumes")
                && route.decoded_frame_copy_scope.contains("Vulkan images")
                && route.zero_copy_claim.contains("metadata")
        }));
        assert_eq!(
            contract.stages[0].kind,
            NativeVulkanVideoPipelineStageKind::Source
        );
        assert_eq!(
            contract.stages[9].kind,
            NativeVulkanVideoPipelineStageKind::AudioClock
        );
        assert!(
            contract
                .stages
                .iter()
                .filter(|stage| matches!(stage.kind, NativeVulkanVideoPipelineStageKind::Demux))
                .all(|stage| stage.owner == "ffmpeg-frontend")
        );
        assert!(
            contract.stages[2].kind == NativeVulkanVideoPipelineStageKind::BitstreamFilter
                && contract.stages[2].owner == "ffmpeg-vulkan-hwdecode"
        );
        assert!(
            contract.stages[3].kind == NativeVulkanVideoPipelineStageKind::PacketQueue
                && contract.stages[3].boundary.contains("avcodec_send_packet")
        );
        assert!(
            contract
                .invariants
                .iter()
                .any(|invariant| invariant.contains("FFmpeg Vulkan hwaccel"))
        );
    }
}
