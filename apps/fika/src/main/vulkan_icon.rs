use std::collections::{BTreeMap, HashMap};

use vulkan_renderer::{
    AccessKind, BlendState, Buffer, ColorTargetState, CompiledGraph, DescriptorHeap,
    DescriptorHeapDescriptor, DescriptorHeapKind, Device, DynamicBuffer, DynamicBufferDescriptor,
    FragmentState, FrameToken, GraphicsPipeline, GraphicsPipelineDescriptor, Image, ImageBlit,
    ImageBlitFilter, ImageDataLayout, ImageDescriptor, ImageUpload, ImageView, ImageViewDescriptor,
    MemoryAllocator, MemoryLocation, MultisampleState, PassId, PipelineCache, PrimitiveState,
    ProgrammableStage, RenderGraph, RenderPass, RenderingEncoder, ResourceBinding, ResourceId,
    ResourceKind, ResourceState, ResourceUse, SampledImageBinding, SampledTextureHeapOffsets,
    SampledTextureShaderBindings, SamplerBinding, SamplerDescriptor, ShaderBindingMap,
    ShaderModuleDescriptor, TexelBlockLayout, UploadBatch, VertexAttribute, VertexBufferLayout,
    VertexState, VertexStepMode, vk,
};

use crate::{
    IconFrame, IconGpuResidentEntry, IconGpuResidentIndex, IconGpuSlot, IconGpuSource,
    IconGpuUploadKey, IconSlotBatch, IconVertex, LoadedIconSource, is_svg_path,
};

use super::vulkan_icon_spirv;

#[path = "vulkan_icon/pipeline.rs"]
mod pipeline;
use pipeline::create_pipeline;

const IMAGE_PUSH_OFFSET: u32 = 0;
const SAMPLER_PUSH_OFFSET: u32 = 4;
const MAX_RESIDENT_TEXTURES: usize = 512;
const MAX_RETIRED_GENERATIONS: u64 = 4;
const INITIAL_VERTEX_CAPACITY: u64 = std::mem::size_of::<IconVertex>() as u64 * 6;
const ICON_IMAGE: ResourceId = ResourceId(1);
const ICON_UPLOAD: PassId = PassId(1);
const ICON_SAMPLE: PassId = PassId(2);
const SCALE_SOURCE: ResourceId = ResourceId(2);
const SCALE_TARGET: ResourceId = ResourceId(3);
const SCALE_SOURCE_UPLOAD: PassId = PassId(10);
const SCALE_TARGET_CLEAR: PassId = PassId(11);
const SCALE_BLIT: PassId = PassId(12);
const SCALE_SAMPLE: PassId = PassId(13);

