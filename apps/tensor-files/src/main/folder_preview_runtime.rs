impl ShellFolderPreviewRoleRuntime {
    fn new() -> Self {
        Self::with_cache_root(default_thumbnail_cache_root())
    }

    fn with_cache_root(cache_root: PathBuf) -> Self {
        let (request_tx, request_rx) = mpsc::channel::<FolderPreviewRoleRequest>();
        let (result_tx, result_rx) = mpsc::channel::<FolderPreviewRoleResult>();
        let request_tx = thread::Builder::new()
            .name("tensor-files-folder-preview".to_string())
            .spawn(move || folder_preview_worker(cache_root, request_rx, result_tx))
            .ok()
            .map(|_| request_tx);
        Self {
            ready: HashMap::new(),
            ready_sizes: HashMap::new(),
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
        let _ = self.touch_ready_preview(key);
    }

    fn touch_ready_preview(
        &mut self,
        key: &FolderPreviewRoleKey,
    ) -> Option<&FolderPreviewReady> {
        let entry = self.ready.get_mut(key)?;
        self.frame = self.frame.wrapping_add(1);
        entry.last_used_frame = self.frame;
        Some(&entry.preview)
    }

    /// Paint-path hit: refresh LRU so scrolled-away previews evict first.
    fn preview_or_closest_touch(
        &mut self,
        path: &Path,
        directory_modified_secs: u64,
        size_px: u16,
    ) -> Option<&FolderPreviewReady> {
        let (indexed_path, stamps) = self.ready_sizes.get_key_value(path)?;
        let sizes = stamps.get(&directory_modified_secs)?;
        let lower = sizes.range(..=size_px).next_back().copied();
        let upper = sizes.range(size_px..).next().copied();
        let selected_size = match (lower, upper) {
            (Some(lower), Some(upper)) if size_px.abs_diff(lower) <= upper.abs_diff(size_px) => {
                lower
            }
            (Some(_), Some(upper)) => upper,
            (Some(lower), None) => lower,
            (None, Some(upper)) => upper,
            (None, None) => return None,
        };
        let key = FolderPreviewRoleKey::new(
            Arc::clone(indexed_path),
            directory_modified_secs,
            selected_size,
        );
        self.touch_ready_preview(&key)
    }

    fn queue_candidates(
        &mut self,
        candidates: impl IntoIterator<Item = FolderPreviewRoleRequest>,
    ) -> FolderPreviewRoleUpdateStats {
        let mut stats = FolderPreviewRoleUpdateStats::default();
        let mut keep = std::mem::take(&mut self.active);
        keep.clear();
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
                    self.insert_ready(result.key.clone(), preview);
                    self.failed.remove(&result.key);
                    stats.applied += 1;
                }
                None => {
                    let previous = self.ready.remove(&result.key).map(|entry| {
                        self.ready_bytes = self.ready_bytes.saturating_sub(entry.bytes);
                        entry.preview
                    });
                    if previous.is_some() {
                        self.remove_ready_size_index(&result.key);
                    }
                    let had_ready = previous.is_some();
                    let was_not_failed = self.failed.insert(result.key.clone());
                    stats.applied += usize::from(had_ready || was_not_failed);
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
        let bytes = preview.source.memory_bytes();
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
        self.ready_sizes
            .entry(Arc::clone(&key.path))
            .or_default()
            .entry(key.directory_modified_secs)
            .or_default()
            .insert(key.size_px);
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
                self.remove_ready_size_index(&victim);
            }
        }
    }

    fn prune_inactive_deferred(&mut self, keep: &HashSet<FolderPreviewRoleKey>) {
        self.pending.retain(|key, priority| {
            *priority != ThumbnailRequestPriority::Deferred || keep.contains(key)
        });
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
        self.rebuild_ready_size_index();
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
        if self.ready_sizes.capacity() > self.ready_sizes.len().saturating_mul(2).max(64) {
            self.ready_sizes.shrink_to_fit();
        }
    }

    fn remove_ready_size_index(&mut self, key: &FolderPreviewRoleKey) {
        let Some(stamps) = self.ready_sizes.get_mut(key.path.as_ref()) else {
            return;
        };
        if let Some(sizes) = stamps.get_mut(&key.directory_modified_secs) {
            sizes.remove(&key.size_px);
            if sizes.is_empty() {
                stamps.remove(&key.directory_modified_secs);
            }
        }
        if stamps.is_empty() {
            self.ready_sizes.remove(key.path.as_ref());
        }
    }

    fn rebuild_ready_size_index(&mut self) {
        self.ready_sizes.clear();
        let keys = self.ready.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            self.ready_sizes
                .entry(key.path)
                .or_default()
                .entry(key.directory_modified_secs)
                .or_default()
                .insert(key.size_px);
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
    let source = folder_preview_gpu_source_for_sources(
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
        source,
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
fn folder_preview_gpu_source_for_sources(
    cache_root: &Path,
    thumbnailers: &ThumbnailerRegistry,
    directory: &Path,
    sources: &[FolderPreviewThumbnailSource],
    priority: ThumbnailRequestPriority,
    size_px: u16,
) -> Option<IconGpuSource> {
    if sources.is_empty() {
        return None;
    }
    let mut children = Vec::with_capacity(sources.len());
    for source in sources {
        if let Some(path) =
            folder_preview_child_gpu_source(cache_root, thumbnailers, source, priority, size_px)
        {
            children.push(path);
        }
    }
    (!children.is_empty()).then(|| IconGpuSource::FolderPreview {
        children: children.into(),
        size_px,
        seed: folder_preview_directory_seed(directory),
    })
}
fn folder_preview_child_gpu_source(
    cache_root: &Path,
    thumbnailers: &ThumbnailerRegistry,
    source: &FolderPreviewThumbnailSource,
    priority: ThumbnailRequestPriority,
    size_px: u16,
) -> Option<PathBuf> {
    let thumbnail_source = ThumbnailRequest::from_entry_metadata_with_mime(
        SHELL_PANE_ID,
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
            ThumbnailSize::for_source_px(size_px),
        )
        .ok()
        .flatten()
    })
    .map(|thumbnail| thumbnail.path().to_path_buf());
    thumbnail_source.or_else(|| folder_preview_direct_image_source(source))
}
fn folder_preview_direct_image_source(source: &FolderPreviewThumbnailSource) -> Option<PathBuf> {
    let mime_type = source.mime_type.as_deref().unwrap_or_default();
    if !mime_type.starts_with("image/") && !thumbnail_extension_may_be_direct_image(&source.path) {
        return None;
    }
    Some(source.path.clone())
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
