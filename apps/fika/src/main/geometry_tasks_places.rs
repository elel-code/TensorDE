fn intersect_rect(rect: ViewRect, clip: ViewRect) -> Option<ViewRect> {
    let x = rect.x.max(clip.x);
    let y = rect.y.max(clip.y);
    let right = rect.right().min(clip.right());
    let bottom = rect.bottom().min(clip.bottom());
    (right > x && bottom > y).then_some(ViewRect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}
fn map_layout_rect_to_draw_rect(
    layout_rect: ViewRect,
    draw_rect: ViewRect,
    adjusted_layout_rect: ViewRect,
) -> ViewRect {
    let scale_x = draw_rect.width / layout_rect.width.max(1.0);
    let scale_y = draw_rect.height / layout_rect.height.max(1.0);
    ViewRect {
        x: draw_rect.x + (adjusted_layout_rect.x - layout_rect.x) * scale_x,
        y: draw_rect.y + (adjusted_layout_rect.y - layout_rect.y) * scale_y,
        width: adjusted_layout_rect.width * scale_x,
        height: adjusted_layout_rect.height * scale_y,
    }
}
fn inset_rect(rect: ViewRect, inset: f32) -> Option<ViewRect> {
    let inset = inset.max(0.0);
    let width = rect.width - inset * 2.0;
    let height = rect.height - inset * 2.0;
    (width > 0.0 && height > 0.0).then_some(ViewRect {
        x: rect.x + inset,
        y: rect.y + inset,
        width,
        height,
    })
}
fn inset_content_scrollbar_slot(slot: ViewRect, scale_factor: f32) -> Option<ViewRect> {
    let inset = (CONTENT_SCROLLBAR_PADDING * scale_factor).round().max(1.0);
    let width = slot.width - inset * 2.0;
    let height = slot.height - inset * 2.0;
    (width > 0.0 && height > 0.0).then_some(ViewRect {
        x: slot.x + inset,
        y: slot.y + inset,
        width,
        height,
    })
}
fn status_zoom_control_contains_point(rects: StatusZoomIndicatorRects, point: ViewPoint) -> bool {
    let track_hit = ViewRect {
        x: rects.track.x,
        y: rects.outer.y,
        width: rects.track.width,
        height: rects.outer.height,
    };
    rects.label.contains(point) || track_hit.contains(point) || rects.thumb_outer.contains(point)
}
fn scrollbar_scroll_from_pointer(
    pointer_axis: f32,
    grab_offset: f32,
    track_origin: f32,
    track_extent: f32,
    thumb_extent: f32,
    max_scroll: f32,
) -> f32 {
    if max_scroll <= f32::EPSILON {
        return 0.0;
    }
    let travel = (track_extent - thumb_extent).max(0.0);
    if travel <= f32::EPSILON {
        return 0.0;
    }
    let thumb_origin = (pointer_axis - grab_offset).clamp(track_origin, track_origin + travel);
    ((thumb_origin - track_origin) / travel * max_scroll).clamp(0.0, max_scroll)
}
fn screen_to_content_point(
    point: ViewPoint,
    scroll_offset: ViewPoint,
    content_rect: ViewRect,
) -> Option<ViewPoint> {
    if !content_rect.contains(point) {
        return None;
    }
    Some(ViewPoint {
        x: point.x - content_rect.x + scroll_offset.x,
        y: point.y - content_rect.y + scroll_offset.y,
    })
}
fn clamped_screen_to_content_point(
    point: ViewPoint,
    scroll_offset: ViewPoint,
    content_rect: ViewRect,
) -> ViewPoint {
    let y = point.y.clamp(content_rect.y, content_rect.bottom());
    let x = point.x.clamp(content_rect.x, content_rect.right());
    ViewPoint {
        x: x - content_rect.x + scroll_offset.x,
        y: y - content_rect.y + scroll_offset.y,
    }
}
fn pane_content_rect_to_screen(rect: ViewRect, projection: &ShellPaneProjection<'_>) -> ViewRect {
    ViewRect {
        x: rect.x - projection.view.scroll_x + projection.geometry.content.x,
        y: rect.y - projection.view.scroll_y + projection.geometry.content.y,
        width: rect.width,
        height: rect.height,
    }
}
fn translated_rect(rect: ViewRect, dx: f32, dy: f32) -> ViewRect {
    ViewRect {
        x: rect.x + dx,
        y: rect.y + dy,
        width: rect.width,
        height: rect.height,
    }
}
fn view_point_from_physical_position(position: PhysicalPosition<f64>) -> ViewPoint {
    ViewPoint {
        x: position.x as f32,
        y: position.y as f32,
    }
}
fn point_distance(left: ViewPoint, right: ViewPoint) -> f32 {
    ((left.x - right.x).powi(2) + (left.y - right.y).powi(2)).sqrt()
}
fn details_size_label(entry: &Entry) -> String {
    if entry.is_dir {
        "Folder".to_string()
    } else if !entry.metadata_complete && entry.size_bytes == 0 && entry.modified_secs.is_none() {
        "-".to_string()
    } else {
        format_size(entry.size_bytes)
    }
}
fn pane_item_text_color(
    view_mode: ShellViewMode,
    entry: &Entry,
    selected: bool,
    theme: ShellTheme,
) -> TextColor {
    if selected {
        if theme.is_dark() {
            TextColor::rgb(241, 245, 249)
        } else {
            TextColor::rgb(15, 23, 42)
        }
    } else if view_mode != ShellViewMode::Details && entry.is_dir {
        theme.accent_text()
    } else {
        theme.primary_text()
    }
}
#[cfg(test)]
fn trash_conflict_dialog_rect(
    _dialog: &ShellTrashConflictDialog,
    size: PhysicalSize<u32>,
) -> ViewRect {
    trash_conflict_dialog_rect_scaled(_dialog, size, 1.0)
}
#[cfg(test)]
fn trash_conflict_dialog_rect_scaled(
    _dialog: &ShellTrashConflictDialog,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_dialog_metric(TRASH_CONFLICT_DIALOG_MARGIN, scale_factor);
    let dialog_width = scaled_dialog_metric(TRASH_CONFLICT_DIALOG_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let dialog_height = scaled_dialog_metric(TRASH_CONFLICT_DIALOG_HEIGHT, scale_factor)
        .min((height - margin * 2.0).max(1.0))
        .max(1.0);
    ViewRect {
        x: ((width - dialog_width) / 2.0).max(margin),
        y: ((height - dialog_height) / 2.0).max(margin),
        width: dialog_width,
        height: dialog_height,
    }
}
fn trash_conflict_dialog_window_size_scaled(scale_factor: f32) -> PhysicalSize<u32> {
    PhysicalSize::new(
        scaled_dialog_metric(TRASH_CONFLICT_DIALOG_WIDTH, scale_factor)
            .ceil()
            .max(1.0) as u32,
        scaled_dialog_metric(TRASH_CONFLICT_DIALOG_HEIGHT, scale_factor)
            .ceil()
            .max(1.0) as u32,
    )
}
fn trash_conflict_dialog_window_rect(size: PhysicalSize<u32>) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    }
}
#[cfg(test)]
#[allow(dead_code)]
fn trash_conflict_dialog_cancel_button_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_cancel_button_rect(dialog_rect)
}
fn trash_conflict_dialog_cancel_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    create_dialog_cancel_button_rect_scaled(dialog_rect, scale_factor)
}
#[cfg(test)]
fn trash_conflict_dialog_replace_button_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_commit_button_rect(dialog_rect)
}
fn trash_conflict_dialog_replace_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    create_dialog_commit_button_rect_scaled(dialog_rect, scale_factor)
}
fn yes_no(value: bool) -> String {
    if value { "Yes" } else { "No" }.to_string()
}
fn count_label(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}
fn path_display_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}
fn paths_task_summary(paths: &[PathBuf]) -> String {
    match paths {
        [] => "No items".to_string(),
        [path] => path_display_label(path),
        [first, ..] => format!(
            "{} including {}",
            count_label(paths.len(), "item", "items"),
            path_display_label(first)
        ),
    }
}
fn task_error_detail(error: &str, administrator_available: bool) -> String {
    if administrator_available {
        format!("{error} | use the administrator action explicitly")
    } else {
        error.to_string()
    }
}
fn transfer_task_detail(
    success_count: usize,
    failure_count: usize,
    target_dir: &Path,
    first_error: Option<&str>,
    administrator_available: bool,
) -> String {
    if failure_count > 0 {
        let base = format!(
            "{} failed, {} completed to {}",
            count_label(failure_count, "item", "items"),
            success_count,
            target_dir.display()
        );
        if let Some(error) = first_error {
            format!(
                "{base} | {}",
                task_error_detail(error, administrator_available)
            )
        } else if administrator_available {
            format!("{base} | use the administrator action explicitly")
        } else {
            base
        }
    } else {
        format!(
            "{} to {}",
            count_label(success_count, "item", "items"),
            target_dir.display()
        )
    }
}
fn trash_view_operation_runtime_failure(operation: TrashViewOperation) -> TrashViewOperationResult {
    TrashViewOperationResult {
        pane_id: WGPU_SHELL_PANE_ID,
        operation,
        success_count: 0,
        failure_count: 1,
        affected_dirs: Vec::new(),
        restore_conflicts: Vec::new(),
    }
}
fn scrollbar_axis_for_view_mode(view_mode: ShellViewMode) -> ContentScrollbarAxis {
    match view_mode {
        ShellViewMode::Compact => ContentScrollbarAxis::Horizontal,
        ShellViewMode::Icons | ShellViewMode::Details => ContentScrollbarAxis::Vertical,
    }
}
fn copy_location_text_for_path(path: &Path) -> String {
    path.display().to_string()
}
fn same_directory(left: &Path, right: &Path) -> bool {
    left == right
        || left
            .canonicalize()
            .ok()
            .zip(right.canonicalize().ok())
            .is_some_and(|(left, right)| left == right)
}
fn file_clipboard_role_as_str(role: FileClipboardRole) -> &'static str {
    match role {
        FileClipboardRole::Copy => "copy",
        FileClipboardRole::Cut => "cut",
    }
}
#[cfg(test)]
fn trash_paths_with_privilege(
    paths: &[PathBuf],
    privileged: bool,
) -> Result<ShellTrashResult, String> {
    if privileged {
        return match run_privileged_command_sync(PrivilegedCommand::Trash {
            paths: paths.to_vec(),
        }) {
            Ok(_) => Ok(ShellTrashResult {
                success_count: paths.len(),
                failure_count: 0,
                trash_pairs: Vec::new(),
                privileged: true,
                administrator_available: false,
                first_error: None,
            }),
            Err(error) => Ok(ShellTrashResult {
                success_count: 0,
                failure_count: paths.len(),
                trash_pairs: Vec::new(),
                privileged: true,
                administrator_available: false,
                first_error: Some(error),
            }),
        };
    }

    let summary = file_ops::trash_paths(paths);
    Ok(ShellTrashResult {
        success_count: summary.successes.len(),
        failure_count: summary.failures.len(),
        trash_pairs: summary
            .successes
            .iter()
            .map(|record| (record.original_path.clone(), record.trash_path.clone()))
            .collect(),
        privileged: false,
        administrator_available: summary
            .failures
            .iter()
            .any(|failure| should_attempt_privileged_operation(failure)),
        first_error: summary.failures.first().cloned(),
    })
}
fn path_name_or_display(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}
fn entry_index_by_name(entries: &[Entry], name: &str) -> Option<usize> {
    entries.iter().position(|entry| entry.name.as_ref() == name)
}
fn build_shell_places() -> Vec<ShellPlace> {
    let user_places_path = default_user_places_path();
    build_shell_places_from_current_devices(&user_places_path)
}
fn build_shell_places_from(user_places_path: &Path) -> Vec<ShellPlace> {
    build_shell_places_from_with_devices(user_places_path, &[])
}
