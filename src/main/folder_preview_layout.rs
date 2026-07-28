fn folder_preview_thumbnail_sources(directory: &Path) -> Vec<FolderPreviewThumbnailSource> {
    if is_network_path(directory) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mime_database = MimeDatabase::shared();
    let mut candidates = Vec::new();
    for entry in entries.flatten().take(FILE_MANAGER_FOLDER_PREVIEW_SCAN_LIMIT) {
        let path = entry.path();
        let name = entry.file_name();
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.'))
        {
            continue;
        }
        if !entry
            .file_type()
            .ok()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let name = name.to_string_lossy();
        let mime_type = folder_preview_child_mime_type(&path, &name, mime_database);
        if !thumbnail_request_may_have_preview(&path, mime_type.as_deref()) {
            continue;
        }
        let Some(modified_secs) = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
        else {
            continue;
        };
        let sort_key = entry.file_name().to_string_lossy().to_ascii_lowercase();
        candidates.push((
            sort_key,
            FolderPreviewThumbnailSource {
                path,
                modified_secs,
                mime_type,
            },
        ));
    }
    candidates.sort_by(|(left_key, left), (right_key, right)| {
        left_key
            .cmp(right_key)
            .then_with(|| left.path.cmp(&right.path))
    });
    candidates
        .into_iter()
        .take(FILE_MANAGER_FOLDER_PREVIEW_MAX_IMAGES)
        .map(|(_, source)| source)
        .collect()
}
fn folder_preview_child_mime_type(
    path: &Path,
    name: &str,
    mime_database: &MimeDatabase,
) -> Option<String> {
    let by_name = mime_database.mime_for_name(name, false, None);
    if thumbnail_request_may_have_preview(path, Some(by_name.as_ref())) {
        return Some(by_name.to_string());
    }

    let mut magic = [0u8; 512];
    let len = fs::File::open(path)
        .and_then(|mut file| file.read(&mut magic))
        .ok()?;
    if len == 0 {
        return Some(by_name.to_string());
    }
    let by_magic = mime_database.mime_for_name(name, false, Some(&magic[..len]));
    Some(by_magic.to_string())
}
#[cfg(test)]
fn folder_preview_thumbnail_stamp(directory: &Path, directory_modified_secs: u64) -> u64 {
    let sources = folder_preview_thumbnail_sources(directory);
    folder_preview_thumbnail_stamp_from_sources(directory_modified_secs, &sources)
}
fn folder_preview_thumbnail_stamp_from_sources(
    directory_modified_secs: u64,
    sources: &[FolderPreviewThumbnailSource],
) -> u64 {
    if sources.is_empty() {
        return directory_modified_secs;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    FOLDER_PREVIEW_LAYOUT_VERSION.hash(&mut hasher);
    directory_modified_secs.hash(&mut hasher);
    sources.len().hash(&mut hasher);
    for source in sources {
        source.path.hash(&mut hasher);
        source.modified_secs.hash(&mut hasher);
    }
    hasher.finish()
}
fn fit_size_to_rect(
    source_width: u32,
    source_height: u32,
    max_width: u32,
    max_height: u32,
) -> (u32, u32) {
    let scale =
        (max_width as f32 / source_width as f32).min(max_height as f32 / source_height as f32);
    let width = ((source_width as f32 * scale).round() as u32).clamp(1, max_width);
    let height = ((source_height as f32 * scale).round() as u32).clamp(1, max_height);
    (width, height)
}
fn thumbnail_request_from_source_request(
    request: &ThumbnailSourceRequest,
) -> Option<ThumbnailRequest> {
    ThumbnailRequest::from_entry_metadata_with_mime(
        WGPU_SHELL_PANE_ID,
        Generation(0),
        ItemId(0),
        request.key.path.clone(),
        request.key.stamp?,
        request.mime_type.clone(),
        request.priority,
    )
}
fn entry_path_for_thumbnail(directory: &Path, entry: &Entry) -> PathBuf {
    entry
        .target_path
        .clone()
        .unwrap_or_else(|| directory.join(entry.name.as_ref()))
}
fn folder_preview_role_cache_size(icon_size: f32) -> u16 {
    if icon_size > 128.0 { 256 } else { 128 }
}
#[derive(Clone, Copy, Debug)]
struct ItemPixmapLayout {
    view_mode: ShellViewMode,
    icon_rect: ViewRect,
    text_rect: ViewRect,
    text_midline_shift: f32,
}
impl ItemPixmapLayout {
    fn from_item_layout(view_mode: ShellViewMode, layout: ItemLayout) -> Self {
        Self {
            view_mode,
            icon_rect: layout.icon_rect,
            text_rect: layout.text_rect,
            text_midline_shift: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IconEmblemKind {
    Link,
    Unreadable,
}

impl IconEmblemKind {
    fn theme_names(self) -> &'static [&'static str] {
        match self {
            Self::Link => &["emblem-symbolic-link"],
            Self::Unreadable => &["emblem-locked", "emblem-unreadable"],
        }
    }
}

fn icon_emblem_kinds_for_path(path: &Path) -> Vec<IconEmblemKind> {
    if is_network_path(path)
        || path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("desktop"))
    {
        return Vec::new();
    }
    let mut emblems = Vec::new();
    let symlink_metadata = fs::symlink_metadata(path).ok();
    if symlink_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.file_type().is_symlink())
    {
        emblems.push(IconEmblemKind::Link);
    }
    let metadata = fs::metadata(path).ok();
    if let Some(metadata) = metadata.as_ref()
        && !path_is_readable(path, metadata)
    {
        emblems.push(IconEmblemKind::Unreadable);
    }
    emblems
}

#[cfg(unix)]
fn path_is_readable(path: &Path, _metadata: &fs::Metadata) -> bool {
    use rustix::fs::{access, Access};
    access(path, Access::READ_OK).is_ok()
}

#[cfg(not(unix))]
fn path_is_readable(_path: &Path, _metadata: &fs::Metadata) -> bool {
    true
}

fn icon_emblem_rects(paint_area: ViewRect, scale: f32) -> [ViewRect; 4] {
    let scale = scale.clamp(1.0, 2.0);
    let logical_icon_size = paint_area.width.min(paint_area.height) / scale;
    let logical_emblem_size = if logical_icon_size < 32.0 {
        8.0
    } else if logical_icon_size <= 48.0 {
        16.0
    } else if logical_icon_size <= 96.0 {
        22.0
    } else if logical_icon_size < 256.0 {
        32.0
    } else {
        64.0
    };
    let emblem_width = (logical_emblem_size * scale).min(paint_area.width);
    let emblem_height = (logical_emblem_size * scale).min(paint_area.height);
    let left = paint_area.x;
    let top = paint_area.y;
    let right = paint_area.right() - emblem_width;
    let bottom = paint_area.bottom() - emblem_height;
    [
        ViewRect {
            x: right,
            y: bottom,
            width: emblem_width,
            height: emblem_height,
        },
        ViewRect {
            x: left,
            y: top,
            width: emblem_width,
            height: emblem_height,
        },
        ViewRect {
            x: right,
            y: top,
            width: emblem_width,
            height: emblem_height,
        },
        ViewRect {
            x: left,
            y: bottom,
            width: emblem_width,
            height: emblem_height,
        },
    ]
}

fn folder_preview_gpu_draw_rect(layout: ItemPixmapLayout, size_px: u16) -> ViewRect {
    let area = folder_preview_role_slot(layout);
    let (width, height) = fit_size_to_rect(
        u32::from(size_px),
        u32::from(size_px),
        area.width.ceil().max(1.0) as u32,
        area.height.ceil().max(1.0) as u32,
    );
    ViewRect {
        x: area.x + (area.width - width as f32) / 2.0,
        y: area.y + (area.height - height as f32) / 2.0,
        width: width as f32,
        height: height as f32,
    }
}
fn folder_preview_role_shell_rect(layout: ItemPixmapLayout) -> ViewRect {
    match layout.view_mode {
        ShellViewMode::Icons => layout.icon_rect,
        ShellViewMode::Compact | ShellViewMode::Details => {
            let center_y =
                layout.text_rect.y + layout.text_rect.height / 2.0 + layout.text_midline_shift;
            ViewRect {
                x: layout.icon_rect.x,
                y: center_y - layout.icon_rect.height / 2.0,
                width: layout.icon_rect.width.max(1.0),
                height: layout.icon_rect.height.max(1.0),
            }
        }
    }
}
fn folder_preview_role_slot(layout: ItemPixmapLayout) -> ViewRect {
    folder_preview_role_shell_rect(layout)
}
#[derive(Clone, Debug)]
struct ShellThumbnailCandidate {
    path: PathBuf,
    modified_secs: u64,
    mime_type: Option<String>,
}
struct IconFrameBuilder<'a> {
    resolver: &'a mut FileIconResolver,
    thumbnails: &'a mut ThumbnailSourceResolver,
    /// Snapshot of GPU-resident icons at frame start (size-free sample path).
    gpu_resident: IconGpuResidentIndex,
    surface_size: PhysicalSize<u32>,
    ui_scale: f32,
    /// Dedup logical icon identities → slot index for this frame.
    slot_by_identity: HashMap<IconGpuUploadKey, u32>,
    slots: Vec<IconGpuSlot>,
    draws: Vec<IconDraw>,
    overlay_draws: Vec<IconDraw>,
    icons: usize,
    fallbacks: usize,
    thumbnails_loaded: usize,
    thumbnail_quads: usize,
    thumbnail_deferred: usize,
    thumbnail_read_ahead_queued: usize,
    folder_previews_loaded: usize,
    folder_preview_quads: usize,
    folder_preview_deferred: usize,
    folder_preview_read_ahead_queued: usize,
    folder_preview_ready_entries: usize,
    folder_preview_ready_bytes: usize,
    cache_hits: usize,
    cache_misses: usize,
    deferred: usize,
    sync_resolve_budget: usize,
    resolve_us: u128,
}
include!("icon_frame_builder/builder.rs");
// Atlas packing removed (scheme C: per-icon GPU textures).
/// Append NDC vertices for a draw that samples a per-icon texture.
fn push_icon_draw_vertices(
    out: &mut Vec<IconVertex>,
    draw: &IconDraw,
    slot: &IconGpuSlot,
    surface_size: PhysicalSize<u32>,
) {
    let surface_width = surface_size.width.max(1) as f32;
    let surface_height = surface_size.height.max(1) as f32;
    let left = draw.screen.x / surface_width * 2.0 - 1.0;
    let right = draw.screen.right() / surface_width * 2.0 - 1.0;
    let top = 1.0 - draw.screen.y / surface_height * 2.0;
    let bottom = 1.0 - draw.screen.bottom() / surface_height * 2.0;
    let texture_width = slot.width.max(1) as f32;
    let texture_height = slot.height.max(1) as f32;
    let u0 = draw.source.x / texture_width;
    let v0 = draw.source.y / texture_height;
    let u1 = (draw.source.x + draw.source.width) / texture_width;
    let v1 = (draw.source.y + draw.source.height) / texture_height;
    let (rounding_bounds, radius_ratio) = slot
        .rounding
        .map(|rounding| (rounding.bounds, rounding.radius_ratio))
        .unwrap_or(([0.0; 4], 0.0));
    let vertex = |position, uv| IconVertex {
        position,
        uv,
        rounding_bounds,
        radius_alpha: [radius_ratio, draw.alpha],
    };
    out.extend_from_slice(&[
        vertex([left, top], [u0, v0]),
        vertex([left, bottom], [u0, v1]),
        vertex([right, bottom], [u1, v1]),
        vertex([left, top], [u0, v0]),
        vertex([right, bottom], [u1, v1]),
        vertex([right, top], [u1, v0]),
    ]);
}
/// Pack draws into per-slot batches and a single vertex buffer range list.
fn pack_icon_batches(
    draws: &[IconDraw],
    slots: &[IconGpuSlot],
    surface_size: PhysicalSize<u32>,
) -> (Vec<IconVertex>, Vec<IconSlotBatch>) {
    if draws.is_empty() {
        return (Vec::new(), Vec::new());
    }
    // Group draw indices by slot while preserving first-seen slot order for locality.
    let mut by_slot: HashMap<u32, Vec<usize>> = HashMap::new();
    let mut slot_order: Vec<u32> = Vec::new();
    for (i, draw) in draws.iter().enumerate() {
        by_slot.entry(draw.slot).or_insert_with(|| {
            slot_order.push(draw.slot);
            Vec::new()
        }).push(i);
    }
    let mut vertices = Vec::with_capacity(draws.len() * 6);
    let mut batches = Vec::with_capacity(slot_order.len());
    for slot in slot_order {
        let Some(indices) = by_slot.get(&slot) else {
            continue;
        };
        let Some(gpu_slot) = slots.get(slot as usize) else {
            continue;
        };
        let start = vertices.len() as u32;
        for &i in indices {
            push_icon_draw_vertices(
                &mut vertices,
                &draws[i],
                gpu_slot,
                surface_size,
            );
        }
        let count = vertices.len() as u32 - start;
        if count > 0 {
            batches.push(IconSlotBatch {
                slot,
                vertex_start: start,
                vertex_count: count,
            });
        }
    }
    (vertices, batches)
}

