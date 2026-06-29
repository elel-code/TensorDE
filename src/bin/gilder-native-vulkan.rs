#[cfg(feature = "native-vulkan-renderer")]
use gilder::core::{FitMode, SceneNodeKind, ScenePathFillRule, SceneSystems, SceneTransform};
#[cfg(feature = "native-vulkan-renderer")]
use gilder::desktop::DesktopCursorParallax;
#[cfg(feature = "native-vulkan-renderer")]
use gilder::renderer::native_vulkan::NativeVulkanClearColor;
#[cfg(all(feature = "native-vulkan-renderer", feature = "native-vulkan-video"))]
use gilder::renderer::native_vulkan::{
    NativeVulkanAudioOutputMode, NativeVulkanSceneVideoBridgeOptions,
    NativeVulkanSceneVideoBridgeSourceOptions, NativeVulkanVideoSessionSmokeOptions,
    native_vulkan_resolve_ffmpeg_video_session_codec, native_vulkan_video_run_route,
};
#[cfg(feature = "native-vulkan-renderer")]
use gilder::renderer::{
    SceneDisplayPlan, SceneRenderLayer, SceneWallpaperPlan,
    scene_wallpaper_plan_from_gscene_path_with_properties,
};
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

#[cfg(all(feature = "native-vulkan-renderer", feature = "native-vulkan-video"))]
fn native_vulkan_scene_video_bridge_options_from_plan(
    plan: &SceneWallpaperPlan,
    base_options: &NativeVulkanVideoSessionSmokeOptions,
    video_width_set: bool,
    video_height_set: bool,
    ready_prefix_playback_frames: u32,
    duration_playback_frames: Option<u32>,
    audio_clock_probe_requested: bool,
    audio_output_mode: NativeVulkanAudioOutputMode,
) -> Result<Option<NativeVulkanSceneVideoBridgeOptions>, Box<dyn std::error::Error>> {
    let mut sources = Vec::new();
    for layer in plan.layers.iter().filter(|layer| {
        layer.kind == SceneNodeKind::Video && layer.opacity > 0.0 && layer.source.is_some()
    }) {
        let Some(source) = layer.source.as_ref() else {
            continue;
        };
        if sources
            .iter()
            .any(|entry: &NativeVulkanSceneVideoBridgeSourceOptions| entry.source == *source)
        {
            continue;
        }
        let mut options = base_options.clone();
        options.codec = native_vulkan_resolve_ffmpeg_video_session_codec(source)?;
        if !video_width_set {
            options.width = native_vulkan_scene_video_bridge_extent(layer.width, options.width);
        }
        if !video_height_set {
            options.height = native_vulkan_scene_video_bridge_extent(layer.height, options.height);
        }

        let route = native_vulkan_video_run_route(
            &options,
            ready_prefix_playback_frames,
            duration_playback_frames,
        );
        if !route.is_vulkanalia_ready_prefix() {
            return Err(format!(
                "scene video layer cannot use Vulkanalia ready-prefix route for {}: {}",
                source.display(),
                route.status
            )
            .into());
        }
        sources.push(NativeVulkanSceneVideoBridgeSourceOptions {
            source: source.clone(),
            codec: route.codec,
            width: route.width,
            height: route.height,
            bitstream_extract_max_samples: options.bitstream_extract_max_samples,
            ready_prefix_frames: route.ready_prefix_frames,
            playback_frames: route.playback_frames,
        });
    }
    if sources.is_empty() {
        return Ok(None);
    }
    Ok(Some(NativeVulkanSceneVideoBridgeOptions {
        sources,
        audio_clock_probe_requested,
        audio_output_mode,
    }))
}

#[cfg(all(feature = "native-vulkan-renderer", feature = "native-vulkan-video"))]
fn native_vulkan_scene_video_bridge_extent(layer_extent: Option<f64>, fallback: u32) -> u32 {
    layer_extent
        .filter(|extent| extent.is_finite() && *extent > 0.0)
        .map(|extent| extent.round().clamp(1.0, f64::from(u32::MAX)) as u32)
        .unwrap_or(fallback)
}

