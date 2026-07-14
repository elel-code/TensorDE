use super::*;

pub(super) fn update_render_sync_resource_footprint(
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

pub(super) fn file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

pub(super) fn source_tree_size(path: &Path) -> u64 {
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

pub(super) fn effective_wallpaper_assignment(
    config: Option<&GilderConfig>,
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

pub(super) fn config_wallpaper_assignment(path: &str) -> WallpaperAssignment {
    WallpaperAssignment {
        path: path.to_owned(),
        variant: None,
    }
}

pub(super) fn output_fit_override(config: Option<&GilderConfig>, output_name: &str) -> Option<FitMode> {
    config
        .and_then(|config| config.outputs.get(output_name))
        .and_then(|output| output.fit)
}

pub(super) fn effective_output_render_properties(
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

#[derive(Debug, Clone, Copy)]
pub(super) struct PlaylistRenderContext<'a> {
    pub(super) desktop: &'a DesktopSnapshot,
    pub(super) output_name: &'a str,
    pub(super) output: Option<&'a DesktopOutput>,
    pub(super) local_clock: PlaylistClockKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistClockKey {
    pub local_minute_of_day: u16,
    pub local_weekday: PlaylistWeekday,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistClockCacheKey {
    pub local_minute_of_day: Option<u16>,
    pub local_weekday: Option<PlaylistWeekday>,
}

pub(super) fn select_playlist_item<'a>(
    items: &'a [PlaylistItem],
    selection: PlaylistSelection,
    context: Option<&PlaylistRenderContext<'_>>,
) -> Option<&'a PlaylistItem> {
    match selection {
        PlaylistSelection::FirstMatch => items
            .iter()
            .find(|item| playlist_item_matches(item, context)),
        PlaylistSelection::WeightedRandom => select_weighted_playlist_item(items, context),
    }
}

pub(super) fn select_weighted_playlist_item<'a>(
    items: &'a [PlaylistItem],
    context: Option<&PlaylistRenderContext<'_>>,
) -> Option<&'a PlaylistItem> {
    let candidates = items
        .iter()
        .filter(|item| playlist_item_matches(item, context))
        .collect::<Vec<_>>();
    let total_weight = candidates
        .iter()
        .map(|item| u64::from(item.weight))
        .sum::<u64>();
    if total_weight == 0 {
        return None;
    }

    let mut selected_weight = playlist_weighted_selection_seed(&candidates, context) % total_weight;
    for item in candidates {
        let item_weight = u64::from(item.weight);
        if selected_weight < item_weight {
            return Some(item);
        }
        selected_weight -= item_weight;
    }
    None
}

pub(super) fn playlist_weighted_selection_seed(
    candidates: &[&PlaylistItem],
    context: Option<&PlaylistRenderContext<'_>>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    "gilder-playlist-weighted-random-v1".hash(&mut hasher);
    if let Some(context) = context {
        context.output_name.hash(&mut hasher);
        context.local_clock.local_minute_of_day.hash(&mut hasher);
        playlist_weekday_seed(context.local_clock.local_weekday).hash(&mut hasher);
        playlist_power_seed(context.desktop.power).hash(&mut hasher);
        context.desktop.session_active.hash(&mut hasher);
        context.desktop.session_locked.hash(&mut hasher);
        if let Some(output) = context.output {
            output.name.hash(&mut hasher);
            output.focused.hash(&mut hasher);
            output.visible.hash(&mut hasher);
            output.has_fullscreen.hash(&mut hasher);
        }
    }
    for item in candidates {
        item.id.hash(&mut hasher);
        item.weight.hash(&mut hasher);
    }
    hasher.finish()
}

pub(super) fn playlist_power_seed(power: PowerState) -> u8 {
    match power {
        PowerState::Unknown => 0,
        PowerState::Ac => 1,
        PowerState::Battery => 2,
    }
}

pub(super) fn playlist_weekday_seed(weekday: PlaylistWeekday) -> u8 {
    match weekday {
        PlaylistWeekday::Monday => 1,
        PlaylistWeekday::Tuesday => 2,
        PlaylistWeekday::Wednesday => 3,
        PlaylistWeekday::Thursday => 4,
        PlaylistWeekday::Friday => 5,
        PlaylistWeekday::Saturday => 6,
        PlaylistWeekday::Sunday => 7,
    }
}

pub(super) fn playlist_item_matches(item: &PlaylistItem, context: Option<&PlaylistRenderContext<'_>>) -> bool {
    let conditions = &item.conditions;
    if !conditions.outputs.is_empty() {
        let Some(context) = context else {
            return false;
        };
        if !conditions
            .outputs
            .iter()
            .any(|output| output == context.output_name)
        {
            return false;
        }
    }
    if let Some(power) = conditions.power {
        let Some(context) = context else {
            return false;
        };
        if !playlist_power_matches(power, context.desktop.power) {
            return false;
        }
    }
    if let Some(local_time) = &conditions.local_time {
        let Some(context) = context else {
            return false;
        };
        if !local_time.contains_minute_of_day(context.local_clock.local_minute_of_day) {
            return false;
        }
    }
    if !conditions.weekdays.is_empty() {
        let Some(context) = context else {
            return false;
        };
        if !conditions
            .weekdays
            .contains(&context.local_clock.local_weekday)
        {
            return false;
        }
    }
    if let Some(expected) = conditions.focused {
        let Some(output) = context.and_then(|context| context.output) else {
            return false;
        };
        if output.focused != expected {
            return false;
        }
    }
    if let Some(expected) = conditions.visible {
        let Some(output) = context.and_then(|context| context.output) else {
            return false;
        };
        if output.visible != expected {
            return false;
        }
    }
    if let Some(expected) = conditions.fullscreen {
        let Some(output) = context.and_then(|context| context.output) else {
            return false;
        };
        if output.has_fullscreen != expected {
            return false;
        }
    }
    if let Some(expected) = conditions.session_active {
        let Some(context) = context else {
            return false;
        };
        if context.desktop.session_active != expected {
            return false;
        }
    }
    if let Some(expected) = conditions.session_locked {
        let Some(context) = context else {
            return false;
        };
        if context.desktop.session_locked != expected {
            return false;
        }
    }
    true
}

pub(super) fn playlist_power_matches(condition: PlaylistPowerCondition, power: PowerState) -> bool {
    matches!(
        (condition, power),
        (PlaylistPowerCondition::Unknown, PowerState::Unknown)
            | (PlaylistPowerCondition::Ac, PowerState::Ac)
            | (PlaylistPowerCondition::Battery, PowerState::Battery)
    )
}

pub fn current_playlist_clock_key() -> PlaylistClockKey {
    let now = jiff::Zoned::now();
    PlaylistClockKey {
        local_minute_of_day: playlist_local_time_override()
            .unwrap_or_else(|| zoned_minute_of_day(now.clone())),
        local_weekday: playlist_local_weekday_override().unwrap_or_else(|| zoned_weekday(now)),
    }
}

pub fn current_playlist_clock_cache_key(
    dependency: PlaylistClockDependency,
) -> Option<PlaylistClockCacheKey> {
    playlist_clock_cache_key(dependency, current_playlist_clock_key())
}

pub(super) fn playlist_clock_cache_key(
    dependency: PlaylistClockDependency,
    clock: PlaylistClockKey,
) -> Option<PlaylistClockCacheKey> {
    if dependency == PlaylistClockDependency::None {
        return None;
    }
    Some(PlaylistClockCacheKey {
        local_minute_of_day: dependency
            .uses_minute()
            .then_some(clock.local_minute_of_day),
        local_weekday: dependency.uses_weekday().then_some(clock.local_weekday),
    })
}

pub(super) fn playlist_local_time_override() -> Option<u16> {
    std::env::var("GILDER_PLAYLIST_LOCAL_TIME")
        .ok()
        .as_deref()
        .and_then(crate::core::manifest::parse_playlist_local_time_minute)
}

pub(super) fn playlist_local_weekday_override() -> Option<PlaylistWeekday> {
    std::env::var("GILDER_PLAYLIST_LOCAL_WEEKDAY")
        .ok()
        .as_deref()
        .and_then(parse_playlist_weekday_name)
}

pub(super) fn parse_playlist_weekday_name(value: &str) -> Option<PlaylistWeekday> {
    match value.trim().to_ascii_lowercase().as_str() {
        "monday" | "mon" => Some(PlaylistWeekday::Monday),
        "tuesday" | "tue" => Some(PlaylistWeekday::Tuesday),
        "wednesday" | "wed" => Some(PlaylistWeekday::Wednesday),
        "thursday" | "thu" => Some(PlaylistWeekday::Thursday),
        "friday" | "fri" => Some(PlaylistWeekday::Friday),
        "saturday" | "sat" => Some(PlaylistWeekday::Saturday),
        "sunday" | "sun" => Some(PlaylistWeekday::Sunday),
        _ => None,
    }
}

pub(super) fn zoned_minute_of_day(now: jiff::Zoned) -> u16 {
    let hour = u16::try_from(now.hour()).unwrap_or(0);
    let minute = u16::try_from(now.minute()).unwrap_or(0);
    hour * 60 + minute
}

pub(super) fn zoned_weekday(now: jiff::Zoned) -> PlaylistWeekday {
    gregorian_weekday(now.year().into(), now.month().into(), now.day().into())
}

pub(super) fn gregorian_weekday(year: i32, month: i32, day: i32) -> PlaylistWeekday {
    const MONTH_OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut adjusted_year = year;
    if month < 3 {
        adjusted_year -= 1;
    }
    let sunday_zero = (adjusted_year + adjusted_year / 4 - adjusted_year / 100
        + adjusted_year / 400
        + MONTH_OFFSETS[(month.clamp(1, 12) - 1) as usize]
        + day)
        .rem_euclid(7);
    match sunday_zero {
        0 => PlaylistWeekday::Sunday,
        1 => PlaylistWeekday::Monday,
        2 => PlaylistWeekday::Tuesday,
        3 => PlaylistWeekday::Wednesday,
        4 => PlaylistWeekday::Thursday,
        5 => PlaylistWeekday::Friday,
        _ => PlaylistWeekday::Saturday,
    }
}

pub(super) fn effective_dynamic_wallpaper_entry(
    entry: &WallpaperEntry,
    playlist_context: &PlaylistRenderContext<'_>,
) -> bool {
    effective_render_wallpaper_entry(entry, playlist_context)
        .map(dynamic_wallpaper_entry)
        .unwrap_or(false)
}

pub(super) fn effective_render_wallpaper_entry<'a>(
    entry: &'a WallpaperEntry,
    playlist_context: &PlaylistRenderContext<'_>,
) -> Option<&'a WallpaperEntry> {
    match entry {
        WallpaperEntry::Playlist { items, selection } => {
            select_playlist_item(items, *selection, Some(playlist_context))
                .map(|item| item.entry.as_ref())
        }
        _ => Some(entry),
    }
}

