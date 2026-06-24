use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanVideoPipelineStageKind {
    Source,
    Demux,
    Parse,
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
    pub gstreamer_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoPipelineContract {
    pub first_reference: &'static str,
    pub second_reference: &'static str,
    pub stages: Vec<NativeVulkanVideoPipelineStageContract>,
    pub invariants: &'static [&'static str],
}

pub fn native_vulkan_video_pipeline_contract() -> NativeVulkanVideoPipelineContract {
    NativeVulkanVideoPipelineContract {
        first_reference: "FFmpeg packet/frame/clock model",
        second_reference: "replaceable GStreamer demux/parser/appsink/audio frontend",
        stages: vec![
            NativeVulkanVideoPipelineStageContract {
                order: 0,
                kind: NativeVulkanVideoPipelineStageKind::Source,
                owner: "render-plan",
                boundary: "manifest/render sync selects source, fit, loop, mute, target fps and decoder policy",
                ffmpeg_reference: "AVFormatContext input URL/options and stream selection",
                gstreamer_reference: "filesrc plus source-specific demux setup",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 1,
                kind: NativeVulkanVideoPipelineStageKind::Demux,
                owner: "replaceable-frontend",
                boundary: "container packets are selected by codec stream without instantiating video display sinks",
                ffmpeg_reference: "av_read_frame packet ownership and stream_index filtering",
                gstreamer_reference: "qtdemux/matroskademux pad-added filtering",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 2,
                kind: NativeVulkanVideoPipelineStageKind::Parse,
                owner: "replaceable-frontend",
                boundary: "codec parser normalizes access units/temporal units and preserves timestamps",
                ffmpeg_reference: "parser/bitstream filter normalization before decoder submit",
                gstreamer_reference: "h264parse/h265parse/av1parse appsink handoff",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 3,
                kind: NativeVulkanVideoPipelineStageKind::PacketQueue,
                owner: "native-vulkan-demux-boundary",
                boundary: "bounded keep-last queue owns compressed payload lifetime until bitstream ring upload",
                ffmpeg_reference: "bounded PacketQueue with serial changes across seek/loop",
                gstreamer_reference: "appsink pull-sample plus EOS/seek replay accounting",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 4,
                kind: NativeVulkanVideoPipelineStageKind::CodecState,
                owner: "native-vulkan-codec",
                boundary: "parameter sets, sequence headers, DPB/reference maps and recovery points are rebuilt from stream evidence",
                ffmpeg_reference: "codec parser state, reference lists, reorder and recovery-point handling",
                gstreamer_reference: "parser output caps/headers and timestamp segments",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 5,
                kind: NativeVulkanVideoPipelineStageKind::Decode,
                owner: "native-vulkan-codec",
                boundary: "Vulkan Video submissions consume a fixed-capacity bitstream ring and write decoded DPB/output images",
                ffmpeg_reference: "decoder consumes AVPacket and emits reference-managed frames",
                gstreamer_reference: "hardware/software decoder remains comparison path, not the direct display owner",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 6,
                kind: NativeVulkanVideoPipelineStageKind::DisplayHandoff,
                owner: "native-vulkan-codec-render-boundary",
                boundary: "decoded image layer, layout, fence/timeline and descriptor identity are handed to render without copying compressed payloads",
                ffmpeg_reference: "AVFrame ownership/refcount handoff after decode",
                gstreamer_reference: "appsink buffer lifetime and caps memory feature diagnostics",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 7,
                kind: NativeVulkanVideoPipelineStageKind::Render,
                owner: "native-vulkan-render",
                boundary: "YUV planes are sampled directly into the swapchain-sized composition pass with fit handling",
                ffmpeg_reference: "filter/display stage consumes frames without mutating decoder state",
                gstreamer_reference: "display sink is bypassed; GStreamer remains a frontend only",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 8,
                kind: NativeVulkanVideoPipelineStageKind::Present,
                owner: "native-vulkan-present",
                boundary: "Wayland surface and Vulkan swapchain present are paced by target fps and compositor feedback",
                ffmpeg_reference: "ffplay video refresh delay and master-clock comparison",
                gstreamer_reference: "QoS/presentation diagnostics are reference signals, not display ownership",
            },
            NativeVulkanVideoPipelineStageContract {
                order: 9,
                kind: NativeVulkanVideoPipelineStageKind::AudioClock,
                owner: "separate-audio-pipeline",
                boundary: "audio decode/clock stays separate from video texture ownership and advances serial on loop/seek",
                ffmpeg_reference: "ffplay audio master clock, packet serial and stale sample rejection",
                gstreamer_reference: "AAC-only appsink clock probe with no video decoder contamination",
            },
        ],
        invariants: &[
            "FFmpeg is the first reference for codec packet/frame/clock semantics",
            "GStreamer is the second reference and current replaceable container/parser/audio frontend",
            "frontend implementations may be replaced by libav/ffmpeg or native demux as long as the packet/audio/clock contracts remain stable",
            "decoded video frontends must expose provider-neutral telemetry and samples before render/import",
            "audio clock frontends must expose provider-neutral runtime telemetry before pacing/render integration",
            "demux/parser ownership must not imply display-sink ownership",
            "compressed payload retention must stay bounded by the packet queue and bitstream ring",
            "decode, render and present telemetry must be independently attributable",
            "audio clock serial changes must invalidate stale video/audio samples across loop or seek",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_pipeline_contract_keeps_ffmpeg_first() {
        let contract = native_vulkan_video_pipeline_contract();

        assert!(contract.first_reference.contains("FFmpeg"));
        assert!(contract.second_reference.contains("GStreamer"));
        assert!(contract.second_reference.contains("replaceable"));
        assert_eq!(contract.stages.len(), 10);
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
                        | NativeVulkanVideoPipelineStageKind::Parse
                ))
                .all(|stage| stage.owner == "replaceable-frontend")
        );
        assert!(contract.stages.iter().any(|stage| stage.kind
            == NativeVulkanVideoPipelineStageKind::PacketQueue
            && stage.ffmpeg_reference.contains("PacketQueue")));
        assert!(
            contract
                .invariants
                .iter()
                .any(|invariant| invariant.contains("packet/audio/clock contracts remain stable"))
        );
        assert!(
            contract
                .invariants
                .iter()
                .any(|invariant| invariant.contains("provider-neutral telemetry and samples"))
        );
        assert!(contract.invariants.iter().any(|invariant| {
            invariant
                .contains("audio clock frontends must expose provider-neutral runtime telemetry")
        }));
    }
}
