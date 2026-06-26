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
    BitstreamNativeDecode,
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
            kind: NativeVulkanVideoPipelineRouteKind::BitstreamNativeDecode,
            frontend_role: "FFmpeg demux/bitstream-filter encoded access-unit provider",
            decode_owner: "gilder-native-vulkan",
            gilder_role: "Vulkan Video decode, decoded image ownership, render and present",
            handoff_contract: "bounded encoded AU/TU packets with timestamps, loop serial and parameter-set snapshots",
            compressed_payload_copy_scope: "AVPacket payload is borrowed until upload into the Vulkan Video bitstream ring",
            decoded_frame_copy_scope: "decoded DPB/output images stay GPU-owned and may be sampled directly by render",
            zero_copy_claim: "decoded-image render/present only; compressed packet upload is a named bitstream-ring copy",
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
                owner: "ffmpeg-frontend",
                boundary: "codec bitstream filters normalize access units/temporal units and preserve timestamps",
                ffmpeg_reference: "h264_mp4toannexb/hevc_mp4toannexb BSF send-drain contract; AV1 container packets follow libavcodec/av1dec.c ff_cbs_read_packet",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 3,
                kind: NativeVulkanVideoPipelineStageKind::PacketQueue,
                owner: "native-vulkan-demux-boundary",
                boundary: "bounded keep-last queue owns packet refs until bitstream ring upload",
                ffmpeg_reference: "ffplay PacketQueue av_packet_move_ref, serial changes across seek/loop",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 4,
                kind: NativeVulkanVideoPipelineStageKind::CodecState,
                owner: "native-vulkan-codec",
                boundary: "parameter sets, sequence headers, DPB/reference maps and recovery points are rebuilt from stream evidence",
                ffmpeg_reference: "codec parser state, reference lists, reorder and recovery-point handling",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 5,
                kind: NativeVulkanVideoPipelineStageKind::Decode,
                owner: "native-vulkan-codec",
                boundary: "Vulkan Video submissions consume a fixed-capacity bitstream ring and write decoded DPB/output images",
                ffmpeg_reference: "decoder consumes AVPacket and emits reference-managed frames",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 6,
                kind: NativeVulkanVideoPipelineStageKind::DisplayHandoff,
                owner: "native-vulkan-codec-render-boundary",
                boundary: "decoded image layer, layout, fence/timeline and descriptor heap identity are handed to render",
                ffmpeg_reference: "AVFrame ownership/refcount handoff after decode",
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
            "the bitstream route uses FFmpeg only before decode; Gilder owns native Vulkan decode",
            "decoded-frame provider routes are deleted from the native Vulkan video mainline",
            "demux/parser ownership must not imply decoded frame or display-sink ownership",
            "compressed payload retention must stay bounded by the packet queue and bitstream ring",
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
            route.kind == NativeVulkanVideoPipelineRouteKind::BitstreamNativeDecode
                && route.decode_owner == "gilder-native-vulkan"
                && route.frontend_role.contains("FFmpeg")
                && route.handoff_contract.contains("encoded")
                && route
                    .compressed_payload_copy_scope
                    .contains("bitstream ring")
                && route.decoded_frame_copy_scope.contains("GPU-owned")
                && route.zero_copy_claim.contains("bitstream-ring copy")
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
                .filter(|stage| matches!(
                    stage.kind,
                    NativeVulkanVideoPipelineStageKind::Demux
                        | NativeVulkanVideoPipelineStageKind::BitstreamFilter
                ))
                .all(|stage| stage.owner == "ffmpeg-frontend")
        );
        assert!(
            contract.stages[3].kind == NativeVulkanVideoPipelineStageKind::PacketQueue
                && contract.stages[3].boundary.contains("keep-last")
        );
        assert!(
            contract
                .invariants
                .iter()
                .any(|invariant| invariant.contains("provider routes are deleted"))
        );
    }
}
