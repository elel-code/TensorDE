fn outgoing_dnd_preview_colors(
    source: Option<&ShellInternalDragPreviewSource>,
    theme: crate::shell::theme::ShellTheme,
) -> ([u8; 4], [u8; 4], Option<DragPreviewLabelStyle>) {
    let Some(source) = source else {
        return (
            [0, 0, 0, 0],
            text_color_to_rgba8(theme.primary_text()),
            Some(DragPreviewLabelStyle::PlainSingleLine),
        );
    };
    let layout = source.layout();
    let background = match layout.background_style {
        DragPreviewBackgroundStyle::SelectedItem => item_background_color_for_palette(
            true,
            false,
            DolphinItemPalette::from_shell_theme(theme),
        ),
        DragPreviewBackgroundStyle::HoveredPlace => place_row_background_color_for_palette(
            false,
            true,
            DolphinItemPalette::from_shell_theme(theme),
        ),
    };
    let label_color = match source {
        ShellInternalDragPreviewSource::PaneItem { .. } => {
            if theme.is_dark() {
                [241, 245, 249, 255]
            } else {
                [15, 23, 42, 255]
            }
        }
        ShellInternalDragPreviewSource::Place { .. } => text_color_to_rgba8(theme.primary_text()),
    };
    (
        ui_color_to_rgba8(background),
        label_color,
        layout.label.map(|label| label.style),
    )
}

fn outgoing_dnd_payload(paths: &[PathBuf]) -> OutgoingDndPayload {
    let uris = paths
        .iter()
        .map(|path| path_uri_from_path(path))
        .collect::<Vec<_>>();
    let text = uris.join("\n");
    OutgoingDndPayload { uris, text }
}

fn outgoing_dnd_preview_raster_for_path(
    renderer: &mut crate::WgpuState,
    source: Option<&ShellInternalDragPreviewSource>,
    path: &Path,
    cache_icon_size: f32,
    icon_size_px: u16,
    scale: f32,
) -> Option<IconRaster> {
    match source {
        Some(ShellInternalDragPreviewSource::PaneItem {
            directory,
            entry,
            items,
            view_mode,
            folder_preview,
            ..
        }) => {
            if let Some(item) = items.iter().find(|item| item.path == path) {
                let source_path = entry_path_for_thumbnail(directory, entry);
                let item_folder_preview = (source_path == path)
                    .then_some(folder_preview.as_ref())
                    .flatten();
                return rasterize_entry_drag_icon(
                    renderer,
                    DragPreviewEntryRasterSource {
                        directory,
                        entry: &item.entry,
                        path,
                        folder_preview: item_folder_preview,
                        view_mode: *view_mode,
                    },
                    cache_icon_size,
                    scale,
                );
            }
            rasterize_path_drag_icon(renderer, path, cache_icon_size, icon_size_px, scale)
        }
        Some(ShellInternalDragPreviewSource::Place { icon_name, .. }) => {
            rasterize_named_drag_icon(renderer, icon_name, icon_size_px)
        }
        _ => rasterize_path_drag_icon(renderer, path, cache_icon_size, icon_size_px, scale),
    }
}

struct DragPreviewEntryRasterSource<'a> {
    directory: &'a Path,
    entry: &'a crate::Entry,
    path: &'a Path,
    folder_preview: Option<&'a FolderPreviewReady>,
    view_mode: ShellViewMode,
}

fn rasterize_entry_drag_icon(
    renderer: &mut crate::WgpuState,
    source: DragPreviewEntryRasterSource<'_>,
    cache_icon_size: f32,
    scale: f32,
) -> Option<IconRaster> {
    let DragPreviewEntryRasterSource {
        directory,
        entry,
        path,
        folder_preview,
        view_mode,
    } = source;
    let icon_size_px = icon_cache_size(cache_icon_size);
    let base = if entry.is_dir {
        let resolved = renderer.icon_renderer.resolver.resolve_entry_visible_fast(
            directory,
            entry,
            cache_icon_size,
        );
        let base = rasterize_resolved_drag_icon(renderer, resolved.path, icon_size_px)?;
        apply_folder_preview_to_drag_icon(base, folder_preview, view_mode)
    } else if let Some(raster) = ready_drag_thumbnail(
        &mut renderer.icon_renderer.raster_cache,
        &mut renderer.icon_renderer.thumbnails,
        directory,
        entry,
        icon_size_px,
    ) {
        raster
    } else {
        let resolved = renderer.icon_renderer.resolver.resolve_entry_visible_fast(
            directory,
            entry,
            cache_icon_size,
        );
        rasterize_resolved_drag_icon(renderer, resolved.path, icon_size_px)?
    };
    Some(apply_drag_emblems(renderer, base, path, scale))
}

