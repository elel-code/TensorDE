fn folder_preview_thumbnail_sources(directory: &Path) -> Vec<FolderPreviewThumbnailSource> {
    if is_network_path(directory) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mime_database = MimeDatabase::shared();
    let mut candidates = Vec::new();
    for entry in entries.flatten().take(DOLPHIN_FOLDER_PREVIEW_SCAN_LIMIT) {
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
        .take(DOLPHIN_FOLDER_PREVIEW_MAX_IMAGES)
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
fn thumbnail_request_from_raster_request(
    request: &ThumbnailRasterRequest,
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

fn folder_preview_role_draw_rect(layout: ItemPixmapLayout, raster: &IconRaster) -> ViewRect {
    let area = folder_preview_role_slot(layout);
    let (width, height) = fit_size_to_rect(
        raster.width,
        raster.height,
        area.width.ceil().max(1.0) as u32,
        area.height.ceil().max(1.0) as u32,
    );
    let width = width as f32;
    let height = height as f32;
    ViewRect {
        x: area.x + (area.width - width) / 2.0,
        y: area.y + (area.height - height) / 2.0,
        width,
        height,
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
    thumbnails: &'a mut ThumbnailRasterResolver,
    icon_rasters: &'a mut IconRasterResolver,
    raster_cache: &'a mut IconRasterCache,
    role_raster_cache: &'a mut IconRoleRasterCache,
    surface_size: PhysicalSize<u32>,
    ui_scale: f32,
    atlas_rasters: HashMap<IconAtlasRasterKey, AtlasRect>,
    uploads: Vec<IconAtlasUpload>,
    draws: Vec<IconDraw>,
    overlay_draws: Vec<IconDraw>,
    width: u32,
    height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
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
    raster_deferred: usize,
    raster_miss_budget: usize,
    resolve_us: u128,
    raster_us: u128,
}
include!("icon_frame_builder/builder.rs");
include!("icon_frame_builder/atlas.rs");
fn icon_draw_vertices(
    draws: &[IconDraw],
    atlas_width: u32,
    atlas_height: u32,
    surface_size: PhysicalSize<u32>,
) -> Vec<TextVertex> {
    let mut vertices = Vec::with_capacity(draws.len() * 6);
    for draw in draws {
        push_textured_rect(
            &mut vertices,
            draw.screen,
            AtlasRect {
                x: draw.atlas.x + draw.source.x,
                y: draw.atlas.y + draw.source.y,
                width: draw.source.width,
                height: draw.source.height,
            },
            atlas_width,
            atlas_height,
            surface_size,
            [1.0, 1.0, 1.0, draw.alpha],
        );
    }
    vertices
}
struct IconRenderer {
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
    overlay_vertex_start: usize,
    overlay_vertex_count: usize,
    last_vertices_hash: Option<u64>,
    last_icon_upload_keys: HashSet<IconAtlasUploadKey>,
    resolver: FileIconResolver,
    thumbnails: ThumbnailRasterResolver,
    icon_rasters: IconRasterResolver,
    raster_cache: IconRasterCache,
    role_raster_cache: IconRoleRasterCache,
}
