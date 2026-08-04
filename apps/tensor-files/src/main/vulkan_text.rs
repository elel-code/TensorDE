use std::collections::{BTreeMap, HashSet};

use vulkan_renderer::{
    AccessKind, BarrierBatch, BlendState, Buffer, BufferUsages, ColorTargetState, CompiledGraph,
    ComponentMapping, DescriptorHeap, DescriptorHeapDescriptor, DescriptorHeapKind, Device,
    DynamicBuffer, DynamicBufferDescriptor, Extent3D, FragmentState, FrameToken, GraphicsPipeline,
    GraphicsPipelineDescriptor, Image, ImageDataLayout, ImageDescriptor, ImageDimension,
    ImageTiling, ImageUpload, ImageView, ImageViewDescriptor, ImageViewDimension, MemoryAllocator,
    MemoryLocation, MultisampleState, Origin3D, PassId, PipelineCache, PrimitiveState,
    ProgrammableStage, RenderGraph, RenderGraphImageState, RenderPass, RenderingEncoder,
    ResourceBinding, ResourceId, ResourceKind, ResourceState, ResourceUse, SampleCount,
    SampledTextureBinding, SamplerDescriptor, ShaderModuleDescriptor, TexelBlockLayout,
    TextureAspects, TextureFormat, TextureLayout, TextureSubresourceLayers, TextureUsages,
    UploadBatch, VertexAttribute, VertexBufferLayout, VertexState, VertexStepMode,
};

use crate::ui::render::texture::TextVertex;
use crate::{TextAtlasUploadKey, TextFrame, text_atlas_upload_should_skip};

use super::vulkan_text_spirv;

const TEXT_IMAGE: ResourceId = ResourceId(1);
const TEXT_UPLOAD: PassId = PassId(1);
const TEXT_SAMPLE: PassId = PassId(2);
/// Byte offset of the `[image_index, sampler_index]` pair in push data.
const IMAGE_PUSH_OFFSET: u32 = 0;
const INITIAL_VERTEX_CAPACITY: u64 = std::mem::size_of::<TextVertex>() as u64 * 6;
const DESCRIPTOR_RING_SLOTS: u64 = 4;

struct VulkanTextAtlas {
    image: Image,
    view: ImageView,
    binding: SampledTextureBinding,
    width: u32,
    height: u32,
    initialized: bool,
}

/// Native R8 glyph-atlas renderer.
///
/// The graphics pipeline uses push-index descriptor mapping, so atlas image
/// replacement changes two pushed descriptor byte offsets without rebuilding
/// the pipeline. Rasterization remains shared with Tensor Files's retained text cache;
/// all compositing and sampling are native Vulkan work.
pub(crate) struct VulkanTextRenderer {
    pipeline: GraphicsPipeline,
    vertices: DynamicBuffer,
    vertex_count: usize,
    resource_heap: DescriptorHeap,
    sampler_heap: DescriptorHeap,
    atlas: Option<VulkanTextAtlas>,
    last_upload_keys: HashSet<TextAtlasUploadKey>,
}

impl VulkanTextRenderer {
    pub(crate) fn new(
        device: &Device,
        allocator: &MemoryAllocator,
        pipeline_cache: &PipelineCache,
        format: TextureFormat,
    ) -> Result<Self, String> {
        let limits = device.device_info().limits.descriptor_heap;
        let resource_heap = device
            .create_descriptor_heap(&DescriptorHeapDescriptor {
                label: Some("tensor-files-vulkan-text-resource-heap".into()),
                kind: DescriptorHeapKind::Resource,
                descriptor_capacity: descriptor_ring_capacity(
                    limits.image_descriptor_size,
                    limits.image_descriptor_alignment,
                )?,
                embedded_samplers: false,
            })
            .map_err(|error| format!("create Vulkan text resource heap: {error}"))?;
        let sampler_heap = device
            .create_descriptor_heap(&DescriptorHeapDescriptor {
                label: Some("tensor-files-vulkan-text-sampler-heap".into()),
                kind: DescriptorHeapKind::Sampler,
                descriptor_capacity: descriptor_ring_capacity(
                    limits.sampler_descriptor_size,
                    limits.sampler_descriptor_alignment,
                )?,
                embedded_samplers: false,
            })
            .map_err(|error| format!("create Vulkan text sampler heap: {error}"))?;
        let vertices = DynamicBuffer::new(
            allocator,
            DynamicBufferDescriptor {
                label: Some("tensor-files-vulkan-text-vertices".into()),
                initial_capacity: INITIAL_VERTEX_CAPACITY,
                usage: BufferUsages::VERTEX,
            },
        )
        .map_err(|error| format!("create Vulkan text vertex buffer: {error}"))?;
        let pipeline = create_pipeline(device, pipeline_cache, format)?;
        Ok(Self {
            pipeline,
            vertices,
            vertex_count: 0,
            resource_heap,
            sampler_heap,
            atlas: None,
            last_upload_keys: HashSet::new(),
        })
    }

