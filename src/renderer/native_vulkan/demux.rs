//! Demux-to-decode packet queue boundary for the native Vulkan video path.
//!
//! This module follows the same broad split as FFmpeg: demux/parser output is
//! retained as codec access units, while decode/render code consumes packets
//! with explicit timestamps, loop serials, and parameter-set snapshots. The
//! packet queue is frontend-agnostic inside the renderer; the current provider
//! is the FFmpeg reader.

use std::collections::VecDeque;

use serde::Serialize;

use super::{NativeVulkanError, native_vulkan_streaming_bootstrap_scan_limit};

pub(super) const FFMPEG_VIDEO_PICTURE_QUEUE_SIZE: usize = 3;
pub(super) const NATIVE_VULKAN_PACKET_HANDOFF_FRAMES: usize = FFMPEG_VIDEO_PICTURE_QUEUE_SIZE;

pub(super) trait NativeVulkanStreamingAccessUnit: Sized {
    type ParameterSets: Clone;
    type Snapshot: Clone;

    const CODEC_LABEL: &'static str;
    const PARAMETER_SETS_LABEL: &'static str;

    fn parse_parameter_sets(bytes: &[u8]) -> Result<Self::ParameterSets, String>;
    fn snapshot(
        index: u32,
        access_unit: &Self,
        parameter_sets: &Self::ParameterSets,
    ) -> Self::Snapshot;
    fn bytes(&self) -> &[u8];
    fn pts_ms(&self) -> Option<u64>;
    fn duration_ms(&self) -> Option<u64>;
    fn has_parameter_sets(&self) -> bool;
    fn is_random_access(&self) -> bool;
    fn is_random_access_with_parameter_sets(&self, _parameter_sets: &Self::ParameterSets) -> bool {
        self.is_random_access()
    }
}

pub(super) trait NativeVulkanStreamingPacketFrontend<A: NativeVulkanStreamingAccessUnit> {
    fn pull_next_access_unit(&mut self, loop_on_eos: bool) -> Result<Option<A>, NativeVulkanError>;
    fn eos_count(&self) -> u32;
    fn loop_count(&self) -> u32;
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanStreamingPacketTimeline {
    pub(super) access_unit_index: u32,
    pub(super) source_loop_index: u32,
    pub(super) pts_ms: Option<u64>,
    pub(super) duration_ms: Option<u64>,
}

pub(super) struct NativeVulkanStreamingPacket<A: NativeVulkanStreamingAccessUnit> {
    pub(super) access_unit: A,
    pub(super) snapshot: A::Snapshot,
    pub(super) source_loop_index: u32,
    #[allow(dead_code)]
    pub(super) timeline: NativeVulkanStreamingPacketTimeline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanStreamingPacketQueueRuntimeSnapshot {
    pub codec: &'static str,
    pub boundary: &'static str,
    pub first_reference: &'static str,
    pub frontend_contract: &'static str,
    pub queue_policy: &'static str,
    pub frame_keep_last_policy: &'static str,
    pub serial_model: &'static str,
    pub capacity: u32,
    pub queued_packets: u32,
    pub pulled_packets: u32,
    pub current_serial: u32,
    pub front_serial: Option<u32>,
    pub back_serial: Option<u32>,
    pub front_pts_ms: Option<u64>,
    pub back_pts_ms: Option<u64>,
    pub retained_payload_bytes: u64,
    pub max_payload_bytes: u64,
    pub bootstrap_discarded_packets: u32,
    pub loop_skip_packets: u32,
    pub loop_skipped_packets: u32,
    pub eos_count: u32,
}

pub(super) struct NativeVulkanStreamingPacketQueue<A: NativeVulkanStreamingAccessUnit> {
    frontend: Box<dyn NativeVulkanStreamingPacketFrontend<A>>,
    pub(super) parameter_sets: A::ParameterSets,
    pub(super) queued: VecDeque<NativeVulkanStreamingPacket<A>>,
    pub(super) capacity: usize,
    next_access_unit_index: u32,
    pub(super) pulled_count: u32,
    pub(super) eos_count: u32,
    pub(super) loop_count: u32,
    pub(super) loop_skip_access_units: u32,
    pub(super) loop_skipped_access_units: u32,
    pub(super) bootstrap_discarded_access_units: u32,
    pub(super) max_payload_bytes: u64,
}

impl<A: NativeVulkanStreamingAccessUnit> NativeVulkanStreamingPacketQueue<A> {
    pub(super) fn runtime_snapshot(&self) -> NativeVulkanStreamingPacketQueueRuntimeSnapshot {
        NativeVulkanStreamingPacketQueueRuntimeSnapshot {
            codec: A::CODEC_LABEL,
            boundary: "replaceable-demux-parser-to-native-decode",
            first_reference: "FFmpeg ffplay PacketQueue serial and bounded packet ownership",
            frontend_contract: "frontend supplies encoded access units/temporal units; native Vulkan owns codec state, decode, render and present",
            queue_policy: "bounded FIFO packet handoff with FFmpeg av_packet_move_ref-style ownership; payload is released immediately after bitstream upload and decoded-frame keep_last=1 is downstream",
            frame_keep_last_policy: "decoded-frame keep-last/direct-DPB ownership is downstream of this packet queue",
            serial_model: "source loop count is the packet serial; stale packets/frames are rejected across loop or seek boundaries",
            capacity: self.capacity.min(u32::MAX as usize) as u32,
            queued_packets: self.queued.len().min(u32::MAX as usize) as u32,
            pulled_packets: self.pulled_count,
            current_serial: self.loop_count,
            front_serial: self.queued.front().map(|packet| packet.source_loop_index),
            back_serial: self.queued.back().map(|packet| packet.source_loop_index),
            front_pts_ms: self
                .queued
                .front()
                .and_then(|packet| packet.timeline.pts_ms),
            back_pts_ms: self.queued.back().and_then(|packet| packet.timeline.pts_ms),
            retained_payload_bytes: self.retained_payload_bytes(),
            max_payload_bytes: self.max_payload_bytes,
            bootstrap_discarded_packets: self.bootstrap_discarded_access_units,
            loop_skip_packets: self.loop_skip_access_units,
            loop_skipped_packets: self.loop_skipped_access_units,
            eos_count: self.eos_count,
        }
    }

