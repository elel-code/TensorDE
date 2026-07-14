#[cfg(feature = "native-vulkan-video")]
pub(super) struct NativeVulkanFfmpegPresentedFrameSetRetentionQueue<'a> {
    device: &'a Device,
    frame_resources: &'a VulkanaliaDecodedImagePresentFrameResources,
    frames: VecDeque<NativeVulkanFfmpegPresentedFrameSetRetention>,
    peak_frame_count: usize,
}

#[cfg(feature = "native-vulkan-video")]
impl<'a> NativeVulkanFfmpegPresentedFrameSetRetentionQueue<'a> {
    pub(super) fn new(
        device: &'a Device,
        frame_resources: &'a VulkanaliaDecodedImagePresentFrameResources,
    ) -> Self {
        Self {
            device,
            frame_resources,
            frames: VecDeque::new(),
            peak_frame_count: 0,
        }
    }

    pub(super) fn push_after_submit(
        &mut self,
        present_frame_slot: u32,
        decoded_frames: Vec<NativeVulkanFfmpegDecodedGpuFrame>,
    ) -> Result<(), String> {
        self.release_completed_slot(present_frame_slot);
        self.frames
            .push_back(NativeVulkanFfmpegPresentedFrameSetRetention {
                present_frame_slot,
                _decoded_frames: decoded_frames,
            });
        self.peak_frame_count = self.peak_frame_count.max(self.retained_frame_ref_count());
        self.release_completed_frames()
    }

    pub(super) fn release_completed_slot(&mut self, present_frame_slot: u32) {
        if let Some(index) = self
            .frames
            .iter()
            .position(|frame| frame.present_frame_slot == present_frame_slot)
        {
            self.frames.remove(index);
        }
    }

    pub(super) fn release_completed_frames(&mut self) -> Result<(), String> {
        let mut index = 0usize;
        while index < self.frames.len() {
            let present_frame_slot = self.frames[index].present_frame_slot;
            if native_vulkan_vulkanalia_try_complete_decoded_image_present_frame_slot(
                self.device,
                self.frame_resources,
                present_frame_slot,
            )? {
                self.frames.remove(index);
            } else {
                index += 1;
            }
        }
        Ok(())
    }

