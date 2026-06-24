//! GStreamer packet frontend for the native Vulkan demux boundary.
//!
//! This module is intentionally a provider: it turns GStreamer appsink samples
//! into codec access units, then hands them to the frontend-agnostic packet
//! queue in `demux.rs`.

use std::marker::PhantomData;
use std::path::Path;
use std::time::{Duration, Instant};

use gst::prelude::*;
use gstreamer as gst;

use super::NativeVulkanError;
use super::demux::{
    NativeVulkanStreamingAccessUnit, NativeVulkanStreamingPacketFrontend,
    NativeVulkanStreamingPacketQueue, native_vulkan_start_streaming_packet_queue_from_frontend,
};

pub(super) trait NativeVulkanGstStreamingAccessUnit:
    NativeVulkanStreamingAccessUnit
{
    fn pipeline(source: &Path) -> Result<gst::Pipeline, NativeVulkanError>;
    fn sink_name() -> &'static str;
    fn from_sample(sample: &gst::Sample) -> Result<Self, NativeVulkanError>;
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
