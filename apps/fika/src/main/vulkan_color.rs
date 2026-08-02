use vulkan_renderer::{
    BlendState, Buffer, BufferUsages, ColorTargetState, Device, DynamicBuffer,
    DynamicBufferDescriptor, FragmentState, GraphicsPipeline, GraphicsPipelineDescriptor,
    MemoryAllocator, MultisampleState, PipelineCache, PrimitiveState, ProgrammableStage,
    RenderingEncoder, ShaderBindingMap, ShaderModuleDescriptor, TextureFormat, UploadBatch,
    VertexAttribute, VertexBufferLayout, VertexState, VertexStepMode,
};

use crate::ui::render::quad::QuadVertex;

use super::vulkan_color_spirv;

pub(crate) struct VulkanColorRenderer {
    pipeline: GraphicsPipeline,
    format: TextureFormat,
}

pub(crate) struct VulkanColorStream {
    vertex_buffer: DynamicBuffer,
    vertex_count: usize,
}

impl VulkanColorRenderer {
    pub(crate) fn new(
        device: &Device,
        pipeline_cache: &PipelineCache,
        format: TextureFormat,
    ) -> Result<Self, String> {
        Ok(Self {
            pipeline: create_pipeline(device, pipeline_cache, format)?,
            format,
        })
    }

    pub(crate) fn create_stream(
        &self,
        allocator: &MemoryAllocator,
        label: &str,
    ) -> Result<VulkanColorStream, String> {
        let vertex_buffer = DynamicBuffer::new(
            allocator,
            DynamicBufferDescriptor {
                label: Some(label.into()),
                initial_capacity: std::mem::size_of::<QuadVertex>() as u64 * 6,
                usage: BufferUsages::VERTEX,
            },
        )
        .map_err(|error| format!("create Vulkan color vertex buffer: {error}"))?;
        Ok(VulkanColorStream {
            vertex_buffer,
            vertex_count: 0,
        })
    }

    pub(crate) fn set_format(
        &mut self,
        device: &Device,
        pipeline_cache: &PipelineCache,
        format: TextureFormat,
    ) -> Result<(), String> {
        if self.format != format {
            self.pipeline = create_pipeline(device, pipeline_cache, format)?;
            self.format = format;
        }
        Ok(())
    }

    pub(crate) fn draw(
        &self,
        rendering: &mut RenderingEncoder<'_>,
        stream: &VulkanColorStream,
    ) -> Result<(), String> {
        if stream.vertex_count == 0 {
            return Ok(());
        }
        rendering
            .bind_pipeline(&self.pipeline)
            .map_err(|error| format!("bind Vulkan color pipeline: {error}"))?;
        unsafe { rendering.set_vertex_buffer(0, stream.vertex_buffer.buffer(), 0) }
            .map_err(|error| format!("bind Vulkan color vertex buffer: {error}"))?;
        unsafe { rendering.draw(0..stream.vertex_count as u32, 0..1) }
            .map_err(|error| format!("draw Vulkan color vertices: {error}"))
    }
}

impl VulkanColorStream {
    pub(crate) fn upload(
        &mut self,
        uploads: &mut UploadBatch<'_>,
        vertices: &[QuadVertex],
    ) -> Result<bool, String> {
        self.vertex_count = vertices.len();
        self.vertex_buffer
            .upload(uploads, bytemuck::cast_slice(vertices))
            .map(|upload| upload.bytes_written != 0)
            .map_err(|error| format!("upload Vulkan color vertices: {error}"))
    }

    pub(crate) fn vertex_buffer(&self) -> Option<&Buffer> {
        (self.vertex_count != 0).then_some(self.vertex_buffer.buffer())
    }
}

fn create_pipeline(
    device: &Device,
    pipeline_cache: &PipelineCache,
    format: TextureFormat,
) -> Result<GraphicsPipeline, String> {
    let vertex_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("fika-vulkan-color-vertex".into()),
            spirv: vulkan_color_spirv::VERTEX.to_vec(),
        })
        .map_err(|error| format!("create Vulkan color vertex shader: {error}"))?;
    let fragment_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("fika-vulkan-color-fragment".into()),
            spirv: vulkan_color_spirv::FRAGMENT.to_vec(),
        })
        .map_err(|error| format!("create Vulkan color fragment shader: {error}"))?;
    let bindings = ShaderBindingMap::default();
    let attributes = [
        VertexAttribute {
            format: vulkan_renderer::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        VertexAttribute {
            format: vulkan_renderer::VertexFormat::Float32x4,
            offset: 8,
            shader_location: 1,
        },
    ];
    let buffers = [VertexBufferLayout {
        slot: 0,
        array_stride: std::mem::size_of::<QuadVertex>() as u64,
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
            label: Some("fika-vulkan-color-pipeline"),
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
            advanced_blend: None,
            local_read_mapping: None,
            cache: Some(pipeline_cache),
        })
        .map_err(|error| format!("create Vulkan color pipeline: {error}"))
}
