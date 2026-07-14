fn native_vulkan_ffmpeg_pooled_payload_copy(
    bytes: &[u8],
    pool: &Arc<NativeVulkanFfmpegPacketPayloadPool>,
) -> NativeVulkanFfmpegPacketPayload {
    let mut output = pool.take(bytes.len());
    output.extend_from_slice(bytes);
    NativeVulkanFfmpegPacketPayload::from_pooled(output, Arc::clone(pool))
}

fn native_vulkan_ffmpeg_append_annexb_unit(output: &mut Vec<u8>, nal: &[u8]) {
    output.extend_from_slice(&NATIVE_VULKAN_ANNEXB_START_CODE);
    output.extend_from_slice(nal);
}

fn native_vulkan_ffmpeg_starts_with_annexb_start_code(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0, 0, 1]) || bytes.starts_with(&NATIVE_VULKAN_ANNEXB_START_CODE)
}

fn native_vulkan_ffmpeg_take_be16(
    bytes: &[u8],
    offset: &mut usize,
    label: &str,
) -> Result<usize, String> {
    let end = offset.saturating_add(2);
    let value = bytes
        .get(*offset..end)
        .ok_or_else(|| format!("{label} is truncated before a 16-bit length"))?;
    *offset = end;
    Ok((usize::from(value[0]) << 8) | usize::from(value[1]))
}

fn native_vulkan_ffmpeg_take_be16_unit<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    label: &str,
) -> Result<&'a [u8], String> {
    let size = native_vulkan_ffmpeg_take_be16(bytes, offset, label)?;
    native_vulkan_ffmpeg_take_payload(bytes, offset, size, label)
}

fn native_vulkan_ffmpeg_take_length_prefixed_size(
    bytes: &[u8],
    offset: &mut usize,
    length_size: usize,
    label: &str,
) -> Result<usize, String> {
    if !(1..=4).contains(&length_size) {
        return Err(format!("{label} has invalid NAL length size {length_size}"));
    }
    let end = offset.saturating_add(length_size);
    let prefix = bytes
        .get(*offset..end)
        .ok_or_else(|| format!("{label} is truncated before a NAL length"))?;
    *offset = end;
    let mut size = 0usize;
    for byte in prefix {
        size = (size << 8) | usize::from(*byte);
    }
    Ok(size)
}

fn native_vulkan_ffmpeg_take_payload<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    size: usize,
    label: &str,
) -> Result<&'a [u8], String> {
    let end = offset.saturating_add(size);
    let payload = bytes
        .get(*offset..end)
        .ok_or_else(|| format!("{label} payload is truncated"))?;
    *offset = end;
    Ok(payload)
}

struct NativeVulkanFfmpegStreamingPacketWorker<A: NativeVulkanFfmpegStreamingAccessUnit> {
    format: NativeVulkanFfmpegFormatContext,
    normalizer: NativeVulkanFfmpegPacketNormalizer,
    payload_pool: Arc<NativeVulkanFfmpegPacketPayloadPool>,
    input_packet: NativeVulkanFfmpegReusablePacket,
    stream_index: c_int,
    stream_time_base: AVRational,
    eos_count: u32,
    loop_count: u32,
    pending_access_units: Option<VecDeque<A>>,
    _access_unit: PhantomData<A>,
}

impl<A: NativeVulkanFfmpegStreamingAccessUnit> NativeVulkanFfmpegStreamingPacketWorker<A> {
    fn new(source: &Path) -> Result<Self, NativeVulkanError> {
        let (format, stream_index) =
            NativeVulkanFfmpegFormatContext::open(source, A::FFMPEG_CODEC)?;
        let normalizer = NativeVulkanFfmpegPacketNormalizer::new(
            A::FFMPEG_CODEC,
            format.stream_extradata(stream_index),
        )?;
        let stream_time_base = format.stream_time_base(stream_index);
        let av_pool = NativeVulkanFfmpegAvObjectPool::new()?;
        let payload_pool = Arc::new(NativeVulkanFfmpegPacketPayloadPool::default());
        let input_packet = NativeVulkanFfmpegReusablePacket::new(av_pool)?;
        Ok(Self {
            format,
            normalizer,
            payload_pool,
            input_packet,
            stream_index,
            stream_time_base,
            eos_count: 0,
            loop_count: 0,
            pending_access_units: A::FFMPEG_PACKET_SPLITS_ACCESS_UNITS.then(VecDeque::new),
            _access_unit: PhantomData,
        })
    }