fn icon_draw_content_hash(
    draws: &[IconDraw],
    overlay_draws: &[IconDraw],
    slots: &[IconGpuSlot],
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (layer, layer_draws) in [(0_u8, draws), (1_u8, overlay_draws)] {
        layer.hash(&mut hasher);
        layer_draws.len().hash(&mut hasher);
        for draw in layer_draws {
            if let Some(slot) = slots.get(draw.slot as usize) {
                slot.identity.hash(&mut hasher);
                slot.content_hash.hash(&mut hasher);
            } else {
                draw.slot.hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

fn icon_draw_geometry_hash(draws: &[IconDraw], overlay_draws: &[IconDraw]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (layer, layer_draws) in [(0_u8, draws), (1_u8, overlay_draws)] {
        layer.hash(&mut hasher);
        layer_draws.len().hash(&mut hasher);
        for draw in layer_draws {
            for value in [
                draw.screen.x,
                draw.screen.y,
                draw.screen.width,
                draw.screen.height,
                draw.source.x,
                draw.source.y,
                draw.source.width,
                draw.source.height,
                draw.alpha,
            ] {
                value.to_bits().hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

fn icon_slot_hash(slots: &[IconGpuSlot]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    slots.len().hash(&mut hasher);
    for slot in slots {
        slot.identity.hash(&mut hasher);
        slot.content_hash.hash(&mut hasher);
        slot.width.hash(&mut hasher);
        slot.height.hash(&mut hasher);
    }
    hasher.finish()
}
/// Resident GPU texture for one logical icon (scheme C, full GPU sample path).
struct IconGpuTexture {
    bind_group: wgpu::BindGroup,
    /// Keeps the texture alive for the bind group.
    #[allow(dead_code)]
    texture: wgpu::Texture,
    width: u32,
    height: u32,
    content_width: u32,
    content_height: u32,
    /// Matches last uploaded CPU content generation.
    content_hash: u64,
    rounding: Option<IconRounding>,
    last_used_frame: u64,
    /// How this texture was last filled (dmabuf import vs CPU upload).
    #[allow(dead_code)]
    source: crate::shell::render::dmabuf::ExternalTextureSource,
}

/// Snapshot of resident GPU icon sizes at frame start (avoids mut borrow splits).
#[derive(Clone, Debug, Default)]
struct IconGpuResidentIndex {
    entries: HashMap<IconGpuUploadKey, IconGpuResidentEntry>,
}

#[derive(Clone, Copy, Debug)]
struct IconGpuResidentEntry {
    width: u32,
    height: u32,
    content_width: u32,
    content_height: u32,
    content_hash: u64,
    rounding: Option<IconRounding>,
}

impl IconGpuResidentIndex {
    fn get(&self, key: &IconGpuUploadKey) -> Option<IconGpuResidentEntry> {
        self.entries.get(key).copied()
    }
}
struct IconRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Persistent logical-key → GPU texture map (scheme C).
    gpu_textures: HashMap<IconGpuUploadKey, IconGpuTexture>,
    /// Approximate RGBA residency for byte-based VRAM eviction.
    gpu_texture_bytes: usize,
    /// Frame-local: slot index → logical cache key for draw.
    frame_slot_keys: Vec<IconGpuUploadKey>,
    content_batches: Vec<IconSlotBatch>,
    overlay_batches: Vec<IconSlotBatch>,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    content_vertex_count: usize,
    overlay_vertex_start: usize,
    overlay_vertex_count: usize,
    last_vertices_hash: Option<u64>,
    gpu_frame: u64,
    /// Negotiated compositor/GPU import plan (updated from feedback).
    dmabuf_plan: Option<crate::shell::render::dmabuf::DmabufImportPlan>,
    /// Adapter supports Vulkan DMA-BUF import feature.
    dmabuf_import_supported: bool,
    /// Lifetime stats (reset never; useful for logs).
    dmabuf_imports: u64,
    gpu_source_renderer: Option<GpuIconSourceRenderer>,
    resolver: FileIconResolver,
    thumbnails: ThumbnailSourceResolver,
}