pub(super) fn playlist_entry_clock_dependency(entry: &WallpaperEntry) -> PlaylistClockDependency {
    let WallpaperEntry::Playlist { items, selection } = entry else {
        return PlaylistClockDependency::None;
    };
    let mut dependency = if *selection == PlaylistSelection::WeightedRandom {
        PlaylistClockDependency::MinuteAndWeekday
    } else {
        PlaylistClockDependency::None
    };
    for item in items {
        if item.conditions.local_time.is_some() {
            dependency = dependency.merge(PlaylistClockDependency::Minute);
        }
        if !item.conditions.weekdays.is_empty() {
            dependency = dependency.merge(PlaylistClockDependency::Weekday);
        }
    }
    dependency
}

pub(super) fn dynamic_wallpaper_entry(entry: &WallpaperEntry) -> bool {
    match entry {
        WallpaperEntry::Video { .. }
        | WallpaperEntry::Slideshow { .. }
        | WallpaperEntry::Web { .. }
        | WallpaperEntry::Shader { .. }
        | WallpaperEntry::Scene { .. } => true,
        WallpaperEntry::StaticImage { .. } => false,
        WallpaperEntry::Playlist { items, .. } => items
            .iter()
            .any(|item| dynamic_wallpaper_entry(item.entry.as_ref())),
    }
}

