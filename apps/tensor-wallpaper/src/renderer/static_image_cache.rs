
fn cached_static_image_variant(
    source_path: &Path,
    fit: FitMode,
    render_target: Option<RenderTargetSize>,
    source_dimensions: Option<RenderTargetSize>,
    context: &mut StaticImageCacheContext<'_>,
) -> Option<PathBuf> {
    if context.max_entries == 0 || !is_runtime_cacheable_static_image(source_path) {
        return None;
    }
    let render_target = render_target?;
    let source_dimensions = source_dimensions?;
    if !should_generate_static_image_cache_variant(source_dimensions, render_target, fit) {
        return None;
    }

    let cache_path = static_image_cache_path(
        context.cache_dir,
        source_path,
        source_dimensions,
        render_target,
        fit,
    );
    if is_nonempty_file(&cache_path) {
        context.stats.static_image_cache_reuses += 1;
        context.protected_files.insert(cache_path.clone());
        mark_static_image_cache_used(&cache_path);
        return Some(cache_path);
    }

    if generate_static_image_cache_variant(
        context.ffmpeg,
        source_path,
        &cache_path,
        render_target,
        fit,
    )
    .is_ok()
    {
        context.stats.static_image_cache_generations += 1;
        context.protected_files.insert(cache_path.clone());
        mark_static_image_cache_used(&cache_path);
        Some(cache_path)
    } else {
        context.stats.static_image_cache_generation_errors += 1;
        let _ = fs::remove_file(&cache_path);
        let _ = fs::remove_file(static_image_cache_used_marker(&cache_path));
        None
    }
}

fn should_generate_static_image_cache_variant(
    source: RenderTargetSize,
    target: RenderTargetSize,
    fit: FitMode,
) -> bool {
    let Some(cache_target) = static_image_cache_target_size(source, target, fit) else {
        return false;
    };
    source.area() >= cache_target.area().saturating_mul(2)
}

fn static_image_cache_target_size(
    source: RenderTargetSize,
    target: RenderTargetSize,
    fit: FitMode,
) -> Option<RenderTargetSize> {
    match fit {
        FitMode::Cover => source.covers(target).then_some(target),
        FitMode::Contain => contain_downscaled_size(source, target),
        FitMode::Stretch => Some(target),
        FitMode::Tile | FitMode::Center => None,
    }
}

fn contain_downscaled_size(
    source: RenderTargetSize,
    target: RenderTargetSize,
) -> Option<RenderTargetSize> {
    if source.width == 0 || source.height == 0 || target.width == 0 || target.height == 0 {
        return None;
    }

    let source_width = u64::from(source.width);
    let source_height = u64::from(source.height);
    let target_width = u64::from(target.width);
    let target_height = u64::from(target.height);

    let (scale_num, scale_den) = if target_width.saturating_mul(source_height)
        <= target_height.saturating_mul(source_width)
    {
        (target_width, source_width)
    } else {
        (target_height, source_height)
    };
    if scale_num >= scale_den {
        return None;
    }

    let width = ((source_width.saturating_mul(scale_num)) / scale_den)
        .max(1)
        .min(u64::from(u32::MAX)) as u32;
    let height = ((source_height.saturating_mul(scale_num)) / scale_den)
        .max(1)
        .min(u64::from(u32::MAX)) as u32;
    Some(RenderTargetSize { width, height })
}

fn static_image_cache_path(
    cache_dir: &Path,
    source_path: &Path,
    source_dimensions: RenderTargetSize,
    render_target: RenderTargetSize,
    fit: FitMode,
) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    source_path.hash(&mut hasher);
    source_dimensions.hash(&mut hasher);
    render_target.hash(&mut hasher);
    fit_cache_name(fit).hash(&mut hasher);
    if let Ok(metadata) = fs::metadata(source_path) {
        metadata.len().hash(&mut hasher);
        if let Ok(modified) = metadata.modified()
            && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
        {
            duration.as_secs().hash(&mut hasher);
            duration.subsec_nanos().hash(&mut hasher);
        }
    }

    cache_dir.join("static-image-cache").join(format!(
        "{}-{}x{}-{}.png",
        fit_cache_name(fit),
        render_target.width,
        render_target.height,
        hasher.finish()
    ))
}

