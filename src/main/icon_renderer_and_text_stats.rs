impl IconRenderer {
    fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fika-wgpu-icon-bind-group-layout"),
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
            label: Some("fika-wgpu-icon-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fika-wgpu-icon-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(ICON_TEXTURE_SHADER)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fika-wgpu-icon-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fika-wgpu-icon-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(IconVertex::layout())],
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

        let vertex_capacity = 6;
        let vertex_buffer = create_icon_vertex_buffer(device, vertex_capacity);
        Self {
            pipeline,
            bind_group_layout,
            sampler,
            gpu_textures: HashMap::new(),
            gpu_texture_bytes: 0,
            frame_slot_keys: Vec::new(),
            content_batches: Vec::new(),
            overlay_batches: Vec::new(),
            vertex_buffer,
            vertex_capacity,
            content_vertex_count: 0,
            overlay_vertex_start: 0,
            overlay_vertex_count: 0,
            last_vertices_hash: None,
            gpu_frame: 0,
            dmabuf_plan: None,
            dmabuf_import_supported: false,
            dmabuf_imports: 0,
            // Create SVG/composite pipelines lazily only when an encoded source
            // first needs a GPU draw. Dmabuf/resident-only frames avoid them.
            gpu_source_renderer: None,
            engine: IconEngine::new(),
        }
    }

    /// Update dmabuf import capability + negotiated plan (from compositor feedback).
    fn set_dmabuf_import_state(
        &mut self,
        supported: bool,
        plan: Option<crate::shell::render::dmabuf::DmabufImportPlan>,
    ) {
        self.dmabuf_import_supported = supported;
        self.dmabuf_plan = plan;
    }

    /// Lifetime count of zero-copy dmabuf imports (diagnostics / tests).
    #[cfg_attr(not(test), allow(dead_code))]
    fn icon_dmabuf_import_count(&self) -> u64 {
        self.dmabuf_imports
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &mut IconFrame,
    ) -> VertexBufferUploadStats {
        self.gpu_frame = self.gpu_frame.wrapping_add(1);
        let mut uploads = 0usize;
        let mut upload_skips = 0usize;
        self.frame_slot_keys.clear();
        self.frame_slot_keys.reserve(frame.slots.len());

        for slot in &mut frame.slots {
            let key = IconGpuUploadKey::from_slot(slot);
            self.frame_slot_keys.push(key.clone());

            // Resident GPU texture with matching content → pure GPU sample.
            let can_skip = self.gpu_textures.get(&key).is_some_and(|entry| {
                entry.content_hash == slot.content_hash
                    && entry.width == slot.width
                    && entry.height == slot.height
            });
            if can_skip {
                if let Some(entry) = self.gpu_textures.get_mut(&key) {
                    entry.last_used_frame = self.gpu_frame;
                    // Align slot UV metadata with resident texture.
                    slot.width = entry.width;
                    slot.height = entry.height;
                    slot.content_width = entry.content_width;
                    slot.content_height = entry.content_height;
                    slot.rounding = entry.rounding;
                }
                slot.source = None;
                let _ = slot.dmabuf.take();
                upload_skips += 1;
                continue;
            }

            // Need pixels (or dmabuf) to fill / rewrite the resident texture.
            if slot.source.is_none() && slot.dmabuf.is_none() {
                // Sample-only draw for an already-resident identity that was
                // not rewritten this frame — still mark used.
                if let Some(entry) = self.gpu_textures.get_mut(&key) {
                    entry.last_used_frame = self.gpu_frame;
                    slot.width = entry.width;
                    slot.height = entry.height;
                    slot.content_width = entry.content_width;
                    slot.content_height = entry.content_height;
                    slot.content_hash = entry.content_hash;
                    slot.rounding = entry.rounding;
                }
                upload_skips += 1;
                continue;
            }

            let Some((texture, source)) = self.upload_slot_texture(device, queue, slot) else {
                // A failed GPU source stays non-resident and is retried from
                // the encoded source on the next frame. Never substitute CPU
                // rasterization here.
                continue;
            };
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group =
                create_icon_bind_group(device, &self.bind_group_layout, &view, &self.sampler);
            if source == crate::shell::render::dmabuf::ExternalTextureSource::DmabufImport {
                self.dmabuf_imports = self.dmabuf_imports.saturating_add(1);
            }
            let texture_bytes = icon_texture_bytes(slot.width, slot.height);
            let replaced = self.gpu_textures.insert(
                key,
                IconGpuTexture {
                    texture,
                    bind_group,
                    width: slot.width,
                    height: slot.height,
                    content_width: slot.content_width,
                    content_height: slot.content_height,
                    content_hash: slot.content_hash,
                    rounding: slot.rounding,
                    last_used_frame: self.gpu_frame,
                    source,
                },
            );
            self.gpu_texture_bytes = self.gpu_texture_bytes.saturating_add(texture_bytes);
            if let Some(replaced) = replaced {
                self.gpu_texture_bytes = self
                    .gpu_texture_bytes
                    .saturating_sub(icon_texture_bytes(replaced.width, replaced.height));
            }
            // Steady state is GPU-only: drop one-shot CPU payload after upload.
            uploads += 1;
        }
        // Free stamped CPU thumbnail staging that is no larger than the
        // resident GPU texture. Keep *larger* CPU sizes so zoom-in can upgrade
        // the size-free content slot instead of staying stuck at the first-open
        // resolution (release used to drop every size for path+mtime).
        let content_resident = frame
            .slots
            .iter()
            .filter_map(|slot| match &slot.identity.identity {
                IconGpuIdentity::Content { path, stamp } => {
                    let size_px = slot
                        .content_width
                        .max(slot.content_height)
                        .min(u32::from(u16::MAX)) as u16;
                    Some(ThumbnailSourceKey::thumbnail(
                        path.clone(),
                        size_px,
                        *stamp,
                    ))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if !content_resident.is_empty() {
            self.thumbnails
                .release_gpu_resident_content_upto(&content_resident);
        }
        self.evict_gpu_textures_if_needed();
        frame.stats.atlas_uploads = uploads;
        frame.stats.atlas_upload_skips = upload_skips;

        self.content_batches = std::mem::take(&mut frame.content_batches);
        self.overlay_batches = std::mem::take(&mut frame.overlay_batches);

        let total_vertices =
            frame.content_vertices.len() + frame.overlay_vertices.len();
        if total_vertices > self.vertex_capacity {
            self.vertex_capacity = total_vertices.next_power_of_two().max(6);
            self.vertex_buffer = create_icon_vertex_buffer(device, self.vertex_capacity);
            self.last_vertices_hash = None;
        }
        self.content_vertex_count = frame.content_vertices.len();
        self.overlay_vertex_start = frame.content_vertices.len();
        self.overlay_vertex_count = frame.overlay_vertices.len();
        let Some(hash) =
            vertex_pair_hash(&frame.content_vertices, &frame.overlay_vertices)
        else {
            self.last_vertices_hash = None;
            return VertexBufferUploadStats::default();
        };
        if self.last_vertices_hash == Some(hash) {
            return VertexBufferUploadStats {
                writes: 0,
                skips: 1,
            };
        }
        let mut vertices = Vec::with_capacity(total_vertices);
        vertices.extend_from_slice(&frame.content_vertices);
        vertices.extend_from_slice(&frame.overlay_vertices);
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        self.last_vertices_hash = Some(hash);
        VertexBufferUploadStats {
            writes: 1,
            skips: 0,
        }
    }

    /// Fill a resident GPU texture from a GPU command or dmabuf import.
    fn upload_slot_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        slot: &mut IconGpuSlot,
    ) -> Option<(
        wgpu::Texture,
        crate::shell::render::dmabuf::ExternalTextureSource,
    )> {
        use crate::shell::render::dmabuf::{ExternalTextureSource, acquire_external_texture};

        let w = slot.width.max(1);
        let h = slot.height.max(1);
        let plane = slot.dmabuf.take().map(|s| s.plane);
        let plan = if self.dmabuf_import_supported {
            self.dmabuf_plan
        } else {
            None
        };
        if let Some(source) = slot.source.take() {
            let texture = create_icon_texture(device, w, h);
            if self.gpu_source_renderer.is_none() {
                self.gpu_source_renderer = GpuIconSourceRenderer::new(device, queue);
            }
            let renderer = self.gpu_source_renderer.as_mut()?;
            return renderer.render(device, queue, &texture, &source).then_some((
                texture,
                ExternalTextureSource::GpuRender,
            ));
        }

        // Optional zero-copy when a producer attached a plane (not required).
        if plane.is_some() && plan.is_some() {
            match acquire_external_texture(
                device,
                plan,
                w,
                h,
                plane,
                Some("fika-icon-dmabuf"),
            ) {
                Ok(ext) => return Some((ext.texture, ext.source)),
                Err(_) => return None,
            }
        } else {
            drop(plane);
        }
        None
    }

    fn evict_gpu_textures_if_needed(&mut self) {
        // Protect both object count and bytes. A count-only cap becomes unsafe
        // when high-zoom previews are 256px (512 textures would approach 128 MiB).
        const MAX_GPU_ICON_TEXTURES: usize = 512;
        const MAX_GPU_ICON_TEXTURE_BYTES: usize = 64 * 1024 * 1024;
        while self.gpu_textures.len() > MAX_GPU_ICON_TEXTURES
            || self.gpu_texture_bytes > MAX_GPU_ICON_TEXTURE_BYTES
        {
            // Skip keys still drawn this frame (would flash empty icons).
            // Previously `break` on the first in-use victim stopped all eviction
            // when the LRU entry happened to still be on-screen.
            let Some(victim) = self
                .gpu_textures
                .iter()
                .filter(|(_, entry)| entry.last_used_frame != self.gpu_frame)
                .min_by_key(|(_, e)| e.last_used_frame)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(removed) = self.gpu_textures.remove(&victim) {
                self.gpu_texture_bytes = self
                    .gpu_texture_bytes
                    .saturating_sub(icon_texture_bytes(removed.width, removed.height));
            }
        }
        if self.gpu_textures.capacity() > self.gpu_textures.len().saturating_mul(2).max(64) {
            self.gpu_textures.shrink_to_fit();
        }
    }

    /// Drop GPU icon textures not referenced by the current frame after a path
    /// change so VRAM does not retain the previous folder's full icon set.
    fn release_unused_gpu_textures(&mut self) {
        let before = self.gpu_textures.len();
        self.gpu_textures
            .retain(|_, entry| entry.last_used_frame == self.gpu_frame);
        self.recount_gpu_texture_bytes();
        if before != self.gpu_textures.len()
            && self.gpu_textures.capacity() > self.gpu_textures.len().saturating_mul(2).max(64)
        {
            self.gpu_textures.shrink_to_fit();
        }
    }

    /// Trim async failure caches and optional path-scoped thumbnail ready data.
    fn release_directory_caches(&mut self, left_path: Option<&Path>) {
        if let Some(path) = left_path {
            self.thumbnails.clear_path_prefix(path);
            // Drop content GPU textures under the left directory (MIME roles stay).
            self.gpu_textures.retain(|key, _| match &key.identity {
                IconGpuIdentity::Content { path: p, .. } => !p.starts_with(path),
                _ => true,
            });
            self.recount_gpu_texture_bytes();
        }
        self.thumbnails.trim_failed(THUMBNAIL_FAILURE_CACHE_MAX_ENTRIES);
    }

    fn gpu_resident_index(&self) -> IconGpuResidentIndex {
        let mut entries = HashMap::with_capacity(self.gpu_textures.len());
        for (key, tex) in &self.gpu_textures {
            entries.insert(
                key.clone(),
                IconGpuResidentEntry {
                    width: tex.width,
                    height: tex.height,
                    content_width: tex.content_width,
                    content_height: tex.content_height,
                    content_hash: tex.content_hash,
                    rounding: tex.rounding,
                },
            );
        }
        IconGpuResidentIndex { entries }
    }

    fn recount_gpu_texture_bytes(&mut self) {
        self.gpu_texture_bytes = self
            .gpu_textures
            .values()
            .map(|texture| icon_texture_bytes(texture.width, texture.height))
            .sum();
    }

    fn draw_batches<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        batches: &'pass [IconSlotBatch],
        vertex_base: u32,
    ) {
        if batches.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        for batch in batches {
            let Some(key) = self.frame_slot_keys.get(batch.slot as usize) else {
                continue;
            };
            let Some(entry) = self.gpu_textures.get(key) else {
                continue;
            };
            pass.set_bind_group(0, &entry.bind_group, &[]);
            let start = vertex_base + batch.vertex_start;
            let end = start + batch.vertex_count;
            pass.draw(start..end, 0..1);
        }
    }

    fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        self.draw_batches(pass, &self.content_batches, 0);
    }

    fn draw_overlay<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        self.draw_batches(
            pass,
            &self.overlay_batches,
            self.overlay_vertex_start as u32,
        );
    }

    fn batch_count(&self) -> usize {
        self.content_batches.len() + self.overlay_batches.len()
    }
}