struct DecodedBitmap {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

struct VulkanIconTexture {
    _image: Image,
    view: ImageView,
    binding: SampledImageBinding,
    width: u32,
    height: u32,
    content_width: u32,
    content_height: u32,
    content_hash: u64,
    rounding: Option<crate::IconRounding>,
    last_used_frame: u64,
}

/// Native sampled-image renderer for the shared retained [`IconFrame`].
///
/// Resident textures use one image descriptor each and share one immutable
/// linear-clamp sampler descriptor. Pipeline state is independent of resident
/// heap slots; each batch pushes only its image offset before drawing.
pub(crate) struct VulkanIconRenderer {
    pipeline: GraphicsPipeline,
    vertices: DynamicBuffer,
    resource_heap: DescriptorHeap,
    sampler_heap: DescriptorHeap,
    sampler: SamplerBinding,
    upload_graph: CompiledGraph,
    scale_graph: CompiledGraph,
    textures: HashMap<IconGpuUploadKey, VulkanIconTexture>,
    unsupported_sources: HashMap<IconGpuUploadKey, (u64, u32, u32)>,
    frame_slot_keys: Vec<IconGpuUploadKey>,
    content_batches: Vec<IconSlotBatch>,
    overlay_batches: Vec<IconSlotBatch>,
    content_vertex_count: usize,
    overlay_vertex_start: usize,
    overlay_vertex_count: usize,
    gpu_frame: u64,
}

impl VulkanIconRenderer {
    pub(crate) fn new(
        device: &Device,
        allocator: &MemoryAllocator,
        pipeline_cache: &PipelineCache,
        format: vk::Format,
    ) -> Result<Self, String> {
        let limits = device.device_info().limits.descriptor_heap;
        let resource_heap = device
            .create_descriptor_heap(&DescriptorHeapDescriptor {
                label: Some("fika-vulkan-icon-resource-heap".into()),
                kind: DescriptorHeapKind::Resource,
                descriptor_capacity: descriptor_capacity(
                    limits.image_descriptor_size,
                    limits.image_descriptor_alignment,
                    MAX_RESIDENT_TEXTURES as u64 * MAX_RETIRED_GENERATIONS,
                )?,
                embedded_samplers: false,
            })
            .map_err(|error| format!("create Vulkan icon resource heap: {error}"))?;
        let sampler_heap = device
            .create_descriptor_heap(&DescriptorHeapDescriptor {
                label: Some("fika-vulkan-icon-sampler-heap".into()),
                kind: DescriptorHeapKind::Sampler,
                descriptor_capacity: descriptor_capacity(
                    limits.sampler_descriptor_size,
                    limits.sampler_descriptor_alignment,
                    1,
                )?,
                embedded_samplers: false,
            })
            .map_err(|error| format!("create Vulkan icon sampler heap: {error}"))?;
        let sampler = SamplerBinding::new(&sampler_heap, SamplerDescriptor::linear_clamp())
            .map_err(|error| format!("create shared Vulkan icon sampler: {error}"))?;
        let vertices = DynamicBuffer::new(
            allocator,
            DynamicBufferDescriptor {
                label: Some("fika-vulkan-icon-vertices".into()),
                initial_capacity: INITIAL_VERTEX_CAPACITY,
                usage: vk::BufferUsageFlags::VERTEX_BUFFER,
            },
        )
        .map_err(|error| format!("create Vulkan icon vertex buffer: {error}"))?;
        Ok(Self {
            pipeline: create_pipeline(device, pipeline_cache, format)?,
            vertices,
            resource_heap,
            sampler_heap,
            sampler,
            upload_graph: compile_upload_graph(device.device_info().queues.graphics)?,
            scale_graph: compile_scale_graph(device.device_info().queues.graphics)?,
            textures: HashMap::new(),
            unsupported_sources: HashMap::new(),
            frame_slot_keys: Vec::new(),
            content_batches: Vec::new(),
            overlay_batches: Vec::new(),
            content_vertex_count: 0,
            overlay_vertex_start: 0,
            overlay_vertex_count: 0,
            gpu_frame: 0,
        })
    }

    pub(crate) fn set_format(
        &mut self,
        device: &Device,
        pipeline_cache: &PipelineCache,
        format: vk::Format,
    ) -> Result<(), String> {
        if self.pipeline.color_formats() != [format] {
            self.pipeline = create_pipeline(device, pipeline_cache, format)?;
        }
        Ok(())
    }

    pub(crate) fn reclaim(&self, completed_timeline: u64) {
        self.resource_heap.reclaim(completed_timeline);
    }

    pub(crate) fn resident_index(&self) -> IconGpuResidentIndex {
        let entries = self
            .textures
            .iter()
            .map(|(key, texture)| {
                (
                    key.clone(),
                    IconGpuResidentEntry {
                        width: texture.width,
                        height: texture.height,
                        content_width: texture.content_width,
                        content_height: texture.content_height,
                        content_hash: texture.content_hash,
                        rounding: texture.rounding,
                    },
                )
            })
            .collect();
        IconGpuResidentIndex { entries }
    }

