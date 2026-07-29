use std::collections::BTreeMap;

use vulkan_renderer::{
    AccessKind, BlendState, BufferDescriptor, ColorAttachment, ColorTargetState, CompiledGraph,
    DescriptorHeap, Device, FragmentState, GraphicsPipeline, GraphicsPipelineDescriptor,
    ImageViewDescriptor, IndexFormat, LoadOp, MemoryAllocator, MemoryLocation, MultisampleState,
    PassId, PipelineCache, PrimitiveState, ProgrammableStage, RenderGraph, RenderPass,
    RenderingDescriptor, ResourceBinding, ResourceId, ResourceKind, ResourceState, ResourceUse,
    SampledImageBinding, ShaderBindingMap, ShaderModuleDescriptor, StoreOp, UploadBatch,
    VertexAttribute, VertexBufferLayout, VertexState, VertexStepMode, vk,
};

use crate::{
    IconGpuSlot, IconGpuSource, LoadedIconSource, SvgVertex, is_svg_path, tessellate_svg,
    vulkan_icon_spirv,
};

use super::{VulkanIconImage, VulkanIconTexture, create_rgba_image};

const SVG_IMAGE: ResourceId = ResourceId(1);
const SVG_VERTICES: ResourceId = ResourceId(2);
const SVG_INDICES: ResourceId = ResourceId(3);
const SVG_UPLOAD: PassId = PassId(1);
const SVG_RENDER: PassId = PassId(2);
const SVG_SAMPLE: PassId = PassId(3);

pub(super) fn is_svg_source(slot: &IconGpuSlot) -> bool {
    matches!(
        slot.source.as_ref(),
        Some(IconGpuSource::File { path, .. }) if is_svg_path(path)
    )
}

pub(super) struct SvgIconRasterizer {
    pipeline: GraphicsPipeline,
    graph: CompiledGraph,
}

impl SvgIconRasterizer {
    pub(super) fn new(
        device: &Device,
        cache: &PipelineCache,
        queue_family: u32,
    ) -> Result<Self, String> {
        Ok(Self {
            pipeline: create_pipeline(device, cache)?,
            graph: compile_graph(queue_family)?,
        })
    }

