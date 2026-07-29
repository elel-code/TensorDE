#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVFormatContext {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVPacket {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct GilderFfmpegObjectPool {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVCodec {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVCodecContext {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVFrame {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct GilderAudioOutput {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVRational {
    num: c_int,
    den: c_int,
}

#[cfg(feature = "native-vulkan-video")]
unsafe extern "C" {
    fn gilder_configure_process_allocator_for_streaming_video();
    fn gilder_trim_process_heap();
    fn gilder_av_error_again() -> c_int;
    fn gilder_av_error_eof() -> c_int;
    fn gilder_av_nopts_value() -> i64;
    fn gilder_av_strerror(errnum: c_int, errbuf: *mut c_char, errbuf_size: usize) -> c_int;
    fn gilder_avformat_open_input(ctx: *mut *mut AVFormatContext, url: *const c_char) -> c_int;
    fn gilder_avformat_close_input(ctx: *mut *mut AVFormatContext);
    fn gilder_av_find_audio_stream(ctx: *mut AVFormatContext) -> c_int;
    fn gilder_av_packet_unref(packet: *mut AVPacket);
    fn gilder_ffmpeg_pool_alloc() -> *mut GilderFfmpegObjectPool;
    fn gilder_ffmpeg_pool_free(pool: *mut *mut GilderFfmpegObjectPool);
    fn gilder_ffmpeg_pool_get_packet(pool: *mut GilderFfmpegObjectPool) -> *mut AVPacket;
    fn gilder_ffmpeg_pool_put_packet(pool: *mut GilderFfmpegObjectPool, packet: *mut *mut AVPacket);
    fn gilder_ffmpeg_pool_get_frame(pool: *mut GilderFfmpegObjectPool) -> *mut AVFrame;
    fn gilder_ffmpeg_pool_put_frame(pool: *mut GilderFfmpegObjectPool, frame: *mut *mut AVFrame);
    fn gilder_av_read_frame(ctx: *mut AVFormatContext, packet: *mut AVPacket) -> c_int;
    fn gilder_av_packet_stream_index(packet: *const AVPacket) -> c_int;
    fn gilder_av_packet_size(packet: *const AVPacket) -> c_int;
    fn gilder_av_packet_pts(packet: *const AVPacket) -> i64;
    fn gilder_av_packet_duration(packet: *const AVPacket) -> i64;
    fn gilder_av_stream_time_base(ctx: *mut AVFormatContext, stream_index: c_int) -> AVRational;
    fn gilder_av_stream_duration(ctx: *mut AVFormatContext, stream_index: c_int) -> i64;
    fn gilder_av_stream_sample_rate(ctx: *mut AVFormatContext, stream_index: c_int) -> c_int;
    fn gilder_av_stream_channels(ctx: *mut AVFormatContext, stream_index: c_int) -> c_int;
    fn gilder_av_seek_stream_start(ctx: *mut AVFormatContext, stream_index: c_int) -> c_int;
    fn gilder_av_stream_decoder(ctx: *mut AVFormatContext, stream_index: c_int) -> *const AVCodec;
    fn gilder_avcodec_alloc_context3(codec: *const AVCodec) -> *mut AVCodecContext;
    fn gilder_avcodec_free_context(ctx: *mut *mut AVCodecContext);
    fn gilder_avcodec_parameters_to_context_for_stream(
        codec_ctx: *mut AVCodecContext,
        format_ctx: *mut AVFormatContext,
        stream_index: c_int,
    ) -> c_int;
    fn gilder_avcodec_open2(ctx: *mut AVCodecContext, codec: *const AVCodec) -> c_int;
    fn gilder_avcodec_send_packet(ctx: *mut AVCodecContext, packet: *const AVPacket) -> c_int;
    fn gilder_avcodec_receive_frame(ctx: *mut AVCodecContext, frame: *mut AVFrame) -> c_int;
    fn gilder_avcodec_context_sample_rate(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_channels(ctx: *const AVCodecContext) -> c_int;
    fn gilder_av_frame_unref(frame: *mut AVFrame);
    fn gilder_av_frame_nb_samples(frame: *const AVFrame) -> c_int;
    fn gilder_av_frame_sample_rate(frame: *const AVFrame) -> c_int;
    fn gilder_av_frame_channels(frame: *const AVFrame) -> c_int;
    fn gilder_audio_output_alloc() -> *mut GilderAudioOutput;
    fn gilder_audio_output_free(output: *mut *mut GilderAudioOutput);
    fn gilder_audio_output_write_frame(
        output: *mut GilderAudioOutput,
        codec_ctx: *mut AVCodecContext,
        frame: *const AVFrame,
        samples_written: *mut c_longlong,
        bytes_written: *mut c_longlong,
        sample_rate: *mut c_int,
        channels: *mut c_int,
        write_calls: *mut c_longlong,
        write_waits: *mut c_longlong,
        process_callbacks: *mut c_longlong,
        buffer_errors: *mut c_longlong,
        timeout_errors: *mut c_longlong,
        stream_ready: *mut c_int,
        state_changes: *mut c_longlong,
        ready_state_changes: *mut c_longlong,
        stream_state: *mut c_int,
        signal_level_micros: *mut c_int,
        pcm: *mut *const f32,
        pcm_sample_count: *mut c_int,
    ) -> c_int;
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_configure_process_allocator_for_streaming_video() {
    static CONFIGURE_ALLOCATOR: Once = Once::new();
    CONFIGURE_ALLOCATOR.call_once(|| unsafe {
        gilder_configure_process_allocator_for_streaming_video();
    });
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_trim_process_heap() {
    unsafe {
        gilder_trim_process_heap();
    }
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegAudioClockReader {
    format: NativeVulkanFfmpegAudioFormatContext,
    input_packet: NativeVulkanFfmpegAudioReusablePacket,
    decoder: Option<NativeVulkanFfmpegAudioDecoder>,
    stream_index: c_int,
    time_base: AVRational,
    stream_duration_ns: Option<u64>,
    sample_rate_hz: Option<u32>,
    channel_count: Option<u32>,
    eos_count: u32,
    loop_count: u32,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegAudioClockReader {
    fn open(source: &PathBuf, output_mode: NativeVulkanAudioOutputMode) -> Result<Self, String> {
        let format = NativeVulkanFfmpegAudioFormatContext::open(source)?;
        let stream_index = unsafe { gilder_av_find_audio_stream(format.ptr.as_ptr()) };
        if stream_index < 0 {
            return Err(native_vulkan_audio_ffmpeg_error(
                stream_index,
                "av_find_best_stream/select audio stream",
            ));
        }
        let time_base = unsafe { gilder_av_stream_time_base(format.ptr.as_ptr(), stream_index) };
        let stream_duration_ns = native_vulkan_audio_ffmpeg_duration_ns(
            unsafe { gilder_av_stream_duration(format.ptr.as_ptr(), stream_index) },
            time_base,
        );
        let sample_rate_hz = native_vulkan_audio_positive_c_int(unsafe {
            gilder_av_stream_sample_rate(format.ptr.as_ptr(), stream_index)
        });
        let channel_count = native_vulkan_audio_positive_c_int(unsafe {
            gilder_av_stream_channels(format.ptr.as_ptr(), stream_index)
        });
        let av_pool = NativeVulkanFfmpegAudioAvObjectPool::new()?;
        let input_packet = NativeVulkanFfmpegAudioReusablePacket::new(Arc::clone(&av_pool))?;
        let decoder = if output_mode == NativeVulkanAudioOutputMode::Auto {
            Some(NativeVulkanFfmpegAudioDecoder::open(
                &format,
                stream_index,
                output_mode,
                av_pool,
            )?)
        } else {
            None
        };
        Ok(Self {
            format,
            input_packet,
            decoder,
            stream_index,
            time_base,
            stream_duration_ns,
            sample_rate_hz,
            channel_count,
            eos_count: 0,
            loop_count: 0,
        })
    }

    fn read_next_packet(
        &mut self,
        loop_on_eos: bool,
    ) -> Result<Option<NativeVulkanAudioClockPacket>, NativeVulkanError> {
        loop {
            let input = self.input_packet.as_mut_ptr();
            let read_ret = unsafe { gilder_av_read_frame(self.format.ptr.as_ptr(), input) };
            if read_ret == 0 {
                let packet_stream_index = unsafe { gilder_av_packet_stream_index(input) };
                if packet_stream_index != self.stream_index {
                    self.input_packet.unref();
                    continue;
                }
                let packet_duration_ns = native_vulkan_audio_ffmpeg_duration_ns(
                    unsafe { gilder_av_packet_duration(input) },
                    self.time_base,
                );
                let decoded = if let Some(decoder) = self.decoder.as_mut() {
                    decoder.decode_packet(input)?
                } else {
                    self.metadata_only_decoded_packet(packet_duration_ns)
                };
                let packet = NativeVulkanAudioClockPacket {
                    serial: self.loop_count,
                    pts_ns: native_vulkan_audio_ffmpeg_timestamp_ns(
                        unsafe { gilder_av_packet_pts(input) },
                        self.time_base,
                    ),
                    duration_ns: packet_duration_ns.or_else(|| {
                        native_vulkan_audio_decoded_duration_ns(
                            decoded.decoded_samples,
                            decoded.sample_rate_hz,
                        )
                    }),
                    payload_bytes: native_vulkan_audio_ffmpeg_packet_size(input),
                    decoded_frames: decoded.decoded_frames,
                    decoded_samples: decoded.decoded_samples,
                    audio_signal_level_micros: decoded.audio_signal_level_micros,
                    audio_spectrum: decoded.audio_spectrum,
                    sample_rate_hz: decoded
                        .sample_rate_hz
                        .or_else(|| self.decoder_sample_rate_hz()),
                    channel_count: decoded
                        .channel_count
                        .or_else(|| self.decoder_channel_count()),
                    output_frames: decoded.output_frames,
                    output_samples: decoded.output_samples,
                    output_bytes: decoded.output_bytes,
                    output_sample_rate_hz: decoded.output_sample_rate_hz,
                    output_channel_count: decoded.output_channel_count,
                    output_write_calls: decoded.output_write_calls,
                    output_write_waits: decoded.output_write_waits,
                    output_process_callbacks: decoded.output_process_callbacks,
                    output_buffer_errors: decoded.output_buffer_errors,
                    output_timeout_errors: decoded.output_timeout_errors,
                    output_state_changes: decoded.output_state_changes,
                    output_ready_state_changes: decoded.output_ready_state_changes,
                    output_stream_state: decoded.output_stream_state,
                    output_stream_ready: decoded.output_stream_ready,
                };
                self.input_packet.unref();
                return Ok(Some(packet));
            }
            self.input_packet.unref();

            if read_ret == unsafe { gilder_av_error_eof() } {
                self.eos_count = self.eos_count.saturating_add(1);
                if !loop_on_eos {
                    return Ok(None);
                }
                let ret = unsafe {
                    gilder_av_seek_stream_start(self.format.ptr.as_ptr(), self.stream_index)
                };
                if ret < 0 {
                    return Err(NativeVulkanError::Video(native_vulkan_audio_ffmpeg_error(
                        ret,
                        "av_seek_frame audio stream start",
                    )));
                }
                self.loop_count = self.loop_count.saturating_add(1);
                continue;
            }

            return Err(NativeVulkanError::Video(native_vulkan_audio_ffmpeg_error(
                read_ret,
                "av_read_frame audio clock",
            )));
        }
    }

    fn decoder_sample_rate_hz(&self) -> Option<u32> {
        self.decoder
            .as_ref()
            .and_then(NativeVulkanFfmpegAudioDecoder::sample_rate_hz)
            .or(self.sample_rate_hz)
    }

    fn decoder_channel_count(&self) -> Option<u32> {
        self.decoder
            .as_ref()
            .and_then(NativeVulkanFfmpegAudioDecoder::channel_count)
            .or(self.channel_count)
    }

    fn metadata_only_decoded_packet(
        &self,
        packet_duration_ns: Option<u64>,
    ) -> NativeVulkanFfmpegAudioDecodedPacket {
        let decoded_samples = match (packet_duration_ns, self.sample_rate_hz) {
            (Some(duration_ns), Some(sample_rate_hz)) => {
                let samples = u128::from(duration_ns)
                    .saturating_mul(u128::from(sample_rate_hz))
                    .saturating_add(999_999_999)
                    / 1_000_000_000u128;
                samples.min(u128::from(u32::MAX)) as u32
            }
            _ => 0,
        };
        NativeVulkanFfmpegAudioDecodedPacket {
            decoded_frames: 1,
            decoded_samples,
            sample_rate_hz: self.sample_rate_hz,
            channel_count: self.channel_count,
            ..NativeVulkanFfmpegAudioDecodedPacket::default()
        }
    }

    fn can_fast_forward_clock_only(&self, target_clock_ns: u64, clock_ns: u64) -> bool {
        target_clock_ns > clock_ns
            && self
                .stream_duration_ns
                .is_some_and(|duration_ns| target_clock_ns <= duration_ns)
    }

    fn metadata_only_fast_forward_packet(&self, duration_ns: u64) -> NativeVulkanAudioClockPacket {
        let decoded_samples = match self.sample_rate_hz {
            Some(sample_rate_hz) => {
                let samples = u128::from(duration_ns)
                    .saturating_mul(u128::from(sample_rate_hz))
                    .saturating_add(999_999_999)
                    / 1_000_000_000u128;
                samples.min(u128::from(u32::MAX)) as u32
            }
            None => 0,
        };
        NativeVulkanAudioClockPacket {
            serial: self.loop_count,
            pts_ns: None,
            duration_ns: Some(duration_ns),
            payload_bytes: 0,
            decoded_frames: u32::from(decoded_samples > 0),
            decoded_samples,
            sample_rate_hz: self.sample_rate_hz,
            channel_count: self.channel_count,
            ..NativeVulkanAudioClockPacket::default()
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, Default)]
struct NativeVulkanFfmpegAudioDecodedPacket {
    decoded_frames: u32,
    decoded_samples: u32,
    audio_signal_level_micros: u32,
    audio_spectrum: Option<StereoSpectrum64>,
    sample_rate_hz: Option<u32>,
    channel_count: Option<u32>,
    output_frames: u32,
    output_samples: u32,
    output_bytes: u64,
    output_sample_rate_hz: Option<u32>,
    output_channel_count: Option<u32>,
    output_write_calls: u64,
    output_write_waits: u64,
    output_process_callbacks: u64,
    output_buffer_errors: u64,
    output_timeout_errors: u64,
    output_state_changes: u64,
    output_ready_state_changes: u64,
    output_stream_state: i32,
    output_stream_ready: bool,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegAudioDecoder {
    context: NonNull<AVCodecContext>,
    frame: NonNull<AVFrame>,
    frame_pool: Arc<NativeVulkanFfmpegAudioAvObjectPool>,
    output: Option<NonNull<GilderAudioOutput>>,
    spectrum_producer: Option<crate::renderer::native_vulkan::audio::spectrum::PcmSpectrumProducer>,
    spectrum_format: Option<(u32, u32)>,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegAudioDecoder {
    fn open(
        format: &NativeVulkanFfmpegAudioFormatContext,
        stream_index: c_int,
        output_mode: NativeVulkanAudioOutputMode,
        frame_pool: Arc<NativeVulkanFfmpegAudioAvObjectPool>,
    ) -> Result<Self, String> {
        let codec = unsafe { gilder_av_stream_decoder(format.ptr.as_ptr(), stream_index) };
        if codec.is_null() {
            return Err("FFmpeg audio decoder was not found for selected stream".to_owned());
        }

        let context = unsafe { gilder_avcodec_alloc_context3(codec) };
        let context = NonNull::new(context)
            .ok_or_else(|| "FFmpeg avcodec_alloc_context3 failed".to_owned())?;
        let frame_pool_for_open = Arc::clone(&frame_pool);
        let result =
            (|| -> Result<(NonNull<AVFrame>, Option<NonNull<GilderAudioOutput>>), String> {
                let ret = unsafe {
                    gilder_avcodec_parameters_to_context_for_stream(
                        context.as_ptr(),
                        format.ptr.as_ptr(),
                        stream_index,
                    )
                };
                if ret < 0 {
                    return Err(native_vulkan_audio_ffmpeg_error(
                        ret,
                        "avcodec_parameters_to_context audio stream",
                    ));
                }
                let ret = unsafe { gilder_avcodec_open2(context.as_ptr(), codec) };
                if ret < 0 {
                    return Err(native_vulkan_audio_ffmpeg_error(
                        ret,
                        "avcodec_open2 audio stream",
                    ));
                }
                let frame = frame_pool_for_open.take_frame()?;
                let output = if output_mode == NativeVulkanAudioOutputMode::Auto {
                    let output = unsafe { gilder_audio_output_alloc() };
                    match NonNull::new(output) {
                        Some(output) => Some(output),
                        None => {
                            frame_pool_for_open.recycle_frame(frame);
                            return Err("PipeWire audio output allocation failed".to_owned());
                        }
                    }
                } else {
                    None
                };
                Ok((frame, output))
            })();

        match result {
            Ok((frame, output)) => Ok(Self {
                context,
                frame,
                frame_pool,
                output,
                spectrum_producer: None,
                spectrum_format: None,
            }),
            Err(err) => {
                let mut ptr = context.as_ptr();
                unsafe {
                    gilder_avcodec_free_context(&mut ptr);
                }
                Err(err)
            }
        }
    }

    fn sample_rate_hz(&self) -> Option<u32> {
        native_vulkan_audio_positive_c_int(unsafe {
            gilder_avcodec_context_sample_rate(self.context.as_ptr())
        })
    }

    fn channel_count(&self) -> Option<u32> {
        native_vulkan_audio_positive_c_int(unsafe {
            gilder_avcodec_context_channels(self.context.as_ptr())
        })
    }

    fn decode_packet(
        &mut self,
        packet: *const AVPacket,
    ) -> Result<NativeVulkanFfmpegAudioDecodedPacket, NativeVulkanError> {
        let send_ret = unsafe { gilder_avcodec_send_packet(self.context.as_ptr(), packet) };
        if send_ret < 0 {
            return Err(NativeVulkanError::Video(native_vulkan_audio_ffmpeg_error(
                send_ret,
                "avcodec_send_packet audio stream",
            )));
        }
        self.receive_available_frames()
    }

    fn receive_available_frames(
        &mut self,
    ) -> Result<NativeVulkanFfmpegAudioDecodedPacket, NativeVulkanError> {
        let mut decoded = NativeVulkanFfmpegAudioDecodedPacket::default();
        loop {
            let receive_ret =
                unsafe { gilder_avcodec_receive_frame(self.context.as_ptr(), self.frame.as_ptr()) };
            if receive_ret == 0 {
                decoded.decoded_frames = decoded.decoded_frames.saturating_add(1);
                let samples = native_vulkan_audio_positive_c_int(unsafe {
                    gilder_av_frame_nb_samples(self.frame.as_ptr())
                })
                .unwrap_or(0);
                decoded.decoded_samples = decoded.decoded_samples.saturating_add(samples);
                if decoded.sample_rate_hz.is_none() {
                    decoded.sample_rate_hz = native_vulkan_audio_positive_c_int(unsafe {
                        gilder_av_frame_sample_rate(self.frame.as_ptr())
                    })
                    .or_else(|| self.sample_rate_hz());
                }
                if decoded.channel_count.is_none() {
                    decoded.channel_count = native_vulkan_audio_positive_c_int(unsafe {
                        gilder_av_frame_channels(self.frame.as_ptr())
                    })
                    .or_else(|| self.channel_count());
                }
                let output_result = self.write_output_frame(&mut decoded);
                unsafe {
                    gilder_av_frame_unref(self.frame.as_ptr());
                }
                output_result?;
                continue;
            }
            unsafe {
                gilder_av_frame_unref(self.frame.as_ptr());
            }
            if receive_ret == unsafe { gilder_av_error_again() }
                || receive_ret == unsafe { gilder_av_error_eof() }
            {
                return Ok(decoded);
            }
            return Err(NativeVulkanError::Video(native_vulkan_audio_ffmpeg_error(
                receive_ret,
                "avcodec_receive_frame audio stream",
            )));
        }
    }

    fn write_output_frame(
        &mut self,
        decoded: &mut NativeVulkanFfmpegAudioDecodedPacket,
    ) -> Result<(), NativeVulkanError> {
        let Some(output) = self.output else {
            return Ok(());
        };
        let mut samples_written: c_longlong = 0;
        let mut bytes_written: c_longlong = 0;
        let mut sample_rate: c_int = 0;
        let mut channels: c_int = 0;
        let mut write_calls: c_longlong = 0;
        let mut write_waits: c_longlong = 0;
        let mut process_callbacks: c_longlong = 0;
        let mut buffer_errors: c_longlong = 0;
        let mut timeout_errors: c_longlong = 0;
        let mut stream_ready: c_int = 0;
        let mut state_changes: c_longlong = 0;
        let mut ready_state_changes: c_longlong = 0;
        let mut stream_state: c_int = 0;
        let mut signal_level_micros: c_int = 0;
        let mut pcm = std::ptr::null();
        let mut pcm_sample_count = 0;
        let ret = unsafe {
            gilder_audio_output_write_frame(
                output.as_ptr(),
                self.context.as_ptr(),
                self.frame.as_ptr(),
                &mut samples_written,
                &mut bytes_written,
                &mut sample_rate,
                &mut channels,
                &mut write_calls,
                &mut write_waits,
                &mut process_callbacks,
                &mut buffer_errors,
                &mut timeout_errors,
                &mut stream_ready,
                &mut state_changes,
                &mut ready_state_changes,
                &mut stream_state,
                &mut signal_level_micros,
                &mut pcm,
                &mut pcm_sample_count,
            )
        };
        if ret < 0 {
            return Err(NativeVulkanError::Video(native_vulkan_audio_ffmpeg_error(
                ret,
                "PipeWire audio output write frame",
            )));
        }

        let output_bytes = native_vulkan_audio_positive_c_longlong_u64(bytes_written);
        if output_bytes > 0 {
            decoded.output_frames = decoded.output_frames.saturating_add(1);
            decoded.output_samples = decoded
                .output_samples
                .saturating_add(native_vulkan_audio_positive_c_longlong_u32(samples_written));
            decoded.output_bytes = decoded.output_bytes.saturating_add(output_bytes);
            decoded.audio_signal_level_micros = decoded
                .audio_signal_level_micros
                .max(native_vulkan_audio_positive_c_int(signal_level_micros).unwrap_or(0));
            let spectrum_format = (
                native_vulkan_audio_positive_c_int(sample_rate).unwrap_or(48_000),
                native_vulkan_audio_positive_c_int(channels).unwrap_or(2),
            );
            if self.spectrum_format != Some(spectrum_format) {
                self.spectrum_producer = Some(
                    crate::renderer::native_vulkan::audio::spectrum::PcmSpectrumProducer::new(
                        spectrum_format.0,
                        spectrum_format.1,
                        crate::renderer::native_vulkan::audio::spectrum::DEFAULT_INPUT_VOLUME,
                        0.0,
                    )
                    .map_err(|error| {
                        NativeVulkanError::Video(format!(
                            "create canonical decoded-audio spectrum analyzer: {error:?}"
                        ))
                    })?,
                );
                self.spectrum_format = Some(spectrum_format);
            }
            if !pcm.is_null()
                && pcm_sample_count > 0
                && let Some(producer) = self.spectrum_producer.as_mut()
            {
                // SAFETY: the C output owns this retained buffer and keeps it valid until the
                // next output write; analysis completes synchronously before that call.
                let pcm = unsafe { std::slice::from_raw_parts(pcm, pcm_sample_count as usize) };
                if let Some(spectrum) = producer.push_interleaved(pcm) {
                    decoded.audio_spectrum = Some(spectrum);
                }
            }
        }
        if decoded.output_sample_rate_hz.is_none() {
            decoded.output_sample_rate_hz = native_vulkan_audio_positive_c_int(sample_rate);
        }
        if decoded.output_channel_count.is_none() {
            decoded.output_channel_count = native_vulkan_audio_positive_c_int(channels);
        }
        decoded.output_write_calls = decoded
            .output_write_calls
            .max(native_vulkan_audio_positive_c_longlong_u64(write_calls));
        decoded.output_write_waits = decoded
            .output_write_waits
            .max(native_vulkan_audio_positive_c_longlong_u64(write_waits));
        decoded.output_process_callbacks =
            decoded
                .output_process_callbacks
                .max(native_vulkan_audio_positive_c_longlong_u64(
                    process_callbacks,
                ));
        decoded.output_buffer_errors = decoded
            .output_buffer_errors
            .max(native_vulkan_audio_positive_c_longlong_u64(buffer_errors));
        decoded.output_timeout_errors = decoded
            .output_timeout_errors
            .max(native_vulkan_audio_positive_c_longlong_u64(timeout_errors));
        decoded.output_state_changes = decoded
            .output_state_changes
            .max(native_vulkan_audio_positive_c_longlong_u64(state_changes));
        decoded.output_ready_state_changes =
            decoded
                .output_ready_state_changes
                .max(native_vulkan_audio_positive_c_longlong_u64(
                    ready_state_changes,
                ));
        if stream_state != 0 {
            decoded.output_stream_state = stream_state;
        }
        decoded.output_stream_ready |= stream_ready != 0;
        Ok(())
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Drop for NativeVulkanFfmpegAudioDecoder {
    fn drop(&mut self) {
        if let Some(output) = self.output {
            let mut output = output.as_ptr();
            unsafe {
                gilder_audio_output_free(&mut output);
            }
        }
        let mut context = self.context.as_ptr();
        self.frame_pool.recycle_frame(self.frame);
        unsafe {
            gilder_avcodec_free_context(&mut context);
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegAudioFormatContext {
    ptr: NonNull<AVFormatContext>,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegAudioFormatContext {
    fn open(source: &PathBuf) -> Result<Self, String> {
        native_vulkan_audio_configure_process_allocator_for_streaming_video();
        let source = CString::new(source.as_os_str().as_bytes())
            .map_err(|_| "FFmpeg audio source path contains an interior NUL".to_owned())?;
        let mut ctx = ptr::null_mut();
        let ret = unsafe { gilder_avformat_open_input(&mut ctx, source.as_ptr()) };
        if ret < 0 {
            return Err(native_vulkan_audio_ffmpeg_error(ret, "avformat_open_input"));
        }
        let ptr = NonNull::new(ctx)
            .ok_or_else(|| "FFmpeg avformat_open_input returned null".to_owned())?;
        Ok(Self { ptr })
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Drop for NativeVulkanFfmpegAudioFormatContext {
    fn drop(&mut self) {
        let mut ptr = self.ptr.as_ptr();
        unsafe {
            gilder_avformat_close_input(&mut ptr);
        }
        native_vulkan_audio_trim_process_heap();
    }
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegAudioReusablePacket {
    packet: NonNull<AVPacket>,
    pool: Arc<NativeVulkanFfmpegAudioAvObjectPool>,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegAudioReusablePacket {
    fn new(pool: Arc<NativeVulkanFfmpegAudioAvObjectPool>) -> Result<Self, String> {
        let packet = pool.take_packet()?;
        Ok(Self { packet, pool })
    }

    fn as_mut_ptr(&mut self) -> *mut AVPacket {
        self.packet.as_ptr()
    }

    fn unref(&mut self) {
        unsafe {
            gilder_av_packet_unref(self.packet.as_ptr());
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Drop for NativeVulkanFfmpegAudioReusablePacket {
    fn drop(&mut self) {
        self.pool.recycle_packet(self.packet);
    }
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegAudioAvObjectPool {
    ptr: Mutex<NonNull<GilderFfmpegObjectPool>>,
}

#[cfg(feature = "native-vulkan-video")]
unsafe impl Send for NativeVulkanFfmpegAudioAvObjectPool {}
#[cfg(feature = "native-vulkan-video")]
unsafe impl Sync for NativeVulkanFfmpegAudioAvObjectPool {}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegAudioAvObjectPool {
    fn new() -> Result<Arc<Self>, String> {
        let pool = unsafe { gilder_ffmpeg_pool_alloc() };
        let ptr = NonNull::new(pool)
            .ok_or_else(|| "FFmpeg audio object pool allocation failed".to_owned())?;
        Ok(Arc::new(Self {
            ptr: Mutex::new(ptr),
        }))
    }

    fn take_packet(&self) -> Result<NonNull<AVPacket>, String> {
        let pool = self.ptr.lock().unwrap_or_else(|err| err.into_inner());
        let packet = unsafe { gilder_ffmpeg_pool_get_packet(pool.as_ptr()) };
        NonNull::new(packet)
            .ok_or_else(|| "FFmpeg audio AVPacket pool allocation failed".to_owned())
    }

    fn recycle_packet(&self, packet: NonNull<AVPacket>) {
        let pool = self.ptr.lock().unwrap_or_else(|err| err.into_inner());
        let mut packet = packet.as_ptr();
        unsafe {
            gilder_ffmpeg_pool_put_packet(pool.as_ptr(), &mut packet);
        }
    }

    fn take_frame(&self) -> Result<NonNull<AVFrame>, String> {
        let pool = self.ptr.lock().unwrap_or_else(|err| err.into_inner());
        let frame = unsafe { gilder_ffmpeg_pool_get_frame(pool.as_ptr()) };
        NonNull::new(frame).ok_or_else(|| "FFmpeg audio AVFrame pool allocation failed".to_owned())
    }

    fn recycle_frame(&self, frame: NonNull<AVFrame>) {
        let pool = self.ptr.lock().unwrap_or_else(|err| err.into_inner());
        let mut frame = frame.as_ptr();
        unsafe {
            gilder_ffmpeg_pool_put_frame(pool.as_ptr(), &mut frame);
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Drop for NativeVulkanFfmpegAudioAvObjectPool {
    fn drop(&mut self) {
        let pool = self.ptr.lock().unwrap_or_else(|err| err.into_inner());
        let mut ptr = pool.as_ptr();
        unsafe {
            gilder_ffmpeg_pool_free(&mut ptr);
        }
        native_vulkan_audio_trim_process_heap();
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_ffmpeg_packet_size(packet: *const AVPacket) -> u32 {
    let size = unsafe { gilder_av_packet_size(packet) };
    if size <= 0 { 0 } else { size as u32 }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_positive_c_int(value: c_int) -> Option<u32> {
    if value > 0 { Some(value as u32) } else { None }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_positive_c_longlong_u32(value: c_longlong) -> u32 {
    if value <= 0 {
        0
    } else {
        (value as u64).min(u64::from(u32::MAX)) as u32
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_positive_c_longlong_u64(value: c_longlong) -> u64 {
    if value <= 0 { 0 } else { value as u64 }
}

fn native_vulkan_pipewire_stream_state_label(state: i32) -> &'static str {
    match state {
        -1 => "error",
        0 => "unconnected",
        1 => "connecting",
        2 => "paused",
        3 => "streaming",
        _ => "unknown",
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_decoded_duration_ns(
    decoded_samples: u32,
    sample_rate_hz: Option<u32>,
) -> Option<u64> {
    let sample_rate_hz = u64::from(sample_rate_hz?);
    if decoded_samples == 0 || sample_rate_hz == 0 {
        return None;
    }
    Some(u64::from(decoded_samples).saturating_mul(1_000_000_000) / sample_rate_hz)
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_ffmpeg_timestamp_ns(value: i64, time_base: AVRational) -> Option<u64> {
    if value == unsafe { gilder_av_nopts_value() } || value < 0 {
        return None;
    }
    native_vulkan_audio_ffmpeg_rescale_to_ns(value, time_base)
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_ffmpeg_duration_ns(value: i64, time_base: AVRational) -> Option<u64> {
    if value <= 0 {
        return None;
    }
    native_vulkan_audio_ffmpeg_rescale_to_ns(value, time_base)
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_ffmpeg_rescale_to_ns(value: i64, time_base: AVRational) -> Option<u64> {
    let den = NonZeroI32::new(time_base.den)?;
    let scaled =
        i128::from(value) * i128::from(time_base.num) * 1_000_000_000i128 / i128::from(den.get());
    if scaled < 0 {
        return None;
    }
    Some(scaled.min(i128::from(u64::MAX)) as u64)
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_ffmpeg_error(ret: c_int, action: &str) -> String {
    let mut buffer = [0 as c_char; 256];
    unsafe {
        let _ = gilder_av_strerror(ret, buffer.as_mut_ptr(), buffer.len());
    }
    let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    format!("FFmpeg audio clock {action} failed: {message} ({ret})")
}
