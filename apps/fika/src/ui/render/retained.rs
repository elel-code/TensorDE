use std::borrow::Cow;

use crate::windowing::PhysicalSize;

use crate::nonzero_size;
use crate::ui::render::gpu::create_text_vertex_buffer;
use crate::ui::render::shaders::RETAINED_SCENE_SHADER;
use crate::ui::render::texture::TextVertex;

pub(crate) struct RetainedSceneRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    format: wgpu::TextureFormat,
    size: PhysicalSize<u32>,
    valid: bool,
}

impl RetainedSceneRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        size: PhysicalSize<u32>,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fika-wgpu-retained-scene-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fika-wgpu-retained-scene-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let texture = create_retained_scene_texture(device, format, size);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group =
            create_retained_scene_bind_group(device, &bind_group_layout, &texture_view, &sampler);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fika-wgpu-retained-scene-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(RETAINED_SCENE_SHADER)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fika-wgpu-retained-scene-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fika-wgpu-retained-scene-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(TextVertex::layout())],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let vertex_buffer = create_text_vertex_buffer(device, 6);
        queue.write_buffer(
            &vertex_buffer,
            0,
            bytemuck::cast_slice(&retained_scene_vertices()),
        );
        Self {
            pipeline,
            bind_group_layout,
            sampler,
            texture,
            texture_view,
            bind_group,
            vertex_buffer,
            format,
            size,
            valid: false,
        }
    }

    pub(crate) fn resize(&mut self, device: &wgpu::Device, size: PhysicalSize<u32>) {
        let size = nonzero_size(size);
        if self.size == size {
            return;
        }
        self.texture = create_retained_scene_texture(device, self.format, size);
        self.texture_view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = create_retained_scene_bind_group(
            device,
            &self.bind_group_layout,
            &self.texture_view,
            &self.sampler,
        );
        self.size = size;
        self.valid = false;
    }

    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.texture_view
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.valid
    }

    pub(crate) fn mark_valid(&mut self) {
        self.valid = true;
    }

    pub(crate) fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }

    pub(crate) fn batch_count(&self) -> usize {
        1
    }
}

fn create_retained_scene_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    size: PhysicalSize<u32>,
) -> wgpu::Texture {
    let size = nonzero_size(size);
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fika-wgpu-retained-scene-texture"),
        size: wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn create_retained_scene_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    texture_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fika-wgpu-retained-scene-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

pub(crate) fn retained_scene_vertices() -> [TextVertex; 6] {
    let color = [1.0; 4];
    [
        TextVertex {
            position: [-1.0, 1.0],
            uv: [0.0, 0.0],
            color,
        },
        TextVertex {
            position: [-1.0, -1.0],
            uv: [0.0, 1.0],
            color,
        },
        TextVertex {
            position: [1.0, -1.0],
            uv: [1.0, 1.0],
            color,
        },
        TextVertex {
            position: [-1.0, 1.0],
            uv: [0.0, 0.0],
            color,
        },
        TextVertex {
            position: [1.0, -1.0],
            uv: [1.0, 1.0],
            color,
        },
        TextVertex {
            position: [1.0, 1.0],
            uv: [1.0, 0.0],
            color,
        },
    ]
}
