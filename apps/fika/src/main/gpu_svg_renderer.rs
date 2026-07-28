// Legacy wgpu submission for renderer-independent SVG geometry.

const GPU_SVG_SHADER: &str = r#"
struct In { @location(0) position: vec2<f32>, @location(1) color: vec4<f32> };
struct Out { @builtin(position) position: vec4<f32>, @location(0) color: vec4<f32> };
@vertex fn vs_main(input: In) -> Out {
    var out: Out;
    out.position = vec4<f32>(input.position, 0.0, 1.0);
    out.color = input.color;
    return out;
}
@fragment fn fs_main(input: Out) -> @location(0) vec4<f32> { return input.color; }
"#;

impl SvgVertex {
    const WGPU_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    fn wgpu_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::WGPU_ATTRIBUTES,
        }
    }
}

struct GpuSvgRenderer {
    rgba_pipeline: wgpu::RenderPipeline,
    bgra_pipeline: wgpu::RenderPipeline,
}

impl GpuSvgRenderer {
    fn new(device: &wgpu::Device) -> Self {
        Self {
            rgba_pipeline: create_gpu_svg_pipeline(device, wgpu::TextureFormat::Rgba8Unorm),
            bgra_pipeline: create_gpu_svg_pipeline(device, wgpu::TextureFormat::Bgra8Unorm),
        }
    }

    fn render_bytes(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::Texture,
        bytes: &[u8],
        width: u32,
        height: u32,
    ) -> bool {
        let Some(geometry) = tessellate_svg(bytes, width, height) else {
            return false;
        };
        use wgpu::util::DeviceExt as _;
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fika-svg-vertices"),
            contents: bytemuck::cast_slice(&geometry.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fika-svg-indices"),
            contents: bytemuck::cast_slice(&geometry.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let format = match target.format() {
            wgpu::TextureFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Bgra8Unorm => wgpu::TextureFormat::Bgra8Unorm,
            _ => return false,
        };
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fika-svg-msaa"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 4,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("fika-svg-encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fika-svg-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let pipeline = match format {
                wgpu::TextureFormat::Rgba8Unorm => &self.rgba_pipeline,
                wgpu::TextureFormat::Bgra8Unorm => &self.bgra_pipeline,
                _ => return false,
            };
            pass.set_pipeline(pipeline);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..geometry.indices.len() as u32, 0, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        true
    }
}

fn create_gpu_svg_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("fika-svg-shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(GPU_SVG_SHADER)),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("fika-svg-pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(SvgVertex::wgpu_layout())],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 4,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