    pub(crate) fn upload(
        &mut self,
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        frame: &mut IconFrame,
        last_submission: Option<FrameToken>,
    ) -> Result<bool, String> {
        self.gpu_frame = self.gpu_frame.wrapping_add(1);
        self.frame_slot_keys.clear();
        self.frame_slot_keys.reserve(frame.slots.len());
        let mut uploaded_textures = 0usize;
        let mut upload_skips = 0usize;

        for slot in &mut frame.slots {
            let key = IconGpuUploadKey::from_slot(slot);
            self.frame_slot_keys.push(key.clone());
            if self.texture_matches(&key, slot) {
                self.touch_texture(&key, slot);
                slot.source = None;
                let _ = slot.dmabuf.take();
                upload_skips += 1;
                continue;
            }
            if self.unsupported_sources.get(&key)
                == Some(&(slot.content_hash, slot.width, slot.height))
            {
                continue;
            }
            let Some(bitmap) = decode_bitmap(slot) else {
                self.unsupported_sources
                    .insert(key, (slot.content_hash, slot.width, slot.height));
                continue;
            };
            self.unsupported_sources.remove(&key);
            let texture = self.create_texture(allocator, uploads, slot, &bitmap)?;
            if let Some(previous) = self.textures.insert(key, texture) {
                self.retire_texture(previous, last_submission)?;
            }
            slot.source = None;
            uploaded_textures += 1;
        }
        frame.stats.atlas_uploads = uploaded_textures;
        frame.stats.atlas_upload_skips = upload_skips;

        self.content_batches = std::mem::take(&mut frame.content_batches);
        self.overlay_batches = std::mem::take(&mut frame.overlay_batches);
        self.content_vertex_count = frame.content_vertices.len();
        self.overlay_vertex_start = self.content_vertex_count;
        self.overlay_vertex_count = frame.overlay_vertices.len();
        let mut vertices = Vec::with_capacity(
            self.content_vertex_count
                .saturating_add(self.overlay_vertex_count),
        );
        vertices.extend_from_slice(&frame.content_vertices);
        vertices.extend_from_slice(&frame.overlay_vertices);
        let vertex_upload = self
            .vertices
            .upload(uploads, bytemuck::cast_slice(&vertices))
            .map_err(|error| format!("upload Vulkan icon vertices: {error}"))?;
        self.evict_unused(last_submission)?;
        if self.unsupported_sources.len() > MAX_RESIDENT_TEXTURES * 4 {
            self.unsupported_sources.clear();
        }
        Ok(vertex_upload.bytes_written != 0)
    }

    pub(crate) fn vertex_buffer(&self) -> Option<&Buffer> {
        (!self.vertices.is_empty()).then_some(self.vertices.buffer())
    }

    pub(crate) fn draw_content(&self, rendering: &mut RenderingEncoder<'_>) -> Result<(), String> {
        self.draw_batches(rendering, &self.content_batches, 0)
    }

    pub(crate) fn draw_overlay(&self, rendering: &mut RenderingEncoder<'_>) -> Result<(), String> {
        self.draw_batches(
            rendering,
            &self.overlay_batches,
            self.overlay_vertex_start as u32,
        )
    }

    fn texture_matches(&self, key: &IconGpuUploadKey, slot: &IconGpuSlot) -> bool {
        self.textures.get(key).is_some_and(|texture| {
            texture.content_hash == slot.content_hash
                && texture.width == slot.width
                && texture.height == slot.height
        })
    }

    fn touch_texture(&mut self, key: &IconGpuUploadKey, slot: &mut IconGpuSlot) {
        if let Some(texture) = self.textures.get_mut(key) {
            texture.last_used_frame = self.gpu_frame;
            slot.width = texture.width;
            slot.height = texture.height;
            slot.content_width = texture.content_width;
            slot.content_height = texture.content_height;
            slot.rounding = texture.rounding;
        }
    }

