use super::*;

#[test]
fn drop_sources_prefer_drop_event_paths() {
    let event_paths = vec![PathBuf::from("/tmp/drop.txt")];
    let tracked = Some(vec![PathBuf::from("/tmp/enter.txt")]);

    assert_eq!(
        external_drag_drop_sources(event_paths, tracked),
        vec![PathBuf::from("/tmp/drop.txt")]
    );
}

#[test]
fn drop_sources_fall_back_to_tracked_enter_paths() {
    let tracked = Some(vec![PathBuf::from("/tmp/enter.txt")]);

    assert_eq!(
        external_drag_drop_sources(Vec::new(), tracked),
        vec![PathBuf::from("/tmp/enter.txt")]
    );
}

#[test]
fn uri_list_data_decodes_file_paths() {
    assert_eq!(
        external_drag_paths_from_uris(vec!["file:///tmp/a%20file.txt".to_string()]),
        vec![PathBuf::from("/tmp/a file.txt")]
    );
}

#[test]
fn outgoing_payload_advertises_uri_list() {
    let payload = outgoing_dnd_payload(&[PathBuf::from("/tmp/a file.txt")]);

    assert_eq!(payload.uris, vec!["file:///tmp/a%20file.txt".to_string()]);
    assert_eq!(payload.text, "file:///tmp/a%20file.txt");
}

#[test]
fn outgoing_preview_metrics_follow_item_icon_size() {
    let metrics = outgoing_dnd_preview_metrics(64, 1.0);

    assert_eq!(metrics.icon_size, 64);
    assert_eq!(metrics.cache_icon_size, 64.0);
    assert_eq!(metrics.icon_rect, PixelRect::new(0, 0, 64, 64));
    assert_eq!(metrics.canvas_width, 64);
}

#[test]
fn outgoing_preview_layout_preserves_scaled_source_size() {
    let layout = crate::ui::drag_preview_layout::place_single_drag_preview_layout(
        424.0,
        60.0,
        44.0,
        36.0,
        tensor_files_core::ViewPoint { x: 88.0, y: 30.0 },
        2.0,
    );
    let metrics = outgoing_dnd_preview_metrics_for_layout(layout, 2.0);

    assert_eq!(metrics.icon_size, 44);
    // Scene-space icon size is what thumbnail/theme caches key on.
    assert_eq!(metrics.cache_icon_size, 44.0);
    assert_eq!(metrics.canvas_width, 424);
    assert_eq!(metrics.canvas_height, 60);
}

#[test]
fn outgoing_preview_cache_size_stays_scene_space_under_fractional_scale() {
    let layout = crate::ui::drag_preview_layout::pane_single_drag_preview_layout(
        crate::ui::options::ShellViewMode::Icons,
        None,
        96.0,
        80.0,
        20.0,
        1.5,
        None,
    );
    let metrics = outgoing_dnd_preview_metrics_for_layout(layout, 1.5);

    // Physical paint size grows with buffer scale; cache key stays scene-sized.
    assert!(metrics.icon_size >= 96);
    assert_eq!(metrics.cache_icon_size, 96.0);
}

#[test]
fn outgoing_preview_metrics_align_fractional_scale_buffers() {
    let layout = crate::ui::drag_preview_layout::place_single_drag_preview_layout(
        212.0 * 1.5,
        30.0 * 1.5,
        22.0 * 1.5,
        18.0 * 1.5,
        tensor_files_core::ViewPoint {
            x: 44.0 * 1.5,
            y: 15.0 * 1.5,
        },
        1.5,
    );
    let metrics = outgoing_dnd_preview_metrics_for_layout(layout, 1.5);

    assert_eq!(metrics.buffer_scale, 2);
    assert_eq!(metrics.icon_size, 44);
    assert_eq!(metrics.canvas_width % metrics.buffer_scale as u32, 0);
    assert_eq!(metrics.canvas_height % metrics.buffer_scale as u32, 0);
    assert_eq!(metrics.icon_size % metrics.buffer_scale as u32, 0);
    assert!(metrics.label_rect.is_some());
}

#[cfg(unix)]
#[test]
fn drag_emblem_kinds_include_link_for_symlink() {
    let dir = std::env::temp_dir().join(format!(
        "tensor-files-dnd-link-emblem-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("target.txt");
    let link = dir.join("link.txt");
    fs::write(&target, "x").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    assert!(icon_emblem_kinds_for_path(&link).contains(&crate::IconEmblemKind::Link));

    fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn drag_emblem_kinds_skip_marker_for_readable_unwritable_file() {
    let dir = std::env::temp_dir().join(format!(
        "tensor-files-dnd-readonly-emblem-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("readonly.txt");
    fs::write(&path, "x").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o444);
    fs::set_permissions(&path, permissions).unwrap();

    assert!(icon_emblem_kinds_for_path(&path).is_empty());

    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o644);
    fs::set_permissions(&path, permissions).unwrap();
    fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn drag_emblem_kinds_prefer_locked_for_unreadable_file() {
    let dir = std::env::temp_dir().join(format!(
        "tensor-files-dnd-unreadable-emblem-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("unreadable.txt");
    fs::write(&path, "x").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(&path, permissions).unwrap();

    let emblems = icon_emblem_kinds_for_path(&path);
    assert!(emblems.contains(&crate::IconEmblemKind::Unreadable));

    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o644);
    fs::set_permissions(&path, permissions).unwrap();
    fs::remove_dir_all(&dir).unwrap();
}