    pub(super) fn drain_after_waits(&mut self) -> Result<(), String> {
        while let Some(frame) = self.frames.pop_front() {
            native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
                self.device,
                self.frame_resources,
                frame.present_frame_slot,
            )?;
            drop(frame);
        }
        Ok(())
    }

    pub(super) fn retained_frame_ref_count(&self) -> usize {
        self.frames.iter().fold(0usize, |sum, frame| {
            sum.saturating_add(frame._decoded_frames.len())
        })
    }

    pub(super) fn frame_count(&self) -> u32 {
        self.retained_frame_ref_count().min(u32::MAX as usize) as u32
    }

    pub(super) fn peak_frame_count(&self) -> u32 {
        self.peak_frame_count.min(u32::MAX as usize) as u32
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Drop for NativeVulkanFfmpegPresentedFrameSetRetentionQueue<'_> {
    pub(super) fn drop(&mut self) {
        if !self.frames.is_empty() {
            let _ = unsafe { self.device.device_wait_idle() };
        }
        self.frames.clear();
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(super) struct NativeVulkanFfmpegPresentSamplerCacheEntry {
    image: vk::Image,
    picture_format: vk::Format,
    array_layers: u32,
    sampler: VulkanaliaDecodedImagePresentSamplerResources,
}

#[cfg(feature = "native-vulkan-video")]
pub(super) struct NativeVulkanFfmpegPresentSamplerCache<'a> {
    device: &'a Device,
    memory_properties: &'a vk::PhysicalDeviceMemoryProperties,
    video_queue_family_index: u32,
    present_queue_family_index: u32,
    descriptor_heap_enabled: bool,
    descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    entries: Vec<NativeVulkanFfmpegPresentSamplerCacheEntry>,
    descriptor_rewrite_count: u32,
    descriptor_recreate_count: u32,
    peak_entry_count: usize,
}

#[cfg(feature = "native-vulkan-video")]
impl<'a> NativeVulkanFfmpegPresentSamplerCache<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        device: &'a Device,
        memory_properties: &'a vk::PhysicalDeviceMemoryProperties,
        video_queue_family_index: u32,
        present_queue_family_index: u32,
        descriptor_heap_enabled: bool,
        descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    ) -> Self {
        Self {
            device,
            memory_properties,
            video_queue_family_index,
            present_queue_family_index,
            descriptor_heap_enabled,
            descriptor_heap_properties,
            entries: Vec::new(),
            descriptor_rewrite_count: 0,
            descriptor_recreate_count: 0,
            peak_entry_count: 0,
        }
    }

    pub(super) fn ensure_for_descriptor_source(
        &mut self,
        descriptor_source: &NativeVulkanFfmpegDecodedGpuFrameDescriptorSource,
    ) -> Result<usize, String> {
        let [plane] = descriptor_source.planes.as_slice() else {
            return Err(format!(
                "FFmpeg AVVkFrame sampler cache requires one multiplane image, got {}",
                descriptor_source.planes.len()
            ));
        };
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.image == plane.image)
        {
            let entry = self
                .entries
                .get_mut(index)
                .expect("image cache index came from position");
            if entry.picture_format == descriptor_source.picture_format
                && entry.array_layers == descriptor_source.array_layers
            {
                return Ok(index);
            }
            let entry = self.entries.remove(index);
            native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
                self.device,
                entry.sampler,
            );
            self.descriptor_recreate_count = self.descriptor_recreate_count.saturating_add(1);
        }

        let sampler =
            native_vulkan_vulkanalia_create_ffmpeg_decoded_gpu_frame_present_sampler_resources(
                self.device,
                self.memory_properties,
                descriptor_source,
                0,
                self.video_queue_family_index,
                self.present_queue_family_index,
                self.descriptor_heap_enabled,
                self.descriptor_heap_properties,
            )?;
        self.entries
            .push(NativeVulkanFfmpegPresentSamplerCacheEntry {
                image: plane.image,
                picture_format: descriptor_source.picture_format,
                array_layers: descriptor_source.array_layers,
                sampler,
            });
        self.peak_entry_count = self.peak_entry_count.max(self.entries.len());
        Ok(self.entries.len().saturating_sub(1))
    }

    pub(super) fn sampler(
        &self,
        index: usize,
    ) -> Result<&VulkanaliaDecodedImagePresentSamplerResources, String> {
        self.entries
            .get(index)
            .map(|entry| &entry.sampler)
            .ok_or_else(|| format!("FFmpeg AVVkFrame sampler cache index {index} is unavailable"))
    }

    pub(super) fn descriptor_heap_plan(
        &self,
        index: usize,
    ) -> Result<&NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot, String> {
        self.entries
            .get(index)
            .map(|entry| &entry.sampler.snapshot.descriptor_heap_plan)
            .ok_or_else(|| {
                format!("FFmpeg AVVkFrame sampler cache index {index} has no descriptor plan")
            })
    }

    pub(super) fn entry_count(&self) -> u32 {
        self.entries.len().min(u32::MAX as usize) as u32
    }

    pub(super) fn peak_entry_count(&self) -> u32 {
        self.peak_entry_count.min(u32::MAX as usize) as u32
    }

    pub(super) fn descriptor_rewrite_count(&self) -> u32 {
        self.descriptor_rewrite_count
    }

    pub(super) fn descriptor_recreate_count(&self) -> u32 {
        self.descriptor_recreate_count
    }

    pub(super) fn resource_heap_bytes(&self) -> u64 {
        self.entries.iter().fold(0u64, |sum, entry| {
            sum.saturating_add(entry.sampler.descriptor_heap.plan.resource_heap_bytes)
        })
    }

    pub(super) fn sampler_heap_bytes(&self) -> u64 {
        self.entries.iter().fold(0u64, |sum, entry| {
            sum.saturating_add(entry.sampler.descriptor_heap.plan.sampler_heap_bytes)
        })
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Drop for NativeVulkanFfmpegPresentSamplerCache<'_> {
    pub(super) fn drop(&mut self) {
        for entry in self.entries.drain(..) {
            native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
                self.device,
                entry.sampler,
            );
        }
    }
}

// Source slots are built before scoped workers start and destroyed only after
// all workers join. The descriptor heap mapped pointer is not mutated by decode
// workers; present only binds the immutable heap handles while decode writes the
// source's separate Vulkan Video image through queue synchronization.
#[cfg(feature = "native-vulkan-video")]
unsafe impl Send for NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot {}

