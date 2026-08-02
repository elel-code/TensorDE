use std::ffi::{CStr, CString, c_char, c_int};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use super::device::FfmpegVulkanDevice;
use super::ffi::{self, AVCodecContext, AVFormatContext, AVFrame, AVPacket};
use super::{FfmpegTimeBase, FfmpegVideoCodec};
use crate::{Error, Result};

pub(super) struct FormatContext {
    raw: *mut AVFormatContext,
    stream: c_int,
    time_base: FfmpegTimeBase,
}

impl FormatContext {
    pub(super) fn open(source: &Path, codec: FfmpegVideoCodec) -> Result<Self> {
        let source = CString::new(source.as_os_str().as_bytes()).map_err(|_| {
            Error::VideoDecode("FFmpeg source path contains an interior NUL".into())
        })?;
        let mut raw = ptr::null_mut();
        ffmpeg_ok(
            unsafe { ffi::vr_ffmpeg_open_input(&mut raw, source.as_ptr()) },
            "open FFmpeg input",
        )?;
        if raw.is_null() {
            return Err(Error::VideoDecode(
                "FFmpeg returned a null format context".into(),
            ));
        }
        let stream = unsafe { ffi::vr_ffmpeg_find_video_stream(raw, codec_id(codec)) };
        if stream < 0 {
            let error = ffmpeg_error(stream, "select exact FFmpeg video stream");
            unsafe { ffi::vr_ffmpeg_close_input(&mut raw) };
            return Err(error);
        }
        let rational = unsafe { ffi::vr_ffmpeg_stream_time_base(raw, stream) };
        let time_base = FfmpegTimeBase::new(rational.numerator, rational.denominator)?;
        Ok(Self {
            raw,
            stream,
            time_base,
        })
    }

    pub(super) const fn stream(&self) -> c_int {
        self.stream
    }

    pub(super) const fn time_base(&self) -> FfmpegTimeBase {
        self.time_base
    }

    pub(super) fn read(&mut self, packet: &mut Packet) -> c_int {
        unsafe { ffi::vr_ffmpeg_read(self.raw, packet.raw) }
    }

    pub(super) fn seek_start(&mut self) -> Result<()> {
        ffmpeg_ok(
            unsafe { ffi::vr_ffmpeg_seek_start(self.raw, self.stream) },
            "seek FFmpeg video stream to start",
        )
    }
}

impl Drop for FormatContext {
    fn drop(&mut self) {
        unsafe { ffi::vr_ffmpeg_close_input(&mut self.raw) };
    }
}

pub(super) struct CodecContext {
    raw: *mut AVCodecContext,
    decoder_name: String,
}

impl CodecContext {
    pub(super) fn open(format: &FormatContext, device: &FfmpegVulkanDevice) -> Result<Self> {
        let codec = unsafe { ffi::vr_ffmpeg_stream_decoder(format.raw, format.stream) };
        if codec.is_null() {
            return Err(Error::VideoDecode(
                "FFmpeg decoder was not found for the selected stream".into(),
            ));
        }
        let decoder_name = unsafe {
            let name = ffi::vr_ffmpeg_codec_name(codec);
            if name.is_null() {
                String::new()
            } else {
                CStr::from_ptr(name).to_string_lossy().into_owned()
            }
        };
        if unsafe { ffi::vr_ffmpeg_codec_has_vulkan(codec) } == 0 {
            return Err(Error::VideoDecode(format!(
                "FFmpeg decoder {decoder_name:?} has no AV_PIX_FMT_VULKAN HW_DEVICE_CTX config"
            )));
        }
        let mut raw = unsafe { ffi::vr_ffmpeg_codec_alloc(codec) };
        if raw.is_null() {
            return Err(Error::VideoDecode(
                "allocate FFmpeg codec context failed".into(),
            ));
        }
        let open = (|| {
            ffmpeg_ok(
                unsafe { ffi::vr_ffmpeg_codec_copy_parameters(raw, format.raw, format.stream) },
                "copy FFmpeg codec parameters",
            )?;
            ffmpeg_ok(
                unsafe { ffi::vr_ffmpeg_codec_open(raw, codec, device.raw()) },
                "open FFmpeg Vulkan decoder",
            )
        })();
        if let Err(error) = open {
            unsafe { ffi::vr_ffmpeg_codec_free(&mut raw) };
            return Err(error);
        }
        Ok(Self { raw, decoder_name })
    }