#[cfg(feature = "native-vulkan-renderer")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use gilder::renderer::StaticWallpaperPlan;
    #[cfg(feature = "native-vulkan-video")]
    use gilder::renderer::native_vulkan::native_vulkan_video_playback_frame_count;
    #[cfg(feature = "native-vulkan-video")]
    use gilder::renderer::native_vulkan::{
        NativeVulkanAudioOutputPolicy, NativeVulkanVideoSessionCodec,
        native_vulkan_extract_av1_sequence_header_for_vulkanalia,
        native_vulkan_extract_h264_parameter_sets_for_vulkanalia,
        native_vulkan_extract_h265_parameter_sets_for_vulkanalia,
        run_vulkanalia_ready_prefix_video,
    };
    use gilder::renderer::native_vulkan::{
        NativeVulkanOptions, NativeVulkanSurfaceProbeOptions, NativeVulkanVideoSessionSmokeOptions,
        backend_contract, capabilities, native_vulkan_scene_runtime_snapshot_from_plan,
        native_vulkan_video_duration_playback_frames, native_vulkan_video_run_route,
        probe_vulkan_video_decode, probe_wayland_surface, run_clear, run_scene, run_static_image,
        wallpaper_type_support_matrix,
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
    use gilder::renderer::native_wayland::NativeWaylandLayer;
    use serde_json::json;
    use std::time::Duration;

    let mut mode = NativeVulkanCliMode::All;
    let mut options = NativeVulkanOptions::default();
    let mut target_fps_set = false;
    let mut duration = Duration::from_secs(5);
    let mut duration_set = false;
    let mut source = None::<PathBuf>;
    let mut fit = FitMode::Cover;
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
    let mut scene_snapshot_time_ms = 0u64;
    let mut _muted = true;
    #[cfg(feature = "native-vulkan-video")]
    let mut audio_clock_probe_requested = false;
    #[cfg(feature = "native-vulkan-video")]
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
                #[cfg(feature = "native-vulkan-video")]
                {
                    audio_output_policy = NativeVulkanAudioOutputPolicy::parse_cli(&value)?;
                }
                #[cfg(not(feature = "native-vulkan-video"))]
                {
                    let _ = value;
                    return Err("--audio-output requires native-vulkan-video feature".into());
                }
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
            if let Some(source) = source.as_ref() {
                if !source.is_file() {
                    return Err(format!("scene source does not exist: {}", source.display()).into());
                }
            }
            let output_name = options
                .host
                .output_name
                .clone()
                .unwrap_or_else(|| "native-vulkan".to_owned());
            let target_max_fps = options.target_max_fps;
            let plan = scene_cli_plan(
                output_name,
                source,
                scene_video_layer,
                scene_root,
                fit,
                background,
                scene_color,
                scene_path_data,
                scene_path_fill_rule,
                scene_stroke_color,
                scene_stroke_width,
                scene_text,
                scene_text_color,
                scene_text_font_size,
                scene_snapshot_time_ms,
                target_max_fps,
            )?;
            json!(native_vulkan_scene_runtime_snapshot_from_plan(&plan)?)
        }
        NativeVulkanCliMode::RunScene => {
            if let Some(source) = source.as_ref() {
                if !source.is_file() {
                    return Err(format!("scene source does not exist: {}", source.display()).into());
                }
            }
            let output_name = options
                .host
                .output_name
                .clone()
                .unwrap_or_else(|| "native-vulkan".to_owned());
            let target_max_fps = options.target_max_fps;
            let plan = scene_cli_plan(
                output_name,
                source,
                scene_video_layer,
                scene_root,
                fit,
                background,
                scene_color,
                scene_path_data,
                scene_path_fill_rule,
                scene_stroke_color,
                scene_stroke_width,
                scene_text,
                scene_text_color,
                scene_text_font_size,
                scene_snapshot_time_ms,
                target_max_fps,
            )?;
            #[cfg(feature = "native-vulkan-video")]
            let video_bridge = native_vulkan_scene_video_bridge_options_from_plan(
                &plan,
                &video_session_options,
                video_width_set,
                video_height_set,
                ready_prefix_playback_frames,
                duration_playback_frames,
                audio_clock_probe_requested,
                audio_output_policy.resolve(_muted),
            )?;
            #[cfg(not(feature = "native-vulkan-video"))]
            let video_bridge = None;
            json!(run_scene(
                options,
                duration,
                plan,
                audio_output_policy.resolve(_muted),
                video_bridge
            )?)
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
                if route.is_vulkanalia_ready_prefix() {
                    json!(run_vulkanalia_ready_prefix_video(
                        options,
                        video_session_options.codec,
                        source,
                        video_session_options.width,
                        video_session_options.height,
                        fit,
                        video_session_options.bitstream_extract_max_samples,
                        route.ready_prefix_frames,
                        route.playback_frames,
                        audio_clock_probe_requested,
                        audio_output_policy.resolve(_muted),
                    )?)
                } else {
                    return Err(format!(
                        "--run-video cannot use Vulkanalia ready-prefix route: {}",
                        route.status
                    )
                    .into());
                }
            }
            #[cfg(not(feature = "native-vulkan-video"))]
            {
                let _ = (options, source, fit, _muted, route);
                return Err(
                    "--run-video Vulkanalia ready-prefix route requires native-vulkan-video feature"
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
fn scene_cli_plan(
    output_name: String,
    source: Option<PathBuf>,
    source_is_video: bool,
    scene_root: Option<PathBuf>,
    fit: FitMode,
    background: Option<String>,
    color: Option<String>,
    path_data: Option<String>,
    path_fill_rule: ScenePathFillRule,
    stroke_color: Option<String>,
    stroke_width: Option<f64>,
    text: Option<String>,
    text_color: Option<String>,
    text_font_size: Option<f64>,
    snapshot_time_ms: u64,
    target_max_fps: Option<u32>,
) -> Result<SceneWallpaperPlan, Box<dyn std::error::Error>> {
    if let Some(source) = source {
        if !source_is_video && scene_cli_source_is_gscene(&source) {
            let package_root =
                scene_root.unwrap_or_else(|| scene_cli_default_gscene_package_root(&source));
            let (render_properties, cursor_parallax_input_ready) =
                scene_cli_cursor_parallax_properties(&output_name);
            return Ok(scene_wallpaper_plan_from_gscene_path_with_properties(
                output_name,
                &package_root,
                source,
                target_max_fps,
                snapshot_time_ms,
                Some(fit),
                render_properties.as_ref(),
                cursor_parallax_input_ready,
            )?);
        }
        if source_is_video {
            let mut layers = Vec::new();
            let display = if let Some(background) = background {
                let mut background_layer = scene_cli_layer("cli-background", SceneNodeKind::Color);
                background_layer.color = Some(background);
                layers.push(background_layer);
                None
            } else {
                Some(SceneDisplayPlan::Color {
                    color: "#000000".to_owned(),
                })
            };
            let mut layer = scene_cli_layer("cli-video", SceneNodeKind::Video);
            layer.source = Some(source);
            layer.fit = fit;
            layers.push(layer);
            return Ok(SceneWallpaperPlan {
                output_name,
                source: None,
                manifest_max_fps: None,
                target_max_fps,
                snapshot_time_ms,
                scene_systems: SceneSystems::default(),
                audio_cue_count: 0,
                bound_properties: Vec::new(),
                timeline_animation_count: 0,
                timeline_animated_layer_count: 0,
                property_binding_count: 0,
                cursor_parallax_input_ready: false,
                scene_input_properties: Default::default(),
                scene_scenescript_binding_count: 0,
                scene_material_graph_count: 0,
                scene_material_graph_resource_count: 0,
                scene_effect_graph_count: 0,
                scene_audio_response_binding_count: 0,
                unsupported_scene_features: Vec::new(),
                scene_size: None,
                scene_fit: fit,
                display,
                layers,
            });
        }
        let mut layer = scene_cli_layer("cli-image", SceneNodeKind::Image);
        layer.source = Some(source.clone());
        layer.fit = fit;
        return Ok(SceneWallpaperPlan {
            output_name,
            source: None,
            manifest_max_fps: None,
            target_max_fps,
            snapshot_time_ms,
            scene_systems: SceneSystems::default(),
            audio_cue_count: 0,
            bound_properties: Vec::new(),
            timeline_animation_count: 0,
            timeline_animated_layer_count: 0,
            property_binding_count: 0,
            cursor_parallax_input_ready: false,
            scene_input_properties: Default::default(),
            scene_scenescript_binding_count: 0,
            scene_material_graph_count: 0,
            scene_material_graph_resource_count: 0,
            scene_effect_graph_count: 0,
            scene_audio_response_binding_count: 0,
            unsupported_scene_features: Vec::new(),
            scene_size: None,
            scene_fit: fit,
            display: Some(SceneDisplayPlan::Image {
                source,
                fit,
                background,
            }),
            layers: vec![layer],
        });
    }

    if let Some(path_data) = path_data {
        if color.as_deref().is_none_or(str::is_empty)
            && stroke_color.as_deref().is_none_or(str::is_empty)
        {
            return Err("--path-data requires --color or --stroke-color".into());
        }
        let mut layer = scene_cli_layer("cli-path", SceneNodeKind::Path);
        layer.path_data = Some(path_data);
        layer.path_fill_rule = path_fill_rule;
        layer.color = color;
        layer.stroke_color = stroke_color;
        layer.stroke_width = stroke_width.or(Some(1.0));
        let background = background.unwrap_or_else(|| "#000000".to_owned());
        return Ok(SceneWallpaperPlan {
            output_name,
            source: None,
            manifest_max_fps: None,
            target_max_fps,
            snapshot_time_ms,
            scene_systems: SceneSystems::default(),
            audio_cue_count: 0,
            bound_properties: Vec::new(),
            timeline_animation_count: 0,
            timeline_animated_layer_count: 0,
            property_binding_count: 0,
            cursor_parallax_input_ready: false,
            scene_input_properties: Default::default(),
            scene_scenescript_binding_count: 0,
            scene_material_graph_count: 0,
            scene_material_graph_resource_count: 0,
            scene_effect_graph_count: 0,
            scene_audio_response_binding_count: 0,
            unsupported_scene_features: Vec::new(),
            scene_size: None,
            scene_fit: fit,
            display: Some(SceneDisplayPlan::Color { color: background }),
            layers: vec![layer],
        });
    }

    if let Some(text) = text {
        let color = text_color.unwrap_or_else(|| "#ffffff".to_owned());
        let mut layer = scene_cli_layer("cli-text", SceneNodeKind::Text);
        layer.text = Some(text);
        layer.color = Some(color);
        layer.font_size = Some(text_font_size.unwrap_or(48.0));
        layer.width = Some(1024.0);
        layer.height = Some(128.0);
        layer.transform.x = 64.0;
        layer.transform.y = 64.0;
        layer.transform.anchor_x = 0.0;
        layer.transform.anchor_y = 0.0;
        let background = background.unwrap_or_else(|| "#000000".to_owned());
        return Ok(SceneWallpaperPlan {
            output_name,
            source: None,
            manifest_max_fps: None,
            target_max_fps,
            snapshot_time_ms,
            scene_systems: SceneSystems::default(),
            audio_cue_count: 0,
            bound_properties: Vec::new(),
            timeline_animation_count: 0,
            timeline_animated_layer_count: 0,
            property_binding_count: 0,
            cursor_parallax_input_ready: false,
            scene_input_properties: Default::default(),
            scene_scenescript_binding_count: 0,
            scene_material_graph_count: 0,
            scene_material_graph_resource_count: 0,
            scene_effect_graph_count: 0,
            scene_audio_response_binding_count: 0,
            unsupported_scene_features: Vec::new(),
            scene_size: None,
            scene_fit: fit,
            display: Some(SceneDisplayPlan::Color { color: background }),
            layers: vec![layer],
        });
    }

    let color = color.ok_or("--run-scene requires --source, --text, or hex --color #rrggbb")?;
    let mut layer = scene_cli_layer("cli-color", SceneNodeKind::Color);
    layer.color = Some(color.clone());
    Ok(SceneWallpaperPlan {
        output_name,
        source: None,
        manifest_max_fps: None,
        target_max_fps,
        snapshot_time_ms,
        scene_systems: SceneSystems::default(),
        audio_cue_count: 0,
        bound_properties: Vec::new(),
        timeline_animation_count: 0,
        timeline_animated_layer_count: 0,
        property_binding_count: 0,
        cursor_parallax_input_ready: false,
        scene_input_properties: Default::default(),
        scene_scenescript_binding_count: 0,
        scene_material_graph_count: 0,
        scene_material_graph_resource_count: 0,
        scene_effect_graph_count: 0,
        scene_audio_response_binding_count: 0,
        unsupported_scene_features: Vec::new(),
        scene_size: None,
        scene_fit: fit,
        display: Some(SceneDisplayPlan::Color { color }),
        layers: vec![layer],
    })
}

#[cfg(feature = "native-vulkan-renderer")]
fn scene_cli_source_is_gscene(path: &Path) -> bool {
    path.file_name()
        .and_then(|file_name| file_name.to_str())
        .is_some_and(|file_name| file_name.ends_with(".gscene.json"))
}

#[cfg(feature = "native-vulkan-renderer")]
fn scene_cli_cursor_parallax_properties(
    output_name: &str,
) -> (
    Option<std::collections::BTreeMap<String, serde_json::Value>>,
    bool,
) {
    std::env::var("GILDER_CURSOR_PARALLAX")
        .ok()
        .map(|value| scene_cli_cursor_parallax_properties_from_override(output_name, &value))
        .unwrap_or((None, false))
}

#[cfg(feature = "native-vulkan-renderer")]
fn scene_cli_cursor_parallax_properties_from_override(
    output_name: &str,
    value: &str,
) -> (
    Option<std::collections::BTreeMap<String, serde_json::Value>>,
    bool,
) {
    let Some((target_output, parallax)) = DesktopCursorParallax::parse_override(value) else {
        return (None, false);
    };
    if target_output
        .as_deref()
        .is_some_and(|target_output| target_output != output_name)
    {
        return (None, false);
    }
    let mut properties = std::collections::BTreeMap::new();
    properties.insert(
        "scene.parallax.x".to_owned(),
        serde_json::Value::from(parallax.x),
    );
    properties.insert(
        "scene.parallax.y".to_owned(),
        serde_json::Value::from(parallax.y),
    );
    (Some(properties), true)
}

#[cfg(feature = "native-vulkan-renderer")]
fn scene_cli_default_gscene_package_root(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if parent.file_name().and_then(|name| name.to_str()) == Some("assets")
        && let Some(root) = parent.parent()
    {
        return root.to_path_buf();
    }
    parent.to_path_buf()
}

#[cfg(feature = "native-vulkan-renderer")]
fn scene_cli_layer(id: &str, kind: SceneNodeKind) -> SceneRenderLayer {
    SceneRenderLayer {
        id: id.to_owned(),
        kind,
        source: None,
        texture_region: None,
        audio: Vec::new(),
        color: None,
        stroke_color: None,
        stroke_width: None,
        corner_radius: None,
        width: None,
        height: None,
        mesh: None,
        text: None,
        font_size: None,
        font_family: None,
        font_source: None,
        font_weight: None,
        text_align: None,
        path_data: None,
        path_fill_rule: gilder::core::ScenePathFillRule::default(),
        fit: FitMode::Cover,
        opacity: 1.0,
        transform: SceneTransform::default(),
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

#[cfg(all(test, feature = "native-vulkan-renderer"))]
mod tests {
    use super::*;

    #[test]
    fn scene_cli_plan_builds_full_extent_image_layer() {
        let plan = scene_cli_plan(
            "HDMI-A-1".to_owned(),
            Some(PathBuf::from("/tmp/wall.png")),
            false,
            None,
            FitMode::Contain,
            Some("#010203".to_owned()),
            None,
            None,
            ScenePathFillRule::default(),
            None,
            None,
            None,
            None,
            None,
            2468,
            Some(30),
        )
        .expect("image scene plan");

        assert_eq!(plan.source, None);
        assert_eq!(plan.target_max_fps, Some(30));
        assert_eq!(plan.snapshot_time_ms, 2468);
        assert_eq!(
            plan.display,
            Some(SceneDisplayPlan::Image {
                source: PathBuf::from("/tmp/wall.png"),
                fit: FitMode::Contain,
                background: Some("#010203".to_owned()),
            })
        );
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0].kind, SceneNodeKind::Image);
        assert_eq!(plan.layers[0].source, Some(PathBuf::from("/tmp/wall.png")));
        assert_eq!(plan.layers[0].fit, FitMode::Contain);
        assert_eq!(plan.layers[0].width, None);
        assert_eq!(plan.layers[0].height, None);
    }

    #[test]
    fn scene_cli_detects_gscene_package_root() {
        let source = Path::new("/tmp/package/assets/scene.gscene.json");

        assert!(scene_cli_source_is_gscene(source));
        assert_eq!(
            scene_cli_default_gscene_package_root(source),
            PathBuf::from("/tmp/package")
        );
        assert!(!scene_cli_source_is_gscene(Path::new(
            "/tmp/package/assets/scene.json"
        )));
    }

    #[test]
    fn scene_cli_cursor_parallax_override_builds_render_properties() {
        let (properties, ready) =
            scene_cli_cursor_parallax_properties_from_override("HDMI-A-1", "HDMI-A-1:0.25,-0.5");
        let properties = properties.expect("cursor parallax properties");

        assert!(ready);
        assert_eq!(
            properties["scene.parallax.x"],
            serde_json::Value::from(0.25)
        );
        assert_eq!(
            properties["scene.parallax.y"],
            serde_json::Value::from(-0.5)
        );

        let (properties, ready) =
            scene_cli_cursor_parallax_properties_from_override("HDMI-A-1", "DP-1:0.25,-0.5");
        assert!(!ready);
        assert_eq!(properties, None);

        let (properties, ready) =
            scene_cli_cursor_parallax_properties_from_override("HDMI-A-1", "2,-2");
        let properties = properties.expect("unnamed cursor parallax properties");
        assert!(ready);
        assert_eq!(properties["scene.parallax.x"], serde_json::Value::from(1.0));
        assert_eq!(
            properties["scene.parallax.y"],
            serde_json::Value::from(-1.0)
        );
    }

    #[test]
    fn scene_cli_plan_loads_gscene_document_source() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "gilder-native-vulkan-cli-gscene-{}-{nonce}",
            std::process::id()
        ));
        let assets = root.join("assets");
        std::fs::create_dir_all(assets.join("audio")).unwrap();
        std::fs::write(assets.join("background.svg"), b"<svg/>").unwrap();
        std::fs::write(assets.join("audio/theme.ogg"), b"not real ogg").unwrap();
        std::fs::write(
            assets.join("scene.gscene.json"),
            br##"{
              "resources": [
                { "id": "background-resource", "type": "image", "source": "assets/background.svg" },
                { "id": "theme-audio", "type": "audio", "source": "assets/audio/theme.ogg" }
              ],
              "nodes": [
                {
                  "id": "background",
                  "type": "image",
                  "resource": "background-resource",
                  "transform": { "x": 0, "y": 0 },
                  "audio": [
                    { "resource": "theme-audio", "playback_mode": "loop" }
                  ]
                }
              ],
              "property_bindings": [
                {
                  "property": "scene_x",
                  "target_node": "background",
                  "target": "x",
                  "scale": 1,
                  "offset": 0
                }
              ]
            }"##,
        )
        .unwrap();
        std::fs::write(
            root.join(gilder::core::MANIFEST_FILE),
            br##"{
              "format": "gilder.wallpaper",
              "format_version": 1,
              "id": "cli-gscene-test",
              "version": "1.0.0",
              "title": "CLI GScene Test",
              "kind": "scene",
              "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 48
              },
              "properties": {
                "scene_x": {
                  "type": "range",
                  "min": 0,
                  "max": 100,
                  "default": 42
                }
              }
            }"##,
        )
        .unwrap();
        let source = assets.join("scene.gscene.json");

        let plan = scene_cli_plan(
            "HDMI-A-1".to_owned(),
            Some(source.clone()),
            false,
            None,
            FitMode::Contain,
            None,
            None,
            None,
            ScenePathFillRule::default(),
            None,
            None,
            None,
            None,
            None,
            2468,
            Some(30),
        )
        .expect("gscene scene plan");

        assert_eq!(plan.source, Some(source));
        assert_eq!(plan.manifest_max_fps, Some(48));
        assert_eq!(plan.snapshot_time_ms, 2468);
        assert!(!plan.cursor_parallax_input_ready);
        assert_eq!(
            plan.scene_input_properties["scene_x"],
            serde_json::json!(42.0)
        );
        assert_eq!(plan.audio_cue_count, 1);
        assert_eq!(plan.layers[0].transform.x, 42.0);
        assert!(
            plan.layers[0]
                .source
                .as_ref()
                .unwrap()
                .ends_with("assets/background.svg")
        );
        assert!(
            plan.layers[0].audio[0]
                .source
                .ends_with("assets/audio/theme.ogg")
        );
        assert_eq!(
            plan.layers[0].audio[0].playback_mode.as_deref(),
            Some("loop")
        );
    }

    #[test]
    fn scene_cli_plan_builds_color_layer() {
        let plan = scene_cli_plan(
            "HDMI-A-1".to_owned(),
            None,
            false,
            None,
            FitMode::Cover,
            None,
            Some("#102030".to_owned()),
            None,
            ScenePathFillRule::default(),
            None,
            None,
            None,
            None,
            None,
            1357,
            None,
        )
        .expect("color scene plan");

        assert_eq!(plan.snapshot_time_ms, 1357);
        assert_eq!(
            plan.display,
            Some(SceneDisplayPlan::Color {
                color: "#102030".to_owned(),
            })
        );
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0].kind, SceneNodeKind::Color);
        assert_eq!(plan.layers[0].color, Some("#102030".to_owned()));
    }

    #[test]
    fn scene_cli_plan_builds_text_layer() {
        let plan = scene_cli_plan(
            "HDMI-A-1".to_owned(),
            None,
            false,
            None,
            FitMode::Cover,
            Some("#101010".to_owned()),
            None,
            None,
            ScenePathFillRule::default(),
            None,
            None,
            Some("Native Text".to_owned()),
            Some("#f8fafc".to_owned()),
            Some(36.0),
            975,
            Some(30),
        )
        .expect("text scene plan");

        assert_eq!(plan.snapshot_time_ms, 975);
        assert_eq!(plan.target_max_fps, Some(30));
        assert_eq!(
            plan.display,
            Some(SceneDisplayPlan::Color {
                color: "#101010".to_owned(),
            })
        );
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0].kind, SceneNodeKind::Text);
        assert_eq!(plan.layers[0].text.as_deref(), Some("Native Text"));
        assert_eq!(plan.layers[0].color.as_deref(), Some("#f8fafc"));
        assert_eq!(plan.layers[0].font_size, Some(36.0));
        assert_eq!(plan.layers[0].width, Some(1024.0));
        assert_eq!(plan.layers[0].transform.anchor_x, 0.0);
        assert_eq!(plan.layers[0].transform.anchor_y, 0.0);
    }

    #[test]
    fn scene_cli_plan_builds_stroked_path_layer() {
        let plan = scene_cli_plan(
            "HDMI-A-1".to_owned(),
            None,
            false,
            None,
            FitMode::Cover,
            Some("#101010".to_owned()),
            None,
            Some("M0 0 L96 0 L48 64 Z".to_owned()),
            ScenePathFillRule::Evenodd,
            Some("#f8fafc".to_owned()),
            Some(5.0),
            None,
            None,
            None,
            2468,
            Some(30),
        )
        .expect("path scene plan");

        assert_eq!(plan.snapshot_time_ms, 2468);
        assert_eq!(plan.target_max_fps, Some(30));
        assert_eq!(
            plan.display,
            Some(SceneDisplayPlan::Color {
                color: "#101010".to_owned(),
            })
        );
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0].kind, SceneNodeKind::Path);
        assert_eq!(
            plan.layers[0].path_data.as_deref(),
            Some("M0 0 L96 0 L48 64 Z")
        );
        assert_eq!(plan.layers[0].color, None);
        assert_eq!(plan.layers[0].path_fill_rule, ScenePathFillRule::Evenodd);
        assert_eq!(plan.layers[0].stroke_color.as_deref(), Some("#f8fafc"));
        assert_eq!(plan.layers[0].stroke_width, Some(5.0));
    }

    #[test]
    fn scene_cli_plan_builds_video_layer() {
        let plan = scene_cli_plan(
            "HDMI-A-1".to_owned(),
            Some(PathBuf::from("/tmp/clip.hevc")),
            true,
            None,
            FitMode::Contain,
            Some("#101010".to_owned()),
            None,
            None,
            ScenePathFillRule::default(),
            None,
            None,
            None,
            None,
            None,
            4321,
            Some(240),
        )
        .expect("video scene plan");

        assert_eq!(plan.snapshot_time_ms, 4321);
        assert_eq!(plan.target_max_fps, Some(240));
        assert_eq!(plan.display, None);
        assert_eq!(plan.layers.len(), 2);
        assert_eq!(plan.layers[0].kind, SceneNodeKind::Color);
        assert_eq!(plan.layers[0].color.as_deref(), Some("#101010"));
        assert_eq!(plan.layers[1].kind, SceneNodeKind::Video);
        assert_eq!(plan.layers[1].source, Some(PathBuf::from("/tmp/clip.hevc")));
        assert_eq!(plan.layers[1].fit, FitMode::Contain);
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
--decode-h264-ready-prefix N extends --probe-vulkanalia-video-session/--run-video with N reference-ready H.264 AU Vulkan Video decode submits.\n\
--decode-h265-ready-prefix N extends --probe-vulkanalia-video-session/--run-video with N ready H.265 AU Vulkan Video decode submits.\n\
--decode-av1-ready-prefix N extends --run-video with N visible AV1 temporal units through Vulkan Video decode/present.\n\
--playback-frames N repeats the ready-prefix AU window for N direct Vulkan Video decode/present frames.\n\
--run-clear uses the Vulkanalia Wayland swapchain runtime, clears frames with CmdPipelineBarrier2/QueueSubmit2, presents, then prints runtime JSON.\n\
--scene-runtime-snapshot builds the same native scene runtime snapshot as --run-scene and exits before presenting, preserving CPU geometry evidence in JSON.\n\
--run-scene builds a scene plan from --source, --scene-root, --scene-video, --path-data, --text, or hex --color and runs the unified native scene presenter.\n\
--run-static uses Vulkanalia sampled-image dynamic rendering for static wallpapers with cover|contain|stretch|tile|center fit and background clear.\n\
--run-video uses Vulkanalia ready-prefix video. Without explicit --decode-*-ready-prefix, it uses the codec default ready-prefix window.\n\
--run-vulkanalia-ready-prefix-video decodes a streaming H.264/H.265 source through Vulkanalia CmdPipelineBarrier2/QueueSubmit2 and prints runtime JSON.\n\
Options: [--output-name NAME] [--layer background|bottom|top|overlay] [--wait-roundtrips N]\n\
         [--duration SECONDS] [--target-fps FPS|--no-fps-limit] [--color #rrggbb|r,g,b]\n\
         [--source PATH] [--scene-root PATH] [--scene-video] [--poster PATH] [--fit cover|contain|stretch|tile|center] [--background #rrggbb] [--text TEXT] [--text-color #rrggbb] [--font-size PX]\n\
         [--path-data SVG_PATH] [--path-fill-rule nonzero|evenodd] [--stroke-color #rrggbb] [--stroke-width PX]\n\
         [--scene-time-ms MS]\n\
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
