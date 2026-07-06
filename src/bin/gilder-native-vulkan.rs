#[cfg(feature = "native-vulkan-renderer")]
use gilder::core::{FitMode, ScenePathFillRule};
#[cfg(feature = "native-vulkan-renderer")]
use gilder::renderer::native_vulkan::NativeVulkanClearColor;
#[cfg(all(feature = "native-vulkan-renderer", feature = "native-vulkan-video"))]
use gilder::renderer::native_vulkan::{
    NativeVulkanAudioOutputMode, NativeVulkanVideoSessionSmokeOptions,
    native_vulkan_resolve_ffmpeg_video_session_codec, native_vulkan_video_run_route,
};
#[cfg(feature = "native-vulkan-renderer")]
use gilder::renderer::scene_engine_plan_from_gscn_path_with_properties;
#[cfg(feature = "native-vulkan-renderer")]
use serde_json::Value;
#[cfg(feature = "native-vulkan-renderer")]
use std::collections::BTreeMap;
#[cfg(feature = "native-vulkan-renderer")]
use std::path::{Path, PathBuf};

#[cfg(feature = "native-vulkan-renderer")]
fn main() {
    #[cfg(all(feature = "native-vulkan-video", target_os = "linux"))]
    native_vulkan_video_allocator_env_bootstrap();

    if let Err(err) = run() {
        eprintln!("gilder-native-vulkan: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "native-vulkan-renderer"))]
fn main() {
    eprintln!("gilder-native-vulkan requires native-vulkan-renderer feature");
    std::process::exit(1);
}

#[cfg(all(feature = "native-vulkan-video", target_os = "linux"))]
fn native_vulkan_video_sync_executable_after_rebuild(executable: &std::path::Path) {
    let Ok(file) = std::fs::File::open(executable) else {
        return;
    };
    let _ = file.sync_all();
}

#[cfg(all(feature = "native-vulkan-video", target_os = "linux"))]
fn native_vulkan_video_allocator_env_bootstrap() {
    const BOOTSTRAPPED: &str = "GILDER_NATIVE_VULKAN_ALLOCATOR_BOOTSTRAPPED";
    const EXE_SYNCED: &str = "GILDER_NATIVE_VULKAN_EXE_SYNCED";
    const REQUIRED_ENV: &[(&str, &str)] = &[
        ("MALLOC_ARENA_MAX", "1"),
        ("MALLOC_MMAP_THRESHOLD_", "131072"),
        ("MALLOC_TRIM_THRESHOLD_", "0"),
        ("MALLOC_TOP_PAD_", "0"),
    ];

    let mut needs_reexec = REQUIRED_ENV
        .iter()
        .any(|(name, value)| std::env::var(name).as_deref() != Ok(*value));
    needs_reexec |= !native_vulkan_video_glibc_tcache_disabled();

    if !needs_reexec {
        if std::env::var_os(EXE_SYNCED).as_deref() != Some(std::ffi::OsStr::new("1"))
            && let Ok(executable) = std::env::current_exe()
        {
            native_vulkan_video_sync_executable_after_rebuild(&executable);
        }
        return;
    }
    if std::env::var_os(BOOTSTRAPPED).as_deref() == Some(std::ffi::OsStr::new("1")) {
        eprintln!("gilder-native-vulkan: allocator bootstrap environment was not applied");
        std::process::exit(127);
    }

    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(err) => {
            eprintln!(
                "gilder-native-vulkan: failed to locate executable for allocator bootstrap: {err}"
            );
            std::process::exit(127);
        }
    };
    native_vulkan_video_sync_executable_after_rebuild(&executable);
    let mut command = std::process::Command::new(executable);
    command.args(std::env::args_os().skip(1));
    command.env(BOOTSTRAPPED, "1");
    command.env(EXE_SYNCED, "1");
    for (name, value) in REQUIRED_ENV {
        command.env(name, value);
    }
    command.env(
        "GLIBC_TUNABLES",
        native_vulkan_video_glibc_tunables_with_tcache_disabled(),
    );

    use std::os::unix::process::CommandExt;
    let err = command.exec();
    eprintln!("gilder-native-vulkan: failed to exec allocator-bootstrapped process: {err}");
    std::process::exit(127);
}

#[cfg(all(feature = "native-vulkan-video", target_os = "linux"))]
fn native_vulkan_video_glibc_tcache_disabled() -> bool {
    std::env::var("GLIBC_TUNABLES").ok().is_some_and(|value| {
        value
            .split(':')
            .any(|entry| entry == "glibc.malloc.tcache_count=0")
    })
}

#[cfg(all(feature = "native-vulkan-video", target_os = "linux"))]
fn native_vulkan_video_glibc_tunables_with_tcache_disabled() -> String {
    let mut entries = std::env::var("GLIBC_TUNABLES")
        .unwrap_or_default()
        .split(':')
        .filter(|entry| !entry.is_empty() && !entry.starts_with("glibc.malloc.tcache_count="))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    entries.push("glibc.malloc.tcache_count=0".to_owned());
    entries.join(":")
}

#[cfg(feature = "native-vulkan-renderer")]
fn native_vulkan_static_source_is_gtex(source: &Path) -> bool {
    source
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gtex"))
}

#[cfg(feature = "native-vulkan-renderer")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use gilder::renderer::StaticWallpaperPlan;
    #[cfg(feature = "native-vulkan-video")]
    use gilder::renderer::native_vulkan::native_vulkan_video_playback_frame_count;
    use gilder::renderer::native_vulkan::{
        NativeVulkanAudioOutputPolicy, NativeVulkanOptions, NativeVulkanSurfaceProbeOptions,
        NativeVulkanVideoSessionSmokeOptions, backend_contract, capabilities,
        native_vulkan_video_duration_playback_frames, native_vulkan_video_run_route,
        probe_vulkan_video_decode, probe_wayland_surface, run_clear, run_static_image,
        wallpaper_type_support_matrix,
    };
    #[cfg(feature = "native-vulkan-video")]
    use gilder::renderer::native_vulkan::{
        NativeVulkanFfmpegVulkanHwVideoPresentOptions, NativeVulkanVideoSessionCodec,
        native_vulkan_extract_av1_sequence_header_for_vulkanalia,
        native_vulkan_extract_h264_parameter_sets_for_vulkanalia,
        native_vulkan_extract_h265_parameter_sets_for_vulkanalia,
        run_native_vulkan_ffmpeg_vulkan_hw_video_present, run_vulkanalia_ready_prefix_video,
    };
    use gilder::renderer::native_vulkan::{
        NativeVulkanVulkanaliaSurfaceSwapchainProbeOptions,
        NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
        NativeVulkanVulkanaliaVideoPresentDeviceProbeOptions,
        NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
        NativeVulkanVulkanaliaVideoSessionBindSmokeOptions, probe_native_vulkan_vulkanalia_devices,
        probe_native_vulkan_vulkanalia_surface_swapchain,
        probe_native_vulkan_vulkanalia_video_present_device,
        probe_native_vulkan_vulkanalia_video_present_session,
        probe_native_vulkan_vulkanalia_video_session_bind,
    };
    use gilder::renderer::native_wayland::{
        NativeWaylandFractionalScaleRounding, NativeWaylandLayer,
    };
    use serde_json::json;
    use std::time::Duration;

    let mut mode = NativeVulkanCliMode::All;
    let mut options = NativeVulkanOptions::default();
    let mut target_fps_set = false;
    let mut duration = Duration::from_secs(5);
    let mut duration_set = false;
    let mut source = None::<PathBuf>;
    let mut fit = FitMode::Cover;
    let mut fit_set = false;
    let mut background = None::<String>;
    let mut scene_color = None::<String>;
    let mut scene_text = None::<String>;
    let mut scene_text_color = None::<String>;
    let mut scene_text_font_size = None::<f64>;
    let mut scene_path_data = None::<String>;
    let mut scene_path_fill_rule = ScenePathFillRule::default();
    let mut scene_stroke_color = None::<String>;
    let mut scene_stroke_width = None::<f64>;
    let mut scene_video_layer = false;
    let mut scene_root = None::<PathBuf>;
    let mut scene_properties = BTreeMap::<String, Value>::new();
    let mut scene_snapshot_time_ms = 0u64;
    let mut _muted = true;
    #[cfg(feature = "native-vulkan-video")]
    let mut audio_clock_probe_requested = false;
    #[cfg(not(feature = "native-vulkan-video"))]
    let audio_clock_probe_requested = false;
    let mut audio_output_policy = NativeVulkanAudioOutputPolicy::Plan;
    let mut allow_foreground_layer = false;
    let mut video_session_options = NativeVulkanVideoSessionSmokeOptions::default();
    let mut vulkanalia_create_empty_session_parameters = false;
    let mut vulkanalia_create_session_parameters = false;
    let mut ready_prefix_playback_frames = 0u32;
    let mut video_width_set = false;
    let mut video_height_set = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--capabilities" => mode = NativeVulkanCliMode::Capabilities,
            "--contract" => mode = NativeVulkanCliMode::Contract,
            "--type-support" => mode = NativeVulkanCliMode::TypeSupport,
            "--probe-surface" => mode = NativeVulkanCliMode::ProbeSurface,
            "--probe-video" => mode = NativeVulkanCliMode::ProbeVideo,
            "--probe-vulkanalia" => mode = NativeVulkanCliMode::ProbeVulkanalia,
            "--probe-vulkanalia-swapchain" => mode = NativeVulkanCliMode::ProbeVulkanaliaSwapchain,
            "--probe-vulkanalia-video-session" => {
                mode = NativeVulkanCliMode::ProbeVulkanaliaVideoSession
            }
            "--probe-vulkanalia-video-present" => {
                mode = NativeVulkanCliMode::ProbeVulkanaliaVideoPresent
            }
            "--probe-vulkanalia-video-present-session" => {
                mode = NativeVulkanCliMode::ProbeVulkanaliaVideoPresentSession
            }
            "--run-vulkanalia-ready-prefix-video" => {
                mode = NativeVulkanCliMode::RunVulkanaliaReadyPrefixVideo
            }
            "--allocate-video-images" => video_session_options.allocate_video_images = true,
            "--allocate-bitstream-buffer" => video_session_options.allocate_bitstream_buffer = true,
            "--create-empty-session-parameters" => {
                vulkanalia_create_empty_session_parameters = true
            }
            "--create-session-parameters" => vulkanalia_create_session_parameters = true,
            "--decode-h264-ready-prefix" => {
                let count = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--decode-h264-ready-prefix requires a count")?;
                video_session_options.decode_h264_ready_prefix_frames = count;
                video_session_options.h264_required_ready_prefix_access_units = count;
                video_session_options.extract_bitstream = true;
                video_session_options.allocate_bitstream_buffer = true;
                video_session_options.allocate_video_images = true;
            }
            "--decode-h265-ready-prefix" => {
                let count = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--decode-h265-ready-prefix requires a count")?;
                video_session_options.decode_h265_ready_prefix_frames = count;
                video_session_options.h265_required_ready_prefix_access_units = count;
                video_session_options.extract_bitstream = true;
                video_session_options.allocate_bitstream_buffer = true;
                video_session_options.allocate_video_images = true;
            }
            "--decode-av1-ready-prefix" => {
                let count = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--decode-av1-ready-prefix requires a count")?;
                video_session_options.decode_av1_ready_prefix_frames = count;
                video_session_options.av1_required_ready_prefix_temporal_units = count;
                video_session_options.extract_bitstream = true;
                video_session_options.allocate_bitstream_buffer = true;
                video_session_options.allocate_video_images = true;
            }
            "--run-clear" => mode = NativeVulkanCliMode::RunClear,
            "--scene-runtime-snapshot" => mode = NativeVulkanCliMode::SceneRuntimeSnapshot,
            "--run-scene" => mode = NativeVulkanCliMode::RunScene,
            "--run-static" => mode = NativeVulkanCliMode::RunStatic,
            "--run-video" => mode = NativeVulkanCliMode::RunVideo,
            "--json" => mode = NativeVulkanCliMode::All,
            "--output-name" => {
                options.host.output_name =
                    Some(args.next().ok_or("--output-name requires a value")?);
            }
            "--layer" => {
                let value = args.next().ok_or("--layer requires a value")?;
                options.host.layer = value.parse::<NativeWaylandLayer>()?;
            }
            "--parent-mapping-buffer" => options.host.attach_parent_mapping_buffer = true,
            "--no-parent-mapping-buffer" => options.host.attach_parent_mapping_buffer = false,
            "--fractional-scale-rounding" => {
                let value = args
                    .next()
                    .ok_or("--fractional-scale-rounding requires ceil, nearest, or floor")?;
                options.host.fractional_scale_rounding =
                    value.parse::<NativeWaylandFractionalScaleRounding>()?;
            }
            "--allow-foreground-layer" => allow_foreground_layer = true,
            "--wait-roundtrips" => {
                options.wait_configure_roundtrips = args
                    .next()
                    .map(|value| value.parse::<usize>())
                    .transpose()?
                    .ok_or("--wait-roundtrips requires a value")?;
            }
            "--duration" => {
                duration = args
                    .next()
                    .map(|value| value.parse::<u64>())
                    .transpose()?
                    .map(Duration::from_secs)
                    .ok_or("--duration requires seconds")?;
                duration_set = true;
            }
            "--target-fps" => {
                options.target_max_fps =
                    args.next().map(|value| value.parse::<u32>()).transpose()?;
                target_fps_set = true;
            }
            "--no-fps-limit" => {
                options.target_max_fps = None;
                target_fps_set = true;
            }
            "--color" => {
                let value = args.next().ok_or("--color requires #rrggbb or r,g,b")?;
                options.clear_color = parse_color(&value)?;
                if value.starts_with('#') {
                    scene_color = Some(value);
                }
            }
            "--source" => {
                source = Some(args.next().ok_or("--source requires a path")?.into());
            }
            "--scene-video" => {
                scene_video_layer = true;
            }
            "--poster" => {
                let _ = args.next().ok_or("--poster requires a path")?;
            }
            "--fit" => {
                let value = args.next().ok_or("--fit requires a value")?;
                fit = parse_fit_mode(&value)?;
                fit_set = true;
            }
            "--background" => {
                background = Some(args.next().ok_or("--background requires #rrggbb")?);
            }
            "--text" => {
                scene_text = Some(args.next().ok_or("--text requires a value")?);
            }
            "--text-color" => {
                scene_text_color = Some(args.next().ok_or("--text-color requires #rrggbb")?);
            }
            "--font-size" => {
                let font_size = args
                    .next()
                    .map(|value| value.parse::<f64>())
                    .transpose()?
                    .ok_or("--font-size requires a number")?;
                if !font_size.is_finite() || font_size <= 0.0 {
                    return Err("--font-size must be finite and greater than zero".into());
                }
                scene_text_font_size = Some(font_size);
            }
            "--path-data" => {
                scene_path_data = Some(args.next().ok_or("--path-data requires SVG path data")?);
            }
            "--path-fill-rule" => {
                scene_path_fill_rule = parse_scene_path_fill_rule(
                    &args
                        .next()
                        .ok_or("--path-fill-rule requires nonzero or evenodd")?,
                )?;
            }
            "--stroke-color" => {
                scene_stroke_color = Some(args.next().ok_or("--stroke-color requires #rrggbb")?);
            }
            "--stroke-width" => {
                let stroke_width = args
                    .next()
                    .map(|value| value.parse::<f64>())
                    .transpose()?
                    .ok_or("--stroke-width requires a number")?;
                if !stroke_width.is_finite() || stroke_width <= 0.0 {
                    return Err("--stroke-width must be finite and greater than zero".into());
                }
                scene_stroke_width = Some(stroke_width);
            }
            "--scene-time-ms" | "--snapshot-time-ms" => {
                scene_snapshot_time_ms = args
                    .next()
                    .map(|value| value.parse::<u64>())
                    .transpose()?
                    .ok_or("--scene-time-ms requires milliseconds")?;
            }
            "--scene-root" => {
                scene_root = Some(PathBuf::from(
                    args.next().ok_or("--scene-root requires PATH")?,
                ));
            }
            "--scene-property" => {
                let value = args.next().ok_or("--scene-property requires KEY=VALUE")?;
                let (key, value) = parse_scene_property_assignment(&value)?;
                scene_properties.insert(key, value);
            }
            "--loop" => {}
            "--no-loop" => {}
            "--muted" => _muted = true,
            "--unmuted" => _muted = false,
            "--audio-clock-probe" => {
                #[cfg(feature = "native-vulkan-video")]
                {
                    audio_clock_probe_requested = true;
                }
                #[cfg(not(feature = "native-vulkan-video"))]
                {
                    return Err("--audio-clock-probe requires native-vulkan-video feature".into());
                }
            }
            "--audio-output" => {
                let value = args.next().ok_or("--audio-output requires a value")?;
                audio_output_policy = NativeVulkanAudioOutputPolicy::parse_cli(&value)?;
            }
            "--decoder" => {
                let value = args.next().ok_or("--decoder requires a value")?;
                let _ = parse_decoder_policy(&value)?;
            }
            "--video-codec" => {
                let value = args.next().ok_or("--video-codec requires a value")?;
                video_session_options.codec = value.parse()?;
            }
            "--width" => {
                video_session_options.width = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--width requires pixels")?;
                video_width_set = true;
            }
            "--height" => {
                video_session_options.height = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--height requires pixels")?;
                video_height_set = true;
            }
            "--bitstream-samples" => {
                video_session_options.bitstream_extract_max_samples = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--bitstream-samples requires a count")?;
            }
            "--require-h265-ready-prefix" => {
                video_session_options.h265_required_ready_prefix_access_units = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--require-h265-ready-prefix requires a count")?;
                video_session_options.extract_bitstream = true;
                video_session_options.allocate_bitstream_buffer = true;
            }
            "--require-h264-ready-prefix" => {
                video_session_options.h264_required_ready_prefix_access_units = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--require-h264-ready-prefix requires a count")?;
                video_session_options.extract_bitstream = true;
                video_session_options.allocate_bitstream_buffer = true;
            }
            "--playback-frames" => {
                ready_prefix_playback_frames = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--playback-frames requires a count")?;
            }
            "--start-offset-ms" => {
                let _ = args
                    .next()
                    .map(|value| value.parse::<u64>())
                    .transpose()?
                    .ok_or("--start-offset-ms requires milliseconds")?;
            }
            "-h" | "--help" => {
                print_usage();
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }

    if !allow_foreground_layer
        && matches!(
            options.host.layer,
            NativeWaylandLayer::Top | NativeWaylandLayer::Overlay
        )
    {
        return Err(format!(
            "--layer {} covers normal application windows; pass --allow-foreground-layer for foreground debug",
            options.host.layer.as_str()
        )
        .into());
    }

    if matches!(
        mode,
        NativeVulkanCliMode::RunScene | NativeVulkanCliMode::SceneRuntimeSnapshot
    ) && !target_fps_set
    {
        options.target_max_fps = None;
    }

    let duration_playback_frames = if duration_set {
        native_vulkan_video_duration_playback_frames(duration, options.target_max_fps)
    } else {
        None
    };
    #[cfg(not(feature = "native-vulkan-video"))]
    let _ = (video_width_set, video_height_set);

    let report = match mode {
        NativeVulkanCliMode::All => {
            json!({ "capabilities": capabilities(), "backend_contract": backend_contract() })
        }
        NativeVulkanCliMode::Capabilities => json!(capabilities()),
        NativeVulkanCliMode::Contract => json!(backend_contract()),
        NativeVulkanCliMode::TypeSupport => json!(wallpaper_type_support_matrix()),
        NativeVulkanCliMode::ProbeSurface => {
            json!(probe_wayland_surface(NativeVulkanSurfaceProbeOptions {
                host: options.host,
                wait_configure_roundtrips: options.wait_configure_roundtrips,
            })?)
        }
        NativeVulkanCliMode::ProbeVideo => json!(probe_vulkan_video_decode()?),
        NativeVulkanCliMode::ProbeVulkanalia => json!(probe_native_vulkan_vulkanalia_devices()?),
        NativeVulkanCliMode::ProbeVulkanaliaSwapchain => {
            json!(probe_native_vulkan_vulkanalia_surface_swapchain(
                NativeVulkanVulkanaliaSurfaceSwapchainProbeOptions {
                    host: options.host,
                    wait_configure_roundtrips: options.wait_configure_roundtrips,
                }
            )?)
        }
        NativeVulkanCliMode::ProbeVulkanaliaVideoPresent => {
            json!(probe_native_vulkan_vulkanalia_video_present_device(
                NativeVulkanVulkanaliaVideoPresentDeviceProbeOptions {
                    host: options.host,
                    wait_configure_roundtrips: options.wait_configure_roundtrips,
                    codec: video_session_options.codec,
                }
            )?)
        }
        NativeVulkanCliMode::ProbeVulkanaliaVideoPresentSession => {
            json!(probe_native_vulkan_vulkanalia_video_present_session(
                NativeVulkanVulkanaliaVideoPresentSessionProbeOptions {
                    host: options.host,
                    wait_configure_roundtrips: options.wait_configure_roundtrips,
                    codec: video_session_options.codec,
                    width: video_session_options.width,
                    height: video_session_options.height,
                    target_max_fps: options.target_max_fps,
                    audio_master_clock:
                        NativeVulkanVulkanaliaVideoPresentAudioMasterClock::DISABLED,
                    clear_color: options.clear_color,
                }
            )?)
        }
        NativeVulkanCliMode::ProbeVulkanaliaVideoSession => {
            if video_session_options.decode_h264_ready_prefix_frames > 0
                || video_session_options.decode_h265_ready_prefix_frames > 0
                || video_session_options.decode_av1_ready_prefix_frames > 0
            {
                return Err(
                    "--decode-*-ready-prefix session-bind decode was removed; use the streaming video runtime"
                        .into(),
                );
            }
            let (h264_parameter_sets, h265_parameter_sets, av1_sequence_header) =
                if vulkanalia_create_session_parameters {
                    let source = source
                        .clone()
                        .ok_or("--create-session-parameters requires --source")?;
                    if !source.is_file() {
                        return Err(format!(
                            "bitstream source does not exist: {}",
                            source.display()
                        )
                        .into());
                    }
                    #[cfg(feature = "native-vulkan-video")]
                    {
                        match video_session_options.codec {
                            NativeVulkanVideoSessionCodec::H264High8 => {
                                let parameter_sets =
                                    native_vulkan_extract_h264_parameter_sets_for_vulkanalia(
                                        source,
                                        video_session_options.bitstream_extract_max_samples,
                                    )?;
                                (Some(parameter_sets), None, None)
                            }
                            NativeVulkanVideoSessionCodec::H265Main8
                            | NativeVulkanVideoSessionCodec::H265Main10 => {
                                let parameter_sets =
                                    native_vulkan_extract_h265_parameter_sets_for_vulkanalia(
                                        source,
                                        video_session_options.codec,
                                        video_session_options.bitstream_extract_max_samples,
                                    )?;
                                (None, Some(parameter_sets), None)
                            }
                            NativeVulkanVideoSessionCodec::Av1Main8
                            | NativeVulkanVideoSessionCodec::Av1Main10 => {
                                let sequence_header =
                                    native_vulkan_extract_av1_sequence_header_for_vulkanalia(
                                        source,
                                        video_session_options.codec,
                                        video_session_options.bitstream_extract_max_samples,
                                    )?;
                                (None, None, Some(sequence_header))
                            }
                        }
                    }
                    #[cfg(not(feature = "native-vulkan-video"))]
                    {
                        let _ = source;
                        return Err(
                            "--create-session-parameters requires native-vulkan-video feature"
                                .into(),
                        );
                    }
                } else {
                    (None, None, None)
                };
            json!(probe_native_vulkan_vulkanalia_video_session_bind(
                NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
                    codec: video_session_options.codec,
                    width: video_session_options.width,
                    height: video_session_options.height,
                    allocate_video_images: video_session_options.allocate_video_images,
                    allocate_bitstream_buffer: video_session_options.allocate_bitstream_buffer,
                    create_empty_session_parameters: vulkanalia_create_empty_session_parameters,
                    create_session_parameters: vulkanalia_create_session_parameters,
                    h264_parameter_sets,
                    h265_parameter_sets,
                    av1_sequence_header,
                }
            )?)
        }
        NativeVulkanCliMode::RunClear => json!(run_clear(options, duration)?),
        NativeVulkanCliMode::SceneRuntimeSnapshot => {
            let source = source
                .as_ref()
                .ok_or("--scene-runtime-snapshot requires --source <scene.gscn>")?;
            if !source.is_file() {
                return Err(format!("scene source does not exist: {}", source.display()).into());
            }
            if scene_video_layer || !scene_cli_source_is_gscn(source) {
                return Err(
                    "--scene-runtime-snapshot only accepts the new .gscn scene-engine path".into(),
                );
            }
            let plan = scene_engine_plan_from_gscn_path_with_properties(
                source.clone(),
                scene_snapshot_time_ms,
                Some(&scene_properties),
            )?;
            json!(scene_engine_cli_snapshot_from_engine_plan(plan))
        }
        NativeVulkanCliMode::RunScene => {
            let source = source
                .as_ref()
                .ok_or("--run-scene requires --source <scene.gscn> on the new scene engine path")?;
            if !source.is_file() {
                return Err(format!("scene source does not exist: {}", source.display()).into());
            }
            if scene_video_layer || !scene_cli_source_is_gscn(source) {
                return Err("--run-scene only accepts the new .gscn scene-engine path".into());
            }
            let plan = scene_engine_plan_from_gscn_path_with_properties(
                source.clone(),
                scene_snapshot_time_ms,
                Some(&scene_properties),
            )?;
            let _ = (
                options,
                duration,
                scene_root,
                fit,
                fit_set,
                background,
                scene_color,
                scene_path_data,
                scene_path_fill_rule,
                scene_stroke_color,
                scene_stroke_width,
                scene_text,
                scene_text_color,
                scene_text_font_size,
                video_session_options,
                video_width_set,
                video_height_set,
                ready_prefix_playback_frames,
                duration_playback_frames,
                audio_clock_probe_requested,
                audio_output_policy,
                _muted,
            );
            return Err(format!(
                "--run-scene new scene present runtime is not connected yet; engine plan is available for {} objects and {} resources",
                plan.objects.len(),
                plan.resources.len()
            )
            .into());
        }
        NativeVulkanCliMode::RunStatic => {
            let source = source.ok_or("--run-static requires --source")?;
            if !source.is_file() {
                return Err(format!("static source does not exist: {}", source.display()).into());
            }
            if !native_vulkan_static_source_is_gtex(&source) {
                return Err(format!(
                    "--run-static requires a native .gtex BC7 source {}; convert PNG/JPG offline with gilder-convert image-gtex",
                    source.display()
                )
                .into());
            }
            let output_name = options
                .host
                .output_name
                .clone()
                .unwrap_or_else(|| "native-vulkan".to_owned());
            json!(run_static_image(
                options,
                duration,
                StaticWallpaperPlan {
                    output_name,
                    source,
                    fit,
                    background,
                },
            )?)
        }
        NativeVulkanCliMode::RunVideo => {
            let source = source.ok_or("--run-video requires --source")?;
            if !source.is_file() {
                return Err(format!("video source does not exist: {}", source.display()).into());
            }
            let route = native_vulkan_video_run_route(
                &video_session_options,
                ready_prefix_playback_frames,
                duration_playback_frames,
            );
            #[cfg(feature = "native-vulkan-video")]
            {
                if route.is_ffmpeg_vulkan_hw_decode() {
                    json!(run_native_vulkan_ffmpeg_vulkan_hw_video_present(
                        NativeVulkanFfmpegVulkanHwVideoPresentOptions {
                            host: options.host,
                            wait_configure_roundtrips: options.wait_configure_roundtrips,
                            source,
                            codec: video_session_options.codec,
                            playback_frame_count: route.playback_frames,
                            target_max_fps: options.target_max_fps,
                            audio_clock_probe_requested,
                            audio_output_mode: audio_output_policy.resolve(_muted),
                            audio_master_clock:
                                NativeVulkanVulkanaliaVideoPresentAudioMasterClock::DISABLED,
                            clear_color: options.clear_color,
                        },
                    )?)
                } else {
                    return Err(format!(
                        "--run-video cannot use FFmpeg Vulkan HW decode route: {}",
                        route.status
                    )
                    .into());
                }
            }
            #[cfg(not(feature = "native-vulkan-video"))]
            {
                let _ = (options, source, fit, _muted, route);
                return Err(
                    "--run-video FFmpeg Vulkan HW decode route requires native-vulkan-video feature"
                        .into(),
                );
            }
        }
        NativeVulkanCliMode::RunVulkanaliaReadyPrefixVideo => {
            let source = source.ok_or("--run-vulkanalia-ready-prefix-video requires --source")?;
            if !source.is_file() {
                return Err(format!("video source does not exist: {}", source.display()).into());
            }
            #[cfg(feature = "native-vulkan-video")]
            let ready_prefix_frames = match video_session_options.codec {
                NativeVulkanVideoSessionCodec::H264High8 => {
                    video_session_options.decode_h264_ready_prefix_frames
                }
                NativeVulkanVideoSessionCodec::H265Main8
                | NativeVulkanVideoSessionCodec::H265Main10 => {
                    video_session_options.decode_h265_ready_prefix_frames
                }
                NativeVulkanVideoSessionCodec::Av1Main8
                | NativeVulkanVideoSessionCodec::Av1Main10 => {
                    video_session_options.decode_av1_ready_prefix_frames
                }
            };
            #[cfg(not(feature = "native-vulkan-video"))]
            let ready_prefix_frames = 0u32;
            if ready_prefix_frames == 0 {
                return Err(
                    "--run-vulkanalia-ready-prefix-video requires --decode-h264-ready-prefix N, --decode-h265-ready-prefix N, or --decode-av1-ready-prefix N matching --video-codec"
                        .into(),
                );
            }
            #[cfg(feature = "native-vulkan-video")]
            {
                let playback_frames = native_vulkan_video_playback_frame_count(
                    ready_prefix_frames,
                    ready_prefix_playback_frames,
                    duration_playback_frames,
                );
                let report = run_vulkanalia_ready_prefix_video(
                    options,
                    video_session_options.codec,
                    source,
                    video_session_options.width,
                    video_session_options.height,
                    fit,
                    video_session_options.bitstream_extract_max_samples,
                    ready_prefix_frames,
                    playback_frames,
                    audio_clock_probe_requested,
                    audio_output_policy.resolve(_muted),
                )?;
                write_json_report(&report)?;
                return Ok(());
            }
            #[cfg(not(feature = "native-vulkan-video"))]
            {
                let _ = (
                    options,
                    source,
                    video_session_options.width,
                    video_session_options.height,
                    fit,
                    video_session_options.bitstream_extract_max_samples,
                    ready_prefix_frames,
                    ready_prefix_playback_frames,
                );
                return Err(
                    "--run-vulkanalia-ready-prefix-video requires native-vulkan-video feature"
                        .into(),
                );
            }
        }
    };
    write_json_report(&report)?;
    Ok(())
}

