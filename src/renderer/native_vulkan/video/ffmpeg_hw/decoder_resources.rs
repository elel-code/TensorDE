impl NativeVulkanFfmpegHwFormatContext {
    fn open(source: &Path, codec_id: c_int) -> Result<(Self, c_int, AVRational), String> {
        let source = CString::new(source.as_os_str().as_bytes())
            .map_err(|_| "FFmpeg source path contains an interior NUL".to_owned())?;
        let mut ptr = ptr::null_mut();
        let ret = unsafe { gilder_avformat_open_input(&mut ptr, source.as_ptr()) };
        native_vulkan_ffmpeg_hw_ok(ret, "avformat_open_input FFmpeg Vulkan hwdecode")?;
        let ptr = NonNull::new(ptr)
            .ok_or_else(|| "FFmpeg avformat_open_input returned null".to_owned())?;
        let format = Self { ptr };
        let stream_index =
            unsafe { gilder_av_find_video_stream_for_codec(format.ptr.as_ptr(), codec_id) };
        if stream_index < 0 {
            return Err(native_vulkan_ffmpeg_hw_error(
                stream_index,
                "av_find_best_stream/select FFmpeg Vulkan hwdecode stream",
            ));
        }
        let time_base = unsafe { gilder_av_stream_time_base(format.ptr.as_ptr(), stream_index) };
        Ok((format, stream_index, time_base))
    }
}

impl Drop for NativeVulkanFfmpegHwFormatContext {
    fn drop(&mut self) {
        let mut ptr = self.ptr.as_ptr();
        unsafe {
            gilder_avformat_close_input(&mut ptr);
            gilder_trim_process_heap();
        }
    }
}

impl NativeVulkanFfmpegHwCodecContext {
    fn open(
        format: &NativeVulkanFfmpegHwFormatContext,
        stream_index: c_int,
        hw_device: &NativeVulkanFfmpegVulkanHwDevice,
    ) -> Result<Self, String> {
        let codec = unsafe { gilder_av_stream_decoder(format.ptr.as_ptr(), stream_index) };
        if codec.is_null() {
            return Err("FFmpeg decoder was not found for selected video stream".to_owned());
        }
        let decoder_name = unsafe {
            let name = gilder_avcodec_name(codec);
            if name.is_null() {
                String::new()
            } else {
                CStr::from_ptr(name).to_string_lossy().into_owned()
            }
        };
        let decoder_has_vulkan_hw_config =
            unsafe { gilder_avcodec_has_vulkan_hw_config(codec) != 0 };
        if !decoder_has_vulkan_hw_config {
            return Err(format!(
                "FFmpeg decoder {decoder_name:?} does not expose AV_PIX_FMT_VULKAN HW_DEVICE_CTX config"
            ));
        }
        let context = unsafe { gilder_avcodec_alloc_context3(codec) };
        let context = NonNull::new(context)
            .ok_or_else(|| "FFmpeg avcodec_alloc_context3 failed".to_owned())?;
        let result = (|| -> Result<(), String> {
            let ret = unsafe {
                gilder_avcodec_parameters_to_context_for_stream(
                    context.as_ptr(),
                    format.ptr.as_ptr(),
                    stream_index,
                )
            };
            native_vulkan_ffmpeg_hw_ok(
                ret,
                "avcodec_parameters_to_context FFmpeg Vulkan hwdecode",
            )?;
            let ret = unsafe {
                gilder_avcodec_open2_vulkan_hw(context.as_ptr(), codec, hw_device.as_ptr())
            };
            native_vulkan_ffmpeg_hw_ok(ret, "avcodec_open2 FFmpeg Vulkan hwdecode")?;
            Ok(())
        })();
        if let Err(err) = result {
            let mut ptr = context.as_ptr();
            unsafe {
                gilder_avcodec_free_context(&mut ptr);
            }
            return Err(err);
        }
        Ok(Self {
            ptr: context,
            decoder_name,
            decoder_has_vulkan_hw_config,
        })
    }
}

impl Drop for NativeVulkanFfmpegHwCodecContext {
    fn drop(&mut self) {
        let mut ptr = self.ptr.as_ptr();
        unsafe {
            gilder_avcodec_free_context(&mut ptr);
        }
    }
}

impl NativeVulkanFfmpegHwObjectPool {
    fn new() -> Result<Self, String> {
        let ptr = unsafe { gilder_ffmpeg_pool_alloc() };
        let ptr =
            NonNull::new(ptr).ok_or_else(|| "FFmpeg object pool allocation failed".to_owned())?;
        Ok(Self { ptr })
    }