    fn create_texture(
        &self,
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        slot: &IconGpuSlot,
        bitmap: &DecodedBitmap,
    ) -> Result<VulkanIconTexture, String> {
        let width = slot.width.max(1);
        let height = slot.height.max(1);
        let image = create_rgba_image(
            allocator,
            "fika-vulkan-resident-icon",
            width,
            height,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        )?;
        let view = image
            .create_view(&ImageViewDescriptor {
                label: Some("fika-vulkan-resident-icon-view".into()),
                view_type: vk::ImageViewType::_2D,
                format: image.format(),
                components: vk::ComponentMapping::default(),
                subresource_range: image.full_subresource_range(vk::ImageAspectFlags::COLOR),
            })
            .map_err(|error| format!("create Vulkan resident icon view: {error}"))?;
        if bitmap.width == width && bitmap.height == height {
            self.upload_exact_bitmap(uploads, &image, &bitmap.pixels)?;
        } else {
            self.scale_bitmap(allocator, uploads, &image, bitmap)?;
        }
        let binding = SampledImageBinding::new(
            &self.resource_heap,
            &view,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        )
        .map_err(|error| format!("create Vulkan resident icon descriptor: {error}"))?;
        Ok(VulkanIconTexture {
            _image: image,
            view,
            binding,
            width,
            height,
            content_width: slot.content_width,
            content_height: slot.content_height,
            content_hash: slot.content_hash,
            rounding: slot.rounding,
            last_used_frame: self.gpu_frame,
        })
    }

    fn upload_exact_bitmap(
        &self,
        uploads: &mut UploadBatch<'_>,
        image: &Image,
        pixels: &[u8],
    ) -> Result<(), String> {
        let (before_upload, before_sample) = self.upload_barriers(image)?;
        unsafe { uploads.encoder_mut().pipeline_barrier(&before_upload) };
        upload_rgba_pixels(uploads, image, pixels)?;
        unsafe { uploads.encoder_mut().pipeline_barrier(&before_sample) };
        Ok(())
    }

    fn scale_bitmap(
        &self,
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        target: &Image,
        bitmap: &DecodedBitmap,
    ) -> Result<(), String> {
        let source = create_rgba_image(
            allocator,
            "fika-vulkan-icon-scale-source",
            bitmap.width,
            bitmap.height,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC,
        )?;
        let bindings = BTreeMap::from([
            (
                SCALE_SOURCE,
                ResourceBinding::Image {
                    image: source.raw(),
                    subresource_range: source.full_subresource_range(vk::ImageAspectFlags::COLOR),
                },
            ),
            (
                SCALE_TARGET,
                ResourceBinding::Image {
                    image: target.raw(),
                    subresource_range: target.full_subresource_range(vk::ImageAspectFlags::COLOR),
                },
            ),
        ]);
        let barrier = |pass, label: &str| {
            self.scale_graph
                .barrier_batch_before(pass, &bindings)
                .map_err(|error| format!("resolve Vulkan icon {label} barrier: {error}"))
        };
        let before_source_upload = barrier(SCALE_SOURCE_UPLOAD, "source upload")?;
        let before_target_clear = barrier(SCALE_TARGET_CLEAR, "target clear")?;
        let before_blit = barrier(SCALE_BLIT, "scale blit")?;
        let before_sample = barrier(SCALE_SAMPLE, "scale sample")?;
        unsafe {
            uploads
                .encoder_mut()
                .pipeline_barrier(&before_source_upload)
        };
        upload_rgba_pixels(uploads, &source, &bitmap.pixels)?;
        unsafe {
            uploads.encoder_mut().pipeline_barrier(&before_target_clear);
            uploads
                .encoder_mut()
                .clear_color_image(
                    target,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    [0.0; 4],
                    &[target.full_subresource_range(vk::ImageAspectFlags::COLOR)],
                )
                .map_err(|error| format!("clear Vulkan scaled icon target: {error}"))?;
            uploads.encoder_mut().pipeline_barrier(&before_blit);
            uploads
                .encoder_mut()
                .blit_image(
                    &source,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    target,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[fit_bitmap_blit(source.extent(), target.extent())],
                    ImageBlitFilter::Linear,
                )
                .map_err(|error| format!("scale Vulkan resident icon: {error}"))?;
            uploads.encoder_mut().pipeline_barrier(&before_sample);
        }
        Ok(())
    }

