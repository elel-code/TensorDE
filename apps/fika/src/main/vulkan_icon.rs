use std::collections::{BTreeMap, HashMap};

use vulkan_renderer::{
    AccessKind, BlendState, Buffer, ColorAttachment, ColorTargetState, CompiledGraph,
    DescriptorHeap, DescriptorHeapDescriptor, DescriptorHeapKind, Device, DmaBufImageDescriptor,
    DmaBufPlaneLayout, DynamicBuffer, DynamicBufferDescriptor, FragmentState, FrameToken,
    GraphicsPipeline, GraphicsPipelineDescriptor, Image, ImageBlit, ImageBlitFilter,
    ImageDataLayout, ImageDescriptor, ImageUpload, ImageView, ImageViewDescriptor,
    ImportedDmaBufImage, LoadOp, MemoryAllocator, MemoryLocation, MultisampleState, PassId,
    PipelineCache, PrimitiveState, ProgrammableStage, RenderGraph, RenderPass, RenderingDescriptor,
    RenderingEncoder, ResourceBinding, ResourceId, ResourceKind, ResourceState, ResourceUse,
    SampledImageBinding, SamplerBinding, SamplerDescriptor, ShaderBindingMap,
    ShaderModuleDescriptor, StoreOp, TexelBlockLayout, UploadBatch, VertexAttribute,
    VertexBufferLayout, VertexState, VertexStepMode, vk,
};

use crate::{
    IconFrame, IconGpuResidentEntry, IconGpuResidentIndex, IconGpuSlot, IconGpuSource,
    IconGpuUploadKey, IconSlotBatch, IconVertex, LoadedIconSource, is_svg_path,
};

use super::vulkan_icon_spirv;

#[path = "vulkan_icon/bitmap.rs"]
mod bitmap;
#[path = "vulkan_icon/dmabuf.rs"]
mod dmabuf;
#[path = "vulkan_icon/folder_preview.rs"]
mod folder_preview;
#[path = "vulkan_icon/pipeline.rs"]
mod pipeline;
#[path = "vulkan_icon/resources.rs"]
mod resources;
#[path = "vulkan_icon/svg.rs"]
mod svg;
use bitmap::{
    DecodedBitmap, SCALE_BLIT, SCALE_SAMPLE, SCALE_SOURCE, SCALE_SOURCE_UPLOAD, SCALE_TARGET,
    SCALE_TARGET_CLEAR, compile_scale_graph, decode_bitmap, fit_bitmap_blit,
};
use dmabuf::compile_import_graph;
use pipeline::create_pipeline;
use resources::{
    VulkanIconImage, color_layers, compile_upload_graph, create_rgba_image, descriptor_capacity,
    upload_rgba_pixels,
};

/// Byte offset of the `[image_index, sampler_index]` pair in push data.
const IMAGE_PUSH_OFFSET: u32 = 0;
const MAX_RESIDENT_TEXTURES: usize = 512;
const MAX_RETIRED_GENERATIONS: u64 = 4;
const INITIAL_VERTEX_CAPACITY: u64 = std::mem::size_of::<IconVertex>() as u64 * 6;
const ICON_IMAGE: ResourceId = ResourceId(1);
const ICON_UPLOAD: PassId = PassId(1);
const ICON_SAMPLE: PassId = PassId(2);
const ICON_IMPORT_SAMPLE: PassId = PassId(3);