pub fn static_wallpaper_plan_for_assignment(
    output_name: impl Into<String>,
    assignment: &WallpaperAssignment,
    cache_dir: impl AsRef<Path>,
) -> Result<StaticWallpaperPlan, RendererPlanError> {
    let package = load_assigned_package(assignment, cache_dir.as_ref())?;
    let output_state = OutputState {
        wallpaper: Some(assignment.clone()),
        ..OutputState::default()
    };
    static_wallpaper_plan(output_name, &package, &output_state)?
        .ok_or(RendererPlanError::MissingAssignment)
}

pub fn wallpaper_plan_for_assignment(
    output_name: impl Into<String>,
    assignment: &WallpaperAssignment,
    cache_dir: impl AsRef<Path>,
    performance: &PerformanceDecision,
    fit_override: Option<FitMode>,
) -> Result<WallpaperRenderPlan, RendererPlanError> {
    wallpaper_plan_for_assignment_with_target(
        output_name,
        assignment,
        cache_dir,
        performance,
        VideoDecoderPolicy::default(),
        fit_override,
        None,
    )
}

pub(super) fn wallpaper_plan_for_assignment_with_target(
    output_name: impl Into<String>,
    assignment: &WallpaperAssignment,
    cache_dir: impl AsRef<Path>,
    performance: &PerformanceDecision,
    video_decoder_policy: VideoDecoderPolicy,
    fit_override: Option<FitMode>,
    render_target: Option<RenderTargetSize>,
) -> Result<WallpaperRenderPlan, RendererPlanError> {
    let package = load_assigned_package(assignment, cache_dir.as_ref())?;
    wallpaper_plan_with_target(
        output_name,
        &package,
        performance,
        video_decoder_policy,
        fit_override,
        assignment.variant.as_deref(),
        render_target,
        None,
        None,
        false,
        None,
    )
}

