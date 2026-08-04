#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
/// Retained cache and resource-footprint telemetry exposed by render planning.
pub struct RenderSyncCacheReport {
    #[serde(default)]
    pub package_cache_entries: usize,
    #[serde(default)]
    pub package_cache_max_entries: usize,
    #[serde(default)]
    pub package_cache_max_retained_unique_resource_bytes: u64,
    #[serde(default)]
    pub package_cache_hits: u64,
    #[serde(default)]
    pub package_cache_misses: u64,
    #[serde(default)]
    pub package_cache_evictions: u64,
    #[serde(default)]
    pub package_cache_retained_resource_references: usize,
    #[serde(default)]
    pub package_cache_retained_unique_resources: usize,
    #[serde(default)]
    pub package_cache_retained_resource_bytes: u64,
    #[serde(default)]
    pub package_cache_retained_unique_resource_bytes: u64,
    #[serde(default)]
    pub package_cache_retained_preview_resource_references: usize,
    #[serde(default)]
    pub package_cache_retained_unique_preview_resources: usize,
    #[serde(default)]
    pub package_cache_retained_preview_resource_bytes: u64,
    #[serde(default)]
    pub package_cache_retained_unique_preview_resource_bytes: u64,
    #[serde(default)]
    pub archive_cache_entries: usize,
    #[serde(default)]
    pub archive_cache_max_entries: usize,
    #[serde(default)]
    pub archive_cache_reuses: u64,
    #[serde(default)]
    pub archive_cache_extractions: u64,
    #[serde(default)]
    pub archive_cache_evictions: u64,
    #[serde(default)]
    pub archive_cache_eviction_errors: u64,
    #[serde(default)]
    pub static_image_cache_entries: usize,
    #[serde(default)]
    pub static_image_cache_max_entries: usize,
    #[serde(default)]
    pub static_image_cache_bytes: u64,
    #[serde(default)]
    pub static_image_cache_max_bytes: u64,
    #[serde(default)]
    pub static_image_cache_generations: u64,
    #[serde(default)]
    pub static_image_cache_reuses: u64,
    #[serde(default)]
    pub static_image_cache_generation_errors: u64,
    #[serde(default)]
    pub static_image_cache_evictions: u64,
    #[serde(default)]
    pub static_image_cache_eviction_errors: u64,
    #[serde(default)]
    pub planned_video_source_references: usize,
    #[serde(default)]
    pub planned_unique_video_sources: usize,
    #[serde(default)]
    pub planned_duplicate_video_source_references: usize,
    #[serde(default)]
    pub planned_max_video_source_outputs: usize,
    #[serde(default)]
    pub planned_video_source_reference_bytes: u64,
    #[serde(default)]
    pub planned_unique_video_source_bytes: u64,
    #[serde(default)]
    pub planned_static_image_resources: usize,
    #[serde(default)]
    pub planned_video_poster_resources: usize,
    #[serde(default)]
    pub planned_slideshow_image_resources: usize,
    #[serde(default)]
    pub planned_scene_image_resources: usize,
    #[serde(default)]
    pub planned_image_resource_references: usize,
    #[serde(default)]
    pub planned_unique_image_resources: usize,
    #[serde(default)]
    pub planned_static_image_resource_bytes: u64,
    #[serde(default)]
    pub planned_video_poster_resource_bytes: u64,
    #[serde(default)]
    pub planned_slideshow_image_resource_bytes: u64,
    #[serde(default)]
    pub planned_scene_image_resource_bytes: u64,
    #[serde(default)]
    pub planned_image_resource_reference_bytes: u64,
    #[serde(default)]
    pub planned_unique_image_resource_bytes: u64,
}