struct VulkanIconTexture {
    image: VulkanIconImage,
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
    import_graph: CompiledGraph,
    scale_graph: CompiledGraph,
    svg: svg::SvgIconRasterizer,
    preview: folder_preview::FolderPreviewCompositor,
    textures: HashMap<IconGpuUploadKey, VulkanIconTexture>,
    unsupported_sources: HashMap<IconGpuUploadKey, (u64, u32, u32)>,
    /// Child-thumbnail descriptors consumed by the frame being recorded;
    /// retired against that frame's token at the next upload.
    transient_bindings: Vec<SampledImageBinding>,
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
        let queue_family = device.device_info().queues.graphics;
        Ok(Self {
            pipeline: create_pipeline(device, pipeline_cache, format)?,
            vertices,
            resource_heap,
            sampler_heap,
            sampler,
            upload_graph: compile_upload_graph(queue_family)?,
            import_graph: compile_import_graph(queue_family)?,
            scale_graph: compile_scale_graph(queue_family)?,
            svg: svg::SvgIconRasterizer::new(device, pipeline_cache, queue_family)?,
            preview: folder_preview::FolderPreviewCompositor::new(
                device,
                pipeline_cache,
                queue_family,
            )?,
            textures: HashMap::new(),
            unsupported_sources: HashMap::new(),
            transient_bindings: Vec::new(),
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
        device: &Device,
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        frame: &mut IconFrame,
        last_submission: Option<FrameToken>,
    ) -> Result<bool, String> {
        self.gpu_frame = self.gpu_frame.wrapping_add(1);
        for binding in std::mem::take(&mut self.transient_bindings) {
            match last_submission {
                Some(frame) => binding.retire(&self.resource_heap, frame),
                None => binding.release(&self.resource_heap),
            }
            .map_err(|error| format!("retire Vulkan preview child descriptor: {error}"))?;
        }
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
            let texture = if slot.source.is_none() && slot.dmabuf.is_some() {
                self.import_dmabuf_texture(device, uploads, slot)?
            } else if folder_preview::is_folder_preview_source(slot) {
                self.create_folder_preview_texture(allocator, uploads, slot)?
            } else if svg::is_svg_source(slot) {
                self.svg.create_texture(
                    &self.resource_heap,
                    allocator,
                    uploads,
                    slot,
                    self.gpu_frame,
                )?
            } else {
                decode_bitmap(slot)
                    .map(|bitmap| self.create_texture(allocator, uploads, slot, &bitmap))
                    .transpose()?
            };
            let Some(texture) = texture else {
                self.unsupported_sources
                    .insert(key, (slot.content_hash, slot.width, slot.height));
                continue;
            };
            self.unsupported_sources.remove(&key);
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

    /// Composites a `FolderPreview` source into one resident texture.
    ///
    /// Mirrors the wgpu `render_folder_preview_gpu` path: each child icon is
    /// rendered at its destination size, then drawn three times (soft shadow,
    /// white frame, content) with a per-child rotation.
    fn create_folder_preview_texture(
        &mut self,
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        slot: &IconGpuSlot,
    ) -> Result<Option<VulkanIconTexture>, String> {
        use crate::ui::folder_preview::{
            FileManagerDirectoryPreviewLayout, folder_preview_thumbnail_angle,
            folder_preview_thumbnail_slots,
        };
        let Some(IconGpuSource::FolderPreview { children, seed, .. }) = slot.source.as_ref() else {
            return Ok(None);
        };
        let side = slot.width.max(1);
        let Some(layout) = FileManagerDirectoryPreviewLayout::new(side) else {
            return Ok(None);
        };
        let seed = *seed;
        let sampler_index = self
            .sampler
            .shader_heap_index()
            .map_err(|error| format!("resolve shared Vulkan icon sampler: {error}"))?;
        let tiles = folder_preview_thumbnail_slots(children.len(), layout);
        let canvas = [side as f32, side as f32];
        let mut draws = Vec::with_capacity(children.len().saturating_mul(3));
        let mut child_views = Vec::new();
        let mut child_bindings = Vec::new();
        for (index, (path, tile)) in children.iter().zip(tiles).enumerate() {
            let Some(child) = LoadedIconSource::load(path) else {
                continue;
            };
            let intrinsic = child.intrinsic_size();
            let border = layout.border_stroke_width.max(1) as f32;
            let available_width = (tile.width as f32 - border * 2.0).max(1.0);
            let available_height = (tile.height as f32 - border * 2.0).max(1.0);
            let scale = (available_width / intrinsic.width)
                .min(available_height / intrinsic.height)
                .min(1.0);
            let width = (intrinsic.width * scale).max(1.0);
            let height = (intrinsic.height * scale).max(1.0);
            let center_x = tile.x as f32 + tile.width as f32 * 0.5;
            let center_y = tile.y as f32 + tile.height as f32 * 0.5;
            let destination = [
                center_x - width * 0.5,
                center_y - height * 0.5,
                width,
                height,
            ];
            let source_width = width.ceil().max(1.0) as u32;
            let source_height = height.ceil().max(1.0) as u32;
            let rendered = match child {
                LoadedIconSource::Svg { bytes, .. } => {
                    self.svg
                        .rasterize(allocator, uploads, &bytes, source_width, source_height)?
                }
                LoadedIconSource::Bitmap {
                    width,
                    height,
                    pixels,
                } => Some(self.render_bitmap_child(
                    allocator,
                    uploads,
                    DecodedBitmap {
                        width,
                        height,
                        pixels,
                    },
                    source_width,
                    source_height,
                )?),
            };
            // The child image handle can drop here: the encoder retains the
            // view (and through it the image) until the submission retires.
            let Some((_image, view)) = rendered else {
                continue;
            };
            let binding = SampledImageBinding::new(
                &self.resource_heap,
                &view,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            )
            .map_err(|error| format!("create Vulkan preview child descriptor: {error}"))?;
            let image_index = binding
                .shader_heap_index()
                .map_err(|error| format!("resolve Vulkan preview child descriptor: {error}"))?;
            let angle = (folder_preview_thumbnail_angle(seed, index) as f32).to_radians();
            let shadow_inset = border * 2.0;
            let content = folder_preview::PreviewCompositeParams {
                image_index,
                sampler_index,
                radius: 0.0,
                mode: 0.0,
                rect: destination,
                color: [1.0; 4],
                canvas,
                angle,
                inset: 0.0,
                blur: 0.0,
                opacity: 0.0,
                _pad: [0.0; 2],
            };
            draws.push(folder_preview::PreviewCompositeParams {
                mode: 2.0,
                rect: [
                    destination[0] - shadow_inset + border * 0.5,
                    destination[1] - shadow_inset + border * 0.5,
                    destination[2] + shadow_inset * 2.0,
                    destination[3] + shadow_inset * 2.0,
                ],
                color: [0.0, 0.0, 0.0, 1.0],
                inset: shadow_inset,
                blur: border.max(1.0),
                opacity: 0.45,
                ..content
            });
            draws.push(folder_preview::PreviewCompositeParams {
                mode: 1.0,
                rect: [
                    destination[0] - border,
                    destination[1] - border,
                    destination[2] + border * 2.0,
                    destination[3] + border * 2.0,
                ],
                ..content
            });
            draws.push(content);
            child_views.push(view);
            child_bindings.push(binding);
        }
        let image = create_rgba_image(
            allocator,
            "fika-vulkan-folder-preview",
            side,
            side,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        )?;
        let view = image
            .create_view(&ImageViewDescriptor {
                label: Some("fika-vulkan-folder-preview-view".into()),
                view_type: vk::ImageViewType::_2D,
                format: image.format(),
                components: vk::ComponentMapping::default(),
                subresource_range: image.full_subresource_range(vk::ImageAspectFlags::COLOR),
            })
            .map_err(|error| format!("create Vulkan folder-preview view: {error}"))?;
        self.preview.composite(
            uploads,
            &self.resource_heap,
            &self.sampler_heap,
            &image,
            &view,
            side,
            &draws,
            &child_views,
        )?;
        self.transient_bindings.extend(child_bindings);
        let binding = SampledImageBinding::new(
            &self.resource_heap,
            &view,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        )
        .map_err(|error| format!("create Vulkan folder-preview descriptor: {error}"))?;
        Ok(Some(VulkanIconTexture {
            image: VulkanIconImage::Resident {
                _image: image,
                view,
            },
            binding,
            width: side,
            height: side,
            content_width: slot.content_width,
            content_height: slot.content_height,
            content_hash: slot.content_hash,
            rounding: slot.rounding,
            last_used_frame: self.gpu_frame,
        }))
    }

    fn render_bitmap_child(
        &self,
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        bitmap: DecodedBitmap,
        width: u32,
        height: u32,
    ) -> Result<(Image, ImageView), String> {
        let image = create_rgba_image(
            allocator,
            "fika-vulkan-preview-child",
            width,
            height,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        )?;
        let view = image
            .create_view(&ImageViewDescriptor {
                label: Some("fika-vulkan-preview-child-view".into()),
                view_type: vk::ImageViewType::_2D,
                format: image.format(),
                components: vk::ComponentMapping::default(),
                subresource_range: image.full_subresource_range(vk::ImageAspectFlags::COLOR),
            })
            .map_err(|error| format!("create Vulkan preview child view: {error}"))?;
        if bitmap.width == width && bitmap.height == height {
            self.upload_exact_bitmap(uploads, &image, &bitmap.pixels)?;
        } else {
            self.scale_bitmap(allocator, uploads, &image, &bitmap)?;
        }
        Ok((image, view))
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
            image: VulkanIconImage::Resident {
                _image: image,
                view,
            },
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
        let sampler_index = self
            .sampler
            .shader_heap_index()
            .map_err(|error| format!("resolve shared Vulkan icon sampler: {error}"))?;
        for batch in batches {
            let Some(key) = self.frame_slot_keys.get(batch.slot as usize) else {
                continue;
            };
            let Some(texture) = self.textures.get(key) else {
                continue;
            };
            let image_index = texture
                .binding
                .shader_heap_index()
                .map_err(|error| format!("resolve Vulkan icon descriptor: {error}"))?;
            let push = [image_index, sampler_index];
            rendering
                .push_data(IMAGE_PUSH_OFFSET, bytemuck::cast_slice(&push))
                .map_err(|error| format!("push Vulkan icon descriptor indices: {error}"))?;
            texture.image.retain(rendering);
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