    fn pull_next(&mut self, loop_on_eos: bool) -> Result<Option<A>, NativeVulkanError> {
        if let Some(pending_access_units) = self.pending_access_units.as_mut()
            && let Some(access_unit) = pending_access_units.pop_front()
        {
            return Ok(Some(access_unit));
        }
        loop {
            let Some((payload, metadata)) = self.read_next_packet(loop_on_eos)? else {
                return Ok(None);
            };
            if !A::FFMPEG_PACKET_SPLITS_ACCESS_UNITS {
                return A::from_ffmpeg_packet(payload, metadata).map(Some);
            }
            let access_units = A::from_ffmpeg_packet_many(payload, metadata)?;
            if access_units.is_empty() {
                continue;
            }
            let mut access_units = access_units.into_iter();
            let first = access_units.next().expect("access_units is not empty");
            self.pending_access_units
                .as_mut()
                .expect("split access unit codecs own a pending access-unit queue")
                .extend(access_units);
            return Ok(Some(first));
        }
    }

    fn read_next_packet(
        &mut self,
        loop_on_eos: bool,
    ) -> Result<
        Option<(
            NativeVulkanFfmpegPacketPayload,
            NativeVulkanFfmpegPacketMetadata,
        )>,
        NativeVulkanError,
    > {
        loop {
            let input = self.input_packet.as_mut_ptr();
            let read_ret = unsafe { gilder_av_read_frame(self.format.ptr.as_ptr(), input) };
            if read_ret == 0 {
                let packet_stream_index = unsafe { gilder_av_packet_stream_index(input) };
                if packet_stream_index != self.stream_index {
                    self.input_packet.unref();
                    continue;
                }
                let metadata = self.metadata_for_packet(input, self.stream_time_base);
                let payload = self
                    .normalizer
                    .normalize(&mut self.input_packet, &self.payload_pool)?;
                return Ok(Some((payload, metadata)));
            }
            self.input_packet.unref();

            if read_ret == native_vulkan_ffmpeg_eof() {
                self.eos_count = self.eos_count.saturating_add(1);
                if !loop_on_eos {
                    return Ok(None);
                }
                self.seek_to_start()?;
                continue;
            }

            return Err(native_vulkan_ffmpeg_error(read_ret, "av_read_frame"));
        }
    }

    fn seek_to_start(&mut self) -> Result<(), NativeVulkanError> {
        let ret =
            unsafe { gilder_av_seek_stream_start(self.format.ptr.as_ptr(), self.stream_index) };
        native_vulkan_ffmpeg_ok(ret, "av_seek_frame stream start")?;
        if let Some(pending_access_units) = self.pending_access_units.as_mut() {
            pending_access_units.clear();
        }
        self.loop_count = self.loop_count.saturating_add(1);
        Ok(())
    }

    fn metadata_for_packet(
        &self,
        packet: *const AVPacket,
        time_base: AVRational,
    ) -> NativeVulkanFfmpegPacketMetadata {
        let pts_ns =
            native_vulkan_ffmpeg_timestamp_ns(unsafe { gilder_av_packet_pts(packet) }, time_base);
        let duration_ns = native_vulkan_ffmpeg_duration_ns(
            unsafe { gilder_av_packet_duration(packet) },
            time_base,
        );
        NativeVulkanFfmpegPacketMetadata {
            pts_ns,
            duration_ns,
            pts_ms: pts_ns.map(|value| value / 1_000_000),
            duration_ms: duration_ns.map(|value| value / 1_000_000),
        }
    }
}

struct NativeVulkanFfmpegStreamingPacketFrontend<A: NativeVulkanFfmpegStreamingAccessUnit> {
    request_sender: Option<SyncSender<()>>,
    receiver: Option<Receiver<NativeVulkanFfmpegStreamingPacketFrontendMessage<A>>>,
    loop_on_eos: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    eos_count: u32,
    loop_count: u32,
    _access_unit: PhantomData<A>,
}

struct NativeVulkanFfmpegStreamingPacketFrontendMessage<A> {
    result: Result<Option<A>, NativeVulkanError>,
    eos_count: u32,
    loop_count: u32,
}

impl<A: NativeVulkanFfmpegStreamingAccessUnit + Send + 'static>
    NativeVulkanFfmpegStreamingPacketFrontend<A>
{
    fn new(source: &Path, _capacity: usize) -> Result<Self, NativeVulkanError> {
        let (request_sender, request_receiver) = sync_channel(0);
        let (sender, receiver) = sync_channel(A::FFMPEG_READ_THREAD_HANDOFF_PACKETS);
        let loop_on_eos = Arc::new(AtomicBool::new(false));
        let worker_loop_on_eos = Arc::clone(&loop_on_eos);
        let source = source.to_path_buf();
        let worker = thread::Builder::new()
            .name(format!("gilder-ffmpeg-{}-read-thread", A::CODEC_LABEL))
            .stack_size(128 * 1024)
            .spawn(move || {
                native_vulkan_ffmpeg_streaming_packet_worker::<A>(
                    source.as_path(),
                    worker_loop_on_eos,
                    request_receiver,
                    sender,
                );
            })
            .map_err(|err| {
                NativeVulkanError::Video(format!(
                    "spawn {} FFmpeg packet read thread: {err}",
                    A::CODEC_LABEL
                ))
            })?;

        Ok(Self {
            request_sender: Some(request_sender),
            receiver: Some(receiver),
            loop_on_eos,
            worker: Some(worker),
            eos_count: 0,
            loop_count: 0,
            _access_unit: PhantomData,
        })
    }
}

