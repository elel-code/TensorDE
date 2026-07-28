#[derive(Clone, Debug)]
struct GpuDragPreviewDraw {
    source: IconGpuSource,
    rect: ViewRect,
}

#[derive(Clone, Debug)]
struct GpuDragPreviewLabel {
    rect: ViewRect,
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
}

#[derive(Clone, Debug)]
struct GpuDragPreview {
    width: u32,
    height: u32,
    background: Option<(ViewRect, f32, [u8; 4])>,
    draws: Vec<GpuDragPreviewDraw>,
    label: Option<GpuDragPreviewLabel>,
}

const GPU_PREVIEW_COMPOSITE_SHADER: &str = r#"
struct Params {
    rect: vec4<f32>,
    canvas: vec2<f32>,
    radius: f32,
    mode: f32,
    color: vec4<f32>,
    angle: f32,
    inset: f32,
    blur: f32,
    opacity: f32,
};

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(1.0, 0.0)
    );
    let uv = corners[index];
    let center = params.rect.xy + params.rect.zw * 0.5;
    let local = (uv - vec2<f32>(0.5)) * params.rect.zw;
    let cosine = cos(params.angle);
    let sine = sin(params.angle);
    let pixel = center + vec2<f32>(
        local.x * cosine - local.y * sine,
        local.x * sine + local.y * cosine
    );
    var out: VertexOutput;
    out.position = vec4<f32>(
        pixel.x / params.canvas.x * 2.0 - 1.0,
        1.0 - pixel.y / params.canvas.y * 2.0,
        0.0,
        1.0
    );
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if params.mode > 1.5 {
        let content_size = max(params.rect.zw - vec2<f32>(params.inset * 2.0), vec2<f32>(1.0));
        let source_uv = (in.uv * params.rect.zw - vec2<f32>(params.inset)) / content_size;
        let dimensions = vec2<f32>(textureDimensions(source_texture));
        var alpha = 0.0;
        for (var y = -2; y <= 2; y = y + 1) {
            for (var x = -2; x <= 2; x = x + 1) {
                let offset = vec2<f32>(f32(x), f32(y)) * params.blur / dimensions;
                let sample_uv = source_uv + offset;
                if all(sample_uv >= vec2<f32>(0.0)) && all(sample_uv <= vec2<f32>(1.0)) {
                    alpha += textureSampleLevel(source_texture, source_sampler, sample_uv, 0.0).a;
                }
            }
        }
        alpha = alpha / 25.0 * params.opacity;
        return vec4<f32>(params.color.rgb * alpha, alpha);
    }
    if params.mode > 0.5 {
        let half_size = params.rect.zw * 0.5;
        let point = abs((in.uv - vec2<f32>(0.5)) * params.rect.zw);
        let radius = min(params.radius, min(half_size.x, half_size.y));
        let delta = max(point - (half_size - vec2<f32>(radius)), vec2<f32>(0.0));
        if length(delta) > radius {
            discard;
        }
    }
    return textureSample(source_texture, source_sampler, in.uv) * params.color;
}
"#;

#[repr(C, align(256))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuPreviewCompositeParams {
    rect: [f32; 4],
    canvas: [f32; 2],
    radius: f32,
    mode: f32,
    color: [f32; 4],
    angle: f32,
    inset: f32,
    blur: f32,
    opacity: f32,
    _padding: [f32; 48],
}

struct GpuPreviewCompositeDraw {
    bind_group: wgpu::BindGroup,
    params: GpuPreviewCompositeParams,
}

struct GpuIconSourceRenderer {
    svg_renderer: GpuSvgRenderer,
    composite_bind_group_layout: wgpu::BindGroupLayout,
    composite_sampler: wgpu::Sampler,
    composite_white_view: wgpu::TextureView,
    composite_pipeline_rgba: wgpu::RenderPipeline,
    composite_pipeline_bgra: wgpu::RenderPipeline,
    composite_params: wgpu::Buffer,
    composite_params_capacity: usize,
}

