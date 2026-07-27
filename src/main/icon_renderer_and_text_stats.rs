impl IconRenderer {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
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
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(TEXTURE_SHADER)),
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

        let vertex_capacity = 6;
        let vertex_buffer = create_text_vertex_buffer(device, vertex_capacity);
        Self {
            pipeline,
            bind_group_layout,
            sampler,
            gpu_textures: HashMap::new(),
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
            cpu_uploads: 0,
            resolver: FileIconResolver::new(),
            thumbnails: ThumbnailRasterResolver::new(),
            icon_rasters: IconRasterResolver::new(),
            raster_cache: IconRasterCache::new(ICON_CACHE_MAX_BYTES),
            role_raster_cache: IconRoleRasterCache::new(ICON_ROLE_RASTER_CACHE_MAX_BYTES),
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

    /// Lifetime counters for scheme-C GPU uploads (diagnostics / tests).
    #[cfg_attr(not(test), allow(dead_code))]
    fn icon_upload_source_stats(&self) -> (u64, u64) {
        (self.dmabuf_imports, self.cpu_uploads)
    }

    fn prewarm_common_file_icon_rasters(&mut self, icon_size: f32) -> usize {
        let size_px = icon_cache_size(icon_size);
        let roles = [
            FileIconRoleCacheKey {
                kind: FileIconKind::Directory,
            },
            FileIconRoleCacheKey {
                kind: FileIconKind::File { extension: None },
            },
        ];
        let mut rasterized = 0usize;
        for role in roles {
            let path_key = FileIconPathCacheKey {
                role: role.clone(),
                size_px,
            };
            let snapshot = self.resolver.resolve_path_cache_key_fast(path_key);
            let Some(path) = snapshot.path else {
                continue;
            };
            let key = IconRasterCacheKey::file_icon(path, size_px, &role.kind);
            if let Some(raster) = self.raster_cache.get(&key) {
                self.role_raster_cache.insert(role, raster);
                continue;
            }
            let Some(raster) = rasterize_icon_for_cache_key(&key) else {
                continue;
            };
            let raster = self.raster_cache.insert(key, raster);
            self.role_raster_cache.insert(role, raster);
            rasterized += 1;
        }
        rasterized
    }

    fn prewarm_small_directory_file_icon_rasters(
        &mut self,
        projections: &[ShellPaneProjection<'_>],
    ) -> IconRasterPrewarmStats {
        self.icon_rasters.drain_results(&mut self.raster_cache);
        let deadline = Instant::now() + DOLPHIN_MAX_BLOCK_TIMEOUT;
        let mut stats = IconRasterPrewarmStats::default();
        let mut seen = HashSet::new();
        for projection in projections {
            if projection.view.filtered_entry_count() > DOLPHIN_RESOLVE_ALL_ITEMS_LIMIT {
                continue;
            }
            let Some(icon_size) = projection.visible_items.first().map(|item| {
                item.layout
                    .icon_rect
                    .width
                    .max(item.layout.icon_rect.height)
                    .clamp(16.0, 256.0)
            }) else {
                continue;
            };
            let size_px = icon_cache_size(icon_size);
            for entry_index in projection.view.filtered_indexes.iter().copied() {
                if Instant::now() >= deadline {
                    stats.over_budget = true;
                    return stats;
                }
                let Some(entry) = projection.view.entries.get(entry_index) else {
                    continue;
                };
                let path = projection.view.path.join(entry.name.as_ref());
                let path_key = file_icon_path_cache_key(
                    &path,
                    entry.is_dir,
                    entry.mime_type.clone(),
                    entry.mime_magic_checked,
                    icon_size,
                );
                let role_key = path_key.role.clone();
                let Some(snapshot) = self.resolver.resolve_path_cache_key(path_key) else {
                    continue;
                };
                let Some(icon_path) = snapshot.path else {
                    stats.failed += 1;
                    continue;
                };
                let raster_key =
                    IconRasterCacheKey::file_icon(icon_path, size_px, &role_key.kind);
                if !seen.insert(raster_key.clone()) {
                    continue;
                }
                stats.entries += 1;
                if let Some(raster) = self.raster_cache.get(&raster_key) {
                    stats.cache_hits += 1;
                    self.role_raster_cache.insert(role_key, raster);
                    continue;
                }
                stats.cache_misses += 1;
                let raster_start = Instant::now();
                let Some(raster) = rasterize_icon_for_cache_key(&raster_key) else {
                    stats.raster_us += raster_start.elapsed().as_micros();
                    stats.failed += 1;
                    continue;
                };
                stats.raster_us += raster_start.elapsed().as_micros();
                let raster = self.raster_cache.insert(raster_key, raster);
                self.role_raster_cache.insert(role_key, raster);
            }
        }
        stats
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
                }
                slot.upload = None;
                let _ = slot.dmabuf.take();
                upload_skips += 1;
                continue;
            }

            // Need pixels (or dmabuf) to fill / rewrite the resident texture.
            if slot.upload.is_none() && slot.dmabuf.is_none() {
                // Sample-only draw for an already-resident identity that was
                // not rewritten this frame — still mark used.
                if let Some(entry) = self.gpu_textures.get_mut(&key) {
                    entry.last_used_frame = self.gpu_frame;
                    slot.width = entry.width;
                    slot.height = entry.height;
                    slot.content_width = entry.content_width;
                    slot.content_height = entry.content_height;
                    slot.content_hash = entry.content_hash;
                }
                upload_skips += 1;
                continue;
            }

            let (texture, source) = self.upload_slot_texture(device, queue, slot);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group =
                create_icon_bind_group(device, &self.bind_group_layout, &view, &self.sampler);
            match source {
                crate::shell::render::dmabuf::ExternalTextureSource::DmabufImport => {
                    self.dmabuf_imports = self.dmabuf_imports.saturating_add(1);
                }
                crate::shell::render::dmabuf::ExternalTextureSource::CpuUpload => {
                    self.cpu_uploads = self.cpu_uploads.saturating_add(1);
                }
            }
            self.gpu_textures.insert(
                key,
                IconGpuTexture {
                    texture,
                    bind_group,
                    width: slot.width,
                    height: slot.height,
                    content_width: slot.content_width,
                    content_height: slot.content_height,
                    content_hash: slot.content_hash,
                    last_used_frame: self.gpu_frame,
                    source,
                },
            );
            // Steady state is GPU-only: drop one-shot CPU payload after upload.
            slot.upload = None;
            uploads += 1;
        }
        // Free stamped CPU thumbnail staging once content textures are resident.
        let content_paths = frame
            .slots
            .iter()
            .filter_map(|slot| match &slot.identity.identity {
                IconGpuIdentity::Content { path, stamp } => Some(IconRasterCacheKey::thumbnail(
                    path.clone(),
                    0,
                    *stamp,
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        // release_gpu_resident matches on path+stamp (size ignored for stamped).
        if !content_paths.is_empty() {
            // Drop any stamped CPU entry for these paths regardless of size_px.
            self.raster_cache
                .release_gpu_resident_content(&content_paths);
            self.thumbnails
                .release_gpu_resident_content(&content_paths);
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
            self.vertex_buffer = create_text_vertex_buffer(device, self.vertex_capacity);
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

    /// Fill a resident GPU texture: optional dmabuf import, else `write_texture`.
    ///
    /// This is the only place CPU pixels touch the GPU for icons. Subsequent
    /// frames sample the resident texture with no CPU involvement.
    fn upload_slot_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        slot: &mut IconGpuSlot,
    ) -> (
        wgpu::Texture,
        crate::shell::render::dmabuf::ExternalTextureSource,
    ) {
        use crate::shell::render::dmabuf::{ExternalTextureSource, acquire_external_texture};

        let w = slot.width.max(1);
        let h = slot.height.max(1);
        let plane = slot.dmabuf.take().map(|s| s.plane);
        let plan = if self.dmabuf_import_supported {
            self.dmabuf_plan
        } else {
            None
        };
        let pixels = slot.upload.as_ref().map(|r| r.pixels.as_ref());

        // Optional zero-copy when a producer attached a plane (not required).
        if plane.is_some() && plan.is_some() {
            match acquire_external_texture(
                device,
                queue,
                plan,
                w,
                h,
                plane,
                pixels,
                Some("fika-icon-dmabuf"),
            ) {
                Ok(ext) => return (ext.texture, ext.source),
                Err(_) => {
                    // Fall through to write_texture.
                }
            }
        } else {
            drop(plane);
        }

        let pixels = slot
            .upload
            .as_ref()
            .map(|r| r.pixels.as_ref())
            .unwrap_or(&[]);
        let texture = create_icon_texture(device, w, h);
        if !pixels.is_empty() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
        (texture, ExternalTextureSource::CpuUpload)
    }

    fn evict_gpu_textures_if_needed(&mut self) {
        // Soft cap: keep GPU icon textures roughly in line with CPU raster cache budget.
        const MAX_GPU_ICON_TEXTURES: usize = 512;
        while self.gpu_textures.len() > MAX_GPU_ICON_TEXTURES {
            // Skip keys still drawn this frame (would flash empty icons).
            // Previously `break` on the first in-use victim stopped all eviction
            // when the LRU entry happened to still be on-screen.
            let Some(victim) = self
                .gpu_textures
                .iter()
                .filter(|(key, _)| !self.frame_slot_keys.contains(key))
                .min_by_key(|(_, e)| e.last_used_frame)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            self.gpu_textures.remove(&victim);
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
            .retain(|key, _| self.frame_slot_keys.contains(key));
        if before != self.gpu_textures.len()
            && self.gpu_textures.capacity() > self.gpu_textures.len().saturating_mul(2).max(64)
        {
            self.gpu_textures.shrink_to_fit();
        }
    }

    /// Trim async failure caches and optional path-scoped thumbnail ready data.
    fn release_directory_caches(&mut self, left_path: Option<&Path>) {
        self.icon_rasters.clear_failed();
        self.icon_rasters.trim_failed(THUMBNAIL_FAILURE_CACHE_MAX_ENTRIES);
        if let Some(path) = left_path {
            self.thumbnails.clear_path_prefix(path);
            // Drop content GPU textures under the left directory (MIME roles stay).
            self.gpu_textures.retain(|key, _| match &key.identity {
                IconGpuIdentity::Content { path: p, .. } => !p.starts_with(path),
                _ => true,
            });
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
                },
            );
        }
        IconGpuResidentIndex { entries }
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
