//! Retained resource accounting and output-level render policy resolution.

use super::*;

pub(in crate::renderer) fn update_render_sync_resource_footprint(
    report: &mut RenderSyncCacheReport,
    plans: &[StaticWallpaperPlan],
    video_plans: &[VideoWallpaperPlan],
    slideshow_plans: &[SlideshowWallpaperPlan],
    scene_plans: &[SceneWallpaperPlan],
) {
    let video_poster_resources = video_plans
        .iter()
        .filter(|plan| plan.poster.is_some())
        .count();
    let slideshow_image_resources = slideshow_plans
        .iter()
        .map(|plan| plan.sources.len())
        .sum::<usize>();
    let static_image_resources = plans.len();
    let static_image_resource_bytes = plans
        .iter()
        .map(|plan| file_size(&plan.source))
        .sum::<u64>();
    let video_poster_resource_bytes = video_plans
        .iter()
        .filter_map(|plan| plan.poster.as_ref())
        .map(|poster| file_size(poster))
        .sum::<u64>();
    let slideshow_image_resource_bytes = slideshow_plans
        .iter()
        .flat_map(|plan| plan.sources.iter())
        .map(|source| file_size(source))
        .sum::<u64>();
    let scene_image_resources = scene_plans
        .iter()
        .map(|plan| plan.image_sources().len())
        .sum::<usize>();
    let scene_image_resource_bytes = scene_plans
        .iter()
        .flat_map(SceneWallpaperPlan::image_sources)
        .map(file_size)
        .sum::<u64>();
    let mut unique_image_resources = BTreeSet::new();
    unique_image_resources.extend(plans.iter().map(|plan| plan.source.clone()));
    unique_image_resources.extend(
        slideshow_plans
            .iter()
            .flat_map(|plan| plan.sources.iter().cloned()),
    );
    unique_image_resources.extend(
        scene_plans
            .iter()
            .flat_map(SceneWallpaperPlan::image_sources)
            .map(Path::to_path_buf),
    );
    let mut video_source_counts = BTreeMap::new();
    for plan in video_plans {
        *video_source_counts
            .entry(plan.source.clone())
            .or_insert(0_usize) += 1;
    }
    let planned_video_source_reference_bytes = video_plans
        .iter()
        .map(|plan| file_size(&plan.source))
        .sum::<u64>();
    let planned_unique_video_source_bytes = video_source_counts
        .keys()
        .map(|source| file_size(source))
        .sum::<u64>();

    report.planned_static_image_resources = static_image_resources;
    report.planned_video_poster_resources = video_poster_resources;
    report.planned_slideshow_image_resources = slideshow_image_resources;
    report.planned_scene_image_resources = scene_image_resources;
    report.planned_image_resource_references =
        plans.len() + slideshow_image_resources + scene_image_resources;
    report.planned_unique_image_resources = unique_image_resources.len();
    report.planned_static_image_resource_bytes = static_image_resource_bytes;
    report.planned_video_poster_resource_bytes = video_poster_resource_bytes;
    report.planned_slideshow_image_resource_bytes = slideshow_image_resource_bytes;
    report.planned_scene_image_resource_bytes = scene_image_resource_bytes;
    report.planned_image_resource_reference_bytes = plans
        .iter()
        .map(|plan| file_size(&plan.source))
        .sum::<u64>()
        + slideshow_image_resource_bytes
        + scene_image_resource_bytes;
    report.planned_unique_image_resource_bytes = unique_image_resources
        .iter()
        .map(|source| file_size(source))
        .sum::<u64>();
    report.planned_video_source_references = video_plans.len();
    report.planned_unique_video_sources = video_source_counts.len();
    report.planned_duplicate_video_source_references =
        video_plans.len().saturating_sub(video_source_counts.len());
    report.planned_max_video_source_outputs = video_source_counts
        .values()
        .copied()
        .max()
        .unwrap_or_default();
    report.planned_video_source_reference_bytes = planned_video_source_reference_bytes;
    report.planned_unique_video_source_bytes = planned_unique_video_source_bytes;
}

pub(in crate::renderer) fn file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

pub(in crate::renderer) fn source_tree_size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.file_type().is_symlink() {
        return 0;
    }
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }

    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| source_tree_size(&entry.path()))
        .sum()
}

pub(in crate::renderer) fn effective_wallpaper_assignment(
    config: Option<&TensorWallpaperConfig>,
    state: &AppState,
    output_name: &str,
    output_state: &OutputState,
) -> Option<WallpaperAssignment> {
    output_state
        .wallpaper
        .clone()
        .or_else(|| state.default_wallpaper.clone())
        .or_else(|| {
            config
                .and_then(|config| config.outputs.get(output_name))
                .and_then(|output| output.wallpaper.as_ref())
                .map(|path| config_wallpaper_assignment(path))
        })
        .or_else(|| {
            config
                .and_then(|config| config.default_wallpaper.as_ref())
                .map(|path| config_wallpaper_assignment(path))
        })
}

pub(in crate::renderer) fn config_wallpaper_assignment(path: &str) -> WallpaperAssignment {
    WallpaperAssignment {
        path: path.to_owned(),
        variant: None,
    }
}

pub(in crate::renderer) fn output_fit_override(
    config: Option<&TensorWallpaperConfig>,
    output_name: &str,
) -> Option<FitMode> {
    config
        .and_then(|config| config.outputs.get(output_name))
        .and_then(|output| output.fit)
}

pub(in crate::renderer) fn effective_output_render_properties(
    state: &AppState,
    output_state: &OutputState,
    output: Option<&DesktopOutput>,
) -> BTreeMap<String, Value> {
    let mut properties = state.properties.clone();
    properties.extend(output_state.properties.clone());
    if let Some(parallax) = output.and_then(|output| output.cursor_parallax) {
        properties.insert("scene.parallax.x".to_owned(), Value::from(parallax.x));
        properties.insert("scene.parallax.y".to_owned(), Value::from(parallax.y));
    }
    properties
}