impl GpuIconSourceRenderer {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        let composite_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("fika-gpu-preview-composite-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: std::num::NonZeroU64::new(64),
                        },
                        count: None,
                    },
                ],
            });
        let composite_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fika-gpu-preview-composite-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fika-gpu-preview-white"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            white.as_image_copy(),
            &[255, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let composite_white_view = white.create_view(&wgpu::TextureViewDescriptor::default());
        let composite_pipeline_rgba = create_gpu_preview_composite_pipeline(
            device,
            &composite_bind_group_layout,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let composite_pipeline_bgra = create_gpu_preview_composite_pipeline(
            device,
            &composite_bind_group_layout,
            wgpu::TextureFormat::Bgra8Unorm,
        );
        let composite_params_capacity = 64;
        let composite_params = create_gpu_preview_params_buffer(device, composite_params_capacity);
        Some(Self {
            svg_renderer: GpuSvgRenderer::new(device),
            composite_bind_group_layout,
            composite_sampler,
            composite_white_view,
            composite_pipeline_rgba,
            composite_pipeline_bgra,
            composite_params,
            composite_params_capacity,
        })
    }

    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        source: &IconGpuSource,
    ) -> bool {
        let side = u32::from(source.size_px().max(1));
        match source {
            IconGpuSource::File { path, .. } => LoadedIconSource::load(path).is_some_and(|source| {
                self.render_loaded_source(device, queue, texture, &source, side, side)
            }),
            IconGpuSource::FolderPreview { children, seed, .. } => self
                .render_folder_preview_gpu(device, queue, texture, children, side, *seed),
        }
    }

    fn render_folder_preview_gpu(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        children: &[PathBuf],
        side: u32,
        seed: u64,
    ) -> bool {
        let Some(layout) = FileManagerDirectoryPreviewLayout::new(side) else {
            return false;
        };
        let slots = folder_preview_thumbnail_slots(children.len(), layout);
        let mut draws = Vec::with_capacity(children.len().saturating_mul(3));
        for (index, (path, slot)) in children.iter().zip(slots).enumerate() {
            let Some(child) = LoadedIconSource::load(path) else {
                continue;
            };
            let intrinsic = child.intrinsic_size();
            let border = layout.border_stroke_width.max(1) as f32;
            let available_width = (slot.width as f32 - border * 2.0).max(1.0);
            let available_height = (slot.height as f32 - border * 2.0).max(1.0);
            let scale = (available_width / intrinsic.width)
                .min(available_height / intrinsic.height)
                .min(1.0);
            let width = (intrinsic.width * scale).max(1.0);
            let height = (intrinsic.height * scale).max(1.0);
            let center_x = slot.x as f32 + slot.width as f32 * 0.5;
            let center_y = slot.y as f32 + slot.height as f32 * 0.5;
            let destination = ViewRect {
                x: center_x - width * 0.5,
                y: center_y - height * 0.5,
                width,
                height,
            };
            let source_width = width.ceil().max(1.0) as u32;
            let source_height = height.ceil().max(1.0) as u32;
            let source_texture = create_icon_texture(device, source_width, source_height);
            if !self.render_loaded_source(
                device,
                queue,
                &source_texture,
                &child,
                source_width,
                source_height,
            ) {
                continue;
            }
            let view = source_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let angle = (folder_preview_thumbnail_angle(seed, index) as f32).to_radians();
            let shadow_inset = border * 2.0;
            let shadow_rect = ViewRect {
                x: destination.x - shadow_inset + border * 0.5,
                y: destination.y - shadow_inset + border * 0.5,
                width: destination.width + shadow_inset * 2.0,
                height: destination.height + shadow_inset * 2.0,
            };
            draws.push(self.composite_draw(
                device,
                &view,
                shadow_rect,
                [side, side],
                0.0,
                2.0,
                [0, 0, 0, 255],
                [angle, shadow_inset, border.max(1.0), 0.45],
            ));
            let frame = ViewRect {
                x: destination.x - border,
                y: destination.y - border,
                width: destination.width + border * 2.0,
                height: destination.height + border * 2.0,
            };
            draws.push(self.composite_draw(
                device,
                &self.composite_white_view,
                frame,
                [side, side],
                0.0,
                1.0,
                [255; 4],
                [angle, 0.0, 0.0, 0.0],
            ));
            draws.push(self.composite_draw(
                device,
                &view,
                destination,
                [side, side],
                0.0,
                0.0,
                [255; 4],
                [angle, 0.0, 0.0, 0.0],
            ));
        }
        self.submit_composite_draws(device, queue, texture, [side, side], draws)
    }

    fn render_drag_preview(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        preview: &GpuDragPreview,
    ) -> bool {
        let mut draws = Vec::with_capacity(preview.draws.len() + 2);
        if let Some((rect, radius, color)) = preview.background {
            draws.push(self.composite_draw(
                device,
                &self.composite_white_view,
                rect,
                [preview.width, preview.height],
                radius,
                1.0,
                color,
                [0.0; 4],
            ));
        }
        for draw in &preview.draws {
            let side = u32::from(draw.source.size_px().max(1));
            let source_texture = create_icon_texture(device, side, side);
            if !self.render(device, queue, &source_texture, &draw.source) {
                continue;
            }
            let view = source_texture.create_view(&wgpu::TextureViewDescriptor::default());
            draws.push(self.composite_draw(
                device,
                &view,
                draw.rect,
                [preview.width, preview.height],
                0.0,
                0.0,
                [255; 4],
                [0.0; 4],
            ));
        }
        if let Some(label) = &preview.label {
            let label_texture = create_icon_texture(device, label.width, label.height);
            let rendered = upload_drag_preview_label_texture(queue, &label_texture, label);
            if rendered {
                let view = label_texture.create_view(&wgpu::TextureViewDescriptor::default());
                draws.push(self.composite_draw(
                    device,
                    &view,
                    label.rect,
                    [preview.width, preview.height],
                    0.0,
                    0.0,
                    [255; 4],
                    [0.0; 4],
                ));
            }
        }
        self.submit_composite_draws(
            device,
            queue,
            texture,
            [preview.width, preview.height],
            draws,
        )
    }

    fn composite_draw(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        rect: ViewRect,
        canvas: [u32; 2],
        radius: f32,
        mode: f32,
        color: [u8; 4],
        effects: [f32; 4],
    ) -> GpuPreviewCompositeDraw {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fika-gpu-preview-composite-bind-group"),
            layout: &self.composite_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.composite_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.composite_params,
                        offset: 0,
                        size: std::num::NonZeroU64::new(64),
                    }),
                },
            ],
        });
        GpuPreviewCompositeDraw {
            bind_group,
            params: GpuPreviewCompositeParams {
                rect: [rect.x, rect.y, rect.width.max(1.0), rect.height.max(1.0)],
                canvas: [canvas[0] as f32, canvas[1] as f32],
                radius,
                mode,
                color: color.map(|channel| channel as f32 / 255.0),
                angle: effects[0],
                inset: effects[1],
                blur: effects[2],
                opacity: effects[3],
                _padding: [0.0; 48],
            },
        }
    }

    fn submit_composite_draws(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        canvas: [u32; 2],
        mut draws: Vec<GpuPreviewCompositeDraw>,
    ) -> bool {
        if draws.is_empty() {
            return false;
        }
        if draws.len() > self.composite_params_capacity {
            return false;
        }
        let params = draws.iter().map(|draw| draw.params).collect::<Vec<_>>();
        queue.write_buffer(&self.composite_params, 0, bytemuck::cast_slice(&params));
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("fika-gpu-preview-composite-encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fika-gpu-preview-composite-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
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
            let pipeline = match texture.format() {
                wgpu::TextureFormat::Rgba8Unorm => &self.composite_pipeline_rgba,
                wgpu::TextureFormat::Bgra8Unorm => &self.composite_pipeline_bgra,
                _ => return false,
            };
            pass.set_pipeline(pipeline);
            for (index, draw) in draws.drain(..).enumerate() {
                pass.set_bind_group(0, &draw.bind_group, &[(index * 256) as u32]);
                pass.draw(0..6, 0..1);
            }
        }
        queue.submit(Some(encoder.finish()));
        canvas[0] > 0 && canvas[1] > 0
    }

    fn render_loaded_source(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        source: &LoadedIconSource,
        width: u32,
        height: u32,
    ) -> bool {
        match source {
            LoadedIconSource::Svg { bytes, .. } => self
                .svg_renderer
                .render_bytes(device, queue, texture, bytes, width, height),
            LoadedIconSource::Bitmap {
                width: source_width,
                height: source_height,
                pixels,
            } => {
                if *source_width == width
                    && *source_height == height
                    && texture.format() == wgpu::TextureFormat::Rgba8Unorm
                {
                    queue.write_texture(
                        texture.as_image_copy(),
                        pixels,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(source_width.saturating_mul(4)),
                            rows_per_image: Some(*source_height),
                        },
                        wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                    );
                    return true;
                }
                let source_texture = create_icon_texture(device, *source_width, *source_height);
                queue.write_texture(
                    source_texture.as_image_copy(),
                    pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(source_width.saturating_mul(4)),
                        rows_per_image: Some(*source_height),
                    },
                    wgpu::Extent3d {
                        width: *source_width,
                        height: *source_height,
                        depth_or_array_layers: 1,
                    },
                );
                let scale = (width as f32 / *source_width as f32)
                    .min(height as f32 / *source_height as f32);
                let draw_width = *source_width as f32 * scale;
                let draw_height = *source_height as f32 * scale;
                let view = source_texture.create_view(&wgpu::TextureViewDescriptor::default());
                let draw = self.composite_draw(
                    device,
                    &view,
                    ViewRect {
                        x: (width as f32 - draw_width) * 0.5,
                        y: (height as f32 - draw_height) * 0.5,
                        width: draw_width,
                        height: draw_height,
                    },
                    [width, height],
                    0.0,
                    0.0,
                    [255; 4],
                    [0.0; 4],
                );
                self.submit_composite_draws(device, queue, texture, [width, height], vec![draw])
            }
        }
    }

}

