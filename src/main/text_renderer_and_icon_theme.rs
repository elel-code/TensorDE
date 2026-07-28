fn text_vertices_for_pending(
    pending: &[PendingTextDraw],
    atlases: &[AtlasRect],
    atlas_width: u32,
    atlas_height: u32,
    surface_size: PhysicalSize<u32>,
) -> Vec<TextVertex> {
    let mut vertices = Vec::with_capacity(pending.len() * 6);
    for (draw, atlas) in pending.iter().zip(atlases.iter()) {
        let guard = TEXT_ATLAS_GUARD_TEXELS as f32;
        let scale_x = draw.label_width as f32 / draw.rect.width.max(1.0);
        let scale_y = draw.label_height as f32 / draw.rect.height.max(1.0);
        let atlas = AtlasRect {
            x: atlas.x + guard + (draw.screen.x - draw.rect.x).max(0.0) * scale_x,
            y: atlas.y + guard + (draw.screen.y - draw.rect.y).max(0.0) * scale_y,
            width: draw.screen.width * scale_x,
            height: draw.screen.height * scale_y,
        };
        push_textured_rect(
            &mut vertices,
            draw.screen,
            atlas,
            atlas_width,
            atlas_height,
            surface_size,
            text_color_to_vertex_color(draw.color),
        );
    }
    vertices
}
struct TextRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    texture_width: u32,
    texture_height: u32,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_count: usize,
    last_vertices_hash: Option<u64>,
    last_text_upload_keys: HashSet<TextAtlasUploadKey>,
    font_system: FontSystem,
    swash_cache: SwashCache,
    text_buffer: Buffer,
    label_cache: LabelRasterCache,
    metrics_cache: LabelMetricsCache,
    atlas_cache: TextAtlasFrameCache,
    staging_pixels: Vec<u8>,
}
impl TextRenderer {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fika-wgpu-text-bind-group-layout"),
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
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fika-wgpu-text-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture = create_text_texture(device, 1, 1);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group =
            create_text_bind_group(device, &bind_group_layout, &texture_view, &sampler);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fika-wgpu-text-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(TEXT_SHADER)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fika-wgpu-text-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fika-wgpu-text-pipeline"),
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let mut font_system = FontSystem::new();
        let mut text_buffer = Buffer::new(
            &mut font_system,
            Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT),
        );
        text_buffer.set_wrap(Wrap::WordOrGlyph);
        let swash_cache = SwashCache::new();
        let label_cache = LabelRasterCache::new(TEXT_LABEL_CACHE_MAX_BYTES);
        let metrics_cache = LabelMetricsCache::new(TEXT_LABEL_METRICS_CACHE_MAX_ENTRIES);
        let atlas_cache = TextAtlasFrameCache::default();
        let vertex_capacity = 6;
        let vertex_buffer = create_text_vertex_buffer(device, vertex_capacity);

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            texture,
            texture_view,
            bind_group,
            texture_width: 1,
            texture_height: 1,
            vertex_buffer,
            vertex_capacity,
            vertex_count: 0,
            last_vertices_hash: None,
            last_text_upload_keys: HashSet::new(),
            font_system,
            swash_cache,
            text_buffer,
            label_cache,
            metrics_cache,
            atlas_cache,
            staging_pixels: Vec::new(),
        }
    }

    fn trim_text_engine_caches(&mut self) -> (usize, usize, bool) {
        let image_entries = self.swash_cache.image_cache.len();
        let outline_entries = self.swash_cache.outline_command_cache.len();
        let reset = image_entries > TEXT_SWASH_IMAGE_CACHE_MAX_ENTRIES
            || outline_entries > TEXT_SWASH_OUTLINE_CACHE_MAX_ENTRIES;
        if reset {
            self.swash_cache = SwashCache::new();
        }
        (image_entries, outline_entries, reset)
    }

    fn take_staging_pixels(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.staging_pixels)
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &mut TextFrame,
    ) -> VertexBufferUploadStats {
        let texture_resized =
            frame.width != self.texture_width || frame.height != self.texture_height;
        if texture_resized {
            let new_texture = create_text_texture(device, frame.width, frame.height);
            let new_texture_view = new_texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.texture = new_texture;
            self.texture_view = new_texture_view;
            self.bind_group = create_text_bind_group(
                device,
                &self.bind_group_layout,
                &self.texture_view,
                &self.sampler,
            );
            self.texture_width = frame.width;
            self.texture_height = frame.height;
            self.last_vertices_hash = None;
            self.last_text_upload_keys.clear();
        }

        let mut current_upload_keys = HashSet::with_capacity(frame.uploads.len());
        let mut atlas_uploads = 0usize;
        let mut atlas_upload_skips = 0usize;
        if !frame.vertices.is_empty() {
            for upload in &frame.uploads {
                let skip_upload = text_atlas_upload_should_skip(
                    upload,
                    &self.last_text_upload_keys,
                    &mut current_upload_keys,
                );
                if skip_upload {
                    atlas_upload_skips += 1;
                    continue;
                }
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: upload.atlas.x as u32,
                            y: upload.atlas.y as u32,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    upload.pixels.as_ref(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(upload.width),
                        rows_per_image: Some(upload.height),
                    },
                    wgpu::Extent3d {
                        width: upload.width,
                        height: upload.height,
                        depth_or_array_layers: 1,
                    },
                );
                atlas_uploads += 1;
            }
        }
        self.last_text_upload_keys = current_upload_keys;
        frame.stats.atlas_uploads = atlas_uploads;
        frame.stats.atlas_upload_skips = atlas_upload_skips;

        if frame.vertices.len() > self.vertex_capacity {
            self.vertex_capacity = frame.vertices.len().next_power_of_two();
            self.vertex_buffer = create_text_vertex_buffer(device, self.vertex_capacity);
            self.last_vertices_hash = None;
        }
        self.vertex_count = frame.vertices.len();
        let vertex_upload_stats = upload_vertex_buffer_if_dirty(
            queue,
            &self.vertex_buffer,
            &frame.vertices,
            &mut self.last_vertices_hash,
        );
        self.staging_pixels = std::mem::take(&mut frame.pixels);
        self.staging_pixels.clear();
        vertex_upload_stats
    }

    fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count as u32, 0..1);
    }

    fn batch_count(&self) -> usize {
        usize::from(self.vertex_count > 0)
    }
}
fn text_atlas_upload_should_skip(
    upload: &TextAtlasUpload,
    last_upload_keys: &HashSet<TextAtlasUploadKey>,
    current_upload_keys: &mut HashSet<TextAtlasUploadKey>,
) -> bool {
    let key = TextAtlasUploadKey::from_upload(upload);
    let skip_upload = last_upload_keys.contains(&key);
    current_upload_keys.insert(key);
    skip_upload
}
fn text_color_to_vertex_color(color: TextColor) -> [f32; 4] {
    let [r, g, b, a] = color.as_rgba();
    [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ]
}
#[derive(Clone, Copy, Debug)]
struct TextAlphaRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn fill_text_alpha_pixels(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    rect: TextAlphaRect,
    color: TextColor,
) {
    let TextAlphaRect {
        x,
        y,
        width: w,
        height: h,
    } = rect;
    if color.a() == 0 || w == 0 || h == 0 {
        return;
    }
    let x0 = x.max(0) as u32;
    let y0 = y.max(0) as u32;
    let x1 = (x.saturating_add(w as i32)).clamp(0, width as i32) as u32;
    let y1 = (y.saturating_add(h as i32)).clamp(0, height as i32) as u32;
    if x1 <= x0 || y1 <= y0 {
        return;
    }

    let src_alpha = color.a();
    for yy in y0..y1 {
        for xx in x0..x1 {
            let offset = (yy * width + xx) as usize;
            pixels[offset] = blend_alpha(pixels[offset], src_alpha);
        }
    }
}
fn blend_alpha(dst: u8, src: u8) -> u8 {
    let src_a = src as f32 / 255.0;
    if src_a <= 0.0 {
        return dst;
    }
    let dst_a = dst as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    (out_a * 255.0).round().clamp(0.0, 255.0) as u8
}
fn shaping_for_label(label: &str, wrap: LabelWrap) -> Shaping {
    if wrap == LabelWrap::None && label.is_ascii() {
        Shaping::Basic
    } else {
        Shaping::Advanced
    }
}
#[derive(Clone, Debug)]
struct IconThemeResolver {
    roots: Vec<PathBuf>,
    themes: Vec<String>,
    search_order: Option<Vec<String>>,
    inherits_cache: HashMap<String, Vec<String>>,
    path_cache: HashMap<(String, u16), Option<PathBuf>>,
    dir_exists_cache: HashMap<PathBuf, bool>,
    renderable_file_cache: HashMap<PathBuf, bool>,
}
impl Default for IconThemeResolver {
    fn default() -> Self {
        Self {
            roots: icon_theme_roots(),
            themes: icon_theme_names(),
            search_order: None,
            inherits_cache: HashMap::new(),
            path_cache: HashMap::new(),
            dir_exists_cache: HashMap::new(),
            renderable_file_cache: HashMap::new(),
        }
    }
}