fn rasterize_path_drag_icon(
    renderer: &mut crate::WgpuState,
    path: &Path,
    cache_icon_size: f32,
    icon_size_px: u16,
    scale: f32,
) -> Option<IconRaster> {
    let key = file_icon_path_cache_key(path, path.is_dir(), None, true, cache_icon_size);
    let resolved = renderer
        .icon_renderer
        .resolver
        .resolve_path_cache_key_fast(key);
    let base = rasterize_resolved_drag_icon(renderer, resolved.path, icon_size_px)?;
    Some(apply_drag_emblems(renderer, base, path, scale))
}

fn ready_drag_thumbnail(
    raster_cache: &mut crate::IconRasterCache,
    thumbnails: &mut crate::ThumbnailRasterResolver,
    directory: &Path,
    entry: &crate::Entry,
    size_px: u16,
) -> Option<IconRaster> {
    let path = entry_path_for_thumbnail(directory, entry);
    let modified_secs = entry.modified_secs?;
    if !thumbnail_request_may_have_preview(&path, entry.mime_type.as_deref()) {
        return None;
    }
    // Prefer the exact live-view size, then any ready neighboring size so the
    // outgoing drag surface still shows the item's thumbnail (Dolphin reads
    // model "iconPixmap", which already holds the preview when available).
    let exact = IconRasterCacheKey::thumbnail(path.clone(), size_px, modified_secs);
    if let Some(raster) = raster_cache.get(&exact) {
        return Some(raster);
    }
    thumbnails.drain_results();
    if let Some(entry) = thumbnails.ready.get_mut(&exact) {
        thumbnails.ready_frame = thumbnails.ready_frame.wrapping_add(1);
        entry.last_used_frame = thumbnails.ready_frame;
        return Some(raster_cache.insert(exact, entry.raster.clone()));
    }
    if let Some(raster) = closest_ready_thumbnail(raster_cache, &path, modified_secs, size_px) {
        return Some(raster);
    }
    closest_ready_thumbnail_from_resolver(thumbnails, raster_cache, &path, modified_secs, size_px)
}

fn closest_ready_thumbnail(
    raster_cache: &mut crate::IconRasterCache,
    path: &Path,
    modified_secs: u64,
    size_px: u16,
) -> Option<IconRaster> {
    let key = raster_cache
        .entries
        .keys()
        .filter(|key| {
            key.stamp == Some(modified_secs)
                && key.path == path
                && key.style == IconRasterStyle::Original
        })
        .min_by_key(|key| key.size_px.abs_diff(size_px))
        .cloned()?;
    raster_cache.get(&key)
}

fn closest_ready_thumbnail_from_resolver(
    thumbnails: &mut crate::ThumbnailRasterResolver,
    raster_cache: &mut crate::IconRasterCache,
    path: &Path,
    modified_secs: u64,
    size_px: u16,
) -> Option<IconRaster> {
    let key = thumbnails
        .ready
        .keys()
        .filter(|key| key.stamp == Some(modified_secs) && key.path == path)
        .min_by_key(|key| key.size_px.abs_diff(size_px))
        .cloned()?;
    let entry = thumbnails.ready.get_mut(&key)?;
    thumbnails.ready_frame = thumbnails.ready_frame.wrapping_add(1);
    entry.last_used_frame = thumbnails.ready_frame;
    Some(raster_cache.insert(key, entry.raster.clone()))
}

fn rasterize_resolved_drag_icon(
    renderer: &mut crate::WgpuState,
    icon_path: Option<PathBuf>,
    size_px: u16,
) -> Option<IconRaster> {
    let icon_path = icon_path?;
    let key = IconRasterCacheKey::icon(icon_path, size_px);
    if let Some(raster) = renderer.icon_renderer.raster_cache.get(&key) {
        return Some(raster);
    }
    let raster = rasterize_icon(&key.path, size_px as u32)?;
    Some(renderer.icon_renderer.raster_cache.insert(key, raster))
}

fn rasterize_named_drag_icon(
    renderer: &mut crate::WgpuState,
    icon_name: &str,
    size_px: u16,
) -> Option<IconRaster> {
    let key = FileIconPathCacheKey {
        role: FileIconRoleCacheKey {
            kind: FileIconKind::Named {
                icon_name: icon_name.to_string(),
                fallback: NamedIconFallback::Service,
            },
        },
        size_px,
    };
    let resolved = renderer
        .icon_renderer
        .resolver
        .resolve_path_cache_key_fast(key);
    rasterize_resolved_drag_icon(renderer, resolved.path, size_px)
}

