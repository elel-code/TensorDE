use std::path::Path;

use super::device::FfmpegVulkanDevice;
use super::ffi;
use super::frame::move_decoded_frame;
use super::resources::{
    CodecContext, FormatContext, Packet, ReusableFrame, error_again, error_eof, ffmpeg_error,
};
use super::{DecodedVideoFormat, DecodedVideoFrame, FfmpegTimeBase, FfmpegVideoCodec};
use crate::video::VideoDecodeDevice;
use crate::{Error, Extent2D, Result};

/// Single-owner FFmpeg send/receive decoder on the renderer's Vulkan device.
pub struct FfmpegVulkanDecoder {
    reusable_frame: ReusableFrame,
    packet: Packet,
    codec_context: CodecContext,
    format: FormatContext,
    hw_device: FfmpegVulkanDevice,
    codec: FfmpegVideoCodec,
    packet_pending: bool,
    draining: bool,
    end_of_stream_count: u32,
    loop_count: u32,
    sent_packet_count: u64,
    sent_packet_payload_bytes: u64,
    max_packet_size_bytes: u32,
}

impl std::fmt::Debug for FfmpegVulkanDecoder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FfmpegVulkanDecoder")
            .field("codec", &self.codec)
            .field("decoder_name", &self.codec_context.decoder_name())
            .field("coded_extent", &self.coded_extent())
            .field("time_base", &self.time_base())
            .field("end_of_stream_count", &self.end_of_stream_count)
            .field("loop_count", &self.loop_count)
            .field("sent_packet_count", &self.sent_packet_count)
            .finish_non_exhaustive()
    }
}

impl VideoDecodeDevice {
    /// Opens an exact-profile FFmpeg Vulkan decoder on this renderer-owned
    /// logical device. No FFmpeg or Vulkan handle is borrowed from the caller.
    pub fn open_ffmpeg_decoder(
        &self,
        source: impl AsRef<Path>,
        codec: FfmpegVideoCodec,
    ) -> Result<FfmpegVulkanDecoder> {
        if !self.requirements().codecs().contains(codec.requirement()) {
            return Err(Error::VideoDecode(format!(
                "codec profile {codec:?} was not requested when the logical device was created"
            )));
        }
        FfmpegVulkanDecoder::open(source.as_ref(), codec, self)
    }
}

impl FfmpegVulkanDecoder {
    fn open(
        source: &Path,
        codec: FfmpegVideoCodec,
        decode_device: &VideoDecodeDevice,
    ) -> Result<Self> {
        let hw_device = FfmpegVulkanDevice::create(decode_device)?;
        let format = FormatContext::open(source, codec)?;
        let codec_context = CodecContext::open(&format, &hw_device)?;
        let packet = Packet::allocate()?;
        let reusable_frame = ReusableFrame::allocate()?;
        Ok(Self {
            reusable_frame,
            packet,
            codec_context,
            format,
            hw_device,
            codec,
            packet_pending: false,
            draining: false,
            end_of_stream_count: 0,
            loop_count: 0,
            sent_packet_count: 0,
            sent_packet_payload_bytes: 0,
            max_packet_size_bytes: 0,
        })
    }

    pub const fn codec(&self) -> FfmpegVideoCodec {
        self.codec
    }

    pub fn decoder_name(&self) -> &str {
        self.codec_context.decoder_name()
    }

    pub fn coded_extent(&self) -> Extent2D {
        let (width, height) = self.codec_context.coded_extent();
        Extent2D::new(width, height)
    }

    pub const fn time_base(&self) -> FfmpegTimeBase {
        self.format.time_base()
    }

    pub const fn end_of_stream_count(&self) -> u32 {
        self.end_of_stream_count
    }

    pub const fn loop_count(&self) -> u32 {
        self.loop_count
    }

    pub const fn sent_packet_count(&self) -> u64 {
        self.sent_packet_count
    }

    pub const fn sent_packet_payload_bytes(&self) -> u64 {
        self.sent_packet_payload_bytes
    }

    pub const fn max_packet_size_bytes(&self) -> u32 {
        self.max_packet_size_bytes
    }