    pub(super) fn create_texture(
        &self,
        heap: &DescriptorHeap,
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        slot: &IconGpuSlot,
        gpu_frame: u64,
    ) -> Result<Option<VulkanIconTexture>, String> {
        let Some(IconGpuSource::File { path, .. }) = slot.source.as_ref() else {
            return Ok(None);
        };
        let Some(LoadedIconSource::Svg { bytes, .. }) = LoadedIconSource::load(path) else {
            return Ok(None);
        };
        let width = slot.width.max(1);
        let height = slot.height.max(1);
        let Some((image, view)) = self.rasterize(allocator, uploads, &bytes, width, height)? else {
            return Ok(None);
        };
        let binding =
            SampledImageBinding::new(heap, &view, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .map_err(|error| format!("create Vulkan SVG icon descriptor: {error}"))?;
        Ok(Some(VulkanIconTexture {
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
            last_used_frame: gpu_frame,
        }))
    }

    /// Records commands rasterizing `bytes` into a fresh sampled RGBA image.
    ///
    /// The image ends in `SHADER_READ_ONLY_OPTIMAL`; geometry buffers are
    /// retained by the encoder until the submission retires.
    pub(super) fn rasterize(
        &self,
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        bytes: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Option<(vulkan_renderer::Image, vulkan_renderer::ImageView)>, String> {
        let Some(geometry) = tessellate_svg(bytes, width, height) else {
            return Ok(None);
        };
        let vertex_bytes = bytemuck::cast_slice(&geometry.vertices);
        let index_bytes = bytemuck::cast_slice(&geometry.indices);
        let vertices = create_geometry_buffer(
            allocator,
            "fika-vulkan-svg-vertices",
            vertex_bytes,
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )?;
        let indices = create_geometry_buffer(
            allocator,
            "fika-vulkan-svg-indices",
            index_bytes,
            vk::BufferUsageFlags::INDEX_BUFFER,
        )?;
        let image = create_rgba_image(
            allocator,
            "fika-vulkan-svg-icon",
            width,
            height,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        )?;
        let view = image
            .create_view(&ImageViewDescriptor {
                label: Some("fika-vulkan-svg-icon-view".into()),
                view_type: vk::ImageViewType::_2D,
                format: image.format(),
                components: vk::ComponentMapping::default(),
                subresource_range: image.full_subresource_range(vk::ImageAspectFlags::COLOR),
            })
            .map_err(|error| format!("create Vulkan SVG icon view: {error}"))?;
        let bindings = resource_bindings(&image, &vertices, &indices);
        unsafe {
            uploads
                .write_buffer(&vertices, 0, vertex_bytes)
                .map_err(|error| format!("upload Vulkan SVG vertices: {error}"))?;
            uploads
                .write_buffer(&indices, 0, index_bytes)
                .map_err(|error| format!("upload Vulkan SVG indices: {error}"))?;
            let before_render = self
                .graph
                .barrier_batch_before(SVG_RENDER, &bindings)
                .map_err(|error| format!("resolve Vulkan SVG render barriers: {error}"))?;
            uploads.encoder_mut().pipeline_barrier(&before_render);
        }
        record_svg_draw(
            uploads,
            &self.pipeline,
            &view,
            &vertices,
            &indices,
            geometry.indices.len(),
            width,
            height,
        )?;
        let before_sample = self
            .graph
            .barrier_batch_before(SVG_SAMPLE, &bindings)
            .map_err(|error| format!("resolve Vulkan SVG sample barrier: {error}"))?;
        unsafe { uploads.encoder_mut().pipeline_barrier(&before_sample) };
        Ok(Some((image, view)))
    }
}

fn create_geometry_buffer(
    allocator: &MemoryAllocator,
    label: &str,
    bytes: &[u8],
    usage: vk::BufferUsageFlags,
) -> Result<vulkan_renderer::Buffer, String> {
    let size = u64::try_from(bytes.len()).map_err(|_| format!("{label} size exceeds u64"))?;
    allocator
        .create_buffer(&BufferDescriptor {
            label: Some(label.into()),
            size,
            usage: usage | vk::BufferUsageFlags::TRANSFER_DST,
            memory: MemoryLocation::Device,
        })
        .map_err(|error| format!("create {label}: {error}"))
}

fn resource_bindings(
    image: &vulkan_renderer::Image,
    vertices: &vulkan_renderer::Buffer,
    indices: &vulkan_renderer::Buffer,
) -> BTreeMap<ResourceId, ResourceBinding> {
    BTreeMap::from([
        (
            SVG_IMAGE,
            ResourceBinding::Image {
                image: image.raw(),
                subresource_range: image.full_subresource_range(vk::ImageAspectFlags::COLOR),
            },
        ),
        (
            SVG_VERTICES,
            ResourceBinding::Buffer {
                buffer: vertices.raw(),
                offset: 0,
                size: vertices.size(),
            },
        ),
        (
            SVG_INDICES,
            ResourceBinding::Buffer {
                buffer: indices.raw(),
                offset: 0,
                size: indices.size(),
            },
        ),
    ])
}

#[allow(clippy::too_many_arguments)]
fn record_svg_draw(
    uploads: &mut UploadBatch<'_>,
    pipeline: &GraphicsPipeline,
    view: &vulkan_renderer::ImageView,
    vertices: &vulkan_renderer::Buffer,
    indices: &vulkan_renderer::Buffer,
    index_count: usize,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let color_attachments = [Some(ColorAttachment {
        view: view.as_attachment(),
        layout: vk::ImageLayout::ATTACHMENT_OPTIMAL,
        resolve_target: None,
        resolve_layout: vk::ImageLayout::UNDEFINED,
        resolve_mode: vk::ResolveModeFlags::NONE,
        load_op: LoadOp::Clear([0.0; 4]),
        store_op: StoreOp::Store,
    })];
    let descriptor = RenderingDescriptor {
        label: Some("fika-vulkan-svg-icon-render"),
        render_area: vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: vk::Extent2D { width, height },
        },
        layer_count: 1,
        view_mask: 0,
        color_attachments: &color_attachments,
        depth_attachment: None,
        stencil_attachment: None,
    };
    let mut rendering = unsafe { uploads.encoder_mut().begin_rendering(&descriptor) }
        .map_err(|error| format!("begin Vulkan SVG icon rendering: {error}"))?;
    rendering
        .set_viewport(vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        })
        .map_err(|error| format!("set Vulkan SVG icon viewport: {error}"))?;
    rendering
        .set_scissor(descriptor.render_area)
        .map_err(|error| format!("set Vulkan SVG icon scissor: {error}"))?;
    rendering
        .bind_pipeline(pipeline)
        .map_err(|error| format!("bind Vulkan SVG icon pipeline: {error}"))?;
    rendering.retain_resource(view);
    unsafe {
        rendering
            .set_vertex_buffer(0, vertices, 0)
            .map_err(|error| format!("bind Vulkan SVG vertex buffer: {error}"))?;
        rendering
            .set_index_buffer(indices, 0, IndexFormat::Uint32)
            .map_err(|error| format!("bind Vulkan SVG index buffer: {error}"))?;
        rendering
            .draw_indexed(
                0..u32::try_from(index_count)
                    .map_err(|_| "Vulkan SVG index count exceeds u32".to_string())?,
                0,
                0..1,
            )
            .map_err(|error| format!("draw Vulkan SVG icon: {error}"))?;
    }
    rendering.end();
    Ok(())
}

