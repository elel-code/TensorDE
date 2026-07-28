fn outgoing_dnd_preview_colors(
    source: Option<&ShellInternalDragPreviewSource>,
    theme: crate::ui::theme::ShellTheme,
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
            FileManagerItemPalette::from_shell_theme(theme),
        ),
        DragPreviewBackgroundStyle::HoveredPlace => place_row_background_color_for_palette(
            false,
            true,
            FileManagerItemPalette::from_shell_theme(theme),
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

fn outgoing_dnd_preview_gpu_draws(
    renderer: &mut crate::WgpuState,
    source: Option<&ShellInternalDragPreviewSource>,
    paths: &[PathBuf],
    metrics: OutgoingDndPreviewMetrics,
    scale: f32,
) -> Vec<GpuDragPreviewDraw> {
    let mut draws = Vec::new();
    for (index, path) in paths.iter().take(metrics.visible_icon_count()).enumerate() {
        let Some(icon_rect) = metrics.icon_rect_at(index) else {
            continue;
        };
        let local = outgoing_dnd_gpu_sources_for_path(
            renderer,
            source,
            path,
            metrics.cache_icon_size,
            metrics.icon_size.min(u32::from(u16::MAX)) as u16,
            scale,
        );
        draws.extend(local.into_iter().map(|(source, rect)| GpuDragPreviewDraw {
            source,
            rect: ViewRect {
                x: (icon_rect.x + rect.x) as f32,
                y: (icon_rect.y + rect.y) as f32,
                width: rect.width as f32,
                height: rect.height as f32,
            },
        }));
    }
    draws
}

fn outgoing_dnd_gpu_sources_for_path(
    renderer: &mut crate::WgpuState,
    source: Option<&ShellInternalDragPreviewSource>,
    path: &Path,
    cache_icon_size: f32,
    icon_size_px: u16,
    scale: f32,
) -> Vec<(IconGpuSource, PixelRect)> {
    let full = PixelRect::new(0, 0, i32::from(icon_size_px), i32::from(icon_size_px));
    let mut draws = Vec::new();
    let mut folder_preview = None;
    let mut view_mode = ShellViewMode::Icons;
    let base = match source {
        Some(ShellInternalDragPreviewSource::PaneItem {
            directory,
            entry,
            items,
            view_mode: mode,
            folder_preview: preview,
            ..
        }) => {
            view_mode = *mode;
            if let Some(item) = items.iter().find(|item| item.path == path) {
                let source_path = entry_path_for_thumbnail(directory, entry);
                if source_path == path {
                    folder_preview = preview.as_ref().map(|ready| ready.source.clone());
                }
                if !item.entry.is_dir
                    && let Some(thumbnail) = ready_drag_thumbnail_source(
                        &mut renderer.icon_renderer.thumbnails,
                        directory,
                        &item.entry,
                        icon_size_px,
                    )
                {
                    Some(thumbnail)
                } else {
                    renderer
                        .icon_renderer
                        .resolver
                        .resolve_entry_visible_fast(directory, &item.entry, cache_icon_size)
                        .path
                        .map(|path| IconGpuSource::file(path, icon_size_px))
                }
            } else {
                resolve_path_drag_source(renderer, path, cache_icon_size, icon_size_px)
            }
        }
        Some(ShellInternalDragPreviewSource::Place { icon_name, .. }) => renderer
            .icon_renderer
            .resolver
            .resolve_named_exact_fast(icon_name, icon_size_px as f32)
            .map(|path| IconGpuSource::file(path, icon_size_px)),
        _ => resolve_path_drag_source(renderer, path, cache_icon_size, icon_size_px),
    };
    if let Some(base) = base {
        draws.push((base, full));
    }
    if let Some(preview) = folder_preview {
        let layout = ItemPixmapLayout {
            view_mode,
            icon_rect: ViewRect {
                x: 0.0,
                y: 0.0,
                width: icon_size_px as f32,
                height: icon_size_px as f32,
            },
            text_rect: ViewRect {
                x: 0.0,
                y: 0.0,
                width: icon_size_px as f32,
                height: icon_size_px as f32,
            },
            text_midline_shift: 0.0,
        };
        let draw = folder_preview_gpu_draw_rect(layout, preview.size_px());
        draws.push((
            preview,
            PixelRect::new(
                draw.x.round() as i32,
                draw.y.round() as i32,
                draw.width.round().max(1.0) as i32,
                draw.height.round().max(1.0) as i32,
            ),
        ));
    }
    let emblem_rects = gpu_drag_emblem_pixel_rects(u32::from(icon_size_px), scale);
    for (index, emblem) in icon_emblem_kinds_for_path(path)
        .into_iter()
        .take(emblem_rects.len())
        .enumerate()
    {
        let rect = emblem_rects[index];
        let size = icon_cache_size(rect.width.max(rect.height) as f32);
        if let Some(path) = emblem.theme_names().iter().find_map(|name| {
            renderer
                .icon_renderer
                .resolver
                .resolve_named_exact_fast(name, size as f32)
        }) {
            draws.push((IconGpuSource::file(path, size), rect));
        }
    }
    draws
}

fn gpu_drag_emblem_pixel_rects(icon_size: u32, scale: f32) -> [PixelRect; 4] {
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

fn resolve_path_drag_source(
    renderer: &mut crate::WgpuState,
    path: &Path,
    cache_icon_size: f32,
    icon_size_px: u16,
) -> Option<IconGpuSource> {
    let key = file_icon_path_cache_key(path, path.is_dir(), None, true, cache_icon_size);
    renderer
        .icon_renderer
        .resolver
        .resolve_path_cache_key_fast(key)
        .path
        .map(|path| IconGpuSource::file(path, icon_size_px))
}

fn ready_drag_thumbnail_source(
    thumbnails: &mut crate::ThumbnailSourceResolver,
    directory: &Path,
    entry: &crate::Entry,
    size_px: u16,
) -> Option<IconGpuSource> {
    let path = entry_path_for_thumbnail(directory, entry);
    let modified_secs = entry.modified_secs?;
    if !thumbnail_request_may_have_preview(&path, entry.mime_type.as_deref()) {
        return None;
    }
    thumbnails.drain_results();
    let exact = ThumbnailSourceKey::thumbnail(path.clone(), size_px, modified_secs);
    let key = if thumbnails.ready.contains_key(&exact) {
        exact
    } else {
        thumbnails
            .ready
            .keys()
            .filter(|key| key.stamp == Some(modified_secs) && key.path == path)
            .min_by_key(|key| key.size_px.abs_diff(size_px))
            .cloned()?
    };
    let entry = thumbnails.ready.get_mut(&key)?;
    thumbnails.ready_frame = thumbnails.ready_frame.wrapping_add(1);
    entry.last_used_frame = thumbnails.ready_frame;
    Some(entry.source.clone())
}

fn outgoing_dnd_gpu_drag_icon(
    renderer: &mut crate::WgpuState,
    metrics: OutgoingDndPreviewMetrics,
    draws: Vec<GpuDragPreviewDraw>,
    label: &str,
    label_color: [u8; 4],
) -> Option<(DragIcon, wgpu::Texture)> {
    let plan = renderer.icon_renderer.dmabuf_plan?;
    let exported = crate::ui::render::dmabuf::create_exportable_dmabuf_texture(
        &renderer.device,
        plan,
        metrics.canvas_width,
        metrics.canvas_height,
        Some("fika-dnd-icon-dmabuf"),
    )
    .ok()?;
    let preview = GpuDragPreview {
        width: metrics.canvas_width,
        height: metrics.canvas_height,
        background: metrics.background_rect.map(|rect| {
            (
                ViewRect {
                    x: rect.x as f32,
                    y: rect.y as f32,
                    width: rect.width as f32,
                    height: rect.height as f32,
                },
                metrics.background_radius.max(0) as f32,
                metrics.background_color,
            )
        }),
        draws,
        label: metrics.label_rect.and_then(|rect| {
            rasterize_gpu_drag_preview_label(
                &mut renderer.text_renderer,
                ViewRect {
                    x: rect.x as f32,
                    y: rect.y as f32,
                    width: rect.width as f32,
                    height: rect.height as f32,
                },
                label,
                label_color,
            )
        }),
    };
    renderer
        .icon_renderer
        .gpu_source_renderer
        .as_mut()?
        .render_drag_preview(
            &renderer.device,
            &renderer.queue,
            &exported.texture,
            &preview,
        )
        .then_some(())?;
    renderer.wait_idle("dnd-icon-gpu-render");
    let plane = wayland_client_runtime::DmabufPlane::new(
        exported.plane.fd,
        0,
        exported.plane.offset,
        exported.plane.stride,
        exported.plane.modifier,
    );
    let params = wayland_client_runtime::DmabufBufferParams::new(
        metrics.canvas_width as i32,
        metrics.canvas_height as i32,
        exported.fourcc,
    )
    .with_plane(plane);
    let icon = DragIcon {
        buffer: params,
        buffer_scale: metrics.buffer_scale,
        offset_x: -metrics.hotspot_logical_x,
        offset_y: -metrics.hotspot_logical_y,
    };
    Some((icon, exported.texture))
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
