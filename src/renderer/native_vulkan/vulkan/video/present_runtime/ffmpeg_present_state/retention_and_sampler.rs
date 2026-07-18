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
    fn drop(&mut self) {
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
    fn drop(&mut self) {
        for entry in self.entries.drain(..) {
            native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
                self.device,
                entry.sampler,
            );
        }
    }
}