    pub(crate) fn set_format(
        &mut self,
        device: &Device,
        pipeline_cache: &PipelineCache,
        format: TextureFormat,
    ) -> Result<(), String> {
        if self.pipeline.color_formats() != [Some(format)] {
            self.pipeline = create_pipeline(device, pipeline_cache, format)?;
        }
        Ok(())
    }

    pub(crate) fn reclaim(&self, completed_timeline: u64) {
        self.resource_heap.reclaim(completed_timeline);
        self.sampler_heap.reclaim(completed_timeline);
    }

    pub(crate) fn upload(
        &mut self,
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        frame: &mut TextFrame,
        last_submission: Option<FrameToken>,
        queue_family: u32,
    ) -> Result<bool, String> {
        self.ensure_atlas(allocator, frame.width, frame.height, last_submission)?;

        self.vertex_count = frame.vertices.len();
        let vertex_upload = self
            .vertices
            .upload(uploads, bytemuck::cast_slice(&frame.vertices))
            .map_err(|error| format!("upload Vulkan text vertices: {error}"))?;

        let mut current_upload_keys = HashSet::with_capacity(frame.uploads.len());
        let mut skipped_uploads = 0usize;
        let pending_uploads = if frame.vertices.is_empty() {
            Vec::new()
        } else {
            frame
                .uploads
                .iter()
                .filter(|upload| {
                    let skip = text_atlas_upload_should_skip(
                        upload,
                        &self.last_upload_keys,
                        &mut current_upload_keys,
                    );
                    skipped_uploads += usize::from(skip);
                    !skip
                })
                .collect::<Vec<_>>()
        };
        self.last_upload_keys = current_upload_keys;
        frame.stats.atlas_uploads = pending_uploads.len();
        frame.stats.atlas_upload_skips = skipped_uploads;

        let atlas = self.atlas.as_mut().expect("text atlas created above");
        if !pending_uploads.is_empty() {
            let (before_upload, before_sample) =
                compile_atlas_barriers(&atlas.image, atlas.initialized, queue_family)?;
            unsafe { uploads.encoder_mut().pipeline_barrier(&before_upload) };
            for upload in pending_uploads {
                let extent = Extent3D::new(upload.width, upload.height, 1);
                let image_upload = ImageUpload {
                    data_layout: ImageDataLayout::tightly_packed(extent, TexelBlockLayout::R8)
                        .map_err(|error| format!("layout Vulkan text upload: {error}"))?,
                    texel_block: TexelBlockLayout::R8,
                    image_subresource: TextureSubresourceLayers::color(0, 0, 1),
                    image_offset: Origin3D::new(upload.atlas.x as i32, upload.atlas.y as i32, 0),
                    image_extent: extent,
                };
                unsafe {
                    uploads
                        .write_image_data(
                            &atlas.image,
                            TextureLayout::TransferDestination,
                            image_upload,
                            upload.pixels.as_ref(),
                        )
                        .map_err(|error| format!("upload Vulkan R8 text atlas: {error}"))?;
                }
            }
            unsafe { uploads.encoder_mut().pipeline_barrier(&before_sample) };
            atlas.initialized = true;
        }
        Ok(vertex_upload.bytes_written != 0)
    }

    pub(crate) fn vertex_buffer(&self) -> Option<&Buffer> {
        (self.vertex_count != 0).then_some(self.vertices.buffer())
    }

    pub(crate) fn draw(&self, rendering: &mut RenderingEncoder<'_>) -> Result<(), String> {
        if self.vertex_count == 0 {
            return Ok(());
        }
        let atlas = self
            .atlas
            .as_ref()
            .ok_or_else(|| "Vulkan text vertices exist without an atlas".to_string())?;
        if !atlas.initialized {
            return Err("Vulkan text atlas was not initialized before draw".into());
        }
        let indices = atlas
            .binding
            .shader_heap_indices()
            .map_err(|error| format!("resolve Vulkan text descriptor indices: {error}"))?;
        rendering
            .bind_pipeline(&self.pipeline)
            .map_err(|error| format!("bind Vulkan text pipeline: {error}"))?;
        unsafe {
            rendering
                .bind_descriptor_heap(&self.resource_heap)
                .map_err(|error| format!("bind Vulkan text resource heap: {error}"))?;
            rendering
                .bind_descriptor_heap(&self.sampler_heap)
                .map_err(|error| format!("bind Vulkan text sampler heap: {error}"))?;
        }
        let push = [indices.image, indices.sampler];
        rendering
            .push_data(IMAGE_PUSH_OFFSET, bytemuck::cast_slice(&push))
            .map_err(|error| format!("push Vulkan text descriptor indices: {error}"))?;
        rendering.retain_resource(&atlas.view);
        unsafe {
            rendering
                .set_vertex_buffer(0, self.vertices.buffer(), 0)
                .map_err(|error| format!("bind Vulkan text vertex buffer: {error}"))?;
            rendering
                .draw(0..self.vertex_count as u32, 0..1)
                .map_err(|error| format!("draw Vulkan text: {error}"))?;
        }
        Ok(())
    }

