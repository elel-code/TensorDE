impl IconThemeResolver {
    fn find(&mut self, icon_name: &str, desired_size: u16) -> Option<PathBuf> {
        let key = (icon_name.to_string(), desired_size);
        if let Some(path) = self.path_cache.get(&key) {
            return path.clone();
        }

        let path = self.find_uncached(icon_name, desired_size);
        self.path_cache.insert(key, path.clone());
        path
    }

    fn first_existing(
        &mut self,
        icon_names: &[String],
        desired_size: u16,
    ) -> Option<(String, PathBuf)> {
        icon_names.iter().find_map(|name| {
            self.find(name, desired_size)
                .map(|path| (name.clone(), path))
        })
    }

    fn find_uncached(&mut self, icon_name: &str, desired_size: u16) -> Option<PathBuf> {
        if let Some(path) = absolute_icon_candidate(icon_name)
            && self.is_renderable_icon_file(&path)
        {
            return Some(path);
        }

        let roots = self.roots.clone();
        for theme in self.theme_search_order() {
            for root in &roots {
                let theme_root = root.join(&theme);
                if let Some(path) = self.find_icon_in_theme(&theme_root, icon_name, desired_size) {
                    return Some(path);
                }
            }
        }

        [
            Path::new("/usr/share/pixmaps"),
            Path::new("/usr/local/share/pixmaps"),
        ]
        .into_iter()
        .find_map(|root| self.find_icon_direct(root, icon_name))
    }

    fn theme_search_order(&mut self) -> Vec<String> {
        if let Some(search_order) = &self.search_order {
            return search_order.clone();
        }
        let mut themes = Vec::new();
        for theme in self.themes.clone() {
            self.push_theme_and_inherits(theme, &mut themes, 0);
        }
        self.search_order = Some(themes.clone());
        themes
    }

    fn push_theme_and_inherits(&mut self, theme: String, themes: &mut Vec<String>, depth: usize) {
        if depth > 8 || theme.is_empty() {
            return;
        }
        let already_seen = themes.iter().any(|existing| existing == &theme);
        push_unique_icon_theme(themes, &theme);
        if already_seen {
            return;
        }
        for inherited in self.inherited_themes(&theme) {
            self.push_theme_and_inherits(inherited, themes, depth + 1);
        }
    }

    fn inherited_themes(&mut self, theme: &str) -> Vec<String> {
        if let Some(inherited) = self.inherits_cache.get(theme) {
            return inherited.clone();
        }
        let mut inherited = Vec::new();
        for root in &self.roots {
            let Ok(contents) = fs::read_to_string(root.join(theme).join("index.theme")) else {
                continue;
            };
            for theme in parse_icon_theme_inherits(&contents) {
                push_unique_icon_theme(&mut inherited, &theme);
            }
        }
        self.inherits_cache
            .insert(theme.to_string(), inherited.clone());
        inherited
    }

    fn find_icon_in_theme(
        &mut self,
        theme_root: &Path,
        icon_name: &str,
        desired_size: u16,
    ) -> Option<PathBuf> {
        const CATEGORIES: &[&str] = &[
            "places",
            "mimetypes",
            "apps",
            "actions",
            "devices",
            "emblems",
            "status",
        ];
        if !self.dir_exists(theme_root) {
            return None;
        }
        if let Some(path) = self.find_icon_direct(theme_root, icon_name) {
            return Some(path);
        }
        for size in preferred_icon_size_dirs(desired_size) {
            for category in CATEGORIES {
                for base in [
                    theme_root.join(&size).join(category),
                    theme_root.join(category).join(&size),
                ] {
                    if let Some(path) = self.find_icon_direct(&base, icon_name) {
                        return Some(path);
                    }
                }
            }
        }
        for category in CATEGORIES {
            if let Some(path) = self.find_icon_direct(&theme_root.join(category), icon_name) {
                return Some(path);
            }
        }
        None
    }

    fn find_icon_direct(&mut self, root: &Path, icon_name: &str) -> Option<PathBuf> {
        if !self.dir_exists(root) {
            return None;
        }
        ["png", "svg", "webp", "jpg", "jpeg", "bmp", "gif", "ico"]
            .into_iter()
            .map(|extension| root.join(format!("{icon_name}.{extension}")))
            .find(|path| self.is_renderable_icon_file(path))
    }

    fn dir_exists(&mut self, path: &Path) -> bool {
        if let Some(exists) = self.dir_exists_cache.get(path) {
            return *exists;
        }
        let exists = path.is_dir();
        self.dir_exists_cache.insert(path.to_path_buf(), exists);
        exists
    }

    fn is_renderable_icon_file(&mut self, path: &Path) -> bool {
        if let Some(is_renderable) = self.renderable_file_cache.get(path) {
            return *is_renderable;
        }
        let is_renderable = is_renderable_icon_file(path);
        if is_renderable {
            self.renderable_file_cache.insert(path.to_path_buf(), true);
        }
        is_renderable
    }
}
fn file_icon_snapshot(
    profile: &FileIconProfile,
    desired_size: u16,
    theme: &mut IconThemeResolver,
) -> ResolvedFileIcon {
    let path = theme
        .first_existing(&profile.icon_candidates, desired_size)
        .or_else(|| theme.first_existing(&profile.generic_candidates, desired_size))
        .or_else(|| {
            theme.first_existing(
                &[
                    "unknown".to_string(),
                    "application-octet-stream".to_string(),
                ],
                desired_size,
            )
        })
        .map(|(_, path)| path);

    ResolvedFileIcon { path }
}
fn absolute_icon_candidate(icon_name: &str) -> Option<PathBuf> {
    let path = Path::new(icon_name);
    path.is_absolute().then(|| path.to_path_buf())
}
fn icon_theme_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = env::var_os("HOME").filter(|home| !home.is_empty()) {
        push_unique_icon_path(&mut roots, PathBuf::from(home).join(".local/share/icons"));
    }
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        push_unique_icon_path(&mut roots, PathBuf::from(data_home).join("icons"));
    }

    let data_dirs =
        env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for dir in data_dirs.split(':').filter(|dir| !dir.is_empty()) {
        push_unique_icon_path(&mut roots, Path::new(dir).join("icons"));
    }
    push_unique_icon_path(&mut roots, PathBuf::from("/usr/share/icons"));
    roots
}
fn icon_theme_names() -> Vec<String> {
    let mut themes = Vec::new();
    for theme in configured_icon_theme_names() {
        push_unique_icon_theme(&mut themes, &theme);
    }
    if env::var_os("KDE_FULL_SESSION").is_some()
        || env::var("XDG_CURRENT_DESKTOP")
            .map(|desktop| desktop.to_ascii_lowercase().contains("kde"))
            .unwrap_or(false)
    {
        push_unique_icon_theme(&mut themes, "breeze");
        push_unique_icon_theme(&mut themes, "breeze-dark");
    }
    for key in [
        "GTK_THEME",
        "ICON_THEME",
        "DESKTOP_SESSION",
        "XDG_CURRENT_DESKTOP",
    ] {
        if let Ok(value) = env::var(key) {
            for part in value.split([':', ';']) {
                let theme = part.trim();
                if !theme.is_empty() {
                    push_unique_icon_theme(&mut themes, theme);
                }
            }
        }
    }
    push_default_icon_theme_fallbacks(&mut themes);
    themes
}
fn push_default_icon_theme_fallbacks(themes: &mut Vec<String>) {
    for fallback in [
        "bloom",
        "bloom-dark",
        "deepin",
        "deepin-dark",
        "breeze",
        "breeze-dark",
        "Papirus",
        "Papirus-Dark",
        "Papirus-Light",
        "Adwaita",
        "hicolor",
    ] {
        push_unique_icon_theme(themes, fallback);
    }
}
fn configured_icon_theme_names() -> Vec<String> {
    let mut themes = Vec::new();
    for path in icon_theme_config_paths() {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        for theme in parse_configured_icon_theme_names(&contents) {
            push_unique_icon_theme(&mut themes, &theme);
        }
    }
    themes
}
fn icon_theme_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        let config_home = PathBuf::from(config_home);
        push_unique_icon_path(&mut paths, config_home.join("kdeglobals"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-4.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-3.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtkrc-2.0"));
    }
    if let Some(home) = env::var_os("HOME").filter(|home| !home.is_empty()) {
        let home = PathBuf::from(home);
        let config_home = home.join(".config");
        push_unique_icon_path(&mut paths, config_home.join("kdeglobals"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-4.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtk-3.0/settings.ini"));
        push_unique_icon_path(&mut paths, config_home.join("gtkrc-2.0"));
        push_unique_icon_path(&mut paths, home.join(".gtkrc-2.0"));
    }
    paths
}
fn parse_configured_icon_theme_names(contents: &str) -> Vec<String> {
    let mut themes = Vec::new();
    let mut in_icons_section = false;
    let mut in_icon_theme_section = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = &line[1..line.len() - 1];
            in_icons_section = section.eq_ignore_ascii_case("Icons");
            in_icon_theme_section = section.eq_ignore_ascii_case("Icon Theme");
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.eq_ignore_ascii_case("gtk-icon-theme-name")
            || (in_icons_section && key.eq_ignore_ascii_case("Theme"))
            || (in_icon_theme_section && key.eq_ignore_ascii_case("Name"))
        {
            let theme = value.trim().trim_matches('"');
            if !theme.is_empty() {
                push_unique_icon_theme(&mut themes, theme);
            }
        }
    }
    themes
}
fn parse_icon_theme_inherits(contents: &str) -> Vec<String> {
    let mut themes = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "Inherits" {
            continue;
        }
        for theme in value
            .split(',')
            .map(str::trim)
            .filter(|theme| !theme.is_empty())
        {
            push_unique_icon_theme(&mut themes, theme);
        }
    }
    themes
}
fn preferred_icon_size_dirs(desired_size: u16) -> Vec<String> {
    let mut dirs = Vec::new();
    let fixed_sizes = [256u16, 128, 96, 64, 48, 32, 24, 22, 16];
    let desired = desired_size.max(16);
    let mut ordered = fixed_sizes.into_iter().collect::<Vec<_>>();
    ordered.sort_by_key(|size| size.abs_diff(desired));
    for size in ordered {
        push_icon_size_dir(&mut dirs, format!("{size}x{size}"));
        push_icon_size_dir(&mut dirs, size.to_string());
    }
    push_icon_size_dir(&mut dirs, "scalable".to_string());
    push_icon_size_dir(&mut dirs, "symbolic".to_string());
    dirs
}
fn push_icon_size_dir(dirs: &mut Vec<String>, value: String) {
    if !dirs.iter().any(|existing| existing == &value) {
        dirs.push(value);
    }
}
fn is_renderable_icon_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() == 0 {
        return false;
    }

    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "svg" | "webp" | "jpg" | "jpeg" | "bmp" | "gif" | "ico")
    )
}
fn push_unique_icon_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}
fn normalized_external_drop_sources(sources: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut normalized = Vec::with_capacity(sources.len());
    for source in sources {
        if source.as_os_str().is_empty() {
            continue;
        }
        if !normalized.iter().any(|existing| existing == &source) {
            normalized.push(source);
        }
    }
    normalized
}
fn push_unique_icon_theme(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}
fn rasterize_icon(path: &Path, target_size: u32) -> Option<IconRaster> {
    let target_size = target_size.clamp(16, 256);
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("svg") => rasterize_svg_icon(path, target_size),
        _ => rasterize_bitmap_icon(path, target_size),
    }
}
fn rasterize_icon_for_cache_key(key: &IconRasterCacheKey) -> Option<IconRaster> {
    let raster = rasterize_icon(&key.path, key.size_px as u32)?;
    Some(match key.style {
        IconRasterStyle::Original => raster,
        IconRasterStyle::RoundedFile => {
            rounded_file_system_icon_raster(raster, FILE_ICON_CORNER_RADIUS_RATIO)
        }
        IconRasterStyle::RoundedFolder => {
            rounded_file_system_icon_raster(raster, FOLDER_ICON_CORNER_RADIUS_RATIO)
        }
    })
}
fn rounded_file_system_icon_raster(mut raster: IconRaster, radius_ratio: f32) -> IconRaster {
    let Some(expected_len) = (raster.width as usize)
        .checked_mul(raster.height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
    else {
        return raster;
    };
    if raster.pixels.len() != expected_len {
        return raster;
    }
    let Some((left, top, right, bottom)) = icon_alpha_content_bounds(
        raster.pixels.as_ref(),
        raster.width,
        raster.height,
    ) else {
        return raster;
    };
    let content_width = right - left;
    let content_height = bottom - top;
    let shortest_side = content_width.min(content_height);
    if shortest_side < 4 {
        return raster;
    }

    let radius = (shortest_side as f32 * radius_ratio.clamp(0.0, 0.5))
        .clamp(1.25, shortest_side as f32 / 2.0);
    let inner_left = left as f32 + radius;
    let inner_right = right as f32 - radius;
    let inner_top = top as f32 + radius;
    let inner_bottom = bottom as f32 - radius;
    let mut pixels = raster.pixels.as_ref().to_vec();

    for y in top..bottom {
        let py = y as f32 + 0.5;
        let dy = if py < inner_top {
            inner_top - py
        } else if py > inner_bottom {
            py - inner_bottom
        } else {
            0.0
        };
        if dy <= 0.0 {
            continue;
        }
        for x in left..right {
            let px = x as f32 + 0.5;
            let dx = if px < inner_left {
                inner_left - px
            } else if px > inner_right {
                px - inner_right
            } else {
                0.0
            };
            if dx <= 0.0 {
                continue;
            }

            let distance = (dx * dx + dy * dy).sqrt();
            let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
            if coverage >= 1.0 {
                continue;
            }
            let offset = ((y * raster.width + x) * 4) as usize;
            let alpha = (pixels[offset + 3] as f32 * coverage).round() as u8;
            pixels[offset + 3] = alpha;
            if alpha == 0 {
                pixels[offset] = 0;
                pixels[offset + 1] = 0;
                pixels[offset + 2] = 0;
            }
        }
    }

    raster.pixels = Arc::from(pixels);
    raster
}
fn icon_alpha_content_bounds(
    pixels: &[u8],
    width: u32,
    height: u32,
) -> Option<(u32, u32, u32, u32)> {
    let mut left = width;
    let mut top = height;
    let mut right = 0;
    let mut bottom = 0;
    for y in 0..height {
        for x in 0..width {
            let offset = ((y * width + x) * 4) as usize;
            if pixels.get(offset + 3).copied().unwrap_or_default() == 0 {
                continue;
            }
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    (right > left && bottom > top).then_some((left, top, right, bottom))
}
fn rasterize_bitmap_icon(path: &Path, target_size: u32) -> Option<IconRaster> {
    let image = image::open(path).ok()?.into_rgba8();
    let source_width = image.width();
    let source_height = image.height();
    if source_width == 0 || source_height == 0 {
        return None;
    }

    // Theme icons and generated freedesktop thumbnails are commonly already
    // in the requested square bucket. Reuse the decoder allocation directly;
    // GPU sampling handles all on-screen scaling from this point onward.
    if source_width == target_size && source_height == target_size {
        return Some(IconRaster {
            pixels: Arc::from(image.into_raw()),
            width: target_size,
            height: target_size,
        });
    }

    let (draw_width, draw_height) = fit_size(source_width, source_height, target_size);
    let mut pixels = vec![0; (target_size * target_size * 4) as usize];
    let x = (target_size - draw_width) / 2;
    let y = (target_size - draw_height) / 2;
    if draw_width == source_width && draw_height == source_height {
        copy_rgba_into(
            image.as_raw(),
            draw_width,
            draw_height,
            &mut pixels,
            target_size,
            x,
            y,
        );
    } else {
        let resized = image::imageops::resize(
            &image,
            draw_width,
            draw_height,
            image::imageops::FilterType::Lanczos3,
        );
        copy_rgba_into(
            resized.as_raw(),
            draw_width,
            draw_height,
            &mut pixels,
            target_size,
            x,
            y,
        );
    }
    Some(IconRaster {
        pixels: Arc::from(pixels),
        width: target_size,
        height: target_size,
    })
}