fn fit_cache_name(fit: FitMode) -> &'static str {
    match fit {
        FitMode::Cover => "cover",
        FitMode::Contain => "contain",
        FitMode::Stretch => "stretch",
        FitMode::Tile => "tile",
        FitMode::Center => "center",
    }
}

fn is_runtime_cacheable_static_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "avif" | "bmp" | "jpeg" | "jpg" | "png" | "webp"
            )
        })
        .unwrap_or(false)
}

fn is_nonempty_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

fn generate_static_image_cache_variant(
    ffmpeg: Option<&Path>,
    source_path: &Path,
    output_path: &Path,
    target: RenderTargetSize,
    fit: FitMode,
) -> Result<(), String> {
    let Some(filter) = static_image_cache_filter(target, fit) else {
        return Err("fit mode is not runtime-cacheable".to_owned());
    };
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create static image cache directory: {err}"))?;
    }
    let temporary_path = output_path.with_extension("png.tmp");
    let _ = fs::remove_file(&temporary_path);

    let executable = ffmpeg.unwrap_or_else(|| Path::new("ffmpeg"));
    let output = Command::new(executable)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(source_path)
        .args(["-frames:v", "1", "-an", "-sn", "-vf", &filter])
        .arg(&temporary_path)
        .output()
        .map_err(|err| format!("failed to run {}: {err}", executable.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let reason = if stderr.is_empty() {
            output.status.to_string()
        } else {
            format!("{}: {stderr}", output.status)
        };
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("{} failed: {reason}", executable.display()));
    }
    if !is_nonempty_file(&temporary_path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!(
            "{} created an empty static image cache file at {}",
            executable.display(),
            temporary_path.display()
        ));
    }

    fs::rename(&temporary_path, output_path)
        .map_err(|err| format!("failed to move static image cache file into place: {err}"))?;
    Ok(())
}

fn static_image_cache_filter(target: RenderTargetSize, fit: FitMode) -> Option<String> {
    match fit {
        FitMode::Cover => Some(format!(
            "scale={}:{}:force_original_aspect_ratio=increase,crop={}:{}",
            target.width, target.height, target.width, target.height
        )),
        FitMode::Contain => Some(format!(
            "scale={}:{}:force_original_aspect_ratio=decrease",
            target.width, target.height
        )),
        FitMode::Stretch => Some(format!("scale={}:{}", target.width, target.height)),
        FitMode::Tile | FitMode::Center => None,
    }
}

fn automatic_variant_source(
    package: &WallpaperPackage,
    render_target: Option<RenderTargetSize>,
) -> Option<&PackagePath> {
    let render_target = render_target?;
    let target_area = render_target.area();
    package
        .manifest
        .variants
        .iter()
        .filter_map(|variant| variant_dimensions(variant).map(|dimensions| (variant, dimensions)))
        .filter(|(_, dimensions)| dimensions.covers(render_target))
        .min_by_key(|(_, dimensions)| {
            (
                dimensions.area().saturating_sub(target_area),
                dimensions.aspect_delta(render_target),
            )
        })
        .map(|(variant, _)| &variant.source)
}

fn variant_dimensions(variant: &Variant) -> Option<RenderTargetSize> {
    Some(RenderTargetSize {
        width: variant.width?,
        height: variant.height?,
    })
}