    fn upload_barriers(
        &self,
        image: &Image,
    ) -> Result<(vulkan_renderer::BarrierBatch, vulkan_renderer::BarrierBatch), String> {
        let bindings = BTreeMap::from([(
            ICON_IMAGE,
            ResourceBinding::Image {
                image: image.raw(),
                subresource_range: image.full_subresource_range(vk::ImageAspectFlags::COLOR),
            },
        )]);
        Ok((
            self.upload_graph
                .barrier_batch_before(ICON_UPLOAD, &bindings)
                .map_err(|error| format!("resolve Vulkan icon upload barrier: {error}"))?,
            self.upload_graph
                .barrier_batch_before(ICON_SAMPLE, &bindings)
                .map_err(|error| format!("resolve Vulkan icon sample barrier: {error}"))?,
        ))
    }

    fn draw_batches(
        &self,
        rendering: &mut RenderingEncoder<'_>,
        batches: &[IconSlotBatch],
        vertex_base: u32,
    ) -> Result<(), String> {
        if batches.is_empty() {
            return Ok(());
        }
        rendering
            .bind_pipeline(&self.pipeline)
            .map_err(|error| format!("bind Vulkan icon pipeline: {error}"))?;
        unsafe {
            rendering
                .bind_descriptor_heap(&self.resource_heap)
                .map_err(|error| format!("bind Vulkan icon resource heap: {error}"))?;
            rendering
                .bind_descriptor_heap(&self.sampler_heap)
                .map_err(|error| format!("bind Vulkan icon sampler heap: {error}"))?;
            rendering
                .set_vertex_buffer(0, self.vertices.buffer(), 0)
                .map_err(|error| format!("bind Vulkan icon vertex buffer: {error}"))?;
        }
        let sampler_offset = self
            .sampler
            .push_index_heap_offset()
            .map_err(|error| format!("resolve shared Vulkan icon sampler: {error}"))?;
        for batch in batches {
            let Some(key) = self.frame_slot_keys.get(batch.slot as usize) else {
                continue;
            };
            let Some(texture) = self.textures.get(key) else {
                continue;
            };
            let offsets = SampledTextureHeapOffsets {
                image: texture
                    .binding
                    .push_index_heap_offset()
                    .map_err(|error| format!("resolve Vulkan icon descriptor: {error}"))?,
                sampler: sampler_offset,
            };
            let push = [offsets.image, offsets.sampler];
            rendering
                .push_data(IMAGE_PUSH_OFFSET, bytemuck::cast_slice(&push))
                .map_err(|error| format!("push Vulkan icon descriptor offsets: {error}"))?;
            rendering.retain_resource(&texture.view);
            let start = vertex_base.saturating_add(batch.vertex_start);
            let end = start.saturating_add(batch.vertex_count);
            unsafe {
                rendering
                    .draw(start..end, 0..1)
                    .map_err(|error| format!("draw Vulkan resident icon: {error}"))?;
            }
        }
        Ok(())
    }

    fn evict_unused(&mut self, last_submission: Option<FrameToken>) -> Result<(), String> {
        while self.textures.len() > MAX_RESIDENT_TEXTURES {
            let Some(victim) = self
                .textures
                .iter()
                .filter(|(_, texture)| texture.last_used_frame != self.gpu_frame)
                .min_by_key(|(_, texture)| texture.last_used_frame)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(texture) = self.textures.remove(&victim) {
                self.retire_texture(texture, last_submission)?;
            }
        }
        Ok(())
    }

