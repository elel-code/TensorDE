use super::*;

#[test]
fn formats_render_decisions_as_csv() {
    let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"plans":[{"output_name":"HDMI-A-1","source":"/tmp/poster.jpg","fit":"contain","background":"#000000"}],"video_plans":[{"output_name":"eDP-1","source":"/tmp/loop.webm","poster":"/tmp/poster.jpg","fit":"cover","loop_playback":true,"muted":true,"manifest_max_fps":60,"target_max_fps":24,"start_offset_ms":0}],"slideshow_plans":[{"output_name":"DP-1","sources":["/tmp/a.jpg","/tmp/b.jpg"],"interval_ms":300000,"transition":"none","fit":"cover","target_max_fps":12}],"scene_plans":[{"output_name":"DP-2","source":"/tmp/scene.gscn","target_max_fps":30,"display":{"type":"image","source":"/tmp/scene-poster.jpg","fit":"cover","background":"#000000"}}],"decisions":[{"output_name":"eDP-1","action":"render","performance":{"mode":"throttled","max_fps":24,"reason":"battery"},"wallpaper":"/tmp/wall.gwpdir"},{"output_name":"HDMI-A-1","action":"remove","performance":{"mode":"paused","max_fps":null,"reason":"fullscreen"},"wallpaper":null},{"output_name":"DP-1","action":"render","performance":{"mode":"throttled","max_fps":12,"reason":"unfocused"},"wallpaper":"/tmp/slides.gwpdir"},{"output_name":"DP-2","action":"render","performance":{"mode":"throttled","max_fps":30,"reason":"adaptive"},"wallpaper":"/tmp/scene.gwpdir"}]}}}"##;

    let csv = render_decisions_csv(response).unwrap();

    assert_eq!(
        csv,
        "output_name,action,mode,reason,max_fps,wallpaper,plan_kind,source,fit,target_max_fps,muted\n\
             eDP-1,render,throttled,battery,24,/tmp/wall.gwpdir,video,/tmp/loop.webm,cover,24,true\n\
             HDMI-A-1,remove,paused,fullscreen,,,static-image,/tmp/poster.jpg,contain,,\n\
             DP-1,render,throttled,unfocused,12,/tmp/slides.gwpdir,slideshow,/tmp/a.jpg,cover,12,\n\
             DP-2,render,throttled,adaptive,30,/tmp/scene.gwpdir,scene,/tmp/scene-poster.jpg,cover,30,\n"
    );
}

#[test]
fn escapes_csv_cells() {
    let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"plans":[{"output_name":"DP,1","source":"/tmp/a,b.png","fit":"cover","background":null}],"decisions":[{"output_name":"DP,1","action":"render","performance":{"mode":"active","max_fps":60,"reason":"interactive"},"wallpaper":"/tmp/a\"b.gwpdir"}]}}}"##;

    let csv = render_decisions_csv(response).unwrap();

    assert_eq!(
        csv,
        "output_name,action,mode,reason,max_fps,wallpaper,plan_kind,source,fit,target_max_fps,muted\n\
             \"DP,1\",render,active,interactive,60,\"/tmp/a\"\"b.gwpdir\",static-image,\"/tmp/a,b.png\",cover,,\n"
    );
}