    fn take_packet(&self) -> Result<NonNull<AVPacket>, String> {
        let packet = unsafe { gilder_ffmpeg_pool_get_packet(self.ptr.as_ptr()) };
        NonNull::new(packet).ok_or_else(|| "FFmpeg AVPacket pool allocation failed".to_owned())
    }

    fn take_frame(&self) -> Result<NonNull<AVFrame>, String> {
        let frame = unsafe { gilder_ffmpeg_pool_get_frame(self.ptr.as_ptr()) };
        NonNull::new(frame).ok_or_else(|| "FFmpeg AVFrame pool allocation failed".to_owned())
    }
}

impl Drop for NativeVulkanFfmpegHwObjectPool {
    fn drop(&mut self) {
        let mut ptr = self.ptr.as_ptr();
        unsafe {
            gilder_ffmpeg_pool_free(&mut ptr);
            gilder_trim_process_heap();
        }
    }
}

impl NativeVulkanFfmpegHwReusablePacket {
    fn new(pool: &NativeVulkanFfmpegHwObjectPool) -> Result<Self, String> {
        Ok(Self {
            packet: pool.take_packet()?,
            pool: pool.ptr,
        })
    }

    fn unref(&mut self) {
        unsafe {
            gilder_av_packet_unref(self.packet.as_ptr());
        }
    }
}

impl Drop for NativeVulkanFfmpegHwReusablePacket {
    fn drop(&mut self) {
        let mut packet = self.packet.as_ptr();
        unsafe {
            gilder_ffmpeg_pool_put_packet(self.pool.as_ptr(), &mut packet);
        }
    }
}

impl NativeVulkanFfmpegHwReusableFrame {
    fn new(pool: &NativeVulkanFfmpegHwObjectPool) -> Result<Self, String> {
        Ok(Self {
            frame: pool.take_frame()?,
            pool: pool.ptr,
        })
    }

    fn unref(&mut self) {
        unsafe {
            gilder_av_frame_unref(self.frame.as_ptr());
        }
    }
}

impl Drop for NativeVulkanFfmpegHwReusableFrame {
    fn drop(&mut self) {
        let mut frame = self.frame.as_ptr();
        unsafe {
            gilder_ffmpeg_pool_put_frame(self.pool.as_ptr(), &mut frame);
        }
    }
}

impl NativeVulkanFfmpegDecodedGpuFrame {
    pub(in crate::renderer::native_vulkan) unsafe fn move_from_avframe(
        frame: *mut AVFrame,
    ) -> Result<Self, String> {
        let probe = native_vulkan_ffmpeg_vulkan_hw_frame_probe(frame);
        if !probe.is_vulkan_hw_frame {
            return Err(format!(
                "FFmpeg decoded frame is not AV_PIX_FMT_VULKAN: format={}, expected={}",
                probe.frame_format, probe.expected_vulkan_format
            ));
        }
        let owned = unsafe { gilder_av_frame_alloc_owned() };
        let owned = NonNull::new(owned)
            .ok_or_else(|| "FFmpeg av_frame_alloc failed for AVVkFrame handoff".to_owned())?;
        unsafe {
            gilder_av_frame_move_ref(owned.as_ptr(), frame);
        }
        Ok(Self { frame: owned })
    }

    pub(in crate::renderer::native_vulkan) fn as_ptr(&self) -> *const AVFrame {
        self.frame.as_ptr()
    }

    pub(in crate::renderer::native_vulkan) fn probe(&self) -> NativeVulkanFfmpegVulkanHwFrameProbe {
        native_vulkan_ffmpeg_vulkan_hw_frame_probe(self.as_ptr())
    }

    pub(in crate::renderer::native_vulkan) fn descriptor_source(
        &self,
    ) -> Result<NativeVulkanFfmpegDecodedGpuFrameDescriptorSource, String> {
        native_vulkan_ffmpeg_decoded_gpu_frame_descriptor_source(self.as_ptr())
    }
}