fn render_target_size(
    compositor: Option<CompositorKind>,
    output: Option<&DesktopOutput>,
) -> Option<RenderTargetSize> {
    let output = output?;
    let width = output.width?;
    let height = output.height?;
    if matches!(compositor, Some(CompositorKind::Hyprland)) {
        return Some(RenderTargetSize { width, height });
    }

    let scale = if output.scale.is_finite() && output.scale > 0.0 {
        output.scale
    } else {
        1.0
    };
    Some(RenderTargetSize {
        width: scaled_dimension(width, scale),
        height: scaled_dimension(height, scale),
    })
}

fn scaled_dimension(value: u32, scale: f32) -> u32 {
    ((f64::from(value) * f64::from(scale))
        .round()
        .clamp(1.0, f64::from(u32::MAX))) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RenderTargetSize {
    width: u32,
    height: u32,
}

impl RenderTargetSize {
    fn covers(self, target: Self) -> bool {
        self.width >= target.width && self.height >= target.height
    }

    fn area(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    fn aspect_delta(self, target: Self) -> u64 {
        let left = u64::from(self.width) * u64::from(target.height);
        let right = u64::from(target.width) * u64::from(self.height);
        left.abs_diff(right)
    }
}

pub fn static_wallpaper_plan(
    output_name: impl Into<String>,
    package: &WallpaperPackage,
    output_state: &OutputState,
) -> Result<Option<StaticWallpaperPlan>, RendererPlanError> {
    let Some(assignment) = &output_state.wallpaper else {
        return Ok(None);
    };
    let WallpaperEntry::StaticImage {
        source,
        fit,
        background,
        ..
    } = &package.manifest.entry
    else {
        return Err(RendererPlanError::UnsupportedEntry(
            package.manifest.entry.kind().as_str(),
        ));
    };
    let variant_source = explicit_variant_source(package, assignment.variant.as_deref())?;

    Ok(Some(StaticWallpaperPlan {
        output_name: output_name.into(),
        source: variant_source.unwrap_or(source).join_to(&package.root),
        fit: *fit,
        background: background.clone(),
    }))
}

fn effective_max_fps(manifest_max_fps: Option<u32>, policy_max_fps: Option<u32>) -> Option<u32> {
    match (manifest_max_fps, policy_max_fps) {
        (Some(manifest), Some(policy)) => Some(manifest.min(policy)),
        (Some(manifest), None) => Some(manifest),
        (None, Some(policy)) => Some(policy),
        (None, None) => None,
    }
}

fn effective_muted(entry_muted: bool, runtime_allow_audio: bool) -> bool {
    entry_muted || !runtime_allow_audio
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RendererPlanError {
    UnsupportedEntry(&'static str),
    MissingAssignment,
    MissingVariant(String),
    PlaylistNoMatch,
    PackageLoad(String),
}

impl fmt::Display for RendererPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedEntry(kind) => write!(f, "{kind} entries are not supported here"),
            Self::MissingAssignment => f.write_str("wallpaper assignment is missing"),
            Self::MissingVariant(variant) => {
                write!(f, "wallpaper variant {variant:?} was not found")
            }
            Self::PlaylistNoMatch => f.write_str("playlist did not match any item"),
            Self::PackageLoad(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for RendererPlanError {}

fn load_assigned_package(
    assignment: &WallpaperAssignment,
    cache_dir: &Path,
) -> Result<WallpaperPackage, RendererPlanError> {
    let mut stats = RenderSyncCacheReport::default();
    let mut protected_archive_dirs = BTreeSet::new();
    load_assigned_package_tracked(
        assignment,
        cache_dir,
        &mut stats,
        &mut protected_archive_dirs,
    )
}

fn load_assigned_package_tracked(
    assignment: &WallpaperAssignment,
    cache_dir: &Path,
    stats: &mut RenderSyncCacheReport,
    protected_archive_dirs: &mut BTreeSet<PathBuf>,
) -> Result<WallpaperPackage, RendererPlanError> {
    let path = Path::new(&assignment.path);
    if path.is_dir() || path.extension().and_then(|extension| extension.to_str()) == Some("gwpdir")
    {
        return crate::core::load_gwpdir(path)
            .map_err(|err| RendererPlanError::PackageLoad(err.to_string()));
    }
    if path.extension().and_then(|extension| extension.to_str()) == Some("gwp") {
        let extract_dir = archive_extract_dir(cache_dir, path);
        protected_archive_dirs.insert(extract_dir.clone());
        if extract_dir.join(crate::core::MANIFEST_FILE).exists()
            || extract_dir.join(crate::core::MANIFEST_TOML_FILE).exists()
        {
            stats.archive_cache_reuses += 1;
            let package = crate::core::load_gwpdir(&extract_dir)
                .map_err(|err| RendererPlanError::PackageLoad(err.to_string()))?;
            mark_archive_cache_used(&extract_dir);
            return Ok(package);
        }
        stats.archive_cache_extractions += 1;
        fs::create_dir_all(
            extract_dir
                .parent()
                .ok_or_else(|| RendererPlanError::PackageLoad("invalid cache path".to_owned()))?,
        )
        .map_err(|err| RendererPlanError::PackageLoad(err.to_string()))?;
        let package = crate::core::load_gwp(path, &extract_dir)
            .map_err(|err| RendererPlanError::PackageLoad(err.to_string()))?;
        mark_archive_cache_used(&extract_dir);
        Ok(package)
    } else {
        Err(RendererPlanError::PackageLoad(format!(
            "unsupported wallpaper path {}",
            path.display()
        )))
    }
}

struct RenderPackageCache<'a> {
    cache_dir: &'a Path,
    max_entries: usize,
    max_retained_unique_resource_bytes: u64,
    packages: BTreeMap<String, Result<Rc<WallpaperPackage>, RendererPlanError>>,
    package_order: VecDeque<String>,
    protected_archive_dirs: BTreeSet<PathBuf>,
    protected_static_cache_files: BTreeSet<PathBuf>,
    stats: RenderSyncCacheReport,
}

impl<'a> RenderPackageCache<'a> {
    fn new(
        cache_dir: &'a Path,
        max_entries: usize,
        max_retained_unique_resource_bytes: u64,
    ) -> Self {
        Self {
            cache_dir,
            max_entries,
            max_retained_unique_resource_bytes,
            packages: BTreeMap::new(),
            package_order: VecDeque::new(),
            protected_archive_dirs: BTreeSet::new(),
            protected_static_cache_files: BTreeSet::new(),
            stats: RenderSyncCacheReport::default(),
        }
    }

    fn package(
        &mut self,
        assignment: &WallpaperAssignment,
    ) -> Result<Rc<WallpaperPackage>, RendererPlanError> {
        if let Some(package) = self.packages.get(&assignment.path) {
            self.stats.package_cache_hits += 1;
            return package.clone();
        }

        self.stats.package_cache_misses += 1;
        let package = load_assigned_package_tracked(
            assignment,
            self.cache_dir,
            &mut self.stats,
            &mut self.protected_archive_dirs,
        )
        .map(Rc::new);
        if self.should_retain_packages() {
            self.prune_for_insert();
            self.packages
                .insert(assignment.path.clone(), package.clone());
            self.package_order.push_back(assignment.path.clone());
            self.prune_to_resource_limit();
        }
        package
    }

    fn should_retain_packages(&self) -> bool {
        self.max_entries > 0 && self.max_retained_unique_resource_bytes > 0
    }

    fn prune_for_insert(&mut self) {
        while self.packages.len() >= self.max_entries {
            let Some(key) = self.package_order.pop_front() else {
                break;
            };
            if self.packages.remove(&key).is_some() {
                self.stats.package_cache_evictions += 1;
            }
        }
    }

    fn prune_to_resource_limit(&mut self) {
        self.update_retained_resource_footprint();
        while self.stats.package_cache_retained_unique_resource_bytes
            > self.max_retained_unique_resource_bytes
        {
            let Some(key) = self.package_order.pop_front() else {
                break;
            };
            if self.packages.remove(&key).is_some() {
                self.stats.package_cache_evictions += 1;
                self.update_retained_resource_footprint();
            }
        }
    }

    fn finish(mut self, cache_config: CacheConfig) -> RenderSyncCacheReport {
        self.update_retained_resource_footprint();
        let prune = prune_render_cache(
            self.cache_dir,
            cache_config.render_cache_max_entries,
            &self.protected_archive_dirs,
        );
        let static_image_prune = prune_static_image_cache(
            self.cache_dir,
            cache_config.static_image_cache_max_entries,
            cache_config.static_image_cache_max_bytes,
            &self.protected_static_cache_files,
        );
        self.stats.package_cache_entries = self.packages.len();
        self.stats.package_cache_max_entries = cache_config.package_cache_max_entries;
        self.stats.package_cache_max_retained_unique_resource_bytes =
            cache_config.package_cache_max_retained_unique_resource_bytes;
        self.stats.archive_cache_entries = prune.entries_after;
        self.stats.archive_cache_max_entries = cache_config.render_cache_max_entries;
        self.stats.archive_cache_evictions = prune.evictions;
        self.stats.archive_cache_eviction_errors = prune.errors;
        self.stats.static_image_cache_entries = static_image_prune.entries_after;
        self.stats.static_image_cache_max_entries = cache_config.static_image_cache_max_entries;
        self.stats.static_image_cache_bytes = static_image_prune.bytes_after;
        self.stats.static_image_cache_max_bytes = cache_config.static_image_cache_max_bytes;
        self.stats.static_image_cache_evictions = static_image_prune.evictions;
        self.stats.static_image_cache_eviction_errors = static_image_prune.errors;
        self.stats
    }

    fn update_retained_resource_footprint(&mut self) {
        let mut resource_references = 0;
        let mut resource_reference_bytes = 0;
        let mut unique_resources = BTreeSet::new();
        let mut preview_resource_references = 0;
        let mut preview_resource_reference_bytes = 0;
        let mut unique_preview_resources = BTreeSet::new();

        for package in self
            .packages
            .values()
            .filter_map(|package| package.as_ref().ok())
        {
            for package_path in manifest_preview_paths(&package.manifest) {
                let path = package_path.join_to(&package.root);
                preview_resource_references += 1;
                preview_resource_reference_bytes += source_tree_size(&path);
                unique_preview_resources.insert(path);
            }
            for package_path in package_resource_paths(package) {
                let path = package_path.join_to(&package.root);
                resource_references += 1;
                resource_reference_bytes += source_tree_size(&path);
                unique_resources.insert(path);
            }
        }

        self.stats.package_cache_retained_resource_references = resource_references;
        self.stats.package_cache_retained_unique_resources = unique_resources.len();
        self.stats.package_cache_retained_resource_bytes = resource_reference_bytes;
        self.stats.package_cache_retained_unique_resource_bytes = unique_resources
            .iter()
            .map(|path| source_tree_size(path))
            .sum();
        self.stats
            .package_cache_retained_preview_resource_references = preview_resource_references;
        self.stats.package_cache_retained_unique_preview_resources = unique_preview_resources.len();
        self.stats.package_cache_retained_preview_resource_bytes = preview_resource_reference_bytes;
        self.stats
            .package_cache_retained_unique_preview_resource_bytes = unique_preview_resources
            .iter()
            .map(|path| source_tree_size(path))
            .sum();
    }
}

fn manifest_preview_paths(manifest: &Manifest) -> Vec<PackagePath> {
    let mut paths = Vec::new();
    if let Some(path) = &manifest.preview.thumbnail {
        paths.push(path.clone());
    }
    if let Some(path) = &manifest.preview.poster {
        paths.push(path.clone());
    }
    paths
}

fn package_resource_paths(package: &WallpaperPackage) -> Vec<PackagePath> {
    package
        .manifest
        .referenced_paths()
        .unwrap_or_else(|_| Vec::new())
}