#[cfg(feature = "native-vulkan-renderer")]
fn write_json_report<T: serde::Serialize>(report: &T) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write as _;
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer_pretty(&mut stdout, report)?;
    stdout.write_all(b"\n")?;
    Ok(())
}

#[cfg(feature = "native-vulkan-renderer")]
fn parse_color(value: &str) -> Result<NativeVulkanClearColor, Box<dyn std::error::Error>> {
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() != 6 {
            return Err("hex color must be #rrggbb".into());
        }
        let r = u8::from_str_radix(&hex[0..2], 16)? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16)? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16)? as f32 / 255.0;
        return Ok(NativeVulkanClearColor { r, g, b, a: 1.0 });
    }

    let parts = value
        .split(',')
        .map(|part| part.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()?;
    match parts.as_slice() {
        [r, g, b] => Ok(NativeVulkanClearColor {
            r: *r,
            g: *g,
            b: *b,
            a: 1.0,
        }),
        [r, g, b, a] => Ok(NativeVulkanClearColor {
            r: *r,
            g: *g,
            b: *b,
            a: *a,
        }),
        _ => Err("color must be #rrggbb, r,g,b, or r,g,b,a".into()),
    }
}

#[cfg(feature = "native-vulkan-renderer")]
fn parse_scene_property_assignment(
    assignment: &str,
) -> Result<(String, Value), Box<dyn std::error::Error>> {
    let Some((key, value)) = assignment.split_once('=') else {
        return Err("--scene-property expects KEY=VALUE".into());
    };
    let key = key.trim();
    if key.is_empty() {
        return Err("--scene-property key must not be empty".into());
    }
    let value = value.trim();
    let parsed = match value.to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" => Value::Bool(true),
        "false" | "off" | "no" => Value::Bool(false),
        _ => value
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(value.to_owned())),
    };
    Ok((key.to_owned(), parsed))
}

