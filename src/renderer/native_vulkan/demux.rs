//! Demux-to-decode packet queue boundary for the native Vulkan video path.
//!
//! This module follows the same broad split as FFmpeg: demux/parser output is
//! retained as codec access units, while decode/render code consumes packets
//! with explicit timestamps, loop serials, and parameter-set snapshots. The
//! packet queue is frontend-agnostic; GStreamer is only the current provider.

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::path::Path;
use std::time::{Duration, Instant};

use gst::prelude::*;
use gstreamer as gst;

use super::{NativeVulkanError, native_vulkan_streaming_bootstrap_scan_limit};

pub(super) trait NativeVulkanStreamingAccessUnit: Sized {
    type ParameterSets: Clone;
    type Snapshot: Clone;

    const CODEC_LABEL: &'static str;
    const PARAMETER_SETS_LABEL: &'static str;
    const RING_SLOT_BYTES_ENV: &'static str;
    const DEFAULT_RING_SLOT_COUNT: u32;

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

pub(super) trait NativeVulkanGstStreamingAccessUnit:
    NativeVulkanStreamingAccessUnit
{
    fn pipeline(source: &Path) -> Result<gst::Pipeline, NativeVulkanError>;
    fn sink_name() -> &'static str;
    fn from_sample(sample: &gst::Sample) -> Result<Self, NativeVulkanError>;
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
    pub(super) bootstrap_discarded_access_units: u32,
    pub(super) max_payload_bytes: u64,
}

impl<A: NativeVulkanStreamingAccessUnit> NativeVulkanStreamingPacketQueue<A> {
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
        self.queued.pop_front().ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "{} streaming packet queue is empty",
                A::CODEC_LABEL
            ))
        })
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
                for _ in 1..self.loop_skip_access_units {
                    let skipped = self.frontend.pull_next_access_unit(loop_on_eos)?;
                    self.sync_frontend_counters();
                    if skipped.is_none() {
                        return Ok(None);
                    }
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

pub(super) fn native_vulkan_start_streaming_packet_queue<
    A: NativeVulkanGstStreamingAccessUnit + 'static,
>(
    source: &Path,
    capacity: usize,
) -> Result<NativeVulkanStreamingPacketQueue<A>, NativeVulkanError> {
    native_vulkan_start_gst_streaming_packet_queue::<A>(source, capacity)
}

pub(super) fn native_vulkan_start_gst_streaming_packet_queue<
    A: NativeVulkanGstStreamingAccessUnit + 'static,
>(
    source: &Path,
    capacity: usize,
) -> Result<NativeVulkanStreamingPacketQueue<A>, NativeVulkanError> {
    let frontend = NativeVulkanGstStreamingPacketFrontend::<A>::new(source)?;
    native_vulkan_start_streaming_packet_queue_from_frontend(Box::new(frontend), capacity)
}

pub(super) fn native_vulkan_start_streaming_packet_queue_from_frontend<
    A: NativeVulkanStreamingAccessUnit,
>(
    mut frontend: Box<dyn NativeVulkanStreamingPacketFrontend<A>>,
    capacity: usize,
) -> Result<NativeVulkanStreamingPacketQueue<A>, NativeVulkanError> {
    let capacity = capacity.max(1);
    let mut pending = VecDeque::<A>::with_capacity(capacity);
    let mut selected_parameter_sets_access_unit = None::<Vec<u8>>;
    let mut pulled_count = 0u32;
    let mut max_payload_bytes = 0u64;
    let mut bootstrap_discarded_access_units = 0u32;
    let bootstrap_scan_limit = native_vulkan_streaming_bootstrap_scan_limit(capacity);

    while pending.len() < capacity || selected_parameter_sets_access_unit.is_none() {
        if selected_parameter_sets_access_unit.is_none()
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
        if selected_parameter_sets_access_unit.is_none() && access_unit.has_parameter_sets() {
            selected_parameter_sets_access_unit = Some(access_unit.bytes().to_vec());
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
    let selected_parameter_sets_access_unit =
        selected_parameter_sets_access_unit.ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "{} streaming packet queue did not find {} in {} bootstrap packet(s)",
                A::CODEC_LABEL,
                A::PARAMETER_SETS_LABEL,
                pulled_count
            ))
        })?;
    let parameter_sets = A::parse_parameter_sets(&selected_parameter_sets_access_unit)
        .map_err(NativeVulkanError::Video)?;

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
        bootstrap_discarded_access_units,
        max_payload_bytes,
    })
}