    fn retire_texture(
        &self,
        texture: VulkanIconTexture,
        last_submission: Option<FrameToken>,
    ) -> Result<(), String> {
        match last_submission {
            Some(frame) => texture.binding.retire(&self.resource_heap, frame),
            None => texture.binding.release(&self.resource_heap),
        }
        .map_err(|error| format!("retire Vulkan resident icon descriptor: {error}"))
    }
}

fn decode_bitmap(slot: &IconGpuSlot) -> Option<DecodedBitmap> {
    let IconGpuSource::File { path, .. } = slot.source.as_ref()? else {
        return None;
    };
    if is_svg_path(path) {
        return None;
    }
    match LoadedIconSource::load(path)? {
        LoadedIconSource::Bitmap {
            width,
            height,
            pixels,
        } => Some(DecodedBitmap {
            width,
            height,
            pixels,
        }),
        LoadedIconSource::Svg { .. } => None,
    }
}

fn create_rgba_image(
    allocator: &MemoryAllocator,
    label: &str,
    width: u32,
    height: u32,
    usage: vk::ImageUsageFlags,
) -> Result<Image, String> {
    allocator
        .create_image(&ImageDescriptor {
            label: Some(label.into()),
            image_type: vk::ImageType::_2D,
            format: vk::Format::R8G8B8A8_UNORM,
            extent: vk::Extent3D {
                width: width.max(1),
                height: height.max(1),
                depth: 1,
            },
            mip_levels: 1,
            array_layers: 1,
            samples: vk::SampleCountFlags::_1,
            tiling: vk::ImageTiling::OPTIMAL,
            usage,
            memory: MemoryLocation::Device,
        })
        .map_err(|error| format!("create Vulkan RGBA image {label}: {error}"))
}

fn upload_rgba_pixels(
    uploads: &mut UploadBatch<'_>,
    image: &Image,
    pixels: &[u8],
) -> Result<(), String> {
    let extent = image.extent();
    unsafe {
        uploads
            .write_image_data(
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                ImageUpload {
                    data_layout: ImageDataLayout::tightly_packed(extent, TexelBlockLayout::RGBA8)
                        .map_err(|error| format!("layout Vulkan RGBA upload: {error}"))?,
                    texel_block: TexelBlockLayout::RGBA8,
                    image_subresource: color_layers(),
                    image_offset: vk::Offset3D::default(),
                    image_extent: extent,
                },
                pixels,
            )
            .map_err(|error| format!("upload Vulkan RGBA image: {error}"))?;
    }
    Ok(())
}

fn fit_bitmap_blit(source: vk::Extent3D, target: vk::Extent3D) -> ImageBlit {
    let scale = (target.width as f64 / source.width as f64)
        .min(target.height as f64 / source.height as f64);
    let width = (source.width as f64 * scale)
        .round()
        .clamp(1.0, target.width as f64) as i32;
    let height = (source.height as f64 * scale)
        .round()
        .clamp(1.0, target.height as f64) as i32;
    let x = (target.width as i32 - width) / 2;
    let y = (target.height as i32 - height) / 2;
    ImageBlit {
        source_subresource: color_layers(),
        source_offsets: [
            vk::Offset3D::default(),
            vk::Offset3D {
                x: source.width as i32,
                y: source.height as i32,
                z: 1,
            },
        ],
        destination_subresource: color_layers(),
        destination_offsets: [
            vk::Offset3D { x, y, z: 0 },
            vk::Offset3D {
                x: x + width,
                y: y + height,
                z: 1,
            },
        ],
    }
}

const fn color_layers() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        mip_level: 0,
        base_array_layer: 0,
        layer_count: 1,
    }
}

fn descriptor_capacity(size: u64, alignment: u64, slots: u64) -> Result<u64, String> {
    if size == 0 || !alignment.is_power_of_two() || slots == 0 {
        return Err("Vulkan icon descriptor layout is unusable".into());
    }
    size.checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .and_then(|stride| stride.checked_mul(slots))
        .ok_or_else(|| "Vulkan icon descriptor capacity overflows".into())
}

