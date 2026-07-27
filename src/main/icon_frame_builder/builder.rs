struct IconFrameResources<'a> {
    resolver: &'a mut FileIconResolver,
    thumbnails: &'a mut ThumbnailRasterResolver,
    icon_rasters: &'a mut IconRasterResolver,
    raster_cache: &'a mut IconRasterCache,
    role_raster_cache: &'a mut IconRoleRasterCache,
    /// Resident GPU icon sizes at frame start (size-free identity → sample).
    gpu_resident: IconGpuResidentIndex,
}

impl<'a> IconFrameResources<'a> {
    fn new(
        resolver: &'a mut FileIconResolver,
        thumbnails: &'a mut ThumbnailRasterResolver,
        icon_rasters: &'a mut IconRasterResolver,
        raster_cache: &'a mut IconRasterCache,
        role_raster_cache: &'a mut IconRoleRasterCache,
        gpu_resident: IconGpuResidentIndex,
    ) -> Self {
        Self {
            resolver,
            thumbnails,
            icon_rasters,
            raster_cache,
            role_raster_cache,
            gpu_resident,
        }
    }

    fn from_renderer(renderer: &'a mut IconRenderer) -> Self {
        let gpu_resident = renderer.gpu_resident_index();
        Self::new(
            &mut renderer.resolver,
            &mut renderer.thumbnails,
            &mut renderer.icon_rasters,
            &mut renderer.raster_cache,
            &mut renderer.role_raster_cache,
            gpu_resident,
        )
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct FolderPreviewCacheStats {
    ready_entries: usize,
    ready_bytes: usize,
}

#[derive(Clone, Copy, Debug)]
struct IconFrameConfig {
    surface_size: PhysicalSize<u32>,
    ui_scale: f32,
    raster_miss_budget: usize,
    folder_preview_cache: FolderPreviewCacheStats,
}

impl IconFrameConfig {
    #[cfg(test)]
    fn new(surface_size: PhysicalSize<u32>, ui_scale: f32, raster_miss_budget: usize) -> Self {
        Self {
            surface_size,
            ui_scale,
            raster_miss_budget,
            folder_preview_cache: FolderPreviewCacheStats::default(),
        }
    }
}

impl<'a> IconFrameBuilder<'a> {
    #[cfg(test)]
    fn new_for_test(
        resolver: &'a mut FileIconResolver,
        thumbnails: &'a mut ThumbnailRasterResolver,
        icon_rasters: &'a mut IconRasterResolver,
        raster_cache: &'a mut IconRasterCache,
        role_raster_cache: &'a mut IconRoleRasterCache,
        surface_size: PhysicalSize<u32>,
    ) -> Self {
        Self::new(
            IconFrameResources::new(
                resolver,
                thumbnails,
                icon_rasters,
                raster_cache,
                role_raster_cache,
                IconGpuResidentIndex::default(),
            ),
            IconFrameConfig::new(surface_size, 1.0, 0),
        )
    }

    fn new(resources: IconFrameResources<'a>, config: IconFrameConfig) -> Self {
        let IconFrameResources {
            resolver,
            thumbnails,
            icon_rasters,
            raster_cache,
            role_raster_cache,
            gpu_resident,
        } = resources;
        let IconFrameConfig {
            surface_size,
            ui_scale,
            raster_miss_budget,
            folder_preview_cache,
        } = config;
        icon_rasters.drain_results(raster_cache);
        Self {
            resolver,
            thumbnails,
            icon_rasters,
            raster_cache,
            role_raster_cache,
            gpu_resident,
            surface_size,
            ui_scale: ui_scale.clamp(1.0, 2.0),
            slot_by_identity: HashMap::new(),
            slots: Vec::with_capacity(64),
            draws: Vec::with_capacity(64),
            overlay_draws: Vec::with_capacity(16),
            icons: 0,
            fallbacks: 0,
            thumbnails_loaded: 0,
            thumbnail_quads: 0,
            thumbnail_deferred: 0,
            thumbnail_read_ahead_queued: 0,
            folder_previews_loaded: 0,
            folder_preview_quads: 0,
            folder_preview_deferred: 0,
            folder_preview_read_ahead_queued: 0,
            folder_preview_ready_entries: folder_preview_cache.ready_entries,
            folder_preview_ready_bytes: folder_preview_cache.ready_bytes,
            cache_hits: 0,
            cache_misses: 0,
            deferred: 0,
            raster_deferred: 0,
            raster_miss_budget,
            resolve_us: 0,
            raster_us: 0,
        }
    }

    fn push_icon(
        &mut self,
        directory: &Path,
        entry: &Entry,
        rect: ViewRect,
        clip: ViewRect,
        layer: IconDrawLayer,
    ) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            self.fallbacks += 1;
            return false;
        }
        let Some(screen) = intersect_rect(rect, clip) else {
            return true;
        };

        self.icons += 1;
        let resolve_start = Instant::now();
        let icon_size = rect.width.max(rect.height).clamp(16.0, 256.0);
        let path = directory.join(entry.name.as_ref());
        let role_key = file_icon_path_cache_key(
            &path,
            entry.is_dir,
            entry.mime_type.clone(),
            entry.mime_magic_checked,
            icon_size,
        )
        .role;
        let (snapshot, deferred) = self
            .resolver
            .resolve_entry_visible(directory, entry, icon_size);
        if deferred {
            self.deferred += 1;
        }
        self.resolve_us += resolve_start.elapsed().as_micros();

        let size_px = icon_cache_size(icon_size);
        // MIME / directory / generic icons share one GPU slot per *role*, not
        // per filesystem path — thousands of /bin entries must not thrash VRAM.
        let gpu_key = IconGpuUploadKey::role(role_key.kind.clone());
        let Some(theme_path) = snapshot.path else {
            if self.raster_miss_budget == 0
                && let Some(raster) = self.role_raster_cache.get(&role_key)
            {
                self.cache_hits += 1;
                self.copy_raster_to_atlas(gpu_key, raster, rect, screen, layer);
                return true;
            }
            self.fallbacks += 1;
            return false;
        };
        let key = IconRasterCacheKey::file_icon(theme_path, size_px, &role_key.kind);
        let raster = if let Some(raster) = self.raster_cache.get(&key) {
            self.cache_hits += 1;
            raster
        } else if let Some(raster) = self.raster_cache.get_closest_icon_variant(&key) {
            self.cache_hits += 1;
            self.icon_rasters.queue_visible(key.clone());
            raster
        } else if self.raster_miss_budget == 0 {
            self.icon_rasters.queue_visible(key.clone());
            if let Some(raster) = self.role_raster_cache.get(&role_key) {
                self.cache_hits += 1;
                self.raster_deferred += 1;
                raster
            } else if let Some(raster) = self.role_raster_cache.get(
                &visible_icon_fallback_key(&FileIconPathCacheKey {
                    role: role_key.clone(),
                    size_px,
                })
                .role,
            ) {
                self.cache_hits += 1;
                self.raster_deferred += 1;
                raster
            } else {
                self.raster_deferred += 1;
                self.fallbacks += 1;
                return false;
            }
        } else {
            self.cache_misses += 1;
            self.raster_miss_budget -= 1;
            let raster_start = Instant::now();
            let Some(raster) = rasterize_icon_for_cache_key(&key) else {
                self.raster_us += raster_start.elapsed().as_micros();
                self.fallbacks += 1;
                return false;
            };
            let raster_us = raster_start.elapsed().as_micros();
            if raster_us >= 2_000 {
                fika_log!(
                    "[fika-wgpu] icon-raster-slow path={} size={} elapsed={}us",
                    key.path.display(),
                    size_px,
                    raster_us
                );
            }
            self.raster_us += raster_us;
            self.raster_cache.insert(key, raster)
        };

        self.role_raster_cache.insert(role_key, raster.clone());
        self.copy_raster_to_atlas(gpu_key, raster, rect, screen, layer);
        true
    }