pub fn wallpaper_plan(
    output_name: impl Into<String>,
    package: &WallpaperPackage,
    performance: &PerformanceDecision,
    fit_override: Option<FitMode>,
    variant_id: Option<&str>,
) -> Result<WallpaperRenderPlan, RendererPlanError> {
    wallpaper_plan_with_target(
        output_name,
        package,
        performance,
        VideoDecoderPolicy::default(),
        fit_override,
        variant_id,
        None,
        None,
        None,
        false,
        None,
    )
}

pub(super) fn wallpaper_plan_with_target(
    output_name: impl Into<String>,
    package: &WallpaperPackage,
    performance: &PerformanceDecision,
    video_decoder_policy: VideoDecoderPolicy,
    fit_override: Option<FitMode>,
    variant_id: Option<&str>,
    render_target: Option<RenderTargetSize>,
    playlist_context: Option<&PlaylistRenderContext<'_>>,
    render_properties: Option<&BTreeMap<String, Value>>,
    cursor_parallax_input_ready: bool,
    static_image_cache: Option<&mut StaticImageCacheContext<'_>>,
) -> Result<WallpaperRenderPlan, RendererPlanError> {
    let output_name = output_name.into();
    let explicit_variant_source = explicit_variant_source(package, variant_id)?;
    wallpaper_entry_plan_with_target(
        &output_name,
        package,
        &package.manifest.entry,
        performance,
        video_decoder_policy,
        fit_override,
        explicit_variant_source,
        true,
        render_target,
        playlist_context,
        render_properties,
        cursor_parallax_input_ready,
        static_image_cache,
    )
}