impl Drop for NativeVulkanFfmpegDecodedGpuFrame {
    fn drop(&mut self) {
        let mut frame = self.frame.as_ptr();
        unsafe {
            gilder_av_frame_free_owned(&mut frame);
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_ffmpeg_hwdecode_constant_names_ready()
-> bool {
    unsafe {
        gilder_av_hwdevice_type_vulkan() >= 0
            && gilder_av_pix_fmt_none() < gilder_av_pix_fmt_vulkan()
    }
}

fn native_vulkan_ffmpeg_c_extension_ptrs(
    extensions: &[&str],
) -> Result<(Vec<CString>, Vec<*const c_char>), String> {
    let c_strings = extensions
        .iter()
        .map(|extension| {
            CString::new(*extension)
                .map_err(|_| format!("Vulkan extension name contains an interior NUL: {extension}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let ptrs = c_strings
        .iter()
        .map(|extension| extension.as_ptr())
        .collect::<Vec<_>>();
    Ok((c_strings, ptrs))
}

fn native_vulkan_ffmpeg_hw_ok(ret: c_int, label: &str) -> Result<(), String> {
    if ret >= 0 {
        return Ok(());
    }
    Err(native_vulkan_ffmpeg_hw_error(ret, label))
}

fn native_vulkan_ffmpeg_hw_error(ret: c_int, label: &str) -> String {
    let mut buffer = [0 as c_char; 256];
    let message = unsafe {
        if gilder_av_strerror(ret, buffer.as_mut_ptr(), buffer.len()) == 0 {
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        } else {
            format!("FFmpeg error {ret}")
        }
    };
    format!("{label} failed: {message} ({ret})")
}

fn native_vulkan_ffmpeg_eof() -> c_int {
    unsafe { gilder_av_error_eof() }
}

fn native_vulkan_ffmpeg_again() -> c_int {
    unsafe { gilder_av_error_again() }
}

fn native_vulkan_ffmpeg_codec_id(codec: NativeVulkanVideoSessionCodec) -> c_int {
    unsafe {
        match codec {
            NativeVulkanVideoSessionCodec::H264High8 => gilder_av_codec_id_h264(),
            NativeVulkanVideoSessionCodec::H265Main8
            | NativeVulkanVideoSessionCodec::H265Main10 => gilder_av_codec_id_hevc(),
            NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
                gilder_av_codec_id_av1()
            }
        }
    }
}

fn native_vulkan_ffmpeg_infer_vulkan_slice_buffer_slot_bytes(packet_size: c_int) -> u64 {
    if packet_size <= 0 {
        return 0;
    }
    let floor = u64::try_from(packet_size).unwrap_or(0).max(1024 * 1024);
    let next_exclusive_power = u64::BITS - floor.leading_zeros();
    1u64.checked_shl(next_exclusive_power).unwrap_or(u64::MAX)
}

fn native_vulkan_ffmpeg_infer_h264_refstruct_picture_bytes(
    codec: NativeVulkanVideoSessionCodec,
    coded_width: c_int,
    coded_height: c_int,
) -> u64 {
    if codec != NativeVulkanVideoSessionCodec::H264High8 || coded_width <= 0 || coded_height <= 0 {
        return 0;
    }

    let mb_width = u64::try_from((coded_width + 15) / 16).unwrap_or(0);
    let mb_height = u64::try_from((coded_height + 15) / 16).unwrap_or(0);
    let mb_stride = mb_width.saturating_add(1);
    let big_mb_num = mb_stride
        .saturating_mul(mb_height.saturating_add(1))
        .saturating_add(1);
    let mb_array_size = mb_stride.saturating_mul(mb_height);
    let b4_stride = mb_width.saturating_mul(4).saturating_add(1);
    let b4_array_size = b4_stride.saturating_mul(mb_height).saturating_mul(4);

    let qscale_table = big_mb_num.saturating_add(mb_stride);
    let mb_type = qscale_table.saturating_mul(4);
    let motion_val_one_list = 2u64
        .saturating_mul(b4_array_size.saturating_add(4))
        .saturating_mul(2);
    let ref_index_one_list = 4u64.saturating_mul(mb_array_size);

    qscale_table
        .saturating_add(mb_type)
        .saturating_add(motion_val_one_list.saturating_mul(2))
        .saturating_add(ref_index_one_list.saturating_mul(2))
}

fn native_vulkan_ffmpeg_codec_resolution_scaled_host_memory_model(
    codec: NativeVulkanVideoSessionCodec,
) -> &'static str {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => "ffmpeg-h264-refstruct-min-three-pictures",
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            "ffmpeg-hevc-refstruct-min-three-pictures-plus-layer-tables-assuming-min-pu4-ctb64"
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            "no-large-resolution-scaled-ffmpeg-av1-host-table-observed"
        }
    }
}

fn native_vulkan_ffmpeg_infer_codec_resolution_scaled_host_bytes(
    codec: NativeVulkanVideoSessionCodec,
    h264_min_three_picture_bytes: u64,
    hevc_min_three_picture_bytes: u64,
    hevc_layer_tables_bytes: u64,
) -> u64 {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => h264_min_three_picture_bytes,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            hevc_min_three_picture_bytes.saturating_add(hevc_layer_tables_bytes)
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => 0,
    }
}

fn native_vulkan_ffmpeg_infer_hevc_refstruct_picture_bytes(
    codec: NativeVulkanVideoSessionCodec,
    coded_width: c_int,
    coded_height: c_int,
) -> u64 {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    ) || coded_width <= 0
        || coded_height <= 0
    {
        return 0;
    }

    // HEVC keeps motion-vector fields and CTB ref-list tables even when Vulkan
    // owns the pixel decode. Without private HEVCSPS access we model the common
    // main-profile shape used by our corpus: min PU 4x4 and CTB 64x64.
    let width = u64::try_from(coded_width).unwrap_or(0);
    let height = u64::try_from(coded_height).unwrap_or(0);
    let min_pu_count = native_vulkan_ffmpeg_ceil_div(width, 4)
        .saturating_mul(native_vulkan_ffmpeg_ceil_div(height, 4));
    let ctb_count = native_vulkan_ffmpeg_ceil_div(width, 64)
        .saturating_mul(native_vulkan_ffmpeg_ceil_div(height, 64));
    const HEVC_MV_FIELD_BYTES: u64 = 12;
    const HEVC_REF_PIC_LIST_TAB_BYTES: u64 = 528;
    min_pu_count
        .saturating_mul(HEVC_MV_FIELD_BYTES)
        .saturating_add(ctb_count.saturating_mul(HEVC_REF_PIC_LIST_TAB_BYTES))
}

fn native_vulkan_ffmpeg_infer_hevc_layer_table_bytes(
    codec: NativeVulkanVideoSessionCodec,
    coded_width: c_int,
    coded_height: c_int,
) -> u64 {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    ) || coded_width <= 0
        || coded_height <= 0
    {
        return 0;
    }