    fn push_thumbnail_or_icon(
        &mut self,
        directory: &Path,
        entry: &Entry,
        folder_preview: Option<&FolderPreviewReady>,
        pixmap_layout: ItemPixmapLayout,
        clip: ViewRect,
    ) -> bool {
        self.push_thumbnail_or_icon_on_layer(
            directory,
            entry,
            folder_preview,
            pixmap_layout,
            clip,
            IconDrawLayer::Content,
        )
    }

    fn push_thumbnail_or_icon_on_layer(
        &mut self,
        directory: &Path,
        entry: &Entry,
        folder_preview: Option<&FolderPreviewReady>,
        pixmap_layout: ItemPixmapLayout,
        clip: ViewRect,
        layer: IconDrawLayer,
    ) -> bool {
        let drew = if entry.is_dir {
            self.push_folder_preview_or_icon(
                directory,
                entry,
                folder_preview,
                pixmap_layout,
                clip,
                layer,
            )
        } else if self.push_thumbnail(directory, entry, pixmap_layout.icon_rect, clip, layer) {
            true
        } else {
            self.push_icon(directory, entry, pixmap_layout.icon_rect, clip, layer)
        };
        if drew {
            self.push_entry_icon_emblems(directory, entry, pixmap_layout.icon_rect, clip, layer);
        }
        drew
    }