impl<A: NativeVulkanFfmpegStreamingAccessUnit> Drop
    for NativeVulkanFfmpegStreamingPacketFrontend<A>
{
    fn drop(&mut self) {
        let _ = self.request_sender.take();
        let _ = self.receiver.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl<A: NativeVulkanFfmpegStreamingAccessUnit + Send + 'static>
    NativeVulkanStreamingPacketFrontend<A> for NativeVulkanFfmpegStreamingPacketFrontend<A>
{
    fn pull_next_access_unit(&mut self, loop_on_eos: bool) -> Result<Option<A>, NativeVulkanError> {
        if loop_on_eos {
            self.loop_on_eos.store(true, Ordering::Release);
        }
        let request_sender = self.request_sender.as_ref().ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "{} FFmpeg packet read thread request channel is closed",
                A::CODEC_LABEL
            ))
        })?;
        request_sender.send(()).map_err(|err| {
            NativeVulkanError::Video(format!(
                "{} FFmpeg packet read thread stopped before accepting a pull request: {err}",
                A::CODEC_LABEL
            ))
        })?;
        let receiver = self.receiver.as_ref().ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "{} FFmpeg packet read thread is closed",
                A::CODEC_LABEL
            ))
        })?;
        let message = receiver.recv().map_err(|err| {
            NativeVulkanError::Video(format!(
                "{} FFmpeg packet read thread stopped before producing an AU: {err}",
                A::CODEC_LABEL
            ))
        })?;
        self.eos_count = message.eos_count;
        self.loop_count = message.loop_count;
        message.result
    }

    fn eos_count(&self) -> u32 {
        self.eos_count
    }

    fn loop_count(&self) -> u32 {
        self.loop_count
    }
}

fn native_vulkan_ffmpeg_streaming_packet_worker<A: NativeVulkanFfmpegStreamingAccessUnit>(
    source: &Path,
    loop_on_eos: Arc<AtomicBool>,
    request_receiver: Receiver<()>,
    sender: SyncSender<NativeVulkanFfmpegStreamingPacketFrontendMessage<A>>,
) {
    let mut worker = match NativeVulkanFfmpegStreamingPacketWorker::<A>::new(source) {
        Ok(worker) => worker,
        Err(err) => {
            if request_receiver.recv().is_ok() {
                let _ = sender.send(NativeVulkanFfmpegStreamingPacketFrontendMessage {
                    result: Err(err),
                    eos_count: 0,
                    loop_count: 0,
                });
            }
            return;
        }
    };

    loop {
        if request_receiver.recv().is_err() {
            break;
        }
        let result = worker.pull_next(loop_on_eos.load(Ordering::Acquire));
        let stop_after_send = result.as_ref().map_or(true, Option::is_none);
        let message = NativeVulkanFfmpegStreamingPacketFrontendMessage {
            result,
            eos_count: worker.eos_count,
            loop_count: worker.loop_count,
        };
        if sender.send(message).is_err() || stop_after_send {
            break;
        }
    }
}

fn native_vulkan_ffmpeg_timestamp_ns(value: i64, time_base: AVRational) -> Option<u64> {
    if value == unsafe { gilder_av_nopts_value() } || value < 0 {
        return None;
    }
    native_vulkan_ffmpeg_rescale_to_ns(value, time_base)
}

fn native_vulkan_ffmpeg_duration_ns(value: i64, time_base: AVRational) -> Option<u64> {
    if value <= 0 {
        return None;
    }
    native_vulkan_ffmpeg_rescale_to_ns(value, time_base)
}

fn native_vulkan_ffmpeg_rescale_to_ns(value: i64, time_base: AVRational) -> Option<u64> {
    let den = NonZeroI32::new(time_base.den)?;
    let scaled =
        i128::from(value) * i128::from(time_base.num) * 1_000_000_000i128 / i128::from(den.get());
    if scaled < 0 {
        return None;
    }
    Some(scaled.min(i128::from(u64::MAX)) as u64)
}

fn native_vulkan_ffmpeg_eof() -> c_int {
    unsafe { gilder_av_error_eof() }
}

fn native_vulkan_ffmpeg_ok(ret: c_int, action: &str) -> Result<(), NativeVulkanError> {
    if ret >= 0 {
        Ok(())
    } else {
        Err(native_vulkan_ffmpeg_error(ret, action))
    }
}

fn native_vulkan_ffmpeg_error(ret: c_int, action: &str) -> NativeVulkanError {
    let mut buffer = [0 as c_char; 256];
    unsafe {
        let _ = gilder_av_strerror(ret, buffer.as_mut_ptr(), buffer.len());
    }
    let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    NativeVulkanError::Video(format!("FFmpeg {action} failed: {message} ({ret})"))
}