#[cfg(feature = "native-vulkan-video")]
unsafe impl Sync for NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot {}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot {
    pub(super) fn decode_wait(
        &self,
        frame: NativeVulkanVulkanaliaDecodedPresentHandoffFrame,
    ) -> super::render_present::VulkanaliaDecodedImagePresentDecodeWait {
        super::render_present::VulkanaliaDecodedImagePresentDecodeWait {
            semaphore: self.decode_complete,
            value: frame.decode_complete_value,
        }
    }

    pub(super) fn present_source(
        &self,
        frame: NativeVulkanVulkanaliaDecodedPresentHandoffFrame,
    ) -> super::render_present::VulkanaliaDecodedImagePresentSource<'_> {
        super::render_present::VulkanaliaDecodedImagePresentSource {
            image: super::render_present::VulkanaliaDecodedImagePresentImageSource {
                image: self.resource_image.image,
                array_layers: self.resource_image.snapshot.array_layers,
                current_layout: vk::ImageLayout::VIDEO_DECODE_DPB_KHR,
                restore_layout: vk::ImageLayout::VIDEO_DECODE_DPB_KHR,
                queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            },
            sampler: &self.sampler,
            sampled_array_layer: frame.sampled_array_layer,
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn destroy_multi_video_decode_source_slot(
    device: &Device,
    slot: NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot,
) {
    native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(device, slot.sampler);
    unsafe {
        device.destroy_semaphore(slot.decode_complete, None);
    }
    native_vulkan_vulkanalia_destroy_video_session_resource_image(device, slot.resource_image);
    native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources(
        device,
        slot.memory_resources,
    );
    native_vulkan_vulkanalia_destroy_video_session(device, slot.session);
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_vulkanalia_prepare_streaming_decode_requests(
    requests: NativeVulkanVulkanaliaStreamingDecodeRequests,
    codec: NativeVulkanVideoSessionCodec,
    session_max_dpb_slots: u32,
) -> Result<NativeVulkanVulkanaliaPreparedStreamingDecode, String> {
    let h264 = if let Some(request) = requests.h264 {
        if codec != NativeVulkanVideoSessionCodec::H264High8 {
            return Err(
                "H.264 streaming decode request does not match the video session codec".to_owned(),
            );
        }
        let mut queue = native_vulkan_start_h264_streaming_packet_queue(
            &request.source,
            request.queue_capacity.max(1),
        )
        .map_err(|err| err.to_string())?;
        let parameter_sets = queue.parameter_sets.clone();
        let bootstrap = native_vulkan_h264_align_streaming_bootstrap(&mut queue, &parameter_sets)
            .map_err(|err| err.to_string())?;
        native_vulkan_vulkanalia_require_streaming_dpb_slots(
            "H.264",
            bootstrap.stream_dpb_slots,
            session_max_dpb_slots,
        )?;
        Some(NativeVulkanVulkanaliaPreparedH264StreamingDecode {
            request,
            queue,
            parameter_sets,
            bootstrap,
        })
    } else {
        None
    };
    let h265 = if let Some(request) = requests.h265 {
        if !matches!(
            codec,
            NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
        ) {
            return Err(
                "H.265 streaming decode request does not match the video session codec".to_owned(),
            );
        }
        let mut queue = native_vulkan_start_h265_streaming_packet_queue(
            &request.source,
            request.queue_capacity.max(1),
        )
        .map_err(|err| err.to_string())?;
        let parameter_sets = queue.parameter_sets.clone();
        let bootstrap = native_vulkan_h265_align_streaming_bootstrap(&mut queue, &parameter_sets)
            .map_err(|err| err.to_string())?;
        native_vulkan_vulkanalia_require_streaming_dpb_slots(
            "H.265",
            bootstrap.stream_dpb_slots,
            session_max_dpb_slots,
        )?;
        Some(NativeVulkanVulkanaliaPreparedH265StreamingDecode {
            request,
            queue,
            parameter_sets,
            bootstrap,
        })
    } else {
        None
    };
    let av1 = if let Some(request) = requests.av1 {
        if !matches!(
            codec,
            NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
        ) {
            return Err(
                "AV1 streaming decode request does not match the video session codec".to_owned(),
            );
        }
        let mut queue = native_vulkan_start_av1_streaming_packet_queue(
            &request.source,
            request.queue_capacity.max(1),
        )
        .map_err(|err| err.to_string())?;
        let sequence_header = queue.parameter_sets.clone();
        let bootstrap = native_vulkan_av1_align_streaming_bootstrap(&mut queue, &sequence_header)
            .map_err(|err| err.to_string())?;
        native_vulkan_vulkanalia_require_streaming_dpb_slots(
            "AV1",
            bootstrap.stream_dpb_slots,
            session_max_dpb_slots,
        )?;
        Some(NativeVulkanVulkanaliaPreparedAv1StreamingDecode {
            request,
            queue,
            sequence_header,
            bootstrap,
        })
    } else {
        None
    };
    Ok(NativeVulkanVulkanaliaPreparedStreamingDecode { h264, h265, av1 })
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_vulkanalia_streaming_decode_requests_for_source(
    source: &NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions,
    session: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
) -> NativeVulkanVulkanaliaStreamingDecodeRequests {
    match source.codec {
        NativeVulkanVideoSessionCodec::H264High8 => NativeVulkanVulkanaliaStreamingDecodeRequests {
            h264: Some(
                NativeVulkanVulkanaliaH264StreamingVideoPresentDecodeOptions {
                    session,
                    source: source.source.clone(),
                    queue_capacity: source.queue_capacity,
                    playback_frame_count: source.playback_frame_count,
                },
            ),
            h265: None,
            av1: None,
        },
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            NativeVulkanVulkanaliaStreamingDecodeRequests {
                h264: None,
                h265: Some(
                    NativeVulkanVulkanaliaH265StreamingVideoPresentDecodeOptions {
                        session,
                        source: source.source.clone(),
                        queue_capacity: source.queue_capacity,
                        playback_frame_count: source.playback_frame_count,
                    },
                ),
                av1: None,
            }
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            NativeVulkanVulkanaliaStreamingDecodeRequests {
                h264: None,
                h265: None,
                av1: Some(
                    NativeVulkanVulkanaliaAv1StreamingVideoPresentDecodeOptions {
                        session,
                        source: source.source.clone(),
                        queue_capacity: source.queue_capacity,
                        playback_frame_count: source.playback_frame_count,
                    },
                ),
            }
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
#[allow(clippy::too_many_arguments)]
pub(super) fn create_multi_video_decode_source_slot(
    instance: &Instance,
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    selection: &super::video_present_device::NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    source_index: usize,
    source: NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions,
    host: crate::renderer::native_wayland::NativeWaylandHostOptions,
    wait_configure_roundtrips: usize,
    target_max_fps: Option<u32>,
    audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    clear_color: NativeVulkanClearColor,
) -> Result<
    (
        NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot,
        NativeVulkanVulkanaliaPreparedStreamingDecode,
    ),
    String,
> {
    if source.width == 0 || source.height == 0 {
        return Err(format!(
            "multi-source video source {} requires non-zero extent",
            source.source.display()
        ));
    }
    let session_options = NativeVulkanVulkanaliaVideoPresentSessionProbeOptions {
        host,
        wait_configure_roundtrips,
        codec: source.codec,
        width: source.width,
        height: source.height,
        target_max_fps,
        audio_master_clock,
        clear_color,
    };
    let requests =
        native_vulkan_vulkanalia_streaming_decode_requests_for_source(&source, session_options);
    with_native_vulkan_vulkanalia_video_session_capabilities(
        instance,
        selection.physical_device,
        source.codec,
        None,
        None,
        |profile_info, queried| {
            let driver_session_max_dpb_slots = native_vulkan_vulkanalia_video_session_max_dpb_slots(
                queried.capabilities.max_dpb_slots,
            );
            let driver_session_max_active_reference_pictures =
                native_vulkan_vulkanalia_video_session_max_active_reference_pictures(
                    queried.capabilities.max_active_reference_pictures,
                    driver_session_max_dpb_slots,
                );
            let prepared_streaming_decode =
                native_vulkan_vulkanalia_prepare_streaming_decode_requests(
                    requests,
                    source.codec,
                    driver_session_max_dpb_slots,
                )?;
            let requested_extent =
                prepared_streaming_decode
                    .coded_extent()
                    .unwrap_or(vk::Extent2D {
                        width: source.width,
                        height: source.height,
                    });
            let av1_sequence_header = prepared_streaming_decode.av1_sequence_header();
            if !native_vulkan_vulkanalia_video_session_extent_supported(
                requested_extent,
                queried.capabilities,
            ) {
                return Err(format!(
                    "multi-source video source {} extent {}x{} is outside driver capabilities",
                    source.source.display(),
                    requested_extent.width,
                    requested_extent.height
                ));
            }
            let required_dpb_slots =
                prepared_streaming_decode.required_resource_image_array_layers();
            let session_max_dpb_slots = native_vulkan_vulkanalia_select_stream_session_dpb_slots(
                required_dpb_slots,
                driver_session_max_dpb_slots,
            )?;
            let required_active_reference_pictures =
                prepared_streaming_decode.required_max_active_reference_pictures();
            let session_max_active_reference_pictures =
                native_vulkan_vulkanalia_select_stream_session_active_reference_pictures(
                    required_active_reference_pictures,
                    driver_session_max_active_reference_pictures,
                    session_max_dpb_slots,
                )?;
            let resource_image_array_layers =
                native_vulkan_vulkanalia_select_stream_resource_image_array_layers(
                    required_dpb_slots,
                    session_max_dpb_slots,
                )?;
            let picture_format = native_vulkan_vulkanalia_video_session_effective_picture_format(
                source.codec,
                av1_sequence_header,
            );
            let video_session_create_flags = native_vulkan_vulkanalia_video_session_create_flags(
                context
                    .video_feature_selection
                    .inline_session_parameters_enabled,
            );
            let create_info = vk::VideoSessionCreateInfoKHR::builder()
                .flags(video_session_create_flags)
                .queue_family_index(selection.video_queue_family_index)
                .video_profile(profile_info)
                .picture_format(picture_format)
                .reference_picture_format(picture_format)
                .max_coded_extent(requested_extent)
                .max_dpb_slots(session_max_dpb_slots)
                .max_active_reference_pictures(session_max_active_reference_pictures)
                .std_header_version(&queried.capabilities.std_header_version)
                .build();
            let session =
                native_vulkan_vulkanalia_create_video_session(&context.device, &create_info)?;
            let mut memory_resources = None;
            let mut resource_image = None;
            let mut sampler = None;
            let mut decode_complete = None;
            let result = (|| {
                let memory_properties = unsafe {
                    instance.get_physical_device_memory_properties(selection.physical_device)
                };
                memory_resources = Some(
                    native_vulkan_vulkanalia_bind_video_session_memory_resources(
                        &context.device,
                        &memory_properties,
                        session,
                    )?,
                );
                let resource_queue_family_indices = video_present_queue_family_indices(
                    selection.video_queue_family_index,
                    selection.present_queue_family_index,
                );
                resource_image = Some(
                    native_vulkan_vulkanalia_create_video_session_resource_image(
                        instance,
                        &context.device,
                        &memory_properties,
                        selection.physical_device,
                        profile_info,
                        requested_extent,
                        resource_image_array_layers,
                        picture_format,
                        queried.decode_capability_flags,
                        &resource_queue_family_indices,
                    )?,
                );
                sampler = Some(
                    native_vulkan_vulkanalia_create_decoded_image_present_sampler_resources(
                        &context.device,
                        &memory_properties,
                        resource_image
                            .as_ref()
                            .expect("multi-source resource image is live"),
                        picture_format,
                        0,
                        selection.video_queue_family_index,
                        selection.present_queue_family_index,
                        context
                            .video_feature_selection
                            .core_features
                            .descriptor_heap,
                        context.video_feature_selection.descriptor_heap_properties,
                    )?,
                );
                decode_complete = Some(native_vulkan_vulkanalia_create_decode_timeline_semaphore(
                    &context.device,
                )?);
                let snapshot =
                    NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSourceSnapshot {
                        source_index,
                        source: source.source.clone(),
                        codec: source.codec,
                        requested_extent: (requested_extent.width, requested_extent.height),
                        playback_frame_count: source.playback_frame_count,
                        decoded_into_retained_resource_image: true,
                        decoded_image_zero_copy_presented: false,
                    };
                Ok((
                    NativeVulkanVulkanaliaMultiVideoDecodeSourceSlot {
                        source_index,
                        source: source.source.clone(),
                        codec: source.codec,
                        requested_extent,
                        picture_format,
                        memory_properties,
                        resource_image_array_layers,
                        session,
                        memory_resources: memory_resources
                            .take()
                            .expect("multi-source session memory is live"),
                        resource_image: resource_image
                            .take()
                            .expect("multi-source resource image is live"),
                        sampler: sampler.take().expect("multi-source sampler is live"),
                        decode_complete: decode_complete
                            .take()
                            .expect("multi-source decode semaphore is live"),
                        snapshot,
                    },
                    prepared_streaming_decode,
                ))
            })();
            if result.is_err() {
                if let Some(sampler) = sampler.take() {
                    native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
                        &context.device,
                        sampler,
                    );
                }
                if let Some(decode_complete) = decode_complete.take() {
                    unsafe {
                        context.device.destroy_semaphore(decode_complete, None);
                    }
                }
                if let Some(resource_image) = resource_image.take() {
                    native_vulkan_vulkanalia_destroy_video_session_resource_image(
                        &context.device,
                        resource_image,
                    );
                }
                if let Some(memory_resources) = memory_resources.take() {
                    native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources(
                        &context.device,
                        memory_resources,
                    );
                }
                native_vulkan_vulkanalia_destroy_video_session(&context.device, session);
            }
            result
        },
    )
}

#[cfg(not(feature = "native-vulkan-video"))]
fn native_vulkan_vulkanalia_prepare_streaming_decode_requests(
    _requests: NativeVulkanVulkanaliaStreamingDecodeRequests,
    _codec: NativeVulkanVideoSessionCodec,
    _session_max_dpb_slots: u32,
) -> Result<(), String> {
    Ok(())
}

pub(super) fn native_vulkan_vulkanalia_require_streaming_dpb_slots(
    codec: &'static str,
    required_dpb_slots: u32,
    session_max_dpb_slots: u32,
) -> Result<(), String> {
    if session_max_dpb_slots == 0 || required_dpb_slots <= session_max_dpb_slots {
        return Ok(());
    }
    Err(format!(
        "{codec} streaming decode requires {required_dpb_slots} DPB slot(s), but the selected Vulkan video session exposes only {session_max_dpb_slots}"
    ))
}

#[cfg(feature = "native-vulkan-video")]
pub(super) struct NativeVulkanVulkanaliaStreamingPtsState {
    source_loop_index: u32,
    pts_offset_ns: u64,
    loop_base_source_pts_ns: Option<u64>,
    last_adjusted_pts_ns: Option<u64>,
    last_duration_ns: Option<u64>,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanVulkanaliaStreamingPtsState {
    pub(super) fn new(source_loop_index: u32) -> Self {
        Self {
            source_loop_index,
            pts_offset_ns: 0,
            loop_base_source_pts_ns: None,
            last_adjusted_pts_ns: None,
            last_duration_ns: None,
        }
    }

    pub(super) fn sync_loop(&mut self, source_loop_index: u32) -> bool {
        if source_loop_index == self.source_loop_index {
            return false;
        }
        self.source_loop_index = source_loop_index;
        self.pts_offset_ns = self
            .last_adjusted_pts_ns
            .map(|pts| pts.saturating_add(self.last_duration_ns.unwrap_or(1).max(1)))
            .unwrap_or(self.pts_offset_ns);
        self.loop_base_source_pts_ns = None;
        true
    }

    pub(super) fn adjusted_pts_ns(
        &mut self,
        source_pts_ns: Option<u64>,
        source_pts_ms: Option<u64>,
        source_duration_ns: Option<u64>,
        source_duration_ms: Option<u64>,
    ) -> Option<u64> {
        let pts_ns =
            source_pts_ns.or_else(|| source_pts_ms.map(|pts| pts.saturating_mul(1_000_000)));
        let duration_ns = source_duration_ns
            .or_else(|| source_duration_ms.map(|duration| duration.saturating_mul(1_000_000)));
        let adjusted = pts_ns.map(|pts| {
            let base = *self.loop_base_source_pts_ns.get_or_insert(pts);
            pts.saturating_sub(base).saturating_add(self.pts_offset_ns)
        });
        if let Some(adjusted) = adjusted {
            self.last_adjusted_pts_ns = Some(adjusted);
        }
        if let Some(duration) = duration_ns {
            self.last_duration_ns = Some(duration);
        }
        adjusted
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_vulkanalia_next_h264_streaming_frame(
    queue: &mut NativeVulkanH264StreamingPacketQueue,
    planner: &mut NativeVulkanH264DecodeReferencePlanner,
    pts_state: &mut NativeVulkanVulkanaliaStreamingPtsState,
) -> Result<NativeVulkanVulkanaliaH264ReadyPrefixFrameInput, String> {
    let packet = queue.next_packet(true).map_err(|err| err.to_string())?;
    if pts_state.sync_loop(packet.source_loop_index) {
        planner.reset();
    }
    let mut snapshot = packet.snapshot;
    let mut entry = planner.plan_next(&snapshot);
    let pts_ns = pts_state.adjusted_pts_ns(
        snapshot.pts_ns,
        snapshot.pts_ms,
        snapshot.duration_ns,
        snapshot.duration_ms,
    );
    entry.pts_ms = pts_ns.map(|pts| pts / 1_000_000).or(snapshot.pts_ms);
    if !entry.ready_for_decode_submit {
        let references = entry
            .references
            .iter()
            .map(|reference| {
                format!(
                    "frame_num={} slot={:?} available={} source_au={:?}",
                    reference.frame_num,
                    reference.dpb_slot,
                    reference.available,
                    reference.source_access_unit_index
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "Vulkanalia H.264 streaming AU {} is not decode-ready: {}; frame_num={:?}; requested_refs={}; available_refs={}; missing_refs={}; planned_output_slot={}; refs=[{}]",
            entry.access_unit_index,
            entry
                .unsupported_reason
                .as_deref()
                .unwrap_or("missing references"),
            entry.current_frame_num,
            entry.requested_reference_count,
            entry.available_reference_count,
            entry.missing_reference_count,
            entry.planned_output_slot,
            references
        ));
    }
    if let Some(err) = &snapshot.first_slice_parse_error {
        return Err(format!(
            "Vulkanalia H.264 streaming AU {} first slice parse failed: {err}",
            snapshot.index
        ));
    }
    let first_slice = snapshot.first_slice.take().ok_or_else(|| {
        format!(
            "Vulkanalia H.264 streaming AU {} has no parsed first slice",
            snapshot.index
        )
    })?;
    if first_slice.slice_offsets.is_empty() {
        return Err(format!(
            "Vulkanalia H.264 streaming AU {} has no slice offsets",
            snapshot.index
        ));
    }
    Ok(NativeVulkanVulkanaliaH264ReadyPrefixFrameInput {
        entry,
        first_slice,
        pts_ns,
        duration_ns: snapshot.duration_ns,
        duration_ms: snapshot.duration_ms,
        access_unit_payload: packet.access_unit.payload,
    })
}

#[cfg(feature = "native-vulkan-video")]
pub(super) fn native_vulkan_vulkanalia_next_h265_streaming_frame(
    queue: &mut NativeVulkanH265StreamingPacketQueue,
    planner: &mut NativeVulkanH265DecodeReferencePlanner,
    pts_state: &mut NativeVulkanVulkanaliaStreamingPtsState,
) -> Result<NativeVulkanVulkanaliaH265ReadyPrefixFrameInput, String> {
    let packet = queue.next_packet(true).map_err(|err| err.to_string())?;
    if pts_state.sync_loop(packet.source_loop_index) {
        planner.reset_for_idr();
    }
    let mut snapshot = packet.snapshot;
    let mut entry = planner.plan_next(&snapshot);
    let pts_ns = pts_state.adjusted_pts_ns(
        snapshot.pts_ns,
        snapshot.pts_ms,
        snapshot.duration_ns,
        snapshot.duration_ms,
    );
    entry.pts_ms = pts_ns.map(|pts| pts / 1_000_000).or(snapshot.pts_ms);
    if !entry.ready_for_decode_submit {
        return Err(format!(
            "Vulkanalia H.265 streaming AU {} is not decode-ready; missing POCs {:?}",
            entry.access_unit_index, entry.missing_reference_pocs
        ));
    }
    if let Some(err) = &snapshot.first_slice_parse_error {
        return Err(format!(
            "Vulkanalia H.265 streaming AU {} first slice parse failed: {err}",
            snapshot.index
        ));
    }
    let first_slice = snapshot.first_slice.take().ok_or_else(|| {
        format!(
            "Vulkanalia H.265 streaming AU {} has no parsed first slice",
            snapshot.index
        )
    })?;
    let slice_segment_offset = first_slice.slice_segment_offset;
    Ok(NativeVulkanVulkanaliaH265ReadyPrefixFrameInput {
        entry,
        first_slice,
        pts_ns,
        duration_ns: snapshot.duration_ns,
        duration_ms: snapshot.duration_ms,
        access_unit_payload: packet.access_unit.payload,
        slice_segment_offset,
    })
}