fn rasterize_named_drag_icon_exact(
    renderer: &mut crate::WgpuState,
    icon_name: &str,
    size_px: u16,
) -> Option<IconRaster> {
    let path = renderer
        .icon_renderer
        .resolver
        .resolve_named_exact_fast(icon_name, size_px as f32)?;
    rasterize_resolved_drag_icon(renderer, Some(path), size_px)
}

fn apply_folder_preview_to_drag_icon(
    base: IconRaster,
    folder_preview: Option<&FolderPreviewReady>,
    view_mode: ShellViewMode,
) -> IconRaster {
    let Some(folder_preview) = folder_preview else {
        return base;
    };
    let layout = ItemPixmapLayout {
        view_mode,
        icon_rect: ViewRect {
            x: 0.0,
            y: 0.0,
            width: base.width as f32,
            height: base.height as f32,
        },
        text_rect: ViewRect {
            x: 0.0,
            y: 0.0,
            width: base.width as f32,
            height: base.height as f32,
        },
        text_midline_shift: 0.0,
    };
    let draw_rect = folder_preview_role_draw_rect(layout, &folder_preview.raster);
    let rect = PixelRect::new(
        draw_rect.x.round() as i32,
        draw_rect.y.round() as i32,
        draw_rect.width.round().max(1.0) as i32,
        draw_rect.height.round().max(1.0) as i32,
    );
    let mut pixels = base.pixels.to_vec();
    draw_raster_scaled(&mut pixels, base.width, &folder_preview.raster, rect, 1.0);
    IconRaster {
        pixels: Arc::from(pixels),
        width: base.width,
        height: base.height,
    }
}

fn apply_drag_emblems(
    renderer: &mut crate::WgpuState,
    base: IconRaster,
    path: &Path,
    scale: f32,
) -> IconRaster {
    let emblems = icon_emblem_kinds_for_path(path);
    if emblems.is_empty() {
        return base;
    }
    let rects = drag_emblem_pixel_rects(base.width, scale);
    let mut pixels = base.pixels.to_vec();
    for (index, emblem) in emblems.into_iter().take(rects.len()).enumerate() {
        let rect = rects[index];
        let size_px = icon_cache_size(rect.width.max(rect.height) as f32);
        for icon_name in emblem.theme_names() {
            if let Some(raster) = rasterize_named_drag_icon_exact(renderer, icon_name, size_px) {
                draw_raster_scaled(&mut pixels, base.width, &raster, rect, 1.0);
                break;
            }
        }
    }
    IconRaster {
        pixels: Arc::from(pixels),
        width: base.width,
        height: base.height,
    }
}

fn drag_emblem_pixel_rects(icon_size: u32, scale: f32) -> [PixelRect; 4] {
    let paint_area = ViewRect {
        x: 0.0,
        y: 0.0,
        width: icon_size as f32,
        height: icon_size as f32,
    };
    icon_emblem_rects(paint_area, scale).map(|rect| {
        PixelRect::new(
            rect.x.round() as i32,
            rect.y.round() as i32,
            rect.width.round().max(1.0) as i32,
            rect.height.round().max(1.0) as i32,
        )
    })
}

fn outgoing_dnd_drag_icon(
    paths: &[PathBuf],
    metrics: OutgoingDndPreviewMetrics,
    rasters: Option<&OutgoingDndPreviewRasters>,
    label: Option<&OutgoingDndPreviewLabelRaster>,
    label_color: [u8; 4],
) -> Option<DragIcon> {
    let pixels =
        outgoing_dnd_preview_pixels_with_label(paths, metrics, rasters, label, label_color);
    let icon = RgbaIcon::new(pixels, metrics.canvas_width, metrics.canvas_height).ok()?;
    // Runtime `DndIcon` offset is logical surface coords relative to the drag
    // hotspot (wayland-client-runtime::dnd). Metrics store that as
    // `hotspot_logical_*` (scene hotspot / ui_scale).
    Some(DragIcon {
        icon,
        buffer_scale: metrics.buffer_scale,
        offset_x: -metrics.hotspot_logical_x,
        offset_y: -metrics.hotspot_logical_y,
    })
}

fn ui_color_to_rgba8(color: [f32; 4]) -> [u8; 4] {
    color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn text_color_to_rgba8(color: cosmic_text::Color) -> [u8; 4] {
    let [r, g, b, a] = color.as_rgba();
    [r, g, b, a]
}

fn outgoing_dnd_fallback_label(paths: &[PathBuf]) -> String {
    if paths.len() > 1 {
        return format!("{} items", paths.len());
    }
    paths
        .first()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Item")
        .to_string()
}