    /// Decodes one retained GPU frame. When `loop_on_eos` is true the demuxer
    /// seeks and flushes only after all decoder output has drained.
    pub fn decode_next_frame(&mut self, loop_on_eos: bool) -> Result<Option<DecodedVideoFrame>> {
        let mut restarted_without_frame = false;
        loop {
            match self.receive_ready_frame()? {
                ReceiveResult::Frame(frame) => return Ok(Some(frame)),
                ReceiveResult::Eof => {
                    if !self.finish_stream(loop_on_eos, &mut restarted_without_frame)? {
                        return Ok(None);
                    }
                    continue;
                }
                ReceiveResult::Again => {}
            }
            if self.draining {
                return Err(Error::VideoDecode(
                    "FFmpeg decoder requested packets after drain was accepted".into(),
                ));
            }
            if self.packet_pending {
                let send = unsafe {
                    ffi::vr_ffmpeg_codec_send(self.codec_context.raw(), self.packet.raw())
                };
                if send == 0 {
                    self.record_packet_stats();
                    self.packet.unref();
                    self.packet_pending = false;
                    continue;
                }
                if send == error_again() {
                    return Err(Error::VideoDecode(
                        "FFmpeg send and receive both returned EAGAIN for one retained packet"
                            .into(),
                    ));
                }
                return Err(ffmpeg_error(send, "send FFmpeg video packet"));
            }
            let read = self.format.read(&mut self.packet);
            if read == 0 {
                if self.packet.stream() != self.format.stream() {
                    self.packet.unref();
                    continue;
                }
                self.packet_pending = true;
                continue;
            }
            self.packet.unref();
            if read != error_eof() {
                return Err(ffmpeg_error(read, "read FFmpeg video packet"));
            }
            let drain =
                unsafe { ffi::vr_ffmpeg_codec_send(self.codec_context.raw(), ffi::null_packet()) };
            if drain == 0 {
                self.draining = true;
                continue;
            }
            if drain == error_eof() {
                if !self.finish_stream(loop_on_eos, &mut restarted_without_frame)? {
                    return Ok(None);
                }
                continue;
            }
            if drain == error_again() {
                return Err(Error::VideoDecode(
                    "FFmpeg drain returned EAGAIN after receive requested input".into(),
                ));
            }
            return Err(ffmpeg_error(drain, "drain FFmpeg Vulkan decoder"));
        }
    }

    fn receive_ready_frame(&mut self) -> Result<ReceiveResult> {
        let receive = unsafe {
            ffi::vr_ffmpeg_codec_receive(self.codec_context.raw(), self.reusable_frame.raw())
        };
        if receive == 0 {
            let frame = move_decoded_frame(
                &mut self.reusable_frame,
                self.hw_device.decode_device(),
                self.format.time_base(),
            )?;
            if frame.format() != expected_format(self.codec) {
                return Err(Error::VideoDecode(format!(
                    "decoded frame format {:?} does not match exact codec profile {:?}",
                    frame.format(),
                    self.codec
                )));
            }
            return Ok(ReceiveResult::Frame(frame));
        }
        self.reusable_frame.unref();
        if receive == error_again() {
            return Ok(ReceiveResult::Again);
        }
        if receive == error_eof() {
            return Ok(ReceiveResult::Eof);
        }
        Err(ffmpeg_error(receive, "receive FFmpeg Vulkan frame"))
    }

    fn finish_stream(
        &mut self,
        loop_on_eos: bool,
        restarted_without_frame: &mut bool,
    ) -> Result<bool> {
        self.end_of_stream_count = self.end_of_stream_count.saturating_add(1);
        self.draining = false;
        if !loop_on_eos {
            return Ok(false);
        }
        if *restarted_without_frame {
            return Err(Error::VideoDecode(
                "FFmpeg stream completed a full loop without a decoded frame".into(),
            ));
        }
        self.format.seek_start()?;
        self.codec_context.flush();
        self.loop_count = self.loop_count.saturating_add(1);
        *restarted_without_frame = true;
        Ok(true)
    }

    fn record_packet_stats(&mut self) {
        self.sent_packet_count = self.sent_packet_count.saturating_add(1);
        let size = u32::try_from(self.packet.size()).unwrap_or(0);
        self.sent_packet_payload_bytes = self
            .sent_packet_payload_bytes
            .saturating_add(u64::from(size));
        self.max_packet_size_bytes = self.max_packet_size_bytes.max(size);
    }
}

enum ReceiveResult {
    Frame(DecodedVideoFrame),
    Again,
    Eof,
}

fn expected_format(codec: FfmpegVideoCodec) -> DecodedVideoFormat {
    match codec {
        FfmpegVideoCodec::H264High8 | FfmpegVideoCodec::H265Main8 | FfmpegVideoCodec::Av1Main8 => {
            DecodedVideoFormat::Nv12
        }
        FfmpegVideoCodec::H265Main10 | FfmpegVideoCodec::Av1Main10 => DecodedVideoFormat::P010,
    }
}

// All inner FFmpeg objects have unique ownership. Moving the decoder enables a
// dedicated decode worker; no method permits concurrent access.
unsafe impl Send for FfmpegVulkanDecoder {}