fn rasterize_gpu_drag_preview_label(
    renderer: &mut TextRenderer,
    rect: ViewRect,
    label: &str,
    color: [u8; 4],
) -> Option<GpuDragPreviewLabel> {
    let width = rect.width.ceil().max(1.0) as u32;
    let height = rect.height.ceil().max(1.0) as u32;
    if label.is_empty() {
        return None;
    }
    let font_size = (height as f32 * 0.58).max(10.0);
    renderer.text_buffer.set_metrics(Metrics::new(font_size, height as f32));
    renderer.text_buffer.set_wrap(Wrap::None);
    renderer.text_buffer.set_size(Some(width as f32), Some(height as f32));
    renderer.text_buffer.set_text(
        label,
        &Attrs::new().family(Family::SansSerif),
        Shaping::Advanced,
        Some(Align::Left),
    );
    let mut alpha = vec![0; width.saturating_mul(height) as usize];
    renderer.text_buffer.draw(
        &mut renderer.font_system,
        &mut renderer.swash_cache,
        TextColor::rgba(255, 255, 255, 255),
        |x, y, w, h, glyph_color| {
            fill_text_alpha_pixels(
                &mut alpha,
                width,
                height,
                TextAlphaRect { x, y, width: w, height: h },
                glyph_color,
            );
        },
    );
    let mut pixels = Vec::with_capacity(alpha.len().saturating_mul(4));
    for glyph_alpha in alpha {
        let a = ((u16::from(glyph_alpha) * u16::from(color[3]) + 127) / 255) as u8;
        pixels.extend_from_slice(&[
            ((u16::from(color[0]) * u16::from(a) + 127) / 255) as u8,
            ((u16::from(color[1]) * u16::from(a) + 127) / 255) as u8,
            ((u16::from(color[2]) * u16::from(a) + 127) / 255) as u8,
            a,
        ]);
    }
    Some(GpuDragPreviewLabel { rect, width, height, pixels: pixels.into() })
}