struct NativeVulkanGstStreamingPacketFrontend<A: NativeVulkanGstStreamingAccessUnit> {
    pipeline: gst::Pipeline,
    sink: gst::Element,
    bus: gst::Bus,
    eos_count: u32,
    loop_count: u32,
    _access_unit: PhantomData<A>,
}

impl<A: NativeVulkanGstStreamingAccessUnit> NativeVulkanGstStreamingPacketFrontend<A> {
    fn new(source: &Path) -> Result<Self, NativeVulkanError> {
        gst::init().map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        let pipeline = A::pipeline(source)?;
        let sink = pipeline.by_name(A::sink_name()).ok_or_else(|| {
            NativeVulkanError::Video(format!("{} bitstream appsink not found", A::CODEC_LABEL))
        })?;
        let bus = pipeline.bus().ok_or_else(|| {
            NativeVulkanError::Video(format!("{} bitstream pipeline has no bus", A::CODEC_LABEL))
        })?;

        pipeline
            .set_state(gst::State::Playing)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;

        Ok(Self {
            pipeline,
            sink,
            bus,
            eos_count: 0,
            loop_count: 0,
            _access_unit: PhantomData,
        })
    }
}

impl<A: NativeVulkanGstStreamingAccessUnit> Drop for NativeVulkanGstStreamingPacketFrontend<A> {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        let _ = self.pipeline.state(gst::ClockTime::from_mseconds(500));
    }
}

impl<A: NativeVulkanGstStreamingAccessUnit> NativeVulkanStreamingPacketFrontend<A>
    for NativeVulkanGstStreamingPacketFrontend<A>
{
    fn pull_next_access_unit(&mut self, loop_on_eos: bool) -> Result<Option<A>, NativeVulkanError> {
        native_vulkan_pull_gst_streaming_access_unit::<A>(
            &self.pipeline,
            &self.sink,
            &self.bus,
            loop_on_eos,
            &mut self.eos_count,
            &mut self.loop_count,
        )
    }

    fn eos_count(&self) -> u32 {
        self.eos_count
    }

    fn loop_count(&self) -> u32 {
        self.loop_count
    }
}

fn native_vulkan_pull_gst_streaming_access_unit<A: NativeVulkanGstStreamingAccessUnit>(
    pipeline: &gst::Pipeline,
    sink: &gst::Element,
    bus: &gst::Bus,
    loop_on_eos: bool,
    eos_count: &mut u32,
    loop_count: &mut u32,
) -> Result<Option<A>, NativeVulkanError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let mut saw_bus_eos = false;
        while let Some(message) = bus.pop() {
            match message.view() {
                gst::MessageView::Error(err) => {
                    let mut message = format!(
                        "{}: {}",
                        err.src()
                            .map(|src| src.path_string())
                            .unwrap_or_else(|| "gstreamer".into()),
                        err.error()
                    );
                    if let Some(debug) = err.debug() {
                        message.push_str(": ");
                        message.push_str(&debug);
                    }
                    return Err(NativeVulkanError::Video(message));
                }
                gst::MessageView::Eos(_) => {
                    saw_bus_eos = true;
                }
                _ => {}
            }
        }

        let timeout_ns = 50_000_000u64;
        let sample = sink.emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&timeout_ns]);
        if let Some(sample) = sample {
            return A::from_sample(&sample).map(Some);
        }
        if saw_bus_eos || native_vulkan_streaming_sink_is_eos(sink) {
            *eos_count = eos_count.saturating_add(1);
            if !loop_on_eos {
                return Ok(None);
            }
            pipeline
                .seek_simple(
                    gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                    gst::ClockTime::ZERO,
                )
                .map_err(|err| {
                    NativeVulkanError::Video(format!(
                        "seek {} streaming packet queue to start: {err}",
                        A::CODEC_LABEL
                    ))
                })?;
            *loop_count = loop_count.saturating_add(1);
        }
    }

    Err(NativeVulkanError::Video(format!(
        "{} streaming packet queue timed out waiting for an AU",
        A::CODEC_LABEL
    )))
}

fn native_vulkan_streaming_sink_is_eos(sink: &gst::Element) -> bool {
    sink.find_property("eos").is_some() && sink.property::<bool>("eos")
}