pub(super) fn wallpaper_entry_plan_with_target(
    output_name: &str,
    package: &WallpaperPackage,
    entry: &WallpaperEntry,
    performance: &PerformanceDecision,
    video_decoder_policy: VideoDecoderPolicy,
    fit_override: Option<FitMode>,
    explicit_variant_source: Option<&PackagePath>,
    allow_automatic_variants: bool,
    render_target: Option<RenderTargetSize>,
    playlist_context: Option<&PlaylistRenderContext<'_>>,
    render_properties: Option<&BTreeMap<String, Value>>,
    cursor_parallax_input_ready: bool,
    static_image_cache: Option<&mut StaticImageCacheContext<'_>>,
) -> Result<WallpaperRenderPlan, RendererPlanError> {
    let variant_render_target = allow_automatic_variants.then_some(render_target).flatten();
    match entry {
        WallpaperEntry::StaticImage {
            source,
            fit,
            background,
            width,
            height,
            ..
        } => Ok(WallpaperRenderPlan::StaticImage(StaticWallpaperPlan {
            output_name: output_name.to_owned(),
            source: static_image_source_path(
                package,
                source,
                effective_fit(*fit, fit_override),
                explicit_variant_source,
                variant_render_target,
                source_dimensions(*width, *height),
                static_image_cache,
            ),
            fit: effective_fit(*fit, fit_override),
            background: background.clone(),
        })),
        WallpaperEntry::Video {
            source,
            poster,
            loop_playback,
            muted,
            fit,
            max_fps,
            start_offset_ms,
        } => {
            let poster = poster
                .as_ref()
                .or(package.manifest.preview.poster.as_ref())
                .map(|poster| poster.join_to(&package.root));
            Ok(WallpaperRenderPlan::Video(VideoWallpaperPlan {
                output_name: output_name.to_owned(),
                source: selected_variant_source(
                    package,
                    explicit_variant_source,
                    variant_render_target,
                )
                .unwrap_or(source)
                .join_to(&package.root),
                poster,
                fit: effective_fit(*fit, fit_override),
                loop_playback: *loop_playback,
                muted: effective_muted(*muted, package.manifest.runtime.allow_audio),
                manifest_max_fps: *max_fps,
                target_max_fps: effective_max_fps(*max_fps, performance.max_fps),
                decoder_policy: video_decoder_policy,
                start_offset_ms: *start_offset_ms,
            }))
        }
        WallpaperEntry::Slideshow {
            sources,
            interval_ms,
            transition,
            fit,
        } => Ok(WallpaperRenderPlan::Slideshow(SlideshowWallpaperPlan {
            output_name: output_name.to_owned(),
            sources: sources
                .iter()
                .map(|source| source.join_to(&package.root))
                .collect(),
            interval_ms: *interval_ms,
            transition: *transition,
            fit: effective_fit(*fit, fit_override),
            target_max_fps: performance.max_fps,
        })),
        WallpaperEntry::Web { fallback, .. } => {
            let Some(fallback) = fallback else {
                return Err(RendererPlanError::UnsupportedEntry(entry.kind().as_str()));
            };
            Ok(WallpaperRenderPlan::StaticImage(StaticWallpaperPlan {
                output_name: output_name.to_owned(),
                source: fallback.join_to(&package.root),
                fit: effective_fit(FitMode::Cover, fit_override),
                background: Some("#000000".to_owned()),
            }))
        }
        WallpaperEntry::Shader { fallback, .. } => {
            let Some(fallback) = fallback else {
                return Err(RendererPlanError::UnsupportedEntry(entry.kind().as_str()));
            };
            Ok(WallpaperRenderPlan::StaticImage(StaticWallpaperPlan {
                output_name: output_name.to_owned(),
                source: fallback.join_to(&package.root),
                fit: effective_fit(FitMode::Cover, fit_override),
                background: Some("#000000".to_owned()),
            }))
        }
        WallpaperEntry::Scene { source, max_fps } => {
            Ok(WallpaperRenderPlan::Scene(scene_wallpaper_plan(
                output_name.to_owned(),
                package,
                source,
                *max_fps,
                performance,
                fit_override,
                render_target,
                render_properties,
                cursor_parallax_input_ready,
            )?))
        }
        WallpaperEntry::Playlist { items, selection } => {
            let item = select_playlist_item(items, *selection, playlist_context)
                .ok_or(RendererPlanError::PlaylistNoMatch)?;
            wallpaper_entry_plan_with_target(
                output_name,
                package,
                item.entry.as_ref(),
                performance,
                video_decoder_policy,
                fit_override,
                None,
                false,
                render_target,
                playlist_context,
                render_properties,
                cursor_parallax_input_ready,
                static_image_cache,
            )
        }
    }
}

