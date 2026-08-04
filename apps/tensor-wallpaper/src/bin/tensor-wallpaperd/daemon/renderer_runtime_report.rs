use serde_json::{Value, json};

use super::RendererRuntimeSnapshot;

pub(super) fn renderer_runtime_report(snapshot: &RendererRuntimeSnapshot) -> Value {
    json!({
        "output_windows": snapshot.output_windows,
        "static_surfaces": snapshot.static_surfaces,
        "static_picture_surfaces": snapshot.static_picture_surfaces,
        "static_css_surfaces": snapshot.static_css_surfaces,
        "static_color_surfaces": snapshot.static_color_surfaces,
        "slideshow_surfaces": snapshot.slideshow_surfaces,
        "video_surfaces": snapshot.video_surfaces,
        "static_surface_resource_references": snapshot.static_surface_resource_references,
        "static_surface_resource_bytes": snapshot.static_surface_resource_bytes,
        "static_surface_unique_resources": snapshot.static_surface_unique_resources,
        "static_surface_unique_resource_bytes": snapshot.static_surface_unique_resource_bytes,
        "static_surface_estimated_decoded_bytes": snapshot.static_surface_estimated_decoded_bytes,
        "slideshow_resource_references": snapshot.slideshow_resource_references,
        "slideshow_resource_bytes": snapshot.slideshow_resource_bytes,
        "slideshow_unique_resources": snapshot.slideshow_unique_resources,
        "slideshow_unique_resource_bytes": snapshot.slideshow_unique_resource_bytes,
        "video_shared_runtimes": snapshot.video_shared_runtimes,
        "video_pipeline_source_references": snapshot.video_pipeline_source_references,
        "video_pipeline_source_reference_bytes": snapshot.video_pipeline_source_reference_bytes,
        "video_pipeline_unique_sources": snapshot.video_pipeline_unique_sources,
        "video_pipeline_unique_source_bytes": snapshot.video_pipeline_unique_source_bytes,
        "video_pipelines": snapshot.video_pipelines,
    })
}

pub(super) fn renderer_telemetry_report(snapshot: &RendererRuntimeSnapshot) -> Value {
    let mut video_qos_messages = 0_u64;
    let mut video_qos_dropped_max = None;

    for pipeline in &snapshot.video_pipelines {
        let Some(frame_stats) = pipeline.get("frame_stats") else {
            continue;
        };
        video_qos_messages = video_qos_messages
            .saturating_add(json_u64(frame_stats, "qos_messages").unwrap_or_default());
        update_optional_max(
            &mut video_qos_dropped_max,
            json_u64(frame_stats, "qos_dropped_max"),
        );
    }

    json!({
        "output_windows": snapshot.output_windows,
        "static_surfaces": snapshot.static_surfaces,
        "static_picture_surfaces": snapshot.static_picture_surfaces,
        "static_css_surfaces": snapshot.static_css_surfaces,
        "static_color_surfaces": snapshot.static_color_surfaces,
        "slideshow_surfaces": snapshot.slideshow_surfaces,
        "video_surfaces": snapshot.video_surfaces,
        "static_surface_resource_references": snapshot.static_surface_resource_references,
        "static_surface_resource_bytes": snapshot.static_surface_resource_bytes,
        "static_surface_unique_resources": snapshot.static_surface_unique_resources,
        "static_surface_unique_resource_bytes": snapshot.static_surface_unique_resource_bytes,
        "static_surface_estimated_decoded_bytes": snapshot.static_surface_estimated_decoded_bytes,
        "slideshow_resource_references": snapshot.slideshow_resource_references,
        "slideshow_resource_bytes": snapshot.slideshow_resource_bytes,
        "slideshow_unique_resources": snapshot.slideshow_unique_resources,
        "slideshow_unique_resource_bytes": snapshot.slideshow_unique_resource_bytes,
        "video_shared_runtimes": snapshot.video_shared_runtimes,
        "video_pipeline_source_references": snapshot.video_pipeline_source_references,
        "video_pipeline_source_reference_bytes": snapshot.video_pipeline_source_reference_bytes,
        "video_pipeline_unique_sources": snapshot.video_pipeline_unique_sources,
        "video_pipeline_unique_source_bytes": snapshot.video_pipeline_unique_source_bytes,
        "video_pipelines": snapshot.video_pipelines.len(),
        "video_qos_messages": video_qos_messages,
        "video_qos_dropped_max": video_qos_dropped_max,
    })
}

fn json_u64(object: &Value, key: &str) -> Option<u64> {
    object.get(key).and_then(Value::as_u64)
}

fn update_optional_max(slot: &mut Option<u64>, value: Option<u64>) {
    let Some(value) = value else {
        return;
    };
    *slot = Some(slot.map_or(value, |current| current.max(value)));
}
