struct IconFrameResources<'a> {
    resolver: &'a mut FileIconResolver,
    thumbnails: &'a mut ThumbnailSourceResolver,
    /// Resident GPU icon sizes at frame start (size-free identity → sample).
    gpu_resident: IconGpuResidentIndex,
}

impl<'a> IconFrameResources<'a> {
    fn new(
        resolver: &'a mut FileIconResolver,
        thumbnails: &'a mut ThumbnailSourceResolver,
        gpu_resident: IconGpuResidentIndex,
    ) -> Self {
        Self {
            resolver,
            thumbnails,
            gpu_resident,
        }
    }

    fn from_renderer(renderer: &'a mut IconRenderer) -> Self {
        let gpu_resident = renderer.gpu_resident_index();
        Self::from_engine(&mut renderer.engine, gpu_resident)
    }

    fn from_engine(engine: &'a mut IconEngine, gpu_resident: IconGpuResidentIndex) -> Self {
        Self::new(&mut engine.resolver, &mut engine.thumbnails, gpu_resident)
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
    sync_resolve_budget: usize,
    folder_preview_cache: FolderPreviewCacheStats,
}

impl IconFrameConfig {
    #[cfg(test)]
    fn new(surface_size: PhysicalSize<u32>, ui_scale: f32, sync_resolve_budget: usize) -> Self {
        Self {
            surface_size,
            ui_scale,
            sync_resolve_budget,
            folder_preview_cache: FolderPreviewCacheStats::default(),
        }
    }
}

#[cfg(test)]
struct IconDmabufDraw {
    identity: IconGpuUploadKey,
    width: u32,
    height: u32,
    content_hash: u64,
    rect: ViewRect,
    screen: ViewRect,
    layer: IconDrawLayer,
    plane: crate::ui::render::dmabuf::DmabufImportPlane,
}

impl<'a> IconFrameBuilder<'a> {
    #[cfg(test)]
    fn new_for_test(
        resolver: &'a mut FileIconResolver,
        thumbnails: &'a mut ThumbnailSourceResolver,
        surface_size: PhysicalSize<u32>,
    ) -> Self {
        Self::new(
            IconFrameResources::new(
                resolver,
                thumbnails,
                IconGpuResidentIndex::default(),
            ),
            IconFrameConfig::new(surface_size, 1.0, 0),
        )
    }