fn compile_upload_graph(queue_family: u32) -> Result<CompiledGraph, String> {
    let mut graph = RenderGraph::default();
    graph.set_initial_state(
        ICON_IMAGE,
        ResourceKind::Image,
        ResourceState::image(
            vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE,
            vk::ImageLayout::UNDEFINED,
            queue_family,
        ),
    );
    graph.add_pass(RenderPass {
        id: ICON_UPLOAD,
        label: "fika-vulkan-icon-upload".into(),
        depends_on: Vec::new(),
        resources: vec![ResourceUse {
            resource: ICON_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Write,
            state: ResourceState::image(
                vk::PipelineStageFlags2::COPY,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                queue_family,
            ),
        }],
    });
    graph.add_pass(RenderPass {
        id: ICON_SAMPLE,
        label: "fika-vulkan-icon-sample".into(),
        depends_on: vec![ICON_UPLOAD],
        resources: vec![ResourceUse {
            resource: ICON_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::image(
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                queue_family,
            ),
        }],
    });
    graph
        .compile()
        .map_err(|error| format!("compile Vulkan icon upload graph: {error}"))
}

fn compile_scale_graph(queue_family: u32) -> Result<CompiledGraph, String> {
    let mut graph = RenderGraph::default();
    for resource in [SCALE_SOURCE, SCALE_TARGET] {
        graph.set_initial_state(
            resource,
            ResourceKind::Image,
            ResourceState::image(
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                vk::ImageLayout::UNDEFINED,
                queue_family,
            ),
        );
    }
    graph.add_pass(RenderPass {
        id: SCALE_SOURCE_UPLOAD,
        label: "fika-vulkan-icon-scale-source-upload".into(),
        depends_on: Vec::new(),
        resources: vec![ResourceUse {
            resource: SCALE_SOURCE,
            kind: ResourceKind::Image,
            access: AccessKind::Write,
            state: ResourceState::image(
                vk::PipelineStageFlags2::COPY,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                queue_family,
            ),
        }],
    });
    graph.add_pass(RenderPass {
        id: SCALE_TARGET_CLEAR,
        label: "fika-vulkan-icon-scale-target-clear".into(),
        depends_on: Vec::new(),
        resources: vec![ResourceUse {
            resource: SCALE_TARGET,
            kind: ResourceKind::Image,
            access: AccessKind::Write,
            state: ResourceState::image(
                vk::PipelineStageFlags2::CLEAR,
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                queue_family,
            ),
        }],
    });
    graph.add_pass(RenderPass {
        id: SCALE_BLIT,
        label: "fika-vulkan-icon-scale-blit".into(),
        depends_on: vec![SCALE_SOURCE_UPLOAD, SCALE_TARGET_CLEAR],
        resources: vec![
            ResourceUse {
                resource: SCALE_SOURCE,
                kind: ResourceKind::Image,
                access: AccessKind::Read,
                state: ResourceState::image(
                    vk::PipelineStageFlags2::BLIT,
                    vk::AccessFlags2::TRANSFER_READ,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    queue_family,
                ),
            },
            ResourceUse {
                resource: SCALE_TARGET,
                kind: ResourceKind::Image,
                access: AccessKind::Write,
                state: ResourceState::image(
                    vk::PipelineStageFlags2::BLIT,
                    vk::AccessFlags2::TRANSFER_WRITE,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    queue_family,
                ),
            },
        ],
    });
    graph.add_pass(RenderPass {
        id: SCALE_SAMPLE,
        label: "fika-vulkan-icon-scale-sample".into(),
        depends_on: vec![SCALE_BLIT],
        resources: vec![ResourceUse {
            resource: SCALE_TARGET,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::image(
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                queue_family,
            ),
        }],
    });
    graph
        .compile()
        .map_err(|error| format!("compile Vulkan icon scale graph: {error}"))
}