#[cfg(feature = "native-vulkan-renderer")]
fn scene_cli_source_is_gscn(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("gscn")
}

#[cfg(feature = "native-vulkan-renderer")]
#[derive(Debug, serde::Serialize)]
struct SceneEngineCliSnapshot {
    engine: &'static str,
    references: [&'static str; 4],
    layer_count: usize,
    resource_count: usize,
    object_count: usize,
    frame: gilder::engine::scene_engine::SceneFramePlan,
    recorded_commands: Vec<gilder::engine::scene_engine::RenderingDeviceCommand>,
}

#[cfg(feature = "native-vulkan-renderer")]
fn scene_engine_cli_snapshot_from_engine_plan(
    plan: gilder::engine::scene_engine::SceneEnginePlan,
) -> SceneEngineCliSnapshot {
    use gilder::engine::scene_engine::{RenderingDevice, RenderingServer};
    use gilder::renderer::native_vulkan::{
        NativeVulkanRendererSceneRender, NativeVulkanRenderingDevice,
    };

    let resource_count = plan.resources.len();
    let object_count = plan.objects.len();
    let context = plan.frame_context();
    let mut server = RenderingServer::new();
    server.replace_scene(plan.resources, plan.objects);
    let renderer = NativeVulkanRendererSceneRender::new();
    let frame = server.draw(&renderer, context);
    let mut device = NativeVulkanRenderingDevice::new();
    device.record_scene_frame(&frame);
    SceneEngineCliSnapshot {
        engine: "rendering-server/renderer-scene-render/rendering-device",
        references: [
            "reverse-engineered/docs/scene-format.md",
            "reverse-engineered/docs/effect-format.md",
            "reverse-engineered/docs/material-format.md",
            "reverse-engineered/docs/exe/blend-and-render.md",
        ],
        layer_count: object_count,
        resource_count,
        object_count,
        frame,
        recorded_commands: device.into_commands(),
    }
}

#[cfg(feature = "native-vulkan-renderer")]
fn parse_fit_mode(value: &str) -> Result<FitMode, String> {
    match value {
        "cover" => Ok(FitMode::Cover),
        "contain" => Ok(FitMode::Contain),
        "stretch" => Ok(FitMode::Stretch),
        "tile" => Ok(FitMode::Tile),
        "center" => Ok(FitMode::Center),
        other => Err(format!("unsupported fit mode: {other}")),
    }
}

#[cfg(feature = "native-vulkan-renderer")]
fn parse_scene_path_fill_rule(value: &str) -> Result<ScenePathFillRule, String> {
    match value {
        "nonzero" | "non-zero" | "winding" => Ok(ScenePathFillRule::Nonzero),
        "evenodd" | "even-odd" => Ok(ScenePathFillRule::Evenodd),
        other => Err(format!("unsupported path fill rule: {other}")),
    }
}

#[cfg(feature = "native-vulkan-renderer")]
fn parse_decoder_policy(
    value: &str,
) -> Result<gilder::config::VideoDecoderPolicy, Box<dyn std::error::Error>> {
    match value {
        "auto" => Ok(gilder::config::VideoDecoderPolicy::Auto),
        "hardware-preferred" | "hw-preferred" => {
            Ok(gilder::config::VideoDecoderPolicy::HardwarePreferred)
        }
        "hardware-required" | "hw-required" => {
            Ok(gilder::config::VideoDecoderPolicy::HardwareRequired)
        }
        "software" => Ok(gilder::config::VideoDecoderPolicy::Software),
        other => Err(format!("unsupported decoder policy: {other}").into()),
    }
}

#[cfg(feature = "native-vulkan-renderer")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeVulkanCliMode {
    All,
    Capabilities,
    Contract,
    TypeSupport,
    ProbeSurface,
    ProbeVideo,
    ProbeVulkanalia,
    ProbeVulkanaliaSwapchain,
    ProbeVulkanaliaVideoPresent,
    ProbeVulkanaliaVideoPresentSession,
    ProbeVulkanaliaVideoSession,
    SceneRuntimeSnapshot,
    RunClear,
    RunScene,
    RunStatic,
    RunVideo,
    RunVulkanaliaReadyPrefixVideo,
}