    let width = u64::try_from(coded_width).unwrap_or(0);
    let height = u64::try_from(coded_height).unwrap_or(0);
    let ctb_count = native_vulkan_ffmpeg_ceil_div(width, 64)
        .saturating_mul(native_vulkan_ffmpeg_ceil_div(height, 64));
    let min_cb_count = native_vulkan_ffmpeg_ceil_div(width, 8)
        .saturating_mul(native_vulkan_ffmpeg_ceil_div(height, 8));
    let min_tb_count = native_vulkan_ffmpeg_ceil_div(width, 4)
        .saturating_mul(native_vulkan_ffmpeg_ceil_div(height, 4));
    let min_pu_width = native_vulkan_ffmpeg_ceil_div(width, 4);
    let min_pu_height = native_vulkan_ffmpeg_ceil_div(height, 4);
    let min_pu_count = min_pu_width.saturating_mul(min_pu_height);
    let pcm_count = min_pu_width
        .saturating_add(1)
        .saturating_mul(min_pu_height.saturating_add(1));
    let boundary_strength_count = width
        .saturating_div(4)
        .saturating_add(1)
        .saturating_mul(height.saturating_div(4).saturating_add(1))
        .saturating_mul(2);

    const HEVC_SAO_PARAMS_BYTES: u64 = 144;
    const HEVC_DB_PARAMS_BYTES: u64 = 8;
    ctb_count
        .saturating_mul(HEVC_SAO_PARAMS_BYTES.saturating_add(HEVC_DB_PARAMS_BYTES))
        .saturating_add(min_cb_count.saturating_mul(2))
        .saturating_add(min_tb_count)
        .saturating_add(min_pu_count)
        .saturating_add(pcm_count)
        .saturating_add(ctb_count.saturating_mul(6))
        .saturating_add(boundary_strength_count)
}

fn native_vulkan_ffmpeg_ceil_div(value: u64, divisor: u64) -> u64 {
    if divisor == 0 {
        return 0;
    }
    value.saturating_add(divisor.saturating_sub(1)) / divisor
}

