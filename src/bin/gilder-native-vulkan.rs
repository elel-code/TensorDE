#[cfg(feature = "native-vulkan-renderer")]
use gilder::core::FitMode;
#[cfg(feature = "native-vulkan-renderer")]
use gilder::renderer::native_vulkan::NativeVulkanClearColor;
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
    use gilder::engine::scene::SceneStorage;
    use gilder::renderer::StaticWallpaperPlan;
    #[cfg(feature = "native-vulkan-video")]
    use gilder::renderer::native_vulkan::native_vulkan_video_playback_frame_count;
    use gilder::renderer::native_vulkan::{
        NativeVulkanAudioOutputPolicy, NativeVulkanOptions, NativeVulkanSceneRunOptions,
        NativeVulkanSurfaceProbeOptions, NativeVulkanVideoSessionSmokeOptions, backend_contract,
        capabilities, native_vulkan_scene_backend_plan,
        native_vulkan_video_duration_playback_frames, native_vulkan_video_run_route,
        probe_vulkan_video_decode, probe_wayland_surface, run_clear, run_scene_with_options,
        run_static_image, wallpaper_type_support_matrix,
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
    let mut duration = Duration::from_secs(5);
    let mut duration_set = false;
    let mut source = None::<PathBuf>;
    let mut vulkan_device = None::<String>;
    let mut vulkan_device_preference = None::<String>;
    let mut capture_frame = None::<PathBuf>;
    let mut capture_frame_number = 1u64;
    let mut capture_frame_number_set = false;
    let mut capture_frame_count = 1u64;
    let mut capture_frame_count_set = false;
    let mut capture_frame_step = 1u64;
    let mut capture_frame_step_set = false;
    let mut capture_frame_downscale = 1u32;
    let mut capture_frame_downscale_set = false;
    let mut capture_frame_region = None::<(u32, u32, u32, u32)>;
    let mut capture_frame_region_set = false;
    let mut capture_frame_reference = None::<PathBuf>;
    let mut capture_frame_reference_set = false;
    let mut capture_frame_time_step_seconds = None::<f32>;
    let mut capture_frame_time_step_set = false;
    let mut capture_scene_graph = None::<u32>;
    let mut scene_surface_width = None::<u32>;
    let mut scene_surface_height = None::<u32>;
    let mut scene_gpu_timing = false;
    let mut fit = FitMode::Cover;
    let mut _fit_set = false;
    let mut background = None::<String>;
    let mut scene_clear_color_override = None::<NativeVulkanClearColor>;
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
    let mut _video_width_set = false;
    let mut _video_height_set = false;
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
            "--scene-backend-plan" => mode = NativeVulkanCliMode::SceneBackendPlan,
            "--run-scene" => mode = NativeVulkanCliMode::RunScene,
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
            }
            "--no-fps-limit" => {
                options.target_max_fps = None;
            }
            "--color" => {
                let value = args.next().ok_or("--color requires #rrggbb or r,g,b")?;
                let color = parse_color(&value)?;
                options.clear_color = color;
                scene_clear_color_override = Some(color);
            }
            "--source" => {
                source = Some(args.next().ok_or("--source requires a path")?.into());
            }
            "--vulkan-device" => {
                vulkan_device = Some(args.next().ok_or("--vulkan-device requires a selector")?);
            }
            "--vulkan-device-preference" => {
                let value = args
                    .next()
                    .ok_or("--vulkan-device-preference requires a value")?;
                if !matches!(value.as_str(), "discrete" | "integrated" | "enumeration") {
                    return Err(
                        "--vulkan-device-preference requires discrete, integrated, or enumeration"
                            .into(),
                    );
                }
                vulkan_device_preference = Some(value);
            }
            "--capture-frame" => {
                capture_frame = Some(parse_capture_frame_path(args.next())?);
            }
            "--capture-frame-number" => {
                capture_frame_number = parse_capture_frame_number(args.next())?;
                capture_frame_number_set = true;
            }
            "--capture-frame-count" => {
                capture_frame_count = parse_capture_frame_count(args.next())?;
                capture_frame_count_set = true;
            }
            "--capture-frame-step" => {
                capture_frame_step = parse_capture_frame_step(args.next())?;
                capture_frame_step_set = true;
            }
            "--capture-frame-downscale" => {
                capture_frame_downscale = parse_capture_frame_downscale(args.next())?;
                capture_frame_downscale_set = true;
            }
            "--capture-frame-region" => {
                capture_frame_region = Some(parse_capture_frame_region(args.next())?);
                capture_frame_region_set = true;
            }
            "--capture-frame-reference" => {
                capture_frame_reference = Some(parse_capture_frame_reference(args.next())?);
                capture_frame_reference_set = true;
            }
            "--capture-frame-time-step" => {
                capture_frame_time_step_seconds = Some(parse_capture_frame_time_step(args.next())?);
                capture_frame_time_step_set = true;
            }
            "--capture-scene-graph" => {
                capture_scene_graph = Some(parse_capture_scene_graph(args.next())?);
            }
            "--surface-width" => {
                scene_surface_width = Some(
                    args.next()
                        .ok_or("--surface-width requires pixels")?
                        .parse::<u32>()?,
                );
            }
            "--surface-height" => {
                scene_surface_height = Some(
                    args.next()
                        .ok_or("--surface-height requires pixels")?
                        .parse::<u32>()?,
                );
            }
            "--gpu-timing" => scene_gpu_timing = true,
            "--scene-video" => {
                return Err("--scene-video was removed with the old scene CLI".into());
            }
            "--poster" => {
                let _ = args.next().ok_or("--poster requires a path")?;
            }
            "--fit" => {
                let value = args.next().ok_or("--fit requires a value")?;
                fit = parse_fit_mode(&value)?;
                _fit_set = true;
            }
            "--background" => {
                background = Some(args.next().ok_or("--background requires #rrggbb")?);
            }
            "--text" | "--text-color" | "--font-size" | "--path-data" | "--path-fill-rule"
            | "--stroke-color" | "--stroke-width" | "--scene-time-ms" | "--snapshot-time-ms"
            | "--scene-root" => {
                return Err(format!("{arg} was removed with the old scene CLI").into());
            }
            "--scene-shader-artifact-root" => {
                return Err(
                    "--scene-shader-artifact-root was removed; scene shaders are engine built-ins"
                        .into(),
                );
            }
            "--scene-property" => {
                return Err("--scene-property was removed with the old scene CLI".into());
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
                    let _ = NativeVulkanAudioOutputPolicy::parse_cli(&value)?;
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
                _video_width_set = true;
            }
            "--height" => {
                video_session_options.height = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--height requires pixels")?;
                _video_height_set = true;
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
    if capture_frame.is_some() && mode != NativeVulkanCliMode::RunScene {
        return Err("--capture-frame requires --run-scene".into());
    }
    if capture_frame.is_none() && capture_frame_number_set {
        return Err("--capture-frame-number requires --capture-frame".into());
    }
    if capture_frame.is_none() && capture_frame_count_set {
        return Err("--capture-frame-count requires --capture-frame".into());
    }
    if capture_frame.is_none() && capture_frame_step_set {
        return Err("--capture-frame-step requires --capture-frame".into());
    }
    if capture_frame.is_none() && capture_frame_downscale_set {
        return Err("--capture-frame-downscale requires --capture-frame".into());
    }
    if capture_frame.is_none() && capture_frame_region_set {
        return Err("--capture-frame-region requires --capture-frame".into());
    }
    if capture_frame.is_none() && capture_frame_reference_set {
        return Err("--capture-frame-reference requires --capture-frame".into());
    }
    if capture_frame_reference.is_some() && capture_frame_count < 3 {
        return Err(
            "--capture-frame-reference requires --capture-frame-count of at least 3".into(),
        );
    }
    if capture_frame_reference.is_some() && capture_frame_time_step_seconds.is_none() {
        return Err("--capture-frame-reference requires --capture-frame-time-step".into());
    }
    if capture_frame.is_none() && capture_frame_time_step_set {
        return Err("--capture-frame-time-step requires --capture-frame".into());
    }
    if capture_frame.is_none() && capture_scene_graph.is_some() {
        return Err("--capture-scene-graph requires --capture-frame".into());
    }
    if scene_gpu_timing && mode != NativeVulkanCliMode::RunScene {
        return Err("--gpu-timing requires --run-scene".into());
    }
    let scene_surface_extent =
        parse_scene_surface_extent(scene_surface_width, scene_surface_height)?;

    let duration_playback_frames = if duration_set {
        native_vulkan_video_duration_playback_frames(duration, options.target_max_fps)
    } else {
        None
    };
    apply_vulkan_device_cli_environment(
        vulkan_device.as_deref(),
        vulkan_device_preference.as_deref(),
    )?;
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
        NativeVulkanCliMode::SceneBackendPlan => {
            let source = source.ok_or("--scene-backend-plan requires --source <file.gscene>")?;
            if !source.is_file() {
                return Err(format!("scene source does not exist: {}", source.display()).into());
            }
            let file = std::fs::File::open(&source)?;
            let storage = SceneStorage::from_binary_reader(file)?;
            json!(native_vulkan_scene_backend_plan(&storage))
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
        NativeVulkanCliMode::RunStatic => {
            let source = source.ok_or("--run-static requires --source")?;
            if !source.is_file() {
                return Err(format!("static source does not exist: {}", source.display()).into());
            }
            if !native_vulkan_static_source_is_gtex(&source) {
                return Err(format!(
                    "--run-static requires a native .gtex BC7 source {}; image conversion must be rebuilt through the new scene engine binary resource format",
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
        NativeVulkanCliMode::RunScene => {
            let source = source.ok_or("--run-scene requires --source <file.gscene>")?;
            if !source.is_file() {
                return Err(format!("scene source does not exist: {}", source.display()).into());
            }
            json!(run_scene_with_options(
                options,
                duration,
                source,
                NativeVulkanSceneRunOptions {
                    pointer_events: true,
                    capture_frame,
                    capture_frame_number,
                    capture_frame_count,
                    capture_frame_step,
                    capture_frame_downscale,
                    capture_frame_region,
                    capture_frame_reference,
                    capture_frame_time_step_seconds,
                    capture_scene_graph,
                    clear_color_override: scene_clear_color_override,
                    surface_extent: scene_surface_extent,
                    gpu_timing: scene_gpu_timing,
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

include!("gilder-native-vulkan/cli_and_usage.rs");