    fn push_folder_preview_or_icon(
        &mut self,
        directory: &Path,
        entry: &Entry,
        folder_preview: Option<&FolderPreviewReady>,
        pixmap_layout: ItemPixmapLayout,
        clip: ViewRect,
        layer: IconDrawLayer,
    ) -> bool {
        let path = entry_path_for_thumbnail(directory, entry);
        let Some(_modified_secs) = entry.modified_secs else {
            return self.push_icon(directory, entry, pixmap_layout.icon_rect, clip, layer);
        };
        if !entry.metadata_complete || is_network_path(&path) {
            return self.push_icon(directory, entry, pixmap_layout.icon_rect, clip, layer);
        }
        let drew_folder_shell =
            self.push_icon(directory, entry, pixmap_layout.icon_rect, clip, layer);
        let Some(preview) = folder_preview else {
            self.folder_preview_deferred += 1;
            return drew_folder_shell;
        };
        let preview_rect = folder_preview_role_draw_rect(pixmap_layout, &preview.raster);
        let Some(screen) = intersect_rect(preview_rect, clip) else {
            return drew_folder_shell;
        };
        let size_px = preview.size_px;
        let key = IconRasterCacheKey::folder_preview(path.clone(), size_px, preview.stamp);
        let gpu_key = IconGpuUploadKey::content(path, preview.stamp);
        let raster = if let Some(raster) = self.raster_cache.get(&key) {
            self.cache_hits += 1;
            raster
        } else if let Some(raster) = self.raster_cache.get_closest_stamped_variant(&key) {
            self.cache_hits += 1;
            raster
        } else {
            self.cache_misses += 1;
            self.folder_previews_loaded += 1;
            self.raster_cache.insert(key, preview.raster.clone())
        };
        self.copy_raster_to_atlas(gpu_key, raster, preview_rect, screen, layer);
        self.folder_preview_quads += 1;
        drew_folder_shell
    }

