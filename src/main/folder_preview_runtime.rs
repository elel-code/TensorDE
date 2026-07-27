impl ShellFolderPreviewRoleRuntime {
    fn new() -> Self {
        Self::with_cache_root(default_thumbnail_cache_root())
    }

    fn with_cache_root(cache_root: PathBuf) -> Self {
        let (request_tx, request_rx) = mpsc::channel::<FolderPreviewRoleRequest>();
        let (result_tx, result_rx) = mpsc::channel::<FolderPreviewRoleResult>();
        let request_tx = thread::Builder::new()
            .name("fika-wgpu-folder-preview".to_string())
            .spawn(move || folder_preview_worker(cache_root, request_rx, result_tx))
            .ok()
            .map(|_| request_tx);
        Self {
            ready: HashMap::new(),
            failed: HashSet::new(),
            pending: HashMap::new(),
            finished: HashSet::new(),
            active: HashSet::new(),
            frame: 0,
            ready_bytes: 0,
            ready_max_bytes: THUMBNAIL_READY_CACHE_MAX_BYTES,
            request_tx,
            result_rx,
        }
    }

    fn touch_ready_key(&mut self, key: &FolderPreviewRoleKey) {
        if let Some(entry) = self.ready.get_mut(key) {
            self.frame = self.frame.wrapping_add(1);
            entry.last_used_frame = self.frame;
        }
    }

    /// Look up without mutating LRU (damage / dirty-key paths are shared `&`).
    fn preview(
        &self,
        path: &Path,
        directory_modified_secs: u64,
        size_px: u16,
    ) -> Option<&FolderPreviewReady> {
        let key = FolderPreviewRoleKey::new(path.to_path_buf(), directory_modified_secs, size_px);
        self.ready.get(&key).map(|entry| &entry.preview)
    }

    fn preview_or_closest(
        &self,
        path: &Path,
        directory_modified_secs: u64,
        size_px: u16,
    ) -> Option<&FolderPreviewReady> {
        self.preview(path, directory_modified_secs, size_px)
            .or_else(|| {
                self.ready
                    .iter()
                    .filter(|(key, _)| {
                        key.path == path && key.directory_modified_secs == directory_modified_secs
                    })
                    .min_by_key(|(key, _)| key.size_px.abs_diff(size_px))
                    .map(|(_, entry)| &entry.preview)
            })
    }

    /// Paint-path hit: refresh LRU so scrolled-away previews evict first.
    fn preview_or_closest_touch(
        &mut self,
        path: &Path,
        directory_modified_secs: u64,
        size_px: u16,
    ) -> Option<&FolderPreviewReady> {
        let exact = FolderPreviewRoleKey::new(path.to_path_buf(), directory_modified_secs, size_px);
        if self.ready.contains_key(&exact) {
            self.touch_ready_key(&exact);
            return self.ready.get(&exact).map(|entry| &entry.preview);
        }
        let closest_key = self
            .ready
            .iter()
            .filter(|(key, _)| {
                key.path == path && key.directory_modified_secs == directory_modified_secs
            })
            .min_by_key(|(key, _)| key.size_px.abs_diff(size_px))
            .map(|(key, _)| key.clone())?;
        self.touch_ready_key(&closest_key);
        self.ready.get(&closest_key).map(|entry| &entry.preview)
    }

    fn queue_candidates(
        &mut self,
        candidates: impl IntoIterator<Item = FolderPreviewRoleRequest>,
    ) -> FolderPreviewRoleUpdateStats {
        let mut stats = FolderPreviewRoleUpdateStats::default();
        let mut keep = HashSet::new();
        for candidate in candidates {
            keep.insert(candidate.key.clone());
            if self.ready.contains_key(&candidate.key) {
                self.touch_ready_key(&candidate.key);
                stats.ready += 1;
                continue;
            }
            if self.failed.contains(&candidate.key) || self.finished.contains(&candidate.key) {
                stats.failed += usize::from(self.failed.contains(&candidate.key));
                continue;
            }
            match candidate.priority {
                ThumbnailRequestPriority::Visible => stats.visible += 1,
                ThumbnailRequestPriority::Deferred => stats.deferred += 1,
            }
            if self.queue(candidate.key, candidate.priority) {
                stats.queued += 1;
            }
        }
        self.prune_inactive_deferred(&keep);
        self.active = keep;
        stats
    }

    fn queue(&mut self, key: FolderPreviewRoleKey, priority: ThumbnailRequestPriority) -> bool {
        if self.ready.contains_key(&key) || self.finished.contains(&key) {
            return false;
        }
        match self.pending.get(&key).copied() {
            Some(ThumbnailRequestPriority::Visible) => return false,
            Some(ThumbnailRequestPriority::Deferred)
                if priority == ThumbnailRequestPriority::Visible =>
            {
                self.pending
                    .insert(key.clone(), ThumbnailRequestPriority::Visible);
            }
            Some(ThumbnailRequestPriority::Deferred) => return false,
            None => {
                self.pending.insert(key.clone(), priority);
            }
        }
        let Some(tx) = self.request_tx.as_ref() else {
            self.failed.insert(key.clone());
            self.finished.insert(key.clone());
            self.pending.retain(|pending_key, _| pending_key != &key);
            return false;
        };
        if tx
            .send(FolderPreviewRoleRequest {
                key: key.clone(),
                priority,
            })
            .is_err()
        {
            self.failed.insert(key.clone());
            self.finished.insert(key.clone());
            self.pending.retain(|pending_key, _| pending_key != &key);
            return false;
        }
        true
    }

    fn drain_results(&mut self) -> FolderPreviewRoleDrainStats {
        let mut stats = FolderPreviewRoleDrainStats::default();
        while let Ok(result) = self.result_rx.try_recv() {
            stats.results += 1;
            self.pending.remove(&result.key);
            if !self.has_active_identity(&result.key) {
                continue;
            }
            self.finished.insert(result.key.clone());
            match result.preview {
                Some(preview) => {
                    let previous = self.insert_ready(result.key.clone(), preview);
                    self.failed.remove(&result.key);
                    stats.applied += 1;
                    stats.changes.push(FolderPreviewRoleChange {
                        key: result.key,
                        previous,
                    });
                }
                None => {
                    let previous = self.ready.remove(&result.key).map(|entry| {
                        self.ready_bytes = self.ready_bytes.saturating_sub(entry.bytes);
                        entry.preview
                    });
                    let had_ready = previous.is_some();
                    let was_not_failed = self.failed.insert(result.key.clone());
                    stats.applied += usize::from(had_ready || was_not_failed);
                    if had_ready || was_not_failed {
                        stats.changes.push(FolderPreviewRoleChange {
                            key: result.key,
                            previous,
                        });
                    }
                }
            }
        }
        stats
    }

    fn has_active_identity(&self, key: &FolderPreviewRoleKey) -> bool {
        self.active.contains(key)
    }

    fn insert_ready(
        &mut self,
        key: FolderPreviewRoleKey,
        preview: FolderPreviewReady,
    ) -> Option<FolderPreviewReady> {
        let bytes = preview.raster.pixels.len();
        self.frame = self.frame.wrapping_add(1);
        let previous = self.ready.insert(
            key.clone(),
            FolderPreviewReadyEntry {
                preview,
                bytes,
                last_used_frame: self.frame,
            },
        );
        let previous_preview = if let Some(old) = previous {
            self.ready_bytes = self.ready_bytes.saturating_sub(old.bytes);
            Some(old.preview)
        } else {
            None
        };
        self.ready_bytes += bytes;
        self.evict_ready_if_needed(&key);
        previous_preview
    }

    fn evict_ready_if_needed(&mut self, protected: &FolderPreviewRoleKey) {
        while self.ready_bytes > self.ready_max_bytes && self.ready.len() > 1 {
            let Some(victim) = self
                .ready
                .iter()
                .filter(|(key, _)| *key != protected)
                .min_by_key(|(_, entry)| entry.last_used_frame)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.ready.remove(&victim) {
                self.ready_bytes = self.ready_bytes.saturating_sub(entry.bytes);
            }
        }
    }

    fn prune_inactive_deferred(&mut self, keep: &HashSet<FolderPreviewRoleKey>) {
        let stale = self
            .pending
            .iter()
            .filter(|(key, priority)| {
                **priority == ThumbnailRequestPriority::Deferred && !keep.contains(*key)
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in stale {
            self.pending.remove(&key);
        }
    }

    fn clear_request_lifecycle(&mut self) {
        self.failed.clear();
        self.finished.clear();
        self.pending.clear();
        self.active.clear();
        self.shrink_maps_if_sparse();
    }

    fn clear_path_prefix(&mut self, path: &Path) {
        self.ready.retain(|key, entry| {
            let keep = !key.path.starts_with(path);
            if !keep {
                self.ready_bytes = self.ready_bytes.saturating_sub(entry.bytes);
            }
            keep
        });
        self.failed.retain(|key| !key.path.starts_with(path));
        self.finished.retain(|key| !key.path.starts_with(path));
        self.active.retain(|key| !key.path.starts_with(path));
        self.pending.retain(|key, _| !key.path.starts_with(path));
        self.trim_failed(THUMBNAIL_FAILURE_CACHE_MAX_ENTRIES);
        self.shrink_maps_if_sparse();
    }

    fn trim_failed(&mut self, max_entries: usize) {
        if self.failed.len() <= max_entries {
            return;
        }
        let excess = self.failed.len() - max_entries;
        let drop_keys = self.failed.iter().take(excess).cloned().collect::<Vec<_>>();
        for key in drop_keys {
            self.failed.remove(&key);
            self.finished.remove(&key);
        }
    }

    fn shrink_maps_if_sparse(&mut self) {
        if self.ready.capacity() > self.ready.len().saturating_mul(2).max(64) {
            self.ready.shrink_to_fit();
        }
        if self.failed.capacity() > self.failed.len().saturating_mul(2).max(64) {
            self.failed.shrink_to_fit();
        }
        if self.finished.capacity() > self.finished.len().saturating_mul(2).max(64) {
            self.finished.shrink_to_fit();
        }
        if self.pending.capacity() > self.pending.len().saturating_mul(2).max(64) {
            self.pending.shrink_to_fit();
        }
        if self.active.capacity() > self.active.len().saturating_mul(2).max(64) {
            self.active.shrink_to_fit();
        }
    }

    fn ready_len(&self) -> usize {
        self.ready.len()
    }

    fn ready_bytes(&self) -> usize {
        self.ready_bytes
    }

    #[cfg(test)]
    fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    fn has_visible_pending(&self) -> bool {
        self.pending
            .values()
            .any(|priority| *priority == ThumbnailRequestPriority::Visible)
    }
}
fn folder_preview_worker(
    cache_root: PathBuf,
    request_rx: Receiver<FolderPreviewRoleRequest>,
    result_tx: Sender<FolderPreviewRoleResult>,
) {
    let thumbnailers = ThumbnailerRegistry::shared_system();
    let mut queue = PriorityWorkerQueue::default();
    while let Some(request) = queue.next_request(&request_rx) {
        let preview = folder_preview_for_request(&cache_root, thumbnailers, &request);
        if result_tx
            .send(FolderPreviewRoleResult {
                key: request.key,
                preview,
            })
            .is_err()
        {
            break;
        }
    }
}
fn folder_preview_for_request(
    cache_root: &Path,
    thumbnailers: &ThumbnailerRegistry,
    request: &FolderPreviewRoleRequest,
) -> Option<FolderPreviewReady> {
    let metadata = folder_preview_role_metadata_for_path(
        &request.key.path,
        request.key.directory_modified_secs,
    )?;
    let raster = folder_preview_raster_for_sources(
        cache_root,
        thumbnailers,
        &request.key.path,
        &metadata.sources,
        request.priority,
        request.key.size_px,
    )?;
    Some(FolderPreviewReady {
        stamp: metadata.stamp,
        size_px: request.key.size_px,
        raster,
    })
}
fn folder_preview_role_metadata_for_path(
    directory: &Path,
    directory_modified_secs: u64,
) -> Option<FolderPreviewRoleMetadata> {
    let sources = folder_preview_thumbnail_sources(directory);
    if sources.is_empty() {
        return None;
    }
    Some(FolderPreviewRoleMetadata {
        stamp: folder_preview_thumbnail_stamp_from_sources(directory_modified_secs, &sources),
        sources,
    })
}
fn folder_preview_raster_for_sources(
    cache_root: &Path,
    thumbnailers: &ThumbnailerRegistry,
    directory: &Path,
    sources: &[FolderPreviewThumbnailSource],
    priority: ThumbnailRequestPriority,
    size_px: u16,
) -> Option<IconRaster> {
    if sources.is_empty() {
        return None;
    }
    let mut rasters = Vec::with_capacity(sources.len());
    for source in sources {
        if let Some(raster) =
            folder_preview_child_raster(cache_root, thumbnailers, source, priority, size_px)
        {
            rasters.push(raster);
        }
    }
    folder_preview_thumbnail_raster_from_children(
        &rasters,
        size_px as u32,
        folder_preview_directory_seed(directory),
    )
}
fn folder_preview_child_raster(
    cache_root: &Path,
    thumbnailers: &ThumbnailerRegistry,
    source: &FolderPreviewThumbnailSource,
    priority: ThumbnailRequestPriority,
    size_px: u16,
) -> Option<IconRaster> {
    let thumbnail_raster = ThumbnailRequest::from_entry_metadata_with_mime(
        WGPU_SHELL_PANE_ID,
        Generation(0),
        ItemId(0),
        source.path.clone(),
        source.modified_secs,
        source.mime_type.clone(),
        priority,
    )
    .and_then(|thumbnail_request| {
        generate_thumbnail_with_external_thumbnailer_registry_size(
            cache_root,
            &thumbnail_request,
            thumbnailers,
            // Each child occupies only a fraction of the composed folder icon;
            // use the nearest source bucket without applying display bias again.
            ThumbnailSize::for_raster_px(size_px),
        )
        .ok()
        .flatten()
    })
    .and_then(|thumbnail| rasterize_icon(thumbnail.path(), size_px as u32));
    thumbnail_raster.or_else(|| folder_preview_direct_image_raster(source, size_px))
}
fn folder_preview_direct_image_raster(
    source: &FolderPreviewThumbnailSource,
    size_px: u16,
) -> Option<IconRaster> {
    let mime_type = source.mime_type.as_deref().unwrap_or_default();
    if !mime_type.starts_with("image/") && !thumbnail_extension_may_be_direct_image(&source.path) {
        return None;
    }
    rasterize_icon(&source.path, size_px as u32)
}
fn thumbnail_extension_may_be_direct_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "svg" | "webp" | "jpg" | "jpeg" | "bmp" | "gif" | "ico")
    )
}
#[cfg(test)]
fn folder_preview_thumbnail_source(directory: &Path) -> Option<FolderPreviewThumbnailSource> {
    folder_preview_thumbnail_sources(directory)
        .into_iter()
        .next()
}