fn create_icon_vertex_buffer(device: &wgpu::Device, vertex_capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fika-wgpu-icon-vertices"),
        size: (vertex_capacity.max(1) * std::mem::size_of::<IconVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn icon_texture_bytes(width: u32, height: u32) -> usize {
    (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4)
}
#[derive(Clone, Copy, Debug, Default)]
struct TextFrameStats {
    labels: usize,
    quads: usize,
    deferred: usize,
    atlas_reused: usize,
    atlas_uploads: usize,
    atlas_upload_skips: usize,
    atlas_width: u32,
    atlas_height: u32,
    atlas_bytes: usize,
    cache_hits: usize,
    cache_misses: usize,
    cache_entries: usize,
    cache_bytes: usize,
    swash_image_entries: usize,
    swash_outline_entries: usize,
    swash_resets: usize,
    raster_us: u128,
}
impl TextFrameStats {
    fn merged(self, other: Self) -> Self {
        Self {
            labels: self.labels + other.labels,
            quads: self.quads + other.quads,
            deferred: self.deferred + other.deferred,
            atlas_reused: self.atlas_reused + other.atlas_reused,
            atlas_uploads: self.atlas_uploads + other.atlas_uploads,
            atlas_upload_skips: self.atlas_upload_skips + other.atlas_upload_skips,
            atlas_width: self.atlas_width.max(other.atlas_width),
            atlas_height: self.atlas_height.max(other.atlas_height),
            atlas_bytes: self.atlas_bytes + other.atlas_bytes,
            cache_hits: self.cache_hits + other.cache_hits,
            cache_misses: self.cache_misses + other.cache_misses,
            cache_entries: self.cache_entries + other.cache_entries,
            cache_bytes: self.cache_bytes + other.cache_bytes,
            swash_image_entries: self.swash_image_entries.max(other.swash_image_entries),
            swash_outline_entries: self.swash_outline_entries.max(other.swash_outline_entries),
            swash_resets: self.swash_resets + other.swash_resets,
            raster_us: self.raster_us + other.raster_us,
        }
    }
}
struct TextFrame {
    vertices: Vec<TextVertex>,
    pixels: Vec<u8>,
    uploads: Vec<TextAtlasUpload>,
    width: u32,
    height: u32,
    stats: TextFrameStats,
}
const TEXT_ATLAS_GUARD_TEXELS: u32 = 1;
#[derive(Clone, Debug)]
struct PendingTextDraw {
    key: LabelCacheKey,
    pixels: Arc<[u8]>,
    atlas_upload_required: bool,
    screen: ViewRect,
    rect: ViewRect,
    label_width: u32,
    label_height: u32,
    color: TextColor,
}
#[derive(Clone, Debug)]
struct TextAtlasUpload {
    atlas: AtlasRect,
    pixels: Arc<[u8]>,
    width: u32,
    height: u32,
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TextAtlasUploadKey {
    atlas_x: u32,
    atlas_y: u32,
    atlas_width: u32,
    atlas_height: u32,
    upload_width: u32,
    upload_height: u32,
    pixels_hash: u64,
}
impl TextAtlasUploadKey {
    fn from_upload(upload: &TextAtlasUpload) -> Self {
        Self {
            atlas_x: upload.atlas.x as u32,
            atlas_y: upload.atlas.y as u32,
            atlas_width: upload.atlas.width as u32,
            atlas_height: upload.atlas.height as u32,
            upload_width: upload.width,
            upload_height: upload.height,
            pixels_hash: hash_bytes_with_len(upload.pixels.as_ref()),
        }
    }
}
fn text_atlas_max_label_width(atlas_width: u32) -> u32 {
    atlas_width
        .saturating_sub(TEXT_PADDING * 2 + TEXT_ATLAS_GUARD_TEXELS * 2)
        .max(1)
}
fn text_atlas_guarded_extent(extent: u32) -> u32 {
    extent + TEXT_ATLAS_GUARD_TEXELS * 2
}
fn padded_text_atlas_pixels(pixels: Arc<[u8]>, width: u32, height: u32) -> (Arc<[u8]>, u32, u32) {
    if TEXT_ATLAS_GUARD_TEXELS == 0 || width == 0 || height == 0 {
        return (pixels, width, height);
    }

    let guard = TEXT_ATLAS_GUARD_TEXELS;
    let padded_width = text_atlas_guarded_extent(width);
    let padded_height = text_atlas_guarded_extent(height);
    let mut padded = vec![0; (padded_width * padded_height) as usize];
    for y in 0..padded_height {
        let src_y = y.saturating_sub(guard).min(height.saturating_sub(1));
        for x in 0..padded_width {
            let src_x = x.saturating_sub(guard).min(width.saturating_sub(1));
            padded[(y * padded_width + x) as usize] = pixels[(src_y * width + src_x) as usize];
        }
    }

    (padded.into(), padded_width, padded_height)
}