    fn push_thumbnail(
        &mut self,
        directory: &Path,
        entry: &Entry,
        rect: ViewRect,
        clip: ViewRect,
        layer: IconDrawLayer,
    ) -> bool {
        // Dolphin still shows previews in Details / Compact at SizeSmall (16px).
        // Only skip below the smallest icon-cache bucket so list mode does not
        // fall back to MIME icons while read-ahead already has a thumbnail.
        if rect.width.max(rect.height) < 16.0 {
            return false;
        }
        let path = entry_path_for_thumbnail(directory, entry);
        let Some(modified_secs) = entry.modified_secs else {
            return false;
        };
        if !entry.metadata_complete
            || is_network_path(&path)
            || mime_magic_resolution_required(
                entry.is_dir,
                entry.size_bytes,
                entry.mime_type.as_deref(),
                entry.mime_magic_checked,
            )
            || !thumbnail_request_may_have_preview(&path, entry.mime_type.as_deref())
        {
            return false;
        }
        let Some(screen) = intersect_rect(rect, clip) else {
            return true;
        };
        let size_px = icon_cache_size(rect.width.max(rect.height).clamp(16.0, 256.0));
        let key = IconRasterCacheKey::thumbnail(path.clone(), size_px, modified_secs);
        // Content previews are path+mtime only — zoom reuses the same GPU texture.
        let gpu_key = IconGpuUploadKey::content(path.clone(), modified_secs);
        let raster = if let Some(raster) = self.raster_cache.get(&key) {
            self.cache_hits += 1;
            raster
        } else if let Some(raster) = self.raster_cache.get_closest_stamped_variant(&key) {
            self.cache_hits += 1;
            let _ = self.thumbnails.resolve(
                &path,
                modified_secs,
                entry
                    .mime_type
                    .as_deref()
                    .map(std::borrow::ToOwned::to_owned),
                size_px,
            );
            raster
        } else {
            match self.thumbnails.resolve(
                &path,
                modified_secs,
                entry
                    .mime_type
                    .as_deref()
                    .map(std::borrow::ToOwned::to_owned),
                size_px,
            ) {
                ThumbnailResolveState::Ready(raster) => {
                    self.cache_misses += 1;
                    self.thumbnails_loaded += 1;
                    self.raster_cache.insert(key, raster)
                }
                ThumbnailResolveState::Pending => {
                    self.thumbnail_deferred += 1;
                    return false;
                }
                ThumbnailResolveState::Failed => return false,
            }
        };
        self.copy_raster_to_atlas(gpu_key, raster, rect, screen, layer);
        self.thumbnail_quads += 1;
        true
    }

