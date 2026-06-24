use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanVideoFlowOwner {
    ReplaceableFrontend,
    NativePacketBoundary,
    NativeCodec,
    NativeRender,
    NativePresent,
    SeparateAudioPipeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanVideoFlowQueueKind {
    PacketQueue,
    DecodedFrameQueue,
    AudioFrameQueue,
    PresentPacer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanVideoFlowThreadKind {
    Read,
    VideoDecode,
    AudioDecode,
    RenderRefresh,
    AudioCallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoFlowQueueContract {
    pub kind: NativeVulkanVideoFlowQueueKind,
    pub owner: NativeVulkanVideoFlowOwner,
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
pub struct NativeVulkanVideoFlowThreadContract {
    pub kind: NativeVulkanVideoFlowThreadKind,
    pub owner: NativeVulkanVideoFlowOwner,
    pub ffmpeg_reference: &'static str,
    pub input: &'static str,
    pub output: &'static str,
    pub blocking_rule: &'static str,
    pub replaceable_rule: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoFlowContract {
    pub first_reference_root: &'static str,
    pub canonical_player_reference: &'static str,
    pub canonical_demux_reference: &'static str,
    pub canonical_decode_reference: &'static str,
    pub queues: Vec<NativeVulkanVideoFlowQueueContract>,
    pub threads: Vec<NativeVulkanVideoFlowThreadContract>,
    pub invariants: &'static [&'static str],
}

pub fn native_vulkan_video_flow_contract() -> NativeVulkanVideoFlowContract {
    NativeVulkanVideoFlowContract {
        first_reference_root: "references/ffmpeg",
        canonical_player_reference: "references/ffmpeg/fftools/ffplay.c",
        canonical_demux_reference: "references/ffmpeg/fftools/ffmpeg_demux.c",
        canonical_decode_reference: "references/ffmpeg/fftools/ffmpeg_dec.c",
        queues: vec![
            NativeVulkanVideoFlowQueueContract {
                kind: NativeVulkanVideoFlowQueueKind::PacketQueue,
                owner: NativeVulkanVideoFlowOwner::NativePacketBoundary,
                ffmpeg_reference: "ffplay.c PacketQueue: packet_queue_put/get/flush/start and queue serial",
                producer: "read/demux/parser frontend",
                consumer: "native codec decode worker",
                payload: "encoded access unit or temporal unit with pts/duration and parameter-set snapshot",
                serial_rule: "flush, seek and loop advance packet serial; decode, audio and frame samples with older serial are stale",
                capacity_rule: "bounded queue; bootstrap may retain only a keep-last prefix required to recover parameter sets and first decodable frame",
                copy_cost_rule: "compressed payload is retained only until upload into the Vulkan Video bitstream ring",
                wake_rule: "producer wakes decode when queue becomes non-empty; consumer never busy-spins on empty queue",
            },
            NativeVulkanVideoFlowQueueContract {
                kind: NativeVulkanVideoFlowQueueKind::DecodedFrameQueue,
                owner: NativeVulkanVideoFlowOwner::NativeCodec,
                ffmpeg_reference: "ffplay.c FrameQueue: pictq with keep_last and per-frame serial",
                producer: "native Vulkan Video decode",
                consumer: "render refresh",
                payload: "decoded DPB/output image identity, layout, timeline/fence, pts and serial",
                serial_rule: "frame serial must match the current packet queue serial before render",
                capacity_rule: "small keep-last queue; old displayed frame may be retained for refresh without copying pixel data",
                copy_cost_rule: "decoded images remain GPU-owned; display handoff must not copy decoded planes unless telemetry names that fallback",
                wake_rule: "decode wakes render when a displayable frame is ready; render sleeps/paces to the selected master clock",
            },
            NativeVulkanVideoFlowQueueContract {
                kind: NativeVulkanVideoFlowQueueKind::AudioFrameQueue,
                owner: NativeVulkanVideoFlowOwner::SeparateAudioPipeline,
                ffmpeg_reference: "ffplay.c sampq/audclk: audio frame queue, audio packet serial and synchronize_audio",
                producer: "audio decode frontend",
                consumer: "audio callback/runtime clock",
                payload: "decoded audio frame timing, sample rate/layout/format metadata and serial",
                serial_rule: "audio sample serial must match audio queue serial; video loop/seek serial invalidates stale clock samples",
                capacity_rule: "audio queue may be deeper than video but remains bounded by the frontend runtime",
                copy_cost_rule: "audio samples are independent from video texture ownership and must not force video-frame copies",
                wake_rule: "audio runtime wakes on clock sample or loop seek; stale samples are dropped before frontend work",
            },
            NativeVulkanVideoFlowQueueContract {
                kind: NativeVulkanVideoFlowQueueKind::PresentPacer,
                owner: NativeVulkanVideoFlowOwner::NativePresent,
                ffmpeg_reference: "ffplay.c video_refresh: compute_target_delay, master clock comparison and remaining_time sleep",
                producer: "render refresh",
                consumer: "Wayland/Vulkan present",
                payload: "selected frame identity, target present time, output surface and compositor pacing evidence",
                serial_rule: "present only uses the frame serial accepted by render; stale frames are discarded before present",
                capacity_rule: "latest-present intent is keep-last; duplicated presents are avoided when the displayed frame and clock permit",
                copy_cost_rule: "present path may still copy through swapchain/compositor; zero-copy claims must stay scoped",
                wake_rule: "sleep until next refresh deadline or compositor/event wakeup instead of polling continuously",
            },
        ],
        threads: vec![
            NativeVulkanVideoFlowThreadContract {
                kind: NativeVulkanVideoFlowThreadKind::Read,
                owner: NativeVulkanVideoFlowOwner::ReplaceableFrontend,
                ffmpeg_reference: "ffplay.c read_thread and ffmpeg_demux.c demux_thread_func/av_read_frame/demux_send",
                input: "container source",
                output: "packet queue boundary",
                blocking_rule: "may block in demux/read; must wake or stop cleanly on EOS, loop, seek and shutdown",
                replaceable_rule: "GStreamer, libavformat or native demux can own this stage if the packet contract is identical",
            },
            NativeVulkanVideoFlowThreadContract {
                kind: NativeVulkanVideoFlowThreadKind::VideoDecode,
                owner: NativeVulkanVideoFlowOwner::NativeCodec,
                ffmpeg_reference: "ffplay.c video_thread and ffmpeg_dec.c decoder_thread send/receive flow",
                input: "packet queue boundary",
                output: "decoded frame queue",
                blocking_rule: "blocks on packet/frame readiness; never owns display sink or present loop",
                replaceable_rule: "direct route keeps decode in Gilder; decoded-frame route can replace this with provider decode",
            },
            NativeVulkanVideoFlowThreadContract {
                kind: NativeVulkanVideoFlowThreadKind::AudioDecode,
                owner: NativeVulkanVideoFlowOwner::SeparateAudioPipeline,
                ffmpeg_reference: "ffplay.c audio_thread plus synchronize_audio",
                input: "audio packets or frontend audio runtime",
                output: "audio clock/frame queue",
                blocking_rule: "serial-aware worker drops obsolete clock samples before doing frontend work",
                replaceable_rule: "GStreamer audio clock is current frontend; libav/audio backends can replace it behind the same telemetry",
            },
            NativeVulkanVideoFlowThreadContract {
                kind: NativeVulkanVideoFlowThreadKind::RenderRefresh,
                owner: NativeVulkanVideoFlowOwner::NativeRender,
                ffmpeg_reference: "ffplay.c video_refresh",
                input: "decoded frame queue plus master clock",
                output: "present pacer",
                blocking_rule: "uses frame duration/master-clock delay to sleep or skip; does not busy-loop on missing frames",
                replaceable_rule: "render stays native Vulkan even when the frontend demux/decode provider changes",
            },
            NativeVulkanVideoFlowThreadContract {
                kind: NativeVulkanVideoFlowThreadKind::AudioCallback,
                owner: NativeVulkanVideoFlowOwner::SeparateAudioPipeline,
                ffmpeg_reference: "ffplay.c audio_callback and audclk update",
                input: "audio frame queue/runtime telemetry",
                output: "audio master clock sample",
                blocking_rule: "audio clock sampling is independent from video image lifetime",
                replaceable_rule: "audio output backend can change without changing video texture ownership",
            },
        ],
        invariants: &[
            "FFmpeg under references/ffmpeg is the first source for queue, serial, clock and refresh semantics",
            "the frontend may be GStreamer, libavformat or native demux, but it must not own native Vulkan render/present",
            "PacketQueue semantics apply to compressed packets; FrameQueue semantics apply to decoded images and keep-last refresh",
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
        let contract = native_vulkan_video_flow_contract();

        assert_eq!(contract.first_reference_root, "references/ffmpeg");
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
                .any(|invariant| invariant.contains("references/ffmpeg"))
        );
    }

    #[test]
    fn flow_contract_maps_ffplay_queue_and_thread_split() {
        let contract = native_vulkan_video_flow_contract();

        assert_eq!(contract.queues.len(), 4);
        assert_eq!(contract.threads.len(), 5);
        assert!(contract.queues.iter().any(|queue| {
            queue.kind == NativeVulkanVideoFlowQueueKind::PacketQueue
                && queue.ffmpeg_reference.contains("PacketQueue")
                && queue.serial_rule.contains("advance packet serial")
                && queue.copy_cost_rule.contains("bitstream ring")
        }));
        assert!(contract.queues.iter().any(|queue| {
            queue.kind == NativeVulkanVideoFlowQueueKind::DecodedFrameQueue
                && queue.ffmpeg_reference.contains("FrameQueue")
                && queue.capacity_rule.contains("keep-last")
                && queue.copy_cost_rule.contains("GPU-owned")
        }));
        assert!(contract.threads.iter().any(|thread| {
            thread.kind == NativeVulkanVideoFlowThreadKind::Read
                && thread.ffmpeg_reference.contains("read_thread")
                && thread.replaceable_rule.contains("GStreamer")
                && thread.replaceable_rule.contains("libavformat")
        }));
        assert!(contract.threads.iter().any(|thread| {
            thread.kind == NativeVulkanVideoFlowThreadKind::RenderRefresh
                && thread.ffmpeg_reference.contains("video_refresh")
                && thread.blocking_rule.contains("does not busy-loop")
        }));
    }

    #[test]
    fn flow_contract_keeps_audio_separate_but_clock_linked() {
        let contract = native_vulkan_video_flow_contract();

        assert!(contract.queues.iter().any(|queue| {
            queue.kind == NativeVulkanVideoFlowQueueKind::AudioFrameQueue
                && queue.owner == NativeVulkanVideoFlowOwner::SeparateAudioPipeline
                && queue.serial_rule.contains("video loop/seek serial")
                && queue
                    .copy_cost_rule
                    .contains("must not force video-frame copies")
        }));
        assert!(contract.threads.iter().any(|thread| {
            thread.kind == NativeVulkanVideoFlowThreadKind::AudioCallback
                && thread.owner == NativeVulkanVideoFlowOwner::SeparateAudioPipeline
                && thread.output.contains("audio master clock")
        }));
    }
}