#[test]
fn formats_daemon_telemetry_as_csv() {
    let response = r##"{"jsonrpc":"2.0","id":1,"result":{"render_sync":{"plans":[],"video_plans":[],"decisions":[]},"telemetry":{"desktop":{"refreshes":7,"refresh_skips":11,"changes":2,"last_refresh_age_ms":42},"render_sync":{"cache_hits":23,"cache_misses":5,"updates_queued":3,"updates_skipped":2,"package_cache_entries":2,"package_cache_max_entries":5,"package_cache_max_retained_unique_resource_bytes":1048576,"package_cache_hits":4,"package_cache_misses":3,"package_cache_evictions":1,"archive_cache_entries":8,"archive_cache_max_entries":32,"archive_cache_reuses":6,"archive_cache_extractions":1,"archive_cache_evictions":9,"archive_cache_evictions_latest":2,"archive_cache_eviction_errors":1,"archive_cache_eviction_errors_latest":1,"static_image_cache_entries":2,"static_image_cache_max_entries":32,"static_image_cache_bytes":5120,"static_image_cache_max_bytes":1048576,"static_image_cache_generations":1,"static_image_cache_reuses":4,"static_image_cache_generation_errors":0,"static_image_cache_evictions":3,"static_image_cache_eviction_errors":0,"planned_video_source_references":3,"planned_unique_video_sources":2,"planned_duplicate_video_source_references":1,"planned_max_video_source_outputs":2,"planned_video_source_reference_bytes":9000,"planned_unique_video_source_bytes":6000,"planned_static_image_resources":2,"planned_video_poster_resources":1,"planned_slideshow_image_resources":3,"planned_image_resource_references":6,"planned_unique_image_resources":5,"planned_static_image_resource_bytes":2048,"planned_video_poster_resource_bytes":512,"planned_slideshow_image_resource_bytes":4096,"planned_image_resource_reference_bytes":6656,"planned_unique_image_resource_bytes":6400,"package_cache_retained_resource_references":9,"package_cache_retained_unique_resources":7,"package_cache_retained_resource_bytes":12345,"package_cache_retained_unique_resource_bytes":12000,"package_cache_retained_preview_resource_references":4,"package_cache_retained_unique_preview_resources":3,"package_cache_retained_preview_resource_bytes":7000,"package_cache_retained_unique_preview_resource_bytes":6500},"adaptive":{"refreshes":5,"refresh_skips":6,"snapshot":{"sample":{"cpu_pressure_some_avg10_x100":123,"memory_pressure_some_avg10_x100":45,"temperature_max_millicelsius":73500,"power_external_online":true,"power_system_battery_present":true,"power_battery_discharging":false,"power_battery_capacity_percent":88,"power_battery_power_microwatts":12000000,"gpu_busy_percent_avg":37,"gpu_busy_percent_max":72,"gpu_busy_sources":["renderD128","card0"]},"active_triggers":[{"metric":"temperature-max-celsius","value_x100":7350,"threshold_x100":7000}]},"action":[{"output_name":"eDP-1","type":"throttle","configured_action":"pause-unfocused","max_fps":15},{"output_name":"HDMI-A-1","type":"pause-dynamic","scope":"dynamic-wallpapers"}]},"renderer":{"output_windows":3,"static_surfaces":2,"static_picture_surfaces":1,"static_css_surfaces":1,"static_color_surfaces":0,"slideshow_surfaces":1,"video_surfaces":2,"video_shared_runtimes":1,"static_surface_resource_references":2,"static_surface_resource_bytes":2048,"static_surface_unique_resources":1,"static_surface_unique_resource_bytes":1024,"static_surface_estimated_decoded_bytes":8294400,"slideshow_resource_references":4,"slideshow_resource_bytes":8192,"slideshow_unique_resources":3,"slideshow_unique_resource_bytes":6144,"video_pipeline_source_references":3,"video_pipeline_source_reference_bytes":18000,"video_pipeline_unique_sources":2,"video_pipeline_unique_source_bytes":12000,"video_pipelines":2,"video_qos_messages":7,"video_qos_dropped_max":3}}}}"##;

    let csv = render_telemetry_csv(response).unwrap();

    assert_eq!(
        csv,
        "desktop_refreshes,desktop_refresh_skips,desktop_changes,last_desktop_refresh_age_ms,render_sync_cache_hits,render_sync_cache_misses,render_sync_updates_queued,render_sync_updates_skipped,render_sync_package_cache_entries,render_sync_package_cache_max_entries,render_sync_package_cache_hits,render_sync_package_cache_misses,render_sync_package_cache_evictions,render_sync_archive_cache_entries,render_sync_archive_cache_max_entries,render_sync_archive_cache_reuses,render_sync_archive_cache_extractions,render_sync_archive_cache_evictions,render_sync_archive_cache_evictions_latest,render_sync_archive_cache_eviction_errors,render_sync_archive_cache_eviction_errors_latest,render_sync_planned_static_image_resources,render_sync_planned_video_poster_resources,render_sync_planned_slideshow_image_resources,render_sync_planned_image_resource_references,render_sync_planned_unique_image_resources,adaptive_refreshes,adaptive_refresh_skips,adaptive_active_triggers,cpu_pressure_some_avg10_x100,memory_pressure_some_avg10_x100,temperature_max_millicelsius,power_external_online,power_system_battery_present,power_battery_discharging,power_battery_capacity_percent,power_battery_power_microwatts,gpu_busy_percent_avg,gpu_busy_percent_max,gpu_busy_sources,adaptive_action_types,adaptive_action_scopes,adaptive_action_configured_actions,adaptive_action_max_fps,renderer_output_windows,renderer_static_surfaces,renderer_static_picture_surfaces,renderer_static_css_surfaces,renderer_static_color_surfaces,renderer_slideshow_surfaces,renderer_video_surfaces,renderer_video_shared_runtimes,renderer_video_pipelines,renderer_video_qos_messages,renderer_video_qos_dropped_max,render_sync_planned_static_image_resource_bytes,render_sync_planned_video_poster_resource_bytes,render_sync_planned_slideshow_image_resource_bytes,render_sync_planned_image_resource_reference_bytes,render_sync_planned_unique_image_resource_bytes,render_sync_package_cache_retained_resource_references,render_sync_package_cache_retained_unique_resources,render_sync_package_cache_retained_resource_bytes,render_sync_package_cache_retained_unique_resource_bytes,renderer_static_surface_resource_references,renderer_static_surface_resource_bytes,renderer_slideshow_resource_references,renderer_slideshow_resource_bytes,renderer_static_surface_unique_resources,renderer_static_surface_unique_resource_bytes,renderer_static_surface_estimated_decoded_bytes,renderer_slideshow_unique_resources,renderer_slideshow_unique_resource_bytes,render_sync_static_image_cache_entries,render_sync_static_image_cache_max_entries,render_sync_static_image_cache_generations,render_sync_static_image_cache_reuses,render_sync_static_image_cache_generation_errors,render_sync_static_image_cache_evictions,render_sync_static_image_cache_eviction_errors,render_sync_planned_video_source_references,render_sync_planned_unique_video_sources,render_sync_planned_duplicate_video_source_references,render_sync_planned_max_video_source_outputs,render_sync_planned_video_source_reference_bytes,render_sync_planned_unique_video_source_bytes,renderer_video_pipeline_source_references,renderer_video_pipeline_source_reference_bytes,renderer_video_pipeline_unique_sources,renderer_video_pipeline_unique_source_bytes,render_sync_package_cache_max_retained_unique_resource_bytes,render_sync_static_image_cache_bytes,render_sync_static_image_cache_max_bytes,render_sync_package_cache_retained_preview_resource_references,render_sync_package_cache_retained_unique_preview_resources,render_sync_package_cache_retained_preview_resource_bytes,render_sync_package_cache_retained_unique_preview_resource_bytes\n\
             7,11,2,42,23,5,3,2,2,5,4,3,1,8,32,6,1,9,2,1,1,2,1,3,6,5,5,6,1,123,45,73500,true,true,false,88,12000000,37,72,card0|renderD128,pause-dynamic|throttle,dynamic-wallpapers,pause-unfocused,15,3,2,1,1,0,1,2,1,2,7,3,2048,512,4096,6656,6400,9,7,12345,12000,2,2048,4,8192,1,1024,8294400,3,6144,2,32,1,4,0,3,0,3,2,1,2,9000,6000,3,18000,2,12000,1048576,5120,1048576,4,3,7000,6500\n"
    );
}

#[test]
fn parses_status_file_invocation() {
    let args = vec![
        "status".to_owned(),
        "--decisions-csv".to_owned(),
        "--from-file".to_owned(),
        "status.json".to_owned(),
    ];

    assert_eq!(
        parse_invocation(&args).unwrap(),
        Invocation {
            command: tensor_wallpaper::ipc::ClientCommand::Status,
            format: ResponseFormat::DecisionsCsv,
            response_file: Some(PathBuf::from("status.json")),
        }
    );
}

#[test]
fn parses_status_telemetry_file_invocation() {
    let args = vec![
        "status".to_owned(),
        "--telemetry-csv".to_owned(),
        "--from-file".to_owned(),
        "status.json".to_owned(),
    ];

    assert_eq!(
        parse_invocation(&args).unwrap(),
        Invocation {
            command: tensor_wallpaper::ipc::ClientCommand::Status,
            format: ResponseFormat::TelemetryCsv,
            response_file: Some(PathBuf::from("status.json")),
        }
    );
}