    fn ensure_atlas(
        &mut self,
        allocator: &MemoryAllocator,
        width: u32,
        height: u32,
        last_submission: Option<FrameToken>,
    ) -> Result<(), String> {
        if self
            .atlas
            .as_ref()
            .is_some_and(|atlas| atlas.width == width && atlas.height == height)
        {
            return Ok(());
        }
        let replacement = create_atlas(
            allocator,
            &self.resource_heap,
            &self.sampler_heap,
            width,
            height,
        )?;
        if let Some(previous) = self.atlas.replace(replacement) {
            match last_submission {
                Some(frame) => {
                    previous
                        .binding
                        .retire(&self.resource_heap, &self.sampler_heap, frame)
                }
                None => previous
                    .binding
                    .release(&self.resource_heap, &self.sampler_heap),
            }
            .map_err(|error| format!("retire replaced Vulkan text descriptors: {error}"))?;
        }
        self.last_upload_keys.clear();
        Ok(())
    }
}

fn create_atlas(
    allocator: &MemoryAllocator,
    resource_heap: &DescriptorHeap,
    sampler_heap: &DescriptorHeap,
    width: u32,
    height: u32,
) -> Result<VulkanTextAtlas, String> {
    let width = width.max(1);
    let height = height.max(1);
    let image = allocator
        .create_image(&ImageDescriptor {
            label: Some("tensor-files-vulkan-r8-text-atlas".into()),
            dimension: ImageDimension::D2,
            format: TextureFormat::R8Unorm,
            extent: Extent3D::new(width, height, 1),
            mip_levels: 1,
            array_layers: 1,
            samples: SampleCount::One,
            tiling: ImageTiling::Optimal,
            usage: TextureUsages::COPY_DESTINATION | TextureUsages::SAMPLED,
            memory: MemoryLocation::Device,
        })
        .map_err(|error| format!("create Vulkan R8 text atlas: {error}"))?;
    let view = image
        .create_view(&ImageViewDescriptor {
            label: Some("tensor-files-vulkan-r8-text-atlas-view".into()),
            dimension: ImageViewDimension::D2,
            format: TextureFormat::R8Unorm,
            components: ComponentMapping::IDENTITY,
            subresource_range: image.full_subresource_range(TextureAspects::COLOR),
        })
        .map_err(|error| format!("create Vulkan R8 text atlas view: {error}"))?;
    let binding = SampledTextureBinding::new(
        resource_heap,
        sampler_heap,
        &view,
        TextureLayout::ShaderReadOnly,
        SamplerDescriptor::linear_clamp(),
    )
    .map_err(|error| format!("create Vulkan text atlas descriptors: {error}"))?;
    Ok(VulkanTextAtlas {
        image,
        view,
        binding,
        width,
        height,
        initialized: false,
    })
}

fn descriptor_ring_capacity(size: u64, alignment: u64) -> Result<u64, String> {
    if size == 0 || !alignment.is_power_of_two() {
        return Err("Vulkan text descriptor layout is unusable".into());
    }
    size.checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .and_then(|stride| stride.checked_mul(DESCRIPTOR_RING_SLOTS))
        .ok_or_else(|| "Vulkan text descriptor ring capacity overflows".into())
}

