fn rebuild_shell_places_for_user_path(user_places_path: &Path) -> Vec<ShellPlace> {
    if user_places_path == default_user_places_path().as_path() {
        build_shell_places_from_current_devices(user_places_path)
    } else {
        build_shell_places_from(user_places_path)
    }
}
fn build_shell_places_from_current_devices(user_places_path: &Path) -> Vec<ShellPlace> {
    let devices = read_gio_devices().unwrap_or_default();
    build_shell_places_from_with_devices(user_places_path, &devices)
}
fn build_shell_places_from_with_devices(
    user_places_path: &Path,
    devices: &[DeviceInfo],
) -> Vec<ShellPlace> {
    const NETWORK_GROUP: &str = "Network";
    const DEVICES_GROUP: &str = "Devices";

    let home = home_dir();
    let mut places = Vec::new();
    push_shell_place(&mut places, "", "H", "Home", home.clone(), false);
    push_existing_shell_place(&mut places, "", "Desk", "Desktop", home.join("Desktop"));
    push_existing_shell_place(&mut places, "", "Doc", "Documents", home.join("Documents"));
    push_existing_shell_place(&mut places, "", "Down", "Downloads", home.join("Downloads"));
    push_existing_shell_place(&mut places, "", "Mus", "Music", home.join("Music"));
    push_existing_shell_place(&mut places, "", "Pic", "Pictures", home.join("Pictures"));
    push_existing_shell_place(&mut places, "", "Vid", "Videos", home.join("Videos"));
    push_shell_place(
        &mut places,
        "",
        "Tr",
        "Trash",
        file_ops::trash_files_dir(),
        false,
    );

    let built_in_paths = places
        .iter()
        .map(|place| place.path.clone())
        .chain(std::iter::once(PathBuf::from("/")))
        .chain(std::iter::once(network_root_path()))
        .collect::<BTreeSet<_>>();
    let mut network_places = Vec::new();
    for place in load_user_places(user_places_path).unwrap_or_default() {
        if built_in_paths.contains(&place.path) {
            continue;
        }
        if is_network_path(&place.path) {
            network_places.push(place);
        } else {
            push_user_shell_place(&mut places, "", place);
        }
    }
    let place_order_path = place_order_path_for_user_places_path(user_places_path);
    let place_order = load_place_order(&place_order_path).unwrap_or_default();
    apply_left_shell_place_order(&mut places, &place_order);

    push_shell_place(
        &mut places,
        NETWORK_GROUP,
        "Net",
        NETWORK_ROOT_LABEL,
        network_root_path(),
        false,
    );
    for place in network_places {
        push_user_shell_place(&mut places, NETWORK_GROUP, place);
    }
    push_shell_place(
        &mut places,
        DEVICES_GROUP,
        "/",
        "Root",
        PathBuf::from("/"),
        false,
    );
    push_device_shell_places(&mut places, DEVICES_GROUP, devices);
    places
}
fn push_device_shell_places(
    places: &mut Vec<ShellPlace>,
    devices_group: &'static str,
    devices: &[DeviceInfo],
) {
    for device in devices {
        if device.mounted
            && !device
                .mount_point
                .as_ref()
                .is_some_and(|path| path.is_dir())
        {
            continue;
        }
        if !device.mounted && !device.ejectable && !device.can_power_off {
            continue;
        }
        let path = device
            .mount_point
            .clone()
            .unwrap_or_else(|| PathBuf::from(&device.id));
        let label = device
            .label
            .clone()
            .unwrap_or_else(|| path_name_or_display(&path));
        let place =
            ShellPlace::new(devices_group, "D", label, path, false).with_device(ShellDevicePlace {
                id: device.id.clone(),
                mounted: device.mounted,
                ejectable: device.ejectable,
                can_power_off: device.can_power_off,
            });
        places.push(place);
    }
}
fn apply_left_shell_place_order(places: &mut Vec<ShellPlace>, order: &[PathBuf]) {
    if order.is_empty() {
        return;
    }

    let first_grouped = places
        .iter()
        .position(|place| !place.group.is_empty())
        .unwrap_or(places.len());
    let mut left_places = places.drain(..first_grouped).collect::<Vec<_>>();
    let mut ordered_places = Vec::with_capacity(left_places.len());

    for path in order {
        if let Some(index) = left_places
            .iter()
            .position(|place| place.path.as_path() == path.as_path())
        {
            ordered_places.push(left_places.remove(index));
        }
    }
    ordered_places.append(&mut left_places);
    places.splice(0..0, ordered_places);
}
fn save_shell_place_order(user_places_path: &Path, places: &[ShellPlace]) -> Result<(), String> {
    let order = places
        .iter()
        .filter(|place| place.group.is_empty())
        .map(|place| place.path.clone())
        .collect::<Vec<_>>();
    save_place_order(
        &place_order_path_for_user_places_path(user_places_path),
        &order,
    )
}
fn add_user_place_at_path(
    user_places_path: &Path,
    path: &Path,
    label: String,
) -> Result<bool, String> {
    let label = label.trim();
    if label.is_empty() {
        return Err("place label cannot be empty".to_string());
    }
    let mut places = load_user_places(user_places_path)?;
    if places.iter().any(|place| place.path.as_path() == path) {
        return Ok(false);
    }
    places.push(UserPlace::new(label.to_string(), path.to_path_buf()));
    save_user_places(user_places_path, &places)?;
    Ok(true)
}
fn remove_user_place_at_path(user_places_path: &Path, path: &Path) -> Result<bool, String> {
    let mut places = load_user_places(user_places_path)?;
    let old_len = places.len();
    places.retain(|place| place.path.as_path() != path);
    if places.len() == old_len {
        return Ok(false);
    }
    save_user_places(user_places_path, &places)?;

    let order_path = place_order_path_for_user_places_path(user_places_path);
    let mut order = load_place_order(&order_path)?;
    let old_order_len = order.len();
    order.retain(|ordered_path| ordered_path.as_path() != path);
    if order.len() != old_order_len {
        save_place_order(&order_path, &order)?;
    }
    Ok(true)
}
fn default_shell_place_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}
fn push_existing_shell_place(
    places: &mut Vec<ShellPlace>,
    group: &'static str,
    marker: &'static str,
    label: &'static str,
    path: PathBuf,
) {
    if path.is_dir() {
        push_shell_place(places, group, marker, label, path, false);
    }
}
fn push_user_shell_place(places: &mut Vec<ShellPlace>, group: &'static str, place: UserPlace) {
    push_shell_place(places, group, "B", place.label, place.path, true);
}
fn push_shell_place(
    places: &mut Vec<ShellPlace>,
    group: &'static str,
    marker: &'static str,
    label: impl Into<String>,
    path: PathBuf,
    editable: bool,
) {
    if places.iter().any(|place| place.path == path) {
        return;
    }
    places.push(ShellPlace::new(group, marker, label, path, editable));
}
fn active_shell_place_index(places: &[ShellPlace], current_path: &Path) -> Option<usize> {
    let mut best = None;
    let mut best_components = 0usize;
    for (index, place) in places.iter().enumerate() {
        if !shell_place_matches_current(place, current_path) {
            continue;
        }
        let components = place.path.components().count();
        if best.is_none() || components > best_components {
            best = Some(index);
            best_components = components;
        }
    }
    best
}
fn shell_place_matches_current(place: &ShellPlace, current_path: &Path) -> bool {
    current_path == place.path || current_path.starts_with(&place.path)
}
fn filtered_indexes_for_entries(entries: &[Entry], show_hidden: bool, pattern: &str) -> Vec<usize> {
    let filter = (!pattern.is_empty()).then(|| NameFilter::plain_text(pattern.to_string()));
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let visible = show_hidden || !is_hidden_entry(entry);
            let matches_filter = filter
                .as_ref()
                .is_none_or(|filter| filter.matches_name(entry.name.as_ref()));
            (visible && matches_filter).then_some(index)
        })
        .collect()
}
fn is_hidden_entry(entry: &Entry) -> bool {
    entry.name.as_ref().starts_with('.')
}
#[cfg(test)]
fn content_height(size: PhysicalSize<u32>) -> f32 {
    (size.height as f32 - TOP_BAR_HEIGHT - STATUS_BAR_HEIGHT).max(1.0)
}
fn text_metrics_for_label_height(
    label_height: u32,
    max_font_size: f32,
    max_line_height: f32,
) -> Metrics {
    let line_height = (label_height as f32).max(1.0).min(max_line_height.max(1.0));
    let font_size = (max_font_size * line_height / max_line_height.max(1.0)).clamp(8.0, 64.0);
    Metrics::new(font_size, line_height)
}
fn file_manager_text_midline_shift_for_font(
    font_system: &mut FontSystem,
    font_size: f32,
    line_height: f32,
) -> f32 {
    let query = fontdb::Query {
        families: &[Family::SansSerif],
        weight: Weight::NORMAL,
        stretch: Stretch::Normal,
        style: Style::Normal,
    };
    let Some(font_id) = font_system.db().query(&query) else {
        return 0.0;
    };
    let Some(font) = font_system.get_font(font_id, Weight::NORMAL) else {
        return 0.0;
    };
    let metrics = font.metrics();
    file_manager_text_midline_shift_from_metrics(
        line_height,
        font_size,
        metrics.units_per_em,
        metrics.descent,
        metrics.cap_height,
    )
}
fn file_manager_text_midline_shift_from_metrics(
    line_height: f32,
    font_size: f32,
    units_per_em: u16,
    descent: f32,
    cap_height: Option<f32>,
) -> f32 {
    let units_per_em = units_per_em.max(1) as f32;
    let descent_px = (-descent / units_per_em * font_size).max(0.0);
    let cap_height_px = cap_height
        .map(|height| (height / units_per_em * font_size).max(0.0))
        .unwrap_or(font_size * 0.70);
    line_height / 2.0 - descent_px - cap_height_px / 2.0
}
include!("tests.rs");