fn upload_drag_preview_label_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    label: &GpuDragPreviewLabel,
) -> bool {
    if label.width == 0
        || label.height == 0
        || label.pixels.len()
            != label.width.saturating_mul(label.height).saturating_mul(4) as usize
    {
        return false;
    }
    queue.write_texture(
        texture.as_image_copy(),
        &label.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(label.width.saturating_mul(4)),
            rows_per_image: Some(label.height),
        },
        wgpu::Extent3d {
            width: label.width,
            height: label.height,
            depth_or_array_layers: 1,
        },
    );
    true
}

fn create_gpu_preview_params_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fika-gpu-preview-composite-params"),
        size: (capacity.max(1) * std::mem::size_of::<GpuPreviewCompositeParams>()) as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_gpu_preview_composite_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("fika-gpu-preview-composite-shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(GPU_PREVIEW_COMPOSITE_SHADER)),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("fika-gpu-preview-composite-pipeline-layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("fika-gpu-preview-composite-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
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
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

enum LoadedIconSource {
    Svg {
        bytes: Vec<u8>,
        intrinsic: SvgIntrinsicSize,
    },
    Bitmap {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    },
}

impl LoadedIconSource {
    fn load(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        if is_svg_path(path) {
            let intrinsic = svg_intrinsic_size(&bytes)?;
            Some(Self::Svg { bytes, intrinsic })
        } else {
            let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
            let (width, height) = image.dimensions();
            if width == 0 || height == 0 {
                return None;
            }
            let mut pixels = image.into_raw();
            premultiply_rgba8(&mut pixels);
            Some(Self::Bitmap {
                width,
                height,
                pixels,
            })
        }
    }

    fn intrinsic_size(&self) -> SvgIntrinsicSize {
        match self {
            Self::Svg { intrinsic, .. } => *intrinsic,
            Self::Bitmap { width, height, .. } => SvgIntrinsicSize {
                width: *width as f32,
                height: *height as f32,
            },
        }
    }
}

fn is_svg_path(path: &Path) -> bool {
    path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
}

fn premultiply_rgba8(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        pixel[0] = ((u16::from(pixel[0]) * alpha + 127) / 255) as u8;
        pixel[1] = ((u16::from(pixel[1]) * alpha + 127) / 255) as u8;
        pixel[2] = ((u16::from(pixel[2]) * alpha + 127) / 255) as u8;
    }
}