    fn push_named_theme_icon(
        &mut self,
        icon_name: &str,
        fallback: NamedIconFallback,
        rect: ViewRect,
        clip: ViewRect,
        layer: IconDrawLayer,
    ) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            self.fallbacks += 1;
            return false;
        }
        let Some(screen) = intersect_rect(rect, clip) else {
            return true;
        };
        self.icons += 1;
        let resolve_start = Instant::now();
        let icon_size = rect.width.max(rect.height).clamp(16.0, 256.0);
        let snapshot = if self.raster_miss_budget > 0 {
            self.resolver
                .resolve_named_fast(icon_name, fallback, icon_size)
        } else {
            self.resolver.resolve_named(icon_name, fallback, icon_size)
        };
        let Some(snapshot) = snapshot else {
            self.resolve_us += resolve_start.elapsed().as_micros();
            self.deferred += 1;
            self.fallbacks += 1;
            return false;
        };
        self.resolve_us += resolve_start.elapsed().as_micros();

        let Some(path) = snapshot.path else {
            self.fallbacks += 1;
            return false;
        };
        let size_px = icon_cache_size(icon_size);
        let gpu_key = IconGpuUploadKey::theme_asset(path.clone());
        let key = IconRasterCacheKey::icon(path, size_px);
        let raster = if let Some(raster) = self.raster_cache.get(&key) {
            self.cache_hits += 1;
            raster
        } else if let Some(raster) = self.raster_cache.get_closest_icon_variant(&key) {
            self.cache_hits += 1;
            raster
        } else {
            self.cache_misses += 1;
            if self.raster_miss_budget == 0 {
                self.icon_rasters.queue_visible(key.clone());
                self.raster_deferred += 1;
                self.fallbacks += 1;
                return false;
            }
            self.raster_miss_budget -= 1;
            let raster_start = Instant::now();
            let Some(raster) = rasterize_icon_for_cache_key(&key) else {
                self.raster_us += raster_start.elapsed().as_micros();
                self.fallbacks += 1;
                return false;
            };
            let raster_us = raster_start.elapsed().as_micros();
            if raster_us >= 2_000 {
                fika_log!(
                    "[fika-wgpu] icon-raster-slow path={} size={} elapsed={}us",
                    key.path.display(),
                    size_px,
                    raster_us
                );
            }
            self.raster_us += raster_us;
            self.raster_cache.insert(key, raster)
        };

        self.copy_raster_to_atlas(gpu_key, raster, rect, screen, layer);
        true
    }

    fn push_named_theme_icon_exact(
        &mut self,
        icon_name: &str,
        rect: ViewRect,
        clip: ViewRect,
        layer: IconDrawLayer,
    ) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return false;
        }
        let Some(screen) = intersect_rect(rect, clip) else {
            return true;
        };
        let icon_name = icon_name.trim();
        if icon_name.is_empty() {
            return false;
        }
        let icon_size = rect.width.max(rect.height).clamp(16.0, 256.0 * self.ui_scale);
        let size_px = icon_cache_size(icon_size);
        let Some(path) = self.resolver.resolve_named_exact_fast(icon_name, icon_size) else {
            return false;
        };
        let gpu_key = IconGpuUploadKey::theme_asset(path.clone());
        let key = IconRasterCacheKey::icon(path, size_px);
        let raster = if let Some(raster) = self.raster_cache.get(&key) {
            self.cache_hits += 1;
            raster
        } else {
            self.cache_misses += 1;
            let Some(raster) = rasterize_icon_for_cache_key(&key) else {
                return false;
            };
            self.raster_cache.insert(key, raster)
        };
        self.copy_raster_to_atlas(gpu_key, raster, rect, screen, layer);
        true
    }

    fn push_entry_icon_emblems(
        &mut self,
        directory: &Path,
        entry: &Entry,
        icon_rect: ViewRect,
        clip: ViewRect,
        layer: IconDrawLayer,
    ) {
        let path = directory.join(entry.name.as_ref());
        let emblems = icon_emblem_kinds_for_path(&path);
        if emblems.is_empty() {
            return;
        }
        let rects = icon_emblem_rects(icon_rect, self.ui_scale);
        for (index, emblem) in emblems.into_iter().take(rects.len()).enumerate() {
            for icon_name in emblem.theme_names() {
                if self.push_named_theme_icon_exact(icon_name, rects[index], clip, layer) {
                    break;
                }
            }
        }
    }

    fn queue_thumbnail_read_ahead(&mut self, candidate: ShellThumbnailCandidate, size_px: u16) {
        let key =
            IconRasterCacheKey::thumbnail(candidate.path.clone(), size_px, candidate.modified_secs);
        if self.raster_cache.contains(&key) {
            return;
        }
        if self.thumbnails.queue_deferred(
            &candidate.path,
            candidate.modified_secs,
            candidate.mime_type,
            size_px,
        ) {
            self.thumbnail_read_ahead_queued += 1;
        }
    }

    fn copy_raster_to_atlas(
        &mut self,
        identity: IconGpuUploadKey,
        raster: IconRaster,
        rect: ViewRect,
        screen: ViewRect,
        layer: IconDrawLayer,
    ) {
        self.push_raster_draw(identity, Some(raster), rect, screen, layer, None);
    }

    /// Attach a dmabuf plane to a logical GPU slot (optional zero-copy producer).
    #[cfg_attr(not(test), allow(dead_code))]
    fn push_raster_with_dmabuf(
        &mut self,
        identity: IconGpuUploadKey,
        raster: IconRaster,
        rect: ViewRect,
        screen: ViewRect,
        layer: IconDrawLayer,
        plane: crate::shell::render::dmabuf::DmabufImportPlane,
    ) {
        self.push_raster_draw(
            identity,
            Some(raster),
            rect,
            screen,
            layer,
            Some(IconDmabufSource { plane }),
        );
    }

    fn push_raster_draw(
        &mut self,
        identity: IconGpuUploadKey,
        raster: Option<IconRaster>,
        rect: ViewRect,
        screen: ViewRect,
        layer: IconDrawLayer,
        dmabuf: Option<IconDmabufSource>,
    ) {
        // Size-free GPU identity: MIME roles and theme assets share one texture
        // across all zoom levels; only content previews are path-bound.
        let resident = self.gpu_resident.get(&identity);
        let slot = if let Some(&slot) = self.slot_by_identity.get(&identity) {
            // Prefer a larger upload if this draw has a higher-res raster.
            if let Some(raster) = raster {
                let content_w = raster.width.max(1);
                let content_h = raster.height.max(1);
                let padded = padded_icon_atlas_raster(&raster);
                let content_hash = hash_bytes_with_len(padded.pixels.as_ref());
                let existing = &mut self.slots[slot as usize];
                let existing_area = existing.content_width.saturating_mul(existing.content_height);
                let new_area = content_w.saturating_mul(content_h);
                if new_area > existing_area
                    || (new_area == existing_area && content_hash != existing.content_hash)
                {
                    existing.width = padded.width;
                    existing.height = padded.height;
                    existing.content_width = content_w;
                    existing.content_height = content_h;
                    existing.content_hash = content_hash;
                    existing.upload = Some(padded);
                    if dmabuf.is_some() {
                        existing.dmabuf = dmabuf;
                    }
                } else {
                    drop(dmabuf);
                }
            } else {
                drop(dmabuf);
            }
            slot
        } else if let Some(entry) = resident {
            // Already on GPU — sample only (no CPU path this frame) unless we
            // have a strictly larger raster to upgrade the resident texture.
            let slot = self.slots.len() as u32;
            let (width, height, content_width, content_height, content_hash, upload) =
                if let Some(raster) = raster {
                    let content_w = raster.width.max(1);
                    let content_h = raster.height.max(1);
                    let new_area = content_w.saturating_mul(content_h);
                    let old_area = entry.content_width.saturating_mul(entry.content_height);
                    if new_area > old_area {
                        let padded = padded_icon_atlas_raster(&raster);
                        let content_hash = hash_bytes_with_len(padded.pixels.as_ref());
                        (
                            padded.width,
                            padded.height,
                            content_w,
                            content_h,
                            content_hash,
                            Some(padded),
                        )
                    } else {
                        (
                            entry.width,
                            entry.height,
                            entry.content_width,
                            entry.content_height,
                            entry.content_hash,
                            None,
                        )
                    }
                } else {
                    (
                        entry.width,
                        entry.height,
                        entry.content_width,
                        entry.content_height,
                        entry.content_hash,
                        None,
                    )
                };
            self.slots.push(IconGpuSlot {
                identity: identity.clone(),
                width,
                height,
                content_width,
                content_height,
                content_hash,
                upload,
                dmabuf,
            });
            self.slot_by_identity.insert(identity, slot);
            slot
        } else {
            let Some(raster) = raster else {
                drop(dmabuf);
                return;
            };
            let content_w = raster.width.max(1);
            let content_h = raster.height.max(1);
            let padded = padded_icon_atlas_raster(&raster);
            let content_hash = hash_bytes_with_len(padded.pixels.as_ref());
            let slot = self.slots.len() as u32;
            self.slots.push(IconGpuSlot {
                identity: identity.clone(),
                width: padded.width,
                height: padded.height,
                content_width: content_w,
                content_height: content_h,
                content_hash,
                upload: Some(padded),
                dmabuf,
            });
            self.slot_by_identity.insert(identity, slot);
            slot
        };

        let gpu_slot = &self.slots[slot as usize];
        let content_w = gpu_slot.content_width.max(1) as f32;
        let content_h = gpu_slot.content_height.max(1) as f32;
        let guard = ICON_ATLAS_GUARD_TEXELS as f32;
        // Sample the full content rect of the resident texture into the screen
        // icon box — zoom changes `rect` only, never the GPU identity.
        let scale_x = content_w / rect.width.max(1.0);
        let scale_y = content_h / rect.height.max(1.0);
        let source = ViewRect {
            x: guard + (screen.x - rect.x).max(0.0) * scale_x,
            y: guard + (screen.y - rect.y).max(0.0) * scale_y,
            width: screen.width * scale_x,
            height: screen.height * scale_y,
        };
        let draw = IconDraw {
            screen,
            slot,
            source,
            alpha: 1.0,
        };
        match layer {
            IconDrawLayer::Content => self.draws.push(draw),
            IconDrawLayer::Overlay => self.overlay_draws.push(draw),
        }
    }

    fn finish(self) -> IconFrame {
        let (content_vertices, content_batches) =
            pack_icon_batches(&self.draws, &self.slots, self.surface_size);
        let (overlay_vertices, overlay_batches) =
            pack_icon_batches(&self.overlay_draws, &self.slots, self.surface_size);
        let (content_hash, geometry_hash, vertex_hash, slot_hash) = if fika_log_enabled() {
            (
                icon_draw_content_hash(&self.draws, &self.overlay_draws, &self.slots),
                icon_draw_geometry_hash(&self.draws, &self.overlay_draws),
                vertex_pair_hash(&content_vertices, &overlay_vertices).unwrap_or_default(),
                icon_slot_hash(&self.slots),
            )
        } else {
            (0, 0, 0, 0)
        };
        let cache_entries = self.raster_cache.len();
        let cache_bytes = self.raster_cache.bytes();
        let thumbnail_ready_entries = self.thumbnails.ready_len();
        let thumbnail_ready_bytes = self.thumbnails.ready_bytes();
        let folder_preview_ready_entries = self.folder_preview_ready_entries;
        let folder_preview_ready_bytes = self.folder_preview_ready_bytes;
        // Stats: "atlas_*" fields mean unique logical GPU icon slots this frame.
        let slot_bytes: usize = self
            .slots
            .iter()
            .map(|s| s.upload.as_ref().map(|r| r.pixels.len()).unwrap_or(0))
            .sum();
        let max_w = self.slots.iter().map(|s| s.width).max().unwrap_or(0);
        let max_h = self.slots.iter().map(|s| s.height).max().unwrap_or(0);
        let atlas_uploads = self.slots.iter().filter(|s| s.upload.is_some()).count();
        IconFrame {
            slots: self.slots,
            content_batches,
            overlay_batches,
            content_vertices,
            overlay_vertices,
            stats: IconFrameStats {
                icons: self.icons,
                quads: self.draws.len() + self.overlay_draws.len(),
                fallbacks: self.fallbacks,
                deferred: self.deferred,
                thumbnails: self.thumbnails_loaded,
                thumbnail_quads: self.thumbnail_quads,
                thumbnail_deferred: self.thumbnail_deferred,
                thumbnail_read_ahead_queued: self.thumbnail_read_ahead_queued,
                thumbnail_ready_entries,
                thumbnail_ready_bytes,
                folder_previews: self.folder_previews_loaded,
                folder_preview_quads: self.folder_preview_quads,
                folder_preview_deferred: self.folder_preview_deferred,
                folder_preview_read_ahead_queued: self.folder_preview_read_ahead_queued,
                folder_preview_ready_entries,
                folder_preview_ready_bytes,
                atlas_uploads,
                atlas_upload_skips: 0,
                atlas_width: max_w,
                atlas_height: max_h,
                atlas_bytes: slot_bytes,
                cache_hits: self.cache_hits,
                cache_misses: self.cache_misses,
                raster_deferred: self.raster_deferred,
                cache_entries,
                cache_bytes,
                content_hash,
                geometry_hash,
                vertex_hash,
                slot_hash,
                resolve_us: self.resolve_us,
                raster_us: self.raster_us,
            },
        }
    }
}