fn create_pipeline(device: &Device, cache: &PipelineCache) -> Result<GraphicsPipeline, String> {
    let vertex_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("fika-vulkan-svg-icon-vertex".into()),
            spirv: vulkan_icon_spirv::SVG_VERTEX.to_vec(),
        })
        .map_err(|error| format!("create Vulkan SVG icon vertex shader: {error}"))?;
    let fragment_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("fika-vulkan-svg-icon-fragment".into()),
            spirv: vulkan_icon_spirv::SVG_FRAGMENT.to_vec(),
        })
        .map_err(|error| format!("create Vulkan SVG icon fragment shader: {error}"))?;
    let bindings = ShaderBindingMap::default();
    let attributes = [
        VertexAttribute {
            format: vk::Format::R32G32_SFLOAT,
            offset: 0,
            shader_location: 0,
        },
        VertexAttribute {
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: 8,
            shader_location: 1,
        },
    ];
    let buffers = [VertexBufferLayout {
        array_stride: std::mem::size_of::<SvgVertex>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    }];
    let targets = [Some(ColorTargetState {
        format: vk::Format::R8G8B8A8_UNORM,
        blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        write_mask: vk::ColorComponentFlags::R
            | vk::ColorComponentFlags::G
            | vk::ColorComponentFlags::B
            | vk::ColorComponentFlags::A,
    })];
    device
        .create_graphics_pipeline(&GraphicsPipelineDescriptor {
            label: Some("fika-vulkan-svg-icon-pipeline"),
            vertex: VertexState {
                stage: ProgrammableStage {
                    module: &vertex_shader,
                    entry_point: c"main",
                    bindings: &bindings,
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
                    bindings: &bindings,
                },
                targets: &targets,
            },
            cache: Some(cache),
        })
        .map_err(|error| format!("create Vulkan SVG icon pipeline: {error}"))
}

fn compile_graph(queue_family: u32) -> Result<CompiledGraph, String> {
    let mut graph = RenderGraph::default();
    graph.set_initial_state(
        SVG_IMAGE,
        ResourceKind::Image,
        ResourceState::image(
            vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE,
            vk::ImageLayout::UNDEFINED,
            queue_family,
        ),
    );
    graph.add_pass(RenderPass {
        id: SVG_UPLOAD,
        label: "fika-vulkan-svg-upload".into(),
        depends_on: Vec::new(),
        resources: vec![
            ResourceUse {
                resource: SVG_VERTICES,
                kind: ResourceKind::Buffer,
                access: AccessKind::Write,
                state: ResourceState::buffer_copy_destination(queue_family),
            },
            ResourceUse {
                resource: SVG_INDICES,
                kind: ResourceKind::Buffer,
                access: AccessKind::Write,
                state: ResourceState::buffer_copy_destination(queue_family),
            },
        ],
    });
    graph.add_pass(RenderPass {
        id: SVG_RENDER,
        label: "fika-vulkan-svg-render".into(),
        depends_on: vec![SVG_UPLOAD],
        resources: vec![
            ResourceUse {
                resource: SVG_IMAGE,
                kind: ResourceKind::Image,
                access: AccessKind::Write,
                state: ResourceState::color_attachment_write(queue_family),
            },
            ResourceUse {
                resource: SVG_VERTICES,
                kind: ResourceKind::Buffer,
                access: AccessKind::Read,
                state: ResourceState::vertex_buffer(queue_family),
            },
            ResourceUse {
                resource: SVG_INDICES,
                kind: ResourceKind::Buffer,
                access: AccessKind::Read,
                state: ResourceState::buffer(
                    vk::PipelineStageFlags2::INDEX_INPUT,
                    vk::AccessFlags2::INDEX_READ,
                    queue_family,
                ),
            },
        ],
    });
    graph.add_pass(RenderPass {
        id: SVG_SAMPLE,
        label: "fika-vulkan-svg-sample".into(),
        depends_on: vec![SVG_RENDER],
        resources: vec![ResourceUse {
            resource: SVG_IMAGE,
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
        .map_err(|error| format!("compile Vulkan SVG icon graph: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_vertex_layout_matches_shader_contract() {
        assert_eq!(std::mem::size_of::<SvgVertex>(), 24);
    }

    #[test]
    fn svg_graph_uploads_then_renders_then_samples() {
        let graph = compile_graph(7).unwrap();
        assert_eq!(graph.ordered_passes, [SVG_UPLOAD, SVG_RENDER, SVG_SAMPLE]);
        assert_eq!(
            graph
                .barriers
                .iter()
                .filter(|barrier| barrier.after == SVG_RENDER)
                .count(),
            3
        );
        assert!(graph.barriers.iter().any(|barrier| {
            barrier.after == SVG_SAMPLE
                && barrier.destination.layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        }));
    }
}
