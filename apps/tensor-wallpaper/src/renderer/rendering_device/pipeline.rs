use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderingDeviceVideoPipelineStageKind {
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
pub struct RenderingDeviceVideoPipelineStageContract {
    pub order: u8,
    pub kind: RenderingDeviceVideoPipelineStageKind,
    pub owner: &'static str,
    pub boundary: &'static str,
    pub ffmpeg_reference: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderingDeviceVideoPipelineRouteKind {
    FfmpegVulkanHwDecode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceVideoPipelineRouteContract {
    pub kind: RenderingDeviceVideoPipelineRouteKind,
    pub frontend_role: &'static str,
    pub decode_owner: &'static str,
    pub tensor_wallpaper_role: &'static str,
    pub handoff_contract: &'static str,
    pub compressed_payload_copy_scope: &'static str,
    pub decoded_frame_copy_scope: &'static str,
    pub zero_copy_claim: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceVideoPipelineContract {
    pub reference: &'static str,
    pub routes: Vec<RenderingDeviceVideoPipelineRouteContract>,
    pub stages: Vec<RenderingDeviceVideoPipelineStageContract>,
    pub invariants: &'static [&'static str],
}

pub fn rendering_device_video_pipeline_contract() -> RenderingDeviceVideoPipelineContract {
    RenderingDeviceVideoPipelineContract {
        reference: "FFmpeg packet/frame/clock model",
        routes: vec![RenderingDeviceVideoPipelineRouteContract {
            kind: RenderingDeviceVideoPipelineRouteKind::FfmpegVulkanHwDecode,
            frontend_role: "FFmpeg demux/parser/bitstream-filter and avcodec send/receive runtime",
            decode_owner: "vulkan-renderer-ffmpeg-vulkan-decode",
            tensor_wallpaper_role: "provide typed media source/codec/clock policy; vulkan-renderer owns decode, opaque plane leases, descriptor sampling, render and present",
            handoff_contract: "renderer-private AVFrame(format=AV_PIX_FMT_VULKAN) lowers to opaque retained Y/UV plane leases plus typed timeline dependency",
            compressed_payload_copy_scope: "AVPacket payload remains renderer-owned FFmpeg state until avcodec consumes it; Tensor Wallpaper does not upload a Vulkan Video bitstream ring",
            decoded_frame_copy_scope: "decoded pixels remain renderer-owned FFmpeg Vulkan images and are never downloaded or CPU-uploaded",
            zero_copy_claim: "decoded-image shader render/present only; descriptor rewrites copy metadata, not frame pixels",
        }],
        stages: vec![
            RenderingDeviceVideoPipelineStageContract {
                order: 0,
                kind: RenderingDeviceVideoPipelineStageKind::Source,
                owner: "tensor-wallpaper-typed-media-policy",
                boundary: "manifest/render sync selects source, fit, loop, mute, target fps and decoder policy",
                ffmpeg_reference: "AVFormatContext input URL/options and stream selection",
            },
            RenderingDeviceVideoPipelineStageContract {
                order: 1,
                kind: RenderingDeviceVideoPipelineStageKind::Demux,
                owner: "vulkan-renderer-ffmpeg-vulkan-decode",
                boundary: "container packets are selected by codec stream without decoded-frame handoff",
                ffmpeg_reference: "av_read_frame packet ownership and stream_index filtering",
            },
            RenderingDeviceVideoPipelineStageContract {
                order: 2,
                kind: RenderingDeviceVideoPipelineStageKind::BitstreamFilter,
                owner: "vulkan-renderer-ffmpeg-vulkan-decode",
                boundary: "FFmpeg owns parser/BSF requirements needed by the selected Vulkan hwaccel",
                ffmpeg_reference: "avcodec parser/BSF and Vulkan hwaccel packet consumption",
            },
            RenderingDeviceVideoPipelineStageContract {
                order: 3,
                kind: RenderingDeviceVideoPipelineStageKind::PacketQueue,
                owner: "vulkan-renderer-ffmpeg-vulkan-decode",
                boundary: "bounded AVPacket refs feed avcodec_send_packet with serial/timestamp ownership",
                ffmpeg_reference: "ffplay PacketQueue av_packet_move_ref and decoder send/receive flow",
            },
            RenderingDeviceVideoPipelineStageContract {
                order: 4,
                kind: RenderingDeviceVideoPipelineStageKind::CodecState,
                owner: "vulkan-renderer-ffmpeg-vulkan-decode",
                boundary: "parameter sets, DPB/reference maps, reorder and recovery points stay inside FFmpeg",
                ffmpeg_reference: "libavcodec parser state plus h264_vulkan/hevc_vulkan/av1_vulkan hwaccel state",
            },
            RenderingDeviceVideoPipelineStageContract {
                order: 5,
                kind: RenderingDeviceVideoPipelineStageKind::Decode,
                owner: "vulkan-renderer-ffmpeg-vulkan-decode",
                boundary: "renderer-owned avcodec_receive_frame validates AV_PIX_FMT_VULKAN output on the renderer device",
                ffmpeg_reference: "libavcodec/vulkan_decode.c and codec-specific Vulkan hwaccels",
            },
            RenderingDeviceVideoPipelineStageContract {
                order: 6,
                kind: RenderingDeviceVideoPipelineStageKind::DisplayHandoff,
                owner: "vulkan-renderer-decoded-plane-lease",
                boundary: "private AVVkFrame image/layout/timeline ownership lowers to opaque Y/UV plane leases and a typed submission dependency",
                ffmpeg_reference: "AVFrame hw_frames_ctx and AVVkFrame lifetime/refcount handoff",
            },
            RenderingDeviceVideoPipelineStageContract {
                order: 7,
                kind: RenderingDeviceVideoPipelineStageKind::Render,
                owner: "vulkan-renderer-command-encoder",
                boundary: "YUV planes are sampled directly into the swapchain-sized composition pass with fit handling",
                ffmpeg_reference: "filter/display stage consumes frames without mutating decoder state",
            },
            RenderingDeviceVideoPipelineStageContract {
                order: 8,
                kind: RenderingDeviceVideoPipelineStageKind::Present,
                owner: "vulkan-renderer-presentation-transaction",
                boundary: "Wayland surface and Vulkan swapchain present retain the decoded lease only through its consuming submit and pace frames by PTS/duration",
                ffmpeg_reference: "ffplay video refresh delay and master-clock comparison",
            },
            RenderingDeviceVideoPipelineStageContract {
                order: 9,
                kind: RenderingDeviceVideoPipelineStageKind::AudioClock,
                owner: "separate-scene-audio-spectrum",
                boundary: "standalone raw audio clock/output was removed; scene audio spectrum stays separate from video texture ownership",
                ffmpeg_reference: "ffplay audio master clock, packet serial and stale sample rejection",
            },
        ],
        invariants: &[
            "FFmpeg is the only frontend reference for codec packet/frame/clock semantics",
            "FFmpeg Vulkan hwaccel is the GPU video decode mainline",
            "Tensor Wallpaper must not provide or borrow a Vulkan device to FFmpeg; vulkan-renderer owns both device integration and decoder lifetime",
            "software decoded frames are rejected rather than uploaded behind a zero-copy label",
            "compressed payload retention must stay bounded by FFmpeg packet queue/send-receive ownership",
            "zero-copy claims must name scope: packet borrow, bitstream upload, decoded-image handoff, render or compositor present",
            "decode, render and present telemetry must be independently attributable",
            "PTS/duration pacing and loop generation must invalidate stale decoded-frame selection across loop or seek",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_pipeline_contract_keeps_ffmpeg_as_frontend_reference() {
        let contract = rendering_device_video_pipeline_contract();

        assert!(contract.reference.contains("FFmpeg"));
        assert_eq!(contract.stages.len(), 10);
        assert_eq!(contract.routes.len(), 1);
        assert!(contract.routes.iter().any(|route| {
            route.kind == RenderingDeviceVideoPipelineRouteKind::FfmpegVulkanHwDecode
                && route.decode_owner == "vulkan-renderer-ffmpeg-vulkan-decode"
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
            RenderingDeviceVideoPipelineStageKind::Source
        );
        assert_eq!(
            contract.stages[9].kind,
            RenderingDeviceVideoPipelineStageKind::AudioClock
        );
        assert!(
            contract
                .stages
                .iter()
                .filter(|stage| matches!(stage.kind, RenderingDeviceVideoPipelineStageKind::Demux))
                .all(|stage| stage.owner == "vulkan-renderer-ffmpeg-vulkan-decode")
        );
        assert!(
            contract.stages[2].kind == RenderingDeviceVideoPipelineStageKind::BitstreamFilter
                && contract.stages[2].owner == "vulkan-renderer-ffmpeg-vulkan-decode"
        );
        assert!(
            contract.stages[3].kind == RenderingDeviceVideoPipelineStageKind::PacketQueue
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