fn create_pipeline(
    device: &Device,
    cache: &PipelineCache,
    format: TextureFormat,
) -> Result<GraphicsPipeline, String> {
    let vertex_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("tensor-files-vulkan-text-vertex".into()),
            spirv: vulkan_text_spirv::VERTEX.to_vec(),
        })
        .map_err(|error| format!("create Vulkan text vertex shader: {error}"))?;
    let fragment_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("tensor-files-vulkan-text-fragment".into()),
            spirv: vulkan_text_spirv::FRAGMENT.to_vec(),
        })
        .map_err(|error| format!("create Vulkan text fragment shader: {error}"))?;
    let vertex_bindings = vulkan_renderer::ShaderBindingMap::default();
    // The Slang fragment shader selects its atlas and sampler through direct
    // descriptor-heap indices in push data, so no binding map exists.
    let fragment_bindings = vulkan_renderer::ShaderBindingMap::default();
    let attributes = [
        VertexAttribute {
            format: vulkan_renderer::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        VertexAttribute {
            format: vulkan_renderer::VertexFormat::Float32x2,
            offset: 8,
            shader_location: 1,
        },
        VertexAttribute {
            format: vulkan_renderer::VertexFormat::Float32x4,
            offset: 16,
            shader_location: 2,
        },
    ];
    let buffers = [VertexBufferLayout {
        slot: 0,
        array_stride: std::mem::size_of::<TextVertex>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    }];
    let targets = [Some(ColorTargetState {
        format,
        blend: Some(BlendState::ALPHA_BLENDING),
        write_mask: vulkan_renderer::ColorWrites::ALL,
    })];
    device
        .create_graphics_pipeline(&GraphicsPipelineDescriptor {
            label: Some("tensor-files-vulkan-text-pipeline"),
            vertex: VertexState {
                stage: ProgrammableStage {
                    module: &vertex_shader,
                    entry_point: c"main",
                    bindings: &vertex_bindings,
                },
                buffers: &buffers,
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: FragmentState {
                stage: ProgrammableStage {
                    module: &fragment_shader,
                    entry_point: c"main",
                    bindings: &fragment_bindings,
                },
                targets: &targets,
            },
            advanced_blend: None,
            local_read_mapping: None,
            cache: Some(cache),
        })
        .map_err(|error| format!("create Vulkan text pipeline: {error}"))
}

fn compile_atlas_barriers(
    image: &Image,
    initialized: bool,
    queue_family: u32,
) -> Result<(BarrierBatch, BarrierBatch), String> {
    let graph = compile_atlas_graph(initialized, queue_family)?;
    let bindings = BTreeMap::from([(TEXT_IMAGE, ResourceBinding::whole_color_image(image))]);
    Ok((
        graph
            .barrier_batch_before(TEXT_UPLOAD, &bindings)
            .map_err(|error| format!("resolve Vulkan text upload barrier: {error}"))?,
        graph
            .barrier_batch_before(TEXT_SAMPLE, &bindings)
            .map_err(|error| format!("resolve Vulkan text sample barrier: {error}"))?,
    ))
}

fn compile_atlas_graph(initialized: bool, queue_family: u32) -> Result<CompiledGraph, String> {
    let mut graph = RenderGraph::default();
    graph.set_initial_state(
        TEXT_IMAGE,
        ResourceKind::Image,
        ResourceState::image(
            if initialized {
                RenderGraphImageState::FragmentSampledRead
            } else {
                RenderGraphImageState::Undefined
            },
            queue_family,
        ),
    );
    graph.add_pass(RenderPass {
        id: TEXT_UPLOAD,
        label: "tensor-files-vulkan-text-atlas-upload".into(),
        depends_on: Vec::new(),
        resources: vec![ResourceUse {
            resource: TEXT_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Write,
            state: ResourceState::image(RenderGraphImageState::CopyDestination, queue_family),
        }],
    });
    graph.add_pass(RenderPass {
        id: TEXT_SAMPLE,
        label: "tensor-files-vulkan-text-atlas-sample".into(),
        depends_on: vec![TEXT_UPLOAD],
        resources: vec![ResourceUse {
            resource: TEXT_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::image(RenderGraphImageState::FragmentSampledRead, queue_family),
        }],
    });
    graph
        .compile()
        .map_err(|error| format!("compile Vulkan text atlas graph: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_vertex_layout_matches_the_native_shader_contract() {
        assert_eq!(std::mem::size_of::<TextVertex>(), 32);
        assert_eq!(IMAGE_PUSH_OFFSET, 0);
    }

    #[test]
    fn descriptor_ring_keeps_three_in_flight_atlases_and_one_replacement() {
        assert_eq!(descriptor_ring_capacity(48, 32).unwrap(), 256);
        assert!(descriptor_ring_capacity(32, 0).is_err());
    }

    #[test]
    fn fresh_and_reused_atlases_have_two_explicit_layout_transitions() {
        let fresh = compile_atlas_graph(false, 2).unwrap();
        assert_eq!(fresh.barriers.len(), 2);
        assert_eq!(
            fresh.barriers[0].source.image_state(),
            Some(RenderGraphImageState::Undefined)
        );
        assert_eq!(
            fresh.barriers[0].destination.image_state(),
            Some(RenderGraphImageState::CopyDestination)
        );
        assert_eq!(
            fresh.barriers[1].destination.image_state(),
            Some(RenderGraphImageState::FragmentSampledRead)
        );

        let reused = compile_atlas_graph(true, 2).unwrap();
        assert_eq!(reused.barriers.len(), 2);
        assert_eq!(
            reused.barriers[0].source.image_state(),
            Some(RenderGraphImageState::FragmentSampledRead)
        );
    }
}