    fn new(resources: IconFrameResources<'a>, config: IconFrameConfig) -> Self {
        let IconFrameResources {
            resolver,
            thumbnails,
            gpu_resident,
        } = resources;
        let IconFrameConfig {
            surface_size,
            ui_scale,
            sync_resolve_budget,
            folder_preview_cache,
        } = config;
        Self {
            resolver,
            thumbnails,
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
            sync_resolve_budget,
            resolve_us: 0,
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
        let path_key = file_icon_path_cache_key_with_stamp(
            &path,
            entry.is_dir,
            entry.mime_type.clone(),
            entry.mime_magic_checked,
            entry.modified_secs,
            icon_size,
        );
        let role_key = path_key.role.clone();
        let (snapshot, deferred) = self
            .resolver
            .resolve_path_cache_key_visible(path_key);
        if deferred {
            self.deferred += 1;
        }
        self.resolve_us += resolve_start.elapsed().as_micros();

        let size_px = icon_cache_size(icon_size);
        // MIME / directory / generic icons share one GPU slot per *role*, not
        // per filesystem path — thousands of /bin entries must not thrash VRAM.
        let gpu_key = IconGpuUploadKey::role(role_key.kind.clone());
        let Some(theme_path) = snapshot.path else {
            self.fallbacks += 1;
            return false;
        };
        self.push_gpu_source_draw(
            gpu_key,
            IconGpuSource::file(theme_path, size_px),
            rect,
            screen,
            layer,
        );
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
        let preview_rect = folder_preview_gpu_draw_rect(pixmap_layout, preview.size_px);
        let Some(screen) = intersect_rect(preview_rect, clip) else {
            return drew_folder_shell;
        };
        let size_px = preview.size_px;
        let gpu_key = IconGpuUploadKey::content(path, preview.stamp);
        if let Some(resident) = self.gpu_resident.get(&gpu_key) {
            let resident_px = resident.content_width.max(resident.content_height) as u16;
            if resident_px >= size_px {
                self.cache_hits += 1;
                self.push_resident_draw(gpu_key, preview_rect, screen, layer);
                self.folder_preview_quads += 1;
                return drew_folder_shell;
            }
        }
        self.folder_previews_loaded += 1;
        self.push_gpu_source_draw(
            gpu_key,
            preview.source.clone(),
            preview_rect,
            screen,
            layer,
        );
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
        // FileManager still shows previews in Details / Compact at SizeSmall (16px).
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
        // Generate a freestanding encoded thumbnail at least as large as the
        // on-screen icon. Scaling and placement happen in the GPU target.
        let display_px = rect.width.max(rect.height).clamp(16.0, 256.0);
        let size_px = thumbnail_display_cache_size(display_px);
        // Content previews are path+mtime only — zoom reuses the same GPU texture.
        let gpu_key = IconGpuUploadKey::content(path.clone(), modified_secs);
        if let Some(resident) = self.gpu_resident.get(&gpu_key) {
            let resident_px = resident.content_width.max(resident.content_height) as u16;
            if resident_px >= size_px {
                self.cache_hits += 1;
                self.push_resident_draw(gpu_key, rect, screen, layer);
                self.thumbnail_quads += 1;
                return true;
            }
        }
        match self.thumbnails.resolve(
            &path,
            modified_secs,
            entry
                .mime_type
                .as_deref()
                .map(std::borrow::ToOwned::to_owned),
            size_px,
        ) {
            ThumbnailResolveState::Ready(source) => {
                self.thumbnails_loaded += 1;
                self.push_gpu_source_draw(gpu_key, source, rect, screen, layer);
                self.thumbnail_quads += 1;
                true
            }
            ThumbnailResolveState::Pending => {
                self.thumbnail_deferred += 1;
                if self.gpu_resident.get(&gpu_key).is_some() {
                    self.push_resident_draw(gpu_key, rect, screen, layer);
                    self.thumbnail_quads += 1;
                    true
                } else {
                    false
                }
            }
            ThumbnailResolveState::Failed => {
                if self.gpu_resident.get(&gpu_key).is_some() {
                    self.push_resident_draw(gpu_key, rect, screen, layer);
                    self.thumbnail_quads += 1;
                    true
                } else {
                    false
                }
            }
        }
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
        let snapshot = if self.sync_resolve_budget > 0 {
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
        self.push_gpu_source_draw(
            gpu_key,
            IconGpuSource::file(path, size_px),
            rect,
            screen,
            layer,
        );
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
        self.push_gpu_source_draw(
            gpu_key,
            IconGpuSource::file(path, size_px),
            rect,
            screen,
            layer,
        );
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
        if self.thumbnails.queue_deferred(
            &candidate.path,
            candidate.modified_secs,
            candidate.mime_type,
            size_px,
        ) {
            self.thumbnail_read_ahead_queued += 1;
        }
    }

    fn push_resident_draw(
        &mut self,
        identity: IconGpuUploadKey,
        rect: ViewRect,
        screen: ViewRect,
        layer: IconDrawLayer,
    ) {
        let slot = if let Some(&slot) = self.slot_by_identity.get(&identity) {
            slot
        } else {
            let Some(resident) = self.gpu_resident.get(&identity) else {
                return;
            };
            let slot = self.slots.len() as u32;
            self.slots.push(IconGpuSlot {
                identity: identity.clone(),
                width: resident.width,
                height: resident.height,
                content_width: resident.content_width,
                content_height: resident.content_height,
                content_hash: resident.content_hash,
                rounding: resident.rounding,
                source: None,
                dmabuf: None,
            });
            self.slot_by_identity.insert(identity, slot);
            slot
        };
        self.push_slot_draw(slot, rect, screen, layer);
    }

    fn push_gpu_source_draw(
        &mut self,
        identity: IconGpuUploadKey,
        source: IconGpuSource,
        rect: ViewRect,
        screen: ViewRect,
        layer: IconDrawLayer,
    ) {
        let side = u32::from(source.size_px().max(1));
        let content_hash = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            source.hash(&mut hasher);
            hasher.finish()
        };
        let slot = if let Some(&slot) = self.slot_by_identity.get(&identity) {
            let existing = &mut self.slots[slot as usize];
            let old_area = existing.content_width.saturating_mul(existing.content_height);
            let new_area = side.saturating_mul(side);
            if new_area > old_area || existing.content_hash != content_hash {
                existing.width = side;
                existing.height = side;
                existing.content_width = side;
                existing.content_height = side;
                existing.content_hash = content_hash;
                existing.rounding = None;
                existing.source = Some(source);
                existing.dmabuf = None;
                self.cache_misses += 1;
            } else {
                self.cache_hits += 1;
            }
            slot
        } else if let Some(resident) = self.gpu_resident.get(&identity) {
            let old_area = resident
                .content_width
                .saturating_mul(resident.content_height);
            let new_area = side.saturating_mul(side);
            let rerender = new_area > old_area || resident.content_hash != content_hash;
            let slot = self.slots.len() as u32;
            self.slots.push(IconGpuSlot {
                identity: identity.clone(),
                width: if rerender { side } else { resident.width },
                height: if rerender { side } else { resident.height },
                content_width: if rerender { side } else { resident.content_width },
                content_height: if rerender { side } else { resident.content_height },
                content_hash: if rerender {
                    content_hash
                } else {
                    resident.content_hash
                },
                rounding: None,
                source: rerender.then_some(source),
                dmabuf: None,
            });
            if rerender {
                self.cache_misses += 1;
            } else {
                self.cache_hits += 1;
            }
            self.slot_by_identity.insert(identity, slot);
            slot
        } else {
            let slot = self.slots.len() as u32;
            self.slots.push(IconGpuSlot {
                identity: identity.clone(),
                width: side,
                height: side,
                content_width: side,
                content_height: side,
                content_hash,
                rounding: None,
                source: Some(source),
                dmabuf: None,
            });
            self.cache_misses += 1;
            self.slot_by_identity.insert(identity, slot);
            slot
        };
        self.push_slot_draw(slot, rect, screen, layer);
    }

    /// Attach a dmabuf plane to a logical GPU slot (zero-copy producer).
    #[cfg(test)]
    fn push_dmabuf_draw(&mut self, draw: IconDmabufDraw) {
        let IconDmabufDraw {
            identity,
            width,
            height,
            content_hash,
            rect,
            screen,
            layer,
            plane,
        } = draw;
        let width = width.max(1);
        let height = height.max(1);
        let slot = self.slots.len() as u32;
        self.slots.push(IconGpuSlot {
            identity: identity.clone(),
            width,
            height,
            content_width: width,
            content_height: height,
            content_hash,
            rounding: None,
            source: None,
            dmabuf: Some(IconDmabufSource { plane }),
        });
        self.slot_by_identity.insert(identity, slot);
        self.push_slot_draw(slot, rect, screen, layer);
    }

    fn push_slot_draw(
        &mut self,
        slot: u32,
        rect: ViewRect,
        screen: ViewRect,
        layer: IconDrawLayer,
    ) {
        let gpu_slot = &self.slots[slot as usize];
        let content_w = gpu_slot.content_width.max(1) as f32;
        let content_h = gpu_slot.content_height.max(1) as f32;
        // Sample the full content rect of the resident texture into the screen
        // icon box — zoom changes `rect` only, never the GPU identity.
        let scale_x = content_w / rect.width.max(1.0);
        let scale_y = content_h / rect.height.max(1.0);
        let source = ViewRect {
            x: (screen.x - rect.x).max(0.0) * scale_x,
            y: (screen.y - rect.y).max(0.0) * scale_y,
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
        let cache_entries = 0;
        let cache_bytes = 0;
        let thumbnail_ready_entries = self.thumbnails.ready_len();
        let thumbnail_ready_bytes = self.thumbnails.ready_bytes();
        let folder_preview_ready_entries = self.folder_preview_ready_entries;
        let folder_preview_ready_bytes = self.folder_preview_ready_bytes;
        // Stats: "atlas_*" fields mean unique logical GPU icon slots this frame.
        let slot_bytes = 0;
        let max_w = self.slots.iter().map(|s| s.width).max().unwrap_or(0);
        let max_h = self.slots.iter().map(|s| s.height).max().unwrap_or(0);
        let atlas_uploads = self
            .slots
            .iter()
            .filter(|slot| slot.source.is_some() || slot.dmabuf.is_some())
            .count();
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
                cache_entries,
                cache_bytes,
                content_hash,
                geometry_hash,
                vertex_hash,
                slot_hash,
                resolve_us: self.resolve_us,
            },
        }
    }
}
