use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderingDeviceVideoFlowOwner {
    FfmpegFrontend,
    FfmpegHwDecode,
    SceneRender,
    SurfacePresent,
    SeparateAudioPipeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderingDeviceVideoFlowQueueKind {
    PacketQueue,
    DecodedFrameQueue,
    AudioFrameQueue,
    PresentPacer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderingDeviceVideoFlowThreadKind {
    Read,
    VideoDecode,
    AudioDecode,
    RenderRefresh,
    AudioCallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceVideoFlowQueueContract {
    pub kind: RenderingDeviceVideoFlowQueueKind,
    pub owner: RenderingDeviceVideoFlowOwner,
    pub ffmpeg_reference: &'static str,
    pub producer: &'static str,
    pub consumer: &'static str,
    pub payload: &'static str,
    pub serial_rule: &'static str,
    pub capacity_rule: &'static str,
    pub copy_cost_rule: &'static str,
    pub wake_rule: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceVideoFlowThreadContract {
    pub kind: RenderingDeviceVideoFlowThreadKind,
    pub owner: RenderingDeviceVideoFlowOwner,
    pub ffmpeg_reference: &'static str,
    pub input: &'static str,
    pub output: &'static str,
    pub blocking_rule: &'static str,
    pub replaceable_rule: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceVideoFlowContract {
    pub first_reference_root: &'static str,
    pub canonical_player_reference: &'static str,
    pub canonical_demux_reference: &'static str,
    pub canonical_decode_reference: &'static str,
    pub queues: Vec<RenderingDeviceVideoFlowQueueContract>,
    pub threads: Vec<RenderingDeviceVideoFlowThreadContract>,
    pub invariants: &'static [&'static str],
}

pub fn rendering_device_video_flow_contract() -> RenderingDeviceVideoFlowContract {
    RenderingDeviceVideoFlowContract {
        first_reference_root: "references/tensor-wallpaper/ffmpeg",
        canonical_player_reference: "references/tensor-wallpaper/ffmpeg/fftools/ffplay.c",
        canonical_demux_reference: "references/tensor-wallpaper/ffmpeg/fftools/ffmpeg_demux.c",
        canonical_decode_reference: "references/tensor-wallpaper/ffmpeg/fftools/ffmpeg_dec.c",
        queues: vec![
            RenderingDeviceVideoFlowQueueContract {
                kind: RenderingDeviceVideoFlowQueueKind::PacketQueue,
                owner: RenderingDeviceVideoFlowOwner::FfmpegHwDecode,
                ffmpeg_reference: "ffplay.c PacketQueue: packet_queue_put/get/flush/start and queue serial",
                producer: "renderer-owned FFmpeg demux/parser frontend",
                consumer: "renderer-owned FFmpeg Vulkan avcodec send/receive",
                payload: "AVPacket refs with pts/duration and packet serial",
                serial_rule: "flush, seek and loop advance packet serial; decode, audio and frame samples with older serial are stale",
                capacity_rule: "bounded queue; FFmpeg parser/codec state owns recovery, reorder and reference bootstrap",
                copy_cost_rule: "compressed payload is consumed by renderer-owned avcodec inside vulkan-renderer; Tensor Wallpaper does not upload a Vulkan Video bitstream ring",
                wake_rule: "producer wakes decode when queue becomes non-empty; consumer never busy-spins on empty queue",
            },
            RenderingDeviceVideoFlowQueueContract {
                kind: RenderingDeviceVideoFlowQueueKind::DecodedFrameQueue,
                owner: RenderingDeviceVideoFlowOwner::FfmpegHwDecode,
                ffmpeg_reference: "ffplay.c FrameQueue: pictq with keep_last and per-frame serial",
                producer: "renderer-owned FFmpeg Vulkan decode",
                consumer: "renderer-owned presentation transaction",
                payload: "opaque retained Y/UV plane leases plus PTS/duration, loop generation and typed timeline dependency",
                serial_rule: "frame serial must match the current packet queue serial before render",
                capacity_rule: "bounded keep-last state; renderer retains the private AVFrame only through the consuming submission retirement",
                copy_cost_rule: "decoded images remain renderer-owned FFmpeg Vulkan images; no transfer/download/upload is allowed",
                wake_rule: "presentation holds the retained frame until its PTS/duration expires and only then asks decode for another",
            },
            RenderingDeviceVideoFlowQueueContract {
                kind: RenderingDeviceVideoFlowQueueKind::AudioFrameQueue,
                owner: RenderingDeviceVideoFlowOwner::SeparateAudioPipeline,
                ffmpeg_reference: "ffplay.c sampq/audclk: audio frame queue, audio packet serial and synchronize_audio",
                producer: "audio decode frontend",
                consumer: "audio callback/runtime clock",
                payload: "decoded audio frame timing, sample rate/layout/format metadata and serial",
                serial_rule: "audio sample serial must match audio queue serial; video loop/seek serial invalidates stale clock samples",
                capacity_rule: "audio queue may be deeper than video but remains bounded by the frontend runtime",
                copy_cost_rule: "audio samples are independent from video texture ownership and must not force video-frame copies",
                wake_rule: "audio runtime wakes on clock sample or loop seek; stale samples are dropped before frontend work",
            },
            RenderingDeviceVideoFlowQueueContract {
                kind: RenderingDeviceVideoFlowQueueKind::PresentPacer,
                owner: RenderingDeviceVideoFlowOwner::SurfacePresent,
                ffmpeg_reference: "ffplay.c video_refresh: compute_target_delay, master clock comparison and remaining_time sleep",
                producer: "renderer-owned PTS frame selector",
                consumer: "renderer-owned Wayland/Vulkan presentation transaction",
                payload: "selected opaque frame lease, PTS deadline, output surface and compositor pacing evidence",
                serial_rule: "present only uses the frame serial accepted by render; stale frames are discarded before present",
                capacity_rule: "latest-present intent is keep-last; high-refresh presents may reuse the current PTS frame without another decode",
                copy_cost_rule: "present path may still copy through swapchain/compositor; zero-copy claims must stay scoped",
                wake_rule: "sleep until next refresh deadline or compositor/event wakeup instead of polling continuously",
            },
        ],
        threads: vec![
            RenderingDeviceVideoFlowThreadContract {
                kind: RenderingDeviceVideoFlowThreadKind::Read,
                owner: RenderingDeviceVideoFlowOwner::FfmpegFrontend,
                ffmpeg_reference: "ffplay.c read_thread and ffmpeg_demux.c demux_thread_func/av_read_frame/demux_send",
                input: "container source",
                output: "packet queue boundary",
                blocking_rule: "may block in demux/read; must wake or stop cleanly on EOS, loop, seek and shutdown",
                replaceable_rule: "FFmpeg owns this stage; replacement requires an identical AVPacket/serial/BSF contract",
            },
            RenderingDeviceVideoFlowThreadContract {
                kind: RenderingDeviceVideoFlowThreadKind::VideoDecode,
                owner: RenderingDeviceVideoFlowOwner::FfmpegHwDecode,
                ffmpeg_reference: "ffplay.c video_thread and ffmpeg_dec.c decoder_thread send/receive flow",
                input: "packet queue boundary",
                output: "opaque retained decoded-plane lease",
                blocking_rule: "blocks on packet/frame readiness; never exposes a descriptor heap, raw image, queue or present loop to Tensor Wallpaper",
                replaceable_rule: "decode stays FFmpeg Vulkan hwaccel on the mainline; unsupported software fallback is an explicit error",
            },
            RenderingDeviceVideoFlowThreadContract {
                kind: RenderingDeviceVideoFlowThreadKind::AudioDecode,
                owner: RenderingDeviceVideoFlowOwner::SeparateAudioPipeline,
                ffmpeg_reference: "ffplay.c audio_thread plus synchronize_audio",
                input: "audio packets or frontend audio runtime",
                output: "audio clock/frame queue",
                blocking_rule: "serial-aware worker drops obsolete clock samples before doing frontend work",
                replaceable_rule: "audio clock backend can change only behind the same FFmpeg-style serial telemetry",
            },
            RenderingDeviceVideoFlowThreadContract {
                kind: RenderingDeviceVideoFlowThreadKind::RenderRefresh,
                owner: RenderingDeviceVideoFlowOwner::SceneRender,
                ffmpeg_reference: "ffplay.c video_refresh",
                input: "retained decoded-plane lease plus PTS/duration policy",
                output: "present pacer",
                blocking_rule: "uses exact PTS/duration delay to retain, repeat or catch up without busy-loop decode",
                replaceable_rule: "renderer owns rendering even when the typed media frontend changes",
            },
            RenderingDeviceVideoFlowThreadContract {
                kind: RenderingDeviceVideoFlowThreadKind::AudioCallback,
                owner: RenderingDeviceVideoFlowOwner::SeparateAudioPipeline,
                ffmpeg_reference: "ffplay.c audio_callback and audclk update",
                input: "audio frame queue/runtime telemetry",
                output: "audio master clock sample",
                blocking_rule: "audio clock sampling is independent from video image lifetime",
                replaceable_rule: "audio output backend can change without changing video texture ownership",
            },
        ],
        invariants: &[
            "FFmpeg under references/tensor-wallpaper/ffmpeg is the first source for queue, serial, clock and refresh semantics",
            "vulkan-renderer owns FFmpeg demux/parser/packet send, Vulkan hw decode, descriptor heaps, render and present; Tensor Wallpaper supplies typed media policy",
            "PacketQueue semantics apply inside the renderer; retained decoded-plane leases apply to keep-last PTS refresh",
            "every cross-thread video/audio handoff carries a serial or is explicitly proven not to cross loop/seek state",
            "lock-free structures are optional; FFmpeg alignment requires bounded ownership, serial invalidation and sleep/wakeup behavior first",
            "copy-reduction evidence must name the boundary: compressed packet retention, bitstream upload, decoded image handoff, render or present",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_contract_uses_local_ffmpeg_as_first_reference() {
        let contract = rendering_device_video_flow_contract();

        assert_eq!(contract.first_reference_root, "references/tensor-wallpaper/ffmpeg");
        assert!(contract.canonical_player_reference.ends_with("ffplay.c"));
        assert!(
            contract
                .canonical_demux_reference
                .ends_with("ffmpeg_demux.c")
        );
        assert!(
            contract
                .canonical_decode_reference
                .ends_with("ffmpeg_dec.c")
        );
        assert!(
            contract
                .invariants
                .iter()
                .any(|invariant| invariant.contains("references/tensor-wallpaper/ffmpeg"))
        );
    }

    #[test]
    fn flow_contract_maps_ffplay_queue_and_thread_split() {
        let contract = rendering_device_video_flow_contract();

        assert_eq!(contract.queues.len(), 4);
        assert_eq!(contract.threads.len(), 5);
        assert!(contract.queues.iter().any(|queue| {
            queue.kind == RenderingDeviceVideoFlowQueueKind::PacketQueue
                && queue.ffmpeg_reference.contains("PacketQueue")
                && queue.serial_rule.contains("advance packet serial")
                && queue.copy_cost_rule.contains("avcodec")
        }));
        assert!(contract.queues.iter().any(|queue| {
            queue.kind == RenderingDeviceVideoFlowQueueKind::DecodedFrameQueue
                && queue.ffmpeg_reference.contains("FrameQueue")
                && queue.capacity_rule.contains("keep-last")
                && queue
                    .copy_cost_rule
                    .contains("decoded images remain renderer-owned")
        }));
        assert!(contract.threads.iter().any(|thread| {
            thread.kind == RenderingDeviceVideoFlowThreadKind::Read
                && thread.ffmpeg_reference.contains("read_thread")
                && thread.replaceable_rule.contains("FFmpeg")
                && thread.replaceable_rule.contains("AVPacket")
        }));
        assert!(contract.threads.iter().any(|thread| {
            thread.kind == RenderingDeviceVideoFlowThreadKind::RenderRefresh
                && thread.ffmpeg_reference.contains("video_refresh")
                && thread.blocking_rule.contains("without busy-loop decode")
        }));
    }

    #[test]
    fn flow_contract_keeps_audio_separate_but_clock_linked() {
        let contract = rendering_device_video_flow_contract();

        assert!(contract.queues.iter().any(|queue| {
            queue.kind == RenderingDeviceVideoFlowQueueKind::AudioFrameQueue
                && queue.owner == RenderingDeviceVideoFlowOwner::SeparateAudioPipeline
                && queue.serial_rule.contains("video loop/seek serial")
                && queue
                    .copy_cost_rule
                    .contains("must not force video-frame copies")
        }));
        assert!(contract.threads.iter().any(|thread| {
            thread.kind == RenderingDeviceVideoFlowThreadKind::AudioCallback
                && thread.owner == RenderingDeviceVideoFlowOwner::SeparateAudioPipeline
                && thread.output.contains("audio master clock")
        }));
    }
}