    pub(super) const fn raw(&self) -> *mut AVCodecContext {
        self.raw
    }

    pub(super) fn decoder_name(&self) -> &str {
        &self.decoder_name
    }

    pub(super) fn coded_extent(&self) -> (u32, u32) {
        let width = unsafe { ffi::vr_ffmpeg_codec_width(self.raw) };
        let height = unsafe { ffi::vr_ffmpeg_codec_height(self.raw) };
        (
            u32::try_from(width).unwrap_or(0),
            u32::try_from(height).unwrap_or(0),
        )
    }

    pub(super) fn flush(&mut self) {
        unsafe { ffi::vr_ffmpeg_codec_flush(self.raw) };
    }
}

impl Drop for CodecContext {
    fn drop(&mut self) {
        unsafe { ffi::vr_ffmpeg_codec_free(&mut self.raw) };
    }
}

pub(super) struct Packet {
    raw: *mut AVPacket,
}

impl Packet {
    pub(super) fn allocate() -> Result<Self> {
        let raw = unsafe { ffi::vr_ffmpeg_packet_alloc() };
        if raw.is_null() {
            return Err(Error::VideoDecode("allocate FFmpeg packet failed".into()));
        }
        Ok(Self { raw })
    }

    pub(super) const fn raw(&self) -> *const AVPacket {
        self.raw
    }

    pub(super) fn stream(&self) -> c_int {
        unsafe { ffi::vr_ffmpeg_packet_stream(self.raw) }
    }

    pub(super) fn size(&self) -> c_int {
        unsafe { ffi::vr_ffmpeg_packet_size(self.raw) }
    }

    pub(super) fn unref(&mut self) {
        unsafe { ffi::vr_ffmpeg_packet_unref(self.raw) };
    }
}

impl Drop for Packet {
    fn drop(&mut self) {
        unsafe { ffi::vr_ffmpeg_packet_free(&mut self.raw) };
    }
}

pub(super) struct ReusableFrame {
    raw: *mut AVFrame,
}

impl ReusableFrame {
    pub(super) fn allocate() -> Result<Self> {
        let raw = unsafe { ffi::vr_ffmpeg_frame_alloc() };
        if raw.is_null() {
            return Err(Error::VideoDecode("allocate FFmpeg frame failed".into()));
        }
        Ok(Self { raw })
    }

    pub(super) const fn raw(&self) -> *mut AVFrame {
        self.raw
    }

    pub(super) fn unref(&mut self) {
        unsafe { ffi::vr_ffmpeg_frame_unref(self.raw) };
    }
}

impl Drop for ReusableFrame {
    fn drop(&mut self) {
        unsafe { ffi::vr_ffmpeg_frame_free(&mut self.raw) };
    }
}

pub(super) fn ffmpeg_ok(result: c_int, operation: &'static str) -> Result<()> {
    if result >= 0 {
        Ok(())
    } else {
        Err(ffmpeg_error(result, operation))
    }
}

pub(super) fn ffmpeg_error(result: c_int, operation: &'static str) -> Error {
    let mut buffer = [0 as c_char; 256];
    let message = unsafe {
        if ffi::vr_ffmpeg_error_string(result, buffer.as_mut_ptr(), buffer.len()) == 0 {
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        } else {
            format!("FFmpeg error {result}")
        }
    };
    Error::VideoDecode(format!("{operation} failed: {message} ({result})"))
}

pub(super) fn error_again() -> c_int {
    unsafe { ffi::vr_ffmpeg_error_again() }
}

pub(super) fn error_eof() -> c_int {
    unsafe { ffi::vr_ffmpeg_error_eof() }
}

fn codec_id(codec: FfmpegVideoCodec) -> c_int {
    unsafe {
        match codec {
            FfmpegVideoCodec::H264High8 => ffi::vr_ffmpeg_codec_h264(),
            FfmpegVideoCodec::H265Main8 | FfmpegVideoCodec::H265Main10 => {
                ffi::vr_ffmpeg_codec_hevc()
            }
            FfmpegVideoCodec::Av1Main8 | FfmpegVideoCodec::Av1Main10 => ffi::vr_ffmpeg_codec_av1(),
        }
    }
}

// Each wrapper has unique mutable ownership and moves only with its decoder.
unsafe impl Send for FormatContext {}
unsafe impl Send for CodecContext {}
unsafe impl Send for Packet {}
unsafe impl Send for ReusableFrame {}