    pub(super) fn bootstrap_access_units(&self) -> Vec<A::Snapshot> {
        self.queued
            .iter()
            .map(|packet| packet.snapshot.clone())
            .collect()
    }

    pub(super) fn retained_payload_bytes(&self) -> u64 {
        self.queued
            .iter()
            .map(|packet| packet.access_unit.bytes().len() as u64)
            .sum()
    }

    pub(super) fn next_packet(
        &mut self,
        loop_on_eos: bool,
    ) -> Result<NativeVulkanStreamingPacket<A>, NativeVulkanError> {
        while self.queued.is_empty() {
            self.fill_one(loop_on_eos)?;
        }
        let packet = self.queued.pop_front().ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "{} streaming packet queue is empty",
                A::CODEC_LABEL
            ))
        })?;
        if self.queued.len() < self.capacity {
            let _ = self.try_fill_one(loop_on_eos)?;
        }
        Ok(packet)
    }

    pub(super) fn ensure_front_packet(
        &mut self,
        loop_on_eos: bool,
    ) -> Result<bool, NativeVulkanError> {
        while self.queued.is_empty() {
            if !self.try_fill_one(loop_on_eos)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn front_packet(&self) -> Option<&NativeVulkanStreamingPacket<A>> {
        self.queued.front()
    }

    pub(super) fn replace_parameter_sets(&mut self, parameter_sets: A::ParameterSets) {
        self.parameter_sets = parameter_sets;
        for packet in &mut self.queued {
            packet.snapshot = A::snapshot(
                packet.timeline.access_unit_index,
                &packet.access_unit,
                &self.parameter_sets,
            );
        }
    }

    pub(super) fn discard_front_for_bootstrap(
        &mut self,
    ) -> Result<Option<NativeVulkanStreamingPacket<A>>, NativeVulkanError> {
        let dropped = self.queued.pop_front();
        if dropped.is_some() {
            self.bootstrap_discarded_access_units =
                self.bootstrap_discarded_access_units.saturating_add(1);
            if self.eos_count == 0 {
                let _ = self.try_fill_one(false)?;
            }
        }
        Ok(dropped)
    }

    pub(super) fn set_loop_skip_access_units(&mut self, skip_access_units: u32) {
        self.loop_skip_access_units = skip_access_units;
    }

    fn pull_next_access_unit(&mut self, loop_on_eos: bool) -> Result<Option<A>, NativeVulkanError> {
        loop {
            let before_loop_count = self.loop_count;
            let access_unit = self.frontend.pull_next_access_unit(loop_on_eos)?;
            self.sync_frontend_counters();
            let Some(access_unit) = access_unit else {
                return Ok(None);
            };
            if loop_on_eos
                && self.loop_skip_access_units > 0
                && self.loop_count != before_loop_count
                && !access_unit.is_random_access_with_parameter_sets(&self.parameter_sets)
            {
                self.loop_skipped_access_units = self.loop_skipped_access_units.saturating_add(1);
                for _ in 1..self.loop_skip_access_units {
                    let skipped = self.frontend.pull_next_access_unit(loop_on_eos)?;
                    self.sync_frontend_counters();
                    if skipped.is_none() {
                        return Ok(None);
                    }
                    self.loop_skipped_access_units =
                        self.loop_skipped_access_units.saturating_add(1);
                }
                continue;
            }
            return Ok(Some(access_unit));
        }
    }

    fn sync_frontend_counters(&mut self) {
        self.eos_count = self.frontend.eos_count();
        self.loop_count = self.frontend.loop_count();
    }

    fn try_fill_one(&mut self, loop_on_eos: bool) -> Result<bool, NativeVulkanError> {
        if self.queued.len() >= self.capacity {
            return Ok(true);
        }
        let Some(access_unit) = self.pull_next_access_unit(loop_on_eos)? else {
            return Ok(false);
        };
        self.pulled_count = self.pulled_count.saturating_add(1);
        self.max_payload_bytes = self.max_payload_bytes.max(access_unit.bytes().len() as u64);
        let access_unit_index = self.next_access_unit_index;
        let snapshot = A::snapshot(access_unit_index, &access_unit, &self.parameter_sets);
        self.next_access_unit_index = self.next_access_unit_index.saturating_add(1);
        self.queued.push_back(NativeVulkanStreamingPacket {
            timeline: NativeVulkanStreamingPacketTimeline {
                access_unit_index,
                source_loop_index: self.loop_count,
                pts_ms: access_unit.pts_ms(),
                duration_ms: access_unit.duration_ms(),
            },
            access_unit,
            snapshot,
            source_loop_index: self.loop_count,
        });
        Ok(true)
    }

    fn fill_one(&mut self, loop_on_eos: bool) -> Result<(), NativeVulkanError> {
        if !self.try_fill_one(loop_on_eos)? {
            return Err(NativeVulkanError::Video(format!(
                "{} streaming packet queue reached EOS",
                A::CODEC_LABEL
            )));
        }
        Ok(())
    }
}

pub(super) fn native_vulkan_require_streaming_bootstrap_window<
    A: NativeVulkanStreamingAccessUnit,
>(
    queue: &NativeVulkanStreamingPacketQueue<A>,
    requested_access_units: u32,
) -> Result<(), NativeVulkanError> {
    if queue.queued.len() >= queue.capacity {
        return Ok(());
    }

    Err(NativeVulkanError::Video(format!(
        "{} streaming bootstrap found a decodable entry, but the source ended after {}/{} queued AU(s); requested {requested_access_units} ready-prefix AU(s), discarded {} leading AU(s), eos_count={}. Use a longer post-entry source window or a smaller decode prefix.",
        A::CODEC_LABEL,
        queue.queued.len(),
        queue.capacity,
        queue.bootstrap_discarded_access_units,
        queue.eos_count
    )))
}

pub(super) fn native_vulkan_start_streaming_packet_queue_from_frontend<
    A: NativeVulkanStreamingAccessUnit,
>(
    mut frontend: Box<dyn NativeVulkanStreamingPacketFrontend<A>>,
    capacity: usize,
) -> Result<NativeVulkanStreamingPacketQueue<A>, NativeVulkanError> {
    let capacity = capacity.max(1);
    let mut pending = VecDeque::<A>::with_capacity(capacity);
    let mut selected_parameter_sets = None::<A::ParameterSets>;
    let mut pulled_count = 0u32;
    let mut max_payload_bytes = 0u64;
    let mut bootstrap_discarded_access_units = 0u32;
    let bootstrap_scan_limit = native_vulkan_streaming_bootstrap_scan_limit(capacity);

    while pending.len() < capacity || selected_parameter_sets.is_none() {
        if selected_parameter_sets.is_none()
            && usize::try_from(pulled_count).unwrap_or(usize::MAX) >= bootstrap_scan_limit
        {
            return Err(NativeVulkanError::Video(format!(
                "{} streaming packet queue scanned {bootstrap_scan_limit} bootstrap packet(s) without finding {}",
                A::CODEC_LABEL,
                A::PARAMETER_SETS_LABEL,
            )));
        }
        let Some(access_unit) = frontend.pull_next_access_unit(false)? else {
            break;
        };
        pulled_count = pulled_count.saturating_add(1);
        max_payload_bytes = max_payload_bytes.max(access_unit.bytes().len() as u64);
        if selected_parameter_sets.is_none() && access_unit.has_parameter_sets() {
            selected_parameter_sets = Some(
                A::parse_parameter_sets(access_unit.bytes()).map_err(NativeVulkanError::Video)?,
            );
        }
        pending.push_back(access_unit);
        while pending.len() > capacity {
            let _ = pending.pop_front();
            bootstrap_discarded_access_units = bootstrap_discarded_access_units.saturating_add(1);
        }
    }

    if pending.is_empty() {
        return Err(NativeVulkanError::Video(format!(
            "{} streaming packet queue produced no bootstrap packets",
            A::CODEC_LABEL
        )));
    }
    let parameter_sets = selected_parameter_sets.ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "{} streaming packet queue did not find {} in {} bootstrap packet(s)",
            A::CODEC_LABEL,
            A::PARAMETER_SETS_LABEL,
            pulled_count
        ))
    })?;

    let mut queued = VecDeque::with_capacity(capacity);
    for (index, access_unit) in pending.into_iter().enumerate() {
        let access_unit_index = index as u32;
        let snapshot = A::snapshot(access_unit_index, &access_unit, &parameter_sets);
        queued.push_back(NativeVulkanStreamingPacket {
            timeline: NativeVulkanStreamingPacketTimeline {
                access_unit_index,
                source_loop_index: frontend.loop_count(),
                pts_ms: access_unit.pts_ms(),
                duration_ms: access_unit.duration_ms(),
            },
            access_unit,
            snapshot,
            source_loop_index: frontend.loop_count(),
        });
    }
    let next_access_unit_index = queued.len().min(u32::MAX as usize) as u32;
    let eos_count = frontend.eos_count();
    let loop_count = frontend.loop_count();

    Ok(NativeVulkanStreamingPacketQueue {
        frontend,
        parameter_sets,
        queued,
        capacity,
        next_access_unit_index,
        pulled_count,
        eos_count,
        loop_count,
        loop_skip_access_units: 0,
        loop_skipped_access_units: 0,
        bootstrap_discarded_access_units,
        max_payload_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestAccessUnit {
        bytes: Vec<u8>,
        pts_ms: Option<u64>,
        parameter_sets: bool,
        random_access: bool,
    }

    impl NativeVulkanStreamingAccessUnit for TestAccessUnit {
        type ParameterSets = Vec<u8>;
        type Snapshot = usize;

        const CODEC_LABEL: &'static str = "test-codec";
        const PARAMETER_SETS_LABEL: &'static str = "test-parameter-sets";

        fn parse_parameter_sets(bytes: &[u8]) -> Result<Self::ParameterSets, String> {
            Ok(bytes.to_vec())
        }

        fn snapshot(
            index: u32,
            access_unit: &Self,
            _parameter_sets: &Self::ParameterSets,
        ) -> Self::Snapshot {
            index as usize + access_unit.bytes.len()
        }

        fn bytes(&self) -> &[u8] {
            &self.bytes
        }

        fn pts_ms(&self) -> Option<u64> {
            self.pts_ms
        }

        fn duration_ms(&self) -> Option<u64> {
            Some(4)
        }

        fn has_parameter_sets(&self) -> bool {
            self.parameter_sets
        }

        fn is_random_access(&self) -> bool {
            self.random_access
        }
    }

    struct TestPacketFrontend {
        access_units: VecDeque<TestAccessUnit>,
        eos_count: u32,
        loop_count: u32,
    }

    impl TestPacketFrontend {
        fn new(access_units: Vec<TestAccessUnit>, loop_count: u32) -> Self {
            Self {
                access_units: access_units.into(),
                eos_count: 0,
                loop_count,
            }
        }
    }

    struct TestLoopingPacketFrontend {
        bootstrap: VecDeque<TestAccessUnit>,
        loop_access_units: Vec<TestAccessUnit>,
        loop_position: usize,
        eos_count: u32,
        loop_count: u32,
        bootstrapping: bool,
        pending_loop_after_eos: bool,
    }

    impl TestLoopingPacketFrontend {
        fn new(bootstrap: Vec<TestAccessUnit>, loop_access_units: Vec<TestAccessUnit>) -> Self {
            Self {
                bootstrap: bootstrap.into(),
                loop_access_units,
                loop_position: 0,
                eos_count: 0,
                loop_count: 0,
                bootstrapping: true,
                pending_loop_after_eos: false,
            }
        }
    }

    impl NativeVulkanStreamingPacketFrontend<TestAccessUnit> for TestPacketFrontend {
        fn pull_next_access_unit(
            &mut self,
            _loop_on_eos: bool,
        ) -> Result<Option<TestAccessUnit>, NativeVulkanError> {
            let access_unit = self.access_units.pop_front();
            if access_unit.is_none() {
                self.eos_count = self.eos_count.saturating_add(1);
            }
            Ok(access_unit)
        }

        fn eos_count(&self) -> u32 {
            self.eos_count
        }

        fn loop_count(&self) -> u32 {
            self.loop_count
        }
    }

    impl NativeVulkanStreamingPacketFrontend<TestAccessUnit> for TestLoopingPacketFrontend {
        fn pull_next_access_unit(
            &mut self,
            loop_on_eos: bool,
        ) -> Result<Option<TestAccessUnit>, NativeVulkanError> {
            if self.pending_loop_after_eos {
                if !loop_on_eos {
                    return Ok(None);
                }
                self.pending_loop_after_eos = false;
                self.loop_count = self.loop_count.saturating_add(1);
                self.loop_position = 0;
            }
            if self.bootstrapping {
                if let Some(access_unit) = self.bootstrap.pop_front() {
                    return Ok(Some(access_unit));
                }
                self.bootstrapping = false;
                self.eos_count = self.eos_count.saturating_add(1);
                if !loop_on_eos {
                    self.pending_loop_after_eos = true;
                    return Ok(None);
                }
                self.loop_count = self.loop_count.saturating_add(1);
                self.loop_position = 0;
            }

            if self.loop_access_units.is_empty() {
                self.eos_count = self.eos_count.saturating_add(1);
                return Ok(None);
            }
            if self.loop_position >= self.loop_access_units.len() {
                self.eos_count = self.eos_count.saturating_add(1);
                if !loop_on_eos {
                    self.pending_loop_after_eos = true;
                    return Ok(None);
                }
                self.loop_count = self.loop_count.saturating_add(1);
                self.loop_position = 0;
            }
            let access_unit = self.loop_access_units[self.loop_position].clone();
            self.loop_position += 1;
            Ok(Some(access_unit))
        }

        fn eos_count(&self) -> u32 {
            self.eos_count
        }

        fn loop_count(&self) -> u32 {
            self.loop_count
        }
    }

    #[test]
    fn packet_queue_runtime_snapshot_reports_ffmpeg_boundary() {
        let frontend = TestPacketFrontend::new(
            vec![
                TestAccessUnit {
                    bytes: vec![1, 2, 3],
                    pts_ms: Some(0),
                    parameter_sets: true,
                    random_access: true,
                },
                TestAccessUnit {
                    bytes: vec![4, 5, 6, 7],
                    pts_ms: Some(4),
                    parameter_sets: false,
                    random_access: false,
                },
            ],
            7,
        );

        let queue = native_vulkan_start_streaming_packet_queue_from_frontend(Box::new(frontend), 2)
            .expect("packet queue");

        let snapshot = queue.runtime_snapshot();

        assert_eq!(snapshot.codec, "test-codec");
        assert_eq!(
            snapshot.boundary,
            "replaceable-demux-parser-to-native-decode"
        );
        assert!(snapshot.first_reference.contains("FFmpeg"));
        assert!(snapshot.first_reference.contains("PacketQueue"));
        assert!(snapshot.queue_policy.contains("bounded FIFO"));
        assert!(
            snapshot
                .frontend_contract
                .contains("native Vulkan owns codec state")
        );
        assert!(snapshot.frame_keep_last_policy.contains("direct-DPB"));
        assert_eq!(snapshot.capacity, 2);
        assert_eq!(snapshot.queued_packets, 2);
        assert_eq!(snapshot.pulled_packets, 2);
        assert_eq!(snapshot.current_serial, 7);
        assert_eq!(snapshot.front_serial, Some(7));
        assert_eq!(snapshot.back_serial, Some(7));
        assert_eq!(snapshot.front_pts_ms, Some(0));
        assert_eq!(snapshot.back_pts_ms, Some(4));
        assert_eq!(snapshot.retained_payload_bytes, 7);
        assert_eq!(snapshot.max_payload_bytes, 4);
        assert_eq!(snapshot.bootstrap_discarded_packets, 0);
        assert_eq!(snapshot.loop_skip_packets, 0);
        assert_eq!(snapshot.loop_skipped_packets, 0);
        assert_eq!(snapshot.eos_count, 0);
    }

    #[test]
    fn packet_queue_counts_actual_loop_skipped_packets() {
        let bootstrap = vec![
            TestAccessUnit {
                bytes: vec![1, 2, 3],
                pts_ms: Some(0),
                parameter_sets: true,
                random_access: true,
            },
            TestAccessUnit {
                bytes: vec![4, 5, 6],
                pts_ms: Some(4),
                parameter_sets: false,
                random_access: false,
            },
        ];
        let loop_access_units = vec![
            TestAccessUnit {
                bytes: vec![7],
                pts_ms: Some(0),
                parameter_sets: false,
                random_access: false,
            },
            TestAccessUnit {
                bytes: vec![8],
                pts_ms: Some(4),
                parameter_sets: false,
                random_access: false,
            },
            TestAccessUnit {
                bytes: vec![1, 2, 3],
                pts_ms: Some(8),
                parameter_sets: true,
                random_access: true,
            },
            TestAccessUnit {
                bytes: vec![9],
                pts_ms: Some(12),
                parameter_sets: false,
                random_access: false,
            },
        ];
        let mut queue = native_vulkan_start_streaming_packet_queue_from_frontend(
            Box::new(TestLoopingPacketFrontend::new(bootstrap, loop_access_units)),
            2,
        )
        .expect("packet queue");
        queue.set_loop_skip_access_units(2);

        assert_eq!(
            queue
                .next_packet(true)
                .expect("first bootstrap packet")
                .access_unit
                .pts_ms(),
            Some(0)
        );
        assert_eq!(
            queue
                .next_packet(true)
                .expect("second bootstrap packet")
                .access_unit
                .pts_ms(),
            Some(4)
        );
        let recovered = queue.next_packet(true).expect("loop recovery packet");

        assert_eq!(recovered.access_unit.pts_ms(), Some(8));
        assert_eq!(recovered.source_loop_index, 1);
        assert_eq!(queue.loop_skip_access_units, 2);
        assert_eq!(queue.loop_skipped_access_units, 4);

        let snapshot = queue.runtime_snapshot();
        assert_eq!(snapshot.loop_skip_packets, 2);
        assert_eq!(snapshot.loop_skipped_packets, 4);
        assert_eq!(snapshot.current_serial, 2);
        assert_eq!(snapshot.front_serial, Some(1));
        assert_eq!(snapshot.back_serial, Some(2));
        assert_eq!(snapshot.front_pts_ms, Some(12));
    }

    #[test]
    fn packet_queue_refill_honors_loop_before_queue_is_empty() {
        let bootstrap = vec![
            TestAccessUnit {
                bytes: vec![1, 2, 3],
                pts_ms: Some(0),
                parameter_sets: true,
                random_access: true,
            },
            TestAccessUnit {
                bytes: vec![4, 5, 6],
                pts_ms: Some(4),
                parameter_sets: false,
                random_access: false,
            },
            TestAccessUnit {
                bytes: vec![7, 8, 9],
                pts_ms: Some(8),
                parameter_sets: false,
                random_access: false,
            },
        ];
        let loop_access_units = vec![
            TestAccessUnit {
                bytes: vec![1, 2, 3],
                pts_ms: Some(0),
                parameter_sets: true,
                random_access: true,
            },
            TestAccessUnit {
                bytes: vec![10],
                pts_ms: Some(4),
                parameter_sets: false,
                random_access: false,
            },
        ];
        let mut queue = native_vulkan_start_streaming_packet_queue_from_frontend(
            Box::new(TestLoopingPacketFrontend::new(bootstrap, loop_access_units)),
            3,
        )
        .expect("packet queue");

        assert_eq!(
            queue.next_packet(true).expect("packet 0").source_loop_index,
            0
        );
        assert_eq!(
            queue.next_packet(true).expect("packet 1").source_loop_index,
            0
        );
        assert_eq!(
            queue.next_packet(true).expect("packet 2").source_loop_index,
            0
        );
        assert_eq!(
            queue
                .next_packet(true)
                .expect("loop packet")
                .source_loop_index,
            1
        );
        assert_eq!(
            queue
                .next_packet(true)
                .expect("same-loop packet")
                .source_loop_index,
            1
        );
        assert!(queue.eos_count >= 1);
        assert!(queue.loop_count >= 1);
    }
}