pub(super) fn scene_wallpaper_plan(
    output_name: String,
    package: &WallpaperPackage,
    source: &PackagePath,
    manifest_max_fps: Option<u32>,
    performance: &PerformanceDecision,
    fit_override: Option<FitMode>,
    _render_target: Option<RenderTargetSize>,
    render_properties: Option<&BTreeMap<String, Value>>,
    cursor_parallax_input_ready: bool,
) -> Result<SceneWallpaperPlan, RendererPlanError> {
    let source_path = source.join_to(&package.root);
    let file = fs::File::open(&source_path).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to open scene engine binary {}: {err}",
            source_path.display()
        ))
    })?;
    let storage = SceneStorage::from_binary_reader(file).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to load scene engine binary {}: {err}",
            source_path.display()
        ))
    })?;
    let scene_engine = RenderingServer::new(&storage).scene_engine_render_plan();
    let render_plan = scene_engine.renderer_scene_render;
    let scene_size = if storage.project().logical_width > 0 && storage.project().logical_height > 0
    {
        Some(SceneSize {
            width: storage.project().logical_width,
            height: storage.project().logical_height,
        })
    } else {
        None
    };
    let mut scene_systems = SceneSystems::default();
    if render_plan.render_graph_count > 0 || render_plan.shader_contract_count > 0 {
        scene_systems.shader_material_graph = SceneSystemStatus::Ready;
    }

    Ok(SceneWallpaperPlan {
        output_name,
        source: Some(source_path),
        manifest_max_fps,
        target_max_fps: effective_max_fps(manifest_max_fps, performance.max_fps),
        snapshot_time_ms: 0,
        scene_size,
        scene_fit: effective_fit(FitMode::Cover, fit_override),
        scene_systems,
        audio_cue_count: 0,
        bound_properties: Vec::new(),
        timeline_animation_count: 0,
        timeline_animated_layer_count: 0,
        puppet_animation_layer_count: 0,
        property_binding_count: 0,
        cursor_parallax_input_ready,
        scene_input_properties: render_properties.cloned().unwrap_or_default(),
        scene_engine: Some(scene_engine),
        scene_scenescript_binding_count: 0,
        scene_material_graph_count: render_plan.material_count,
        scene_material_graph_resource_count: render_plan.resource_count,
        scene_effect_graph_count: render_plan.render_graph_count,
        scene_mesh_count: render_plan.mesh_count,
        scene_mesh_vertex_count: render_plan.mesh_vertex_count,
        scene_mesh_index_count: render_plan.mesh_index_count,
        scene_audio_response_binding_count: 0,
        unsupported_scene_features: scene_engine_unsupported_features(&storage),
        display: None,
        layers: Vec::new(),
    })
}

pub(super) fn scene_engine_unsupported_features(storage: &SceneStorage) -> Vec<String> {
    storage
        .document()
        .unsupported
        .iter()
        .map(|unsupported| {
            let feature = storage
                .string(unsupported.feature)
                .unwrap_or("<invalid-feature>");
            let containment = storage
                .string(unsupported.containment)
                .unwrap_or("<invalid-containment>");
            format!(
                "scene-engine:{}:object={}:pass={}:{}",
                feature, unsupported.object.0, unsupported.pass_index, containment
            )
        })
        .collect()
}

pub(super) fn effective_fit(manifest_fit: FitMode, output_fit: Option<FitMode>) -> FitMode {
    output_fit.unwrap_or(manifest_fit)
}

pub(super) fn explicit_variant_source<'a>(
    package: &'a WallpaperPackage,
    variant_id: Option<&str>,
) -> Result<Option<&'a PackagePath>, RendererPlanError> {
    let Some(variant_id) = variant_id else {
        return Ok(None);
    };
    package
        .manifest
        .variants
        .iter()
        .find(|variant| variant.id == variant_id)
        .map(|variant| Some(&variant.source))
        .ok_or_else(|| RendererPlanError::MissingVariant(variant_id.to_owned()))
}

pub(super) fn selected_variant_source<'a>(
    package: &'a WallpaperPackage,
    explicit_source: Option<&'a PackagePath>,
    render_target: Option<RenderTargetSize>,
) -> Option<&'a PackagePath> {
    explicit_source.or_else(|| automatic_variant_source(package, render_target))
}

pub(super) struct StaticImageCacheContext<'a> {
    pub(super) cache_dir: &'a Path,
    pub(super) max_entries: usize,
    pub(super) stats: &'a mut RenderSyncCacheReport,
    pub(super) protected_files: &'a mut BTreeSet<PathBuf>,
    pub(super) ffmpeg: Option<&'a Path>,
}

pub(super) fn static_image_source_path(
    package: &WallpaperPackage,
    source: &PackagePath,
    fit: FitMode,
    explicit_source: Option<&PackagePath>,
    render_target: Option<RenderTargetSize>,
    source_dimensions: Option<RenderTargetSize>,
    static_image_cache: Option<&mut StaticImageCacheContext<'_>>,
) -> PathBuf {
    if let Some(selected) = selected_variant_source(package, explicit_source, render_target) {
        return selected.join_to(&package.root);
    }

    let source_path = source.join_to(&package.root);
    if explicit_source.is_some() {
        return source_path;
    }

    let Some(cache) = static_image_cache else {
        return source_path;
    };
    cached_static_image_variant(&source_path, fit, render_target, source_dimensions, cache)
        .unwrap_or(source_path)
}

pub(super) fn source_dimensions(width: Option<u32>, height: Option<u32>) -> Option<RenderTargetSize> {
    Some(RenderTargetSize {
        width: width?,
        height: height?,
    })
}