#[cfg(feature = "native-vulkan-renderer")]
fn print_usage() {
    println!(
        "Usage: gilder-native-vulkan [--json|--capabilities|--contract|--type-support|--probe-surface|--probe-video|--probe-vulkanalia|--probe-vulkanalia-swapchain|--probe-vulkanalia-video-present|--probe-vulkanalia-video-present-session|--probe-vulkanalia-video-session|--scene-runtime-snapshot|--run-clear|--run-scene|--run-static|--run-video|--run-vulkanalia-ready-prefix-video]\n\
\n\
Print native Vulkan spike capabilities and backend contract.\n\
--probe-surface creates a layer-shell Wayland surface and VK_KHR_wayland_surface, then exits.\n\
--probe-video enumerates Vulkan Video decode extensions and queue families, then exits.\n\
--probe-vulkanalia enumerates the vulkanalia Vulkan 1.4 physical-device/video/external-memory gates, then exits.\n\
--probe-vulkanalia-swapchain creates a Wayland VkSurfaceKHR, Vulkanalia device, swapchain and swapchain image list, then exits.\n\
--probe-vulkanalia-video-present creates one Vulkanalia device with video-decode and graphics/present queues plus a Wayland swapchain, then exits.\n\
--probe-vulkanalia-video-present-session creates one Vulkanalia video+present device, video session, sampled DPB/output image, and Wayland swapchain, then exits.\n\
--probe-vulkanalia-video-session creates and binds a Vulkanalia Vulkan Video session for --video-codec, then exits.\n\
--allocate-video-images extends --probe-vulkanalia-video-session with codec-matching 2-plane 4:2:0 DPB/output sampled image allocation.\n\
--allocate-bitstream-buffer extends --probe-vulkanalia-video-session with an FFmpeg-sized mapped VIDEO_DECODE_SRC slices buffer.\n\
--create-empty-session-parameters extends --probe-vulkanalia-video-session with an H.264/H.265 empty capacity VkVideoSessionParametersKHR smoke.\n\
--create-session-parameters extends --probe-vulkanalia-video-session with real H.264 SPS/PPS, H.265 VPS/SPS/PPS, or AV1 sequence-header VkVideoSessionParametersKHR creation from --source.\n\
--decode-h264-ready-prefix N configures the legacy Vulkanalia compatibility route with N reference-ready H.264 AU decode submits.\n\
--decode-h265-ready-prefix N configures the legacy Vulkanalia compatibility route with N ready H.265 AU decode submits.\n\
--decode-av1-ready-prefix N configures the legacy Vulkanalia compatibility route with N visible AV1 temporal units.\n\
--playback-frames N sets the FFmpeg Vulkan HW present frame budget or repeats the legacy ready-prefix window.\n\
--run-clear uses the Vulkanalia Wayland swapchain runtime, clears frames with CmdPipelineBarrier2/QueueSubmit2, presents, then prints runtime JSON.\n\
--scene-runtime-snapshot builds a new engine snapshot from --source <scene.gscn> and exits before presenting.\n\
--run-scene accepts only --source <scene.gscn> on the new scene-engine path; Vulkan scene present runtime connection is still explicit work.\n\
--run-static uses Vulkanalia sampled-image dynamic rendering for static wallpapers with cover|contain|stretch|tile|center fit and background clear.\n\
--run-video selects the FFmpeg Vulkan HW decode mainline and requires AV_PIX_FMT_VULKAN/AVVkFrame before descriptor-heap present.\n\
--run-vulkanalia-ready-prefix-video runs the legacy Vulkanalia Vulkan Video compatibility route and prints runtime JSON.\n\
Options: [--output-name NAME] [--layer background|bottom|top|overlay] [--parent-mapping-buffer|--no-parent-mapping-buffer] [--fractional-scale-rounding ceil|nearest|floor] [--wait-roundtrips N]\n\
         [--duration SECONDS] [--target-fps FPS|--no-fps-limit] [--color #rrggbb|r,g,b]\n\
         [--source PATH] [--scene-root PATH] [--scene-video] [--poster PATH] [--fit cover|contain|stretch|tile|center] [--background #rrggbb] [--text TEXT] [--text-color #rrggbb] [--font-size PX]\n\
         [--path-data SVG_PATH] [--path-fill-rule nonzero|evenodd] [--stroke-color #rrggbb] [--stroke-width PX]\n\
         [--scene-time-ms MS] [--scene-property KEY=VALUE]\n\
         [--loop|--no-loop] [--muted|--unmuted] [--audio-output plan|clock-only|auto] [--audio-clock-probe]\n\
         [--decoder auto|hardware-preferred|hardware-required|software]\n\
         [--video-codec h264|h265|h265-main-10|av1|av1-main-10] [--width PX] [--height PX]\n\
         [--allocate-video-images] [--allocate-bitstream-buffer]\n\
         [--create-session-parameters] [--bitstream-samples N]\n\
         [--decode-h264-ready-prefix N] [--require-h264-ready-prefix N]\n\
         [--decode-h265-ready-prefix N]\n\
         [--decode-av1-ready-prefix N]\n\
         [--require-h265-ready-prefix N] [--playback-frames N]\n\
         [--start-offset-ms MS]"
    );
}