fn native_vulkan_ffmpeg_video_codec_operation_labels(
    operations: vk::VideoCodecOperationFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if operations.contains(vk::VideoCodecOperationFlagsKHR::DECODE_H264) {
        labels.push("decode-h264");
    }
    if operations.contains(vk::VideoCodecOperationFlagsKHR::DECODE_H265) {
        labels.push("decode-h265");
    }
    if operations.contains(vk::VideoCodecOperationFlagsKHR::DECODE_AV1) {
        labels.push("decode-av1");
    }
    labels
}

fn native_vulkan_ffmpeg_decoded_gpu_frame_descriptor_source(
    frame: *const AVFrame,
) -> Result<NativeVulkanFfmpegDecodedGpuFrameDescriptorSource, String> {
    let probe = native_vulkan_ffmpeg_vulkan_hw_frame_probe(frame);
    if !probe.is_vulkan_hw_frame {
        return Err(format!(
            "FFmpeg descriptor source requires AV_PIX_FMT_VULKAN, got format {}",
            probe.frame_format
        ));
    }
    if probe.vulkan_image_count != 1 {
        return Err(format!(
            "FFmpeg descriptor source currently requires one multiplane AVVkFrame image, got {}",
            probe.vulkan_image_count
        ));
    }

    let sw_format = unsafe { gilder_av_frame_hw_sw_format(frame) };
    let (sw_format_label, picture_format) =
        native_vulkan_ffmpeg_sw_format_to_picture_format(sw_format)?;
    let image = unsafe { gilder_av_frame_vulkan_image(frame, 0) };
    if image == 0 {
        return Err("FFmpeg AVVkFrame image[0] is null".to_owned());
    }
    let width = unsafe { gilder_av_frame_width(frame) };
    let height = unsafe { gilder_av_frame_height(frame) };
    let array_layers = unsafe { gilder_av_frame_vulkan_nb_layers(frame) };
    if width <= 0 || height <= 0 || array_layers <= 0 {
        return Err(format!(
            "FFmpeg AVVkFrame has invalid extent/layers: {width}x{height}, layers={array_layers}"
        ));
    }

    let layout = unsafe { gilder_av_frame_vulkan_layout(frame, 0) };
    let semaphore = unsafe { gilder_av_frame_vulkan_timeline_semaphore(frame, 0) };
    let plane = NativeVulkanFfmpegDecodedGpuFramePlane {
        image: vk::Image::from_raw(image),
        layout: vk::ImageLayout::from_raw(layout),
        timeline_semaphore: vk::Semaphore::from_raw(semaphore),
        timeline_value: unsafe { gilder_av_frame_vulkan_timeline_semaphore_value(frame, 0) },
        queue_family_index: unsafe { gilder_av_frame_vulkan_queue_family(frame, 0) },
    };
    Ok(NativeVulkanFfmpegDecodedGpuFrameDescriptorSource {
        binding: "ffmpeg-avvkframe",
        route: "descriptor-heap-decoded-gpu-frame-source",
        picture_format,
        sw_format: sw_format_label,
        extent: (width as u32, height as u32, 1),
        array_layers: array_layers as u32,
        planes: vec![plane],
        pts_raw: native_vulkan_ffmpeg_optional_timestamp(unsafe { gilder_av_frame_pts(frame) }),
        duration_raw: native_vulkan_ffmpeg_optional_duration(unsafe {
            gilder_av_frame_duration(frame)
        }),
        zero_copy_scope: "AVVkFrame VkImage is sampled directly; descriptor heap copies image/sampler metadata only",
    })
}

fn native_vulkan_ffmpeg_sw_format_to_picture_format(
    sw_format: c_int,
) -> Result<(&'static str, vk::Format), String> {
    unsafe {
        if sw_format == gilder_av_pix_fmt_nv12() {
            return Ok(("AV_PIX_FMT_NV12", vk::Format::G8_B8R8_2PLANE_420_UNORM));
        }
        if sw_format == gilder_av_pix_fmt_p010le() {
            return Ok((
                "AV_PIX_FMT_P010LE",
                vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
            ));
        }
    }
    Err(format!(
        "FFmpeg AVVkFrame software format {sw_format} is not mapped to a descriptor-heap Y/UV picture format"
    ))
}

fn native_vulkan_ffmpeg_optional_timestamp(value: i64) -> Option<i64> {
    if value < 0 { None } else { Some(value) }
}

fn native_vulkan_ffmpeg_optional_duration(value: i64) -> Option<i64> {
    if value <= 0 { None } else { Some(value) }
}

#[cfg(test)]
mod tests;
