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
    let mut capture_frame = None::<PathBuf>;
    let mut capture_frame_number = 1u64;
    let mut capture_frame_number_set = false;
    let mut fit = FitMode::Cover;
    let mut _fit_set = false;
    let mut background = None::<String>;
    let mut _scene_color = None::<String>;
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
                options.clear_color = parse_color(&value)?;
                if value.starts_with('#') {
                    _scene_color = Some(value);
                }
            }
            "--source" => {
                source = Some(args.next().ok_or("--source requires a path")?.into());
            }
            "--capture-frame" => {
                capture_frame = Some(parse_capture_frame_path(args.next())?);
            }
            "--capture-frame-number" => {
                capture_frame_number = parse_capture_frame_number(args.next())?;
                capture_frame_number_set = true;
            }
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
                    capture_frame,
                    capture_frame_number,
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
fn parse_capture_frame_path(value: Option<String>) -> Result<PathBuf, &'static str> {
    value
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or("--capture-frame requires a path")
}

#[cfg(feature = "native-vulkan-renderer")]
fn parse_capture_frame_number(value: Option<String>) -> Result<u64, &'static str> {
    value
        .ok_or("--capture-frame-number requires a positive frame number")?
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or("--capture-frame-number requires a positive frame number")
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
    SceneBackendPlan,
    RunClear,
    RunStatic,
    RunScene,
    RunVideo,
    RunVulkanaliaReadyPrefixVideo,
}

#[cfg(feature = "native-vulkan-renderer")]
fn print_usage() {
    println!(
        "Usage: gilder-native-vulkan [--json|--capabilities|--contract|--type-support|--probe-surface|--probe-video|--probe-vulkanalia|--probe-vulkanalia-swapchain|--probe-vulkanalia-video-present|--probe-vulkanalia-video-present-session|--probe-vulkanalia-video-session|--scene-backend-plan|--run-clear|--run-static|--run-scene|--run-video|--run-vulkanalia-ready-prefix-video]\n\
\n\
Print native Vulkan spike capabilities and backend contract.\n\
--probe-surface creates a layer-shell Wayland surface and VK_KHR_wayland_surface, then exits.\n\
--probe-video enumerates Vulkan Video decode extensions and queue families, then exits.\n\
--probe-vulkanalia enumerates the vulkanalia Vulkan 1.4 physical-device/video/external-memory gates, then exits.\n\
--probe-vulkanalia-swapchain creates a Wayland VkSurfaceKHR, Vulkanalia device, swapchain and swapchain image list, then exits.\n\
--probe-vulkanalia-video-present creates one Vulkanalia device with video-decode and graphics/present queues plus a Wayland swapchain, then exits.\n\
--probe-vulkanalia-video-present-session creates one Vulkanalia video+present device, video session, sampled DPB/output image, and Wayland swapchain, then exits.\n\
--probe-vulkanalia-video-session creates and binds a Vulkanalia Vulkan Video session for --video-codec, then exits.\n\
--scene-backend-plan reads --source file.gscene and prints the native Vulkan scene storage/pipeline/executor plan, then exits.\n\
--allocate-video-images extends --probe-vulkanalia-video-session with codec-matching 2-plane 4:2:0 DPB/output sampled image allocation.\n\
--allocate-bitstream-buffer extends --probe-vulkanalia-video-session with an FFmpeg-sized mapped VIDEO_DECODE_SRC slices buffer.\n\
--create-empty-session-parameters extends --probe-vulkanalia-video-session with an H.264/H.265 empty capacity VkVideoSessionParametersKHR smoke.\n\
--create-session-parameters extends --probe-vulkanalia-video-session with real H.264 SPS/PPS, H.265 VPS/SPS/PPS, or AV1 sequence-header VkVideoSessionParametersKHR creation from --source.\n\
--decode-h264-ready-prefix N configures the legacy Vulkanalia compatibility route with N reference-ready H.264 AU decode submits.\n\
--decode-h265-ready-prefix N configures the legacy Vulkanalia compatibility route with N ready H.265 AU decode submits.\n\
--decode-av1-ready-prefix N configures the legacy Vulkanalia compatibility route with N visible AV1 temporal units.\n\
--playback-frames N sets the FFmpeg Vulkan HW present frame budget or repeats the legacy ready-prefix window.\n\
--run-clear uses the Vulkanalia Wayland swapchain runtime, clears frames with CmdPipelineBarrier2/QueueSubmit2, presents, then prints runtime JSON.\n\
--run-static uses Vulkanalia sampled-image dynamic rendering for static wallpapers with cover|contain|stretch|tile|center fit and background clear.\n\
--run-scene reads --source file.gscene, validates the scene engine backend plan and runs the FIFO latest ready scene present loop.\n\
--capture-frame PATH writes a completed --run-scene frame directly from the Vulkan swapchain as an RGBA8 PNG.\n\
--capture-frame-number N selects the 1-based submitted frame captured by --capture-frame; the default is 1.\n\
--run-video selects the FFmpeg Vulkan HW decode mainline and requires AV_PIX_FMT_VULKAN/AVVkFrame before descriptor-heap present.\n\
--run-vulkanalia-ready-prefix-video runs the legacy Vulkanalia Vulkan Video compatibility route and prints runtime JSON.\n\
Options: [--output-name NAME] [--layer background|bottom|top|overlay] [--parent-mapping-buffer|--no-parent-mapping-buffer] [--fractional-scale-rounding ceil|nearest|floor] [--wait-roundtrips N]\n\
         [--duration SECONDS] [--target-fps FPS|--no-fps-limit] [--color #rrggbb|r,g,b] [--capture-frame PATH] [--capture-frame-number N]\n\
         [--source PATH] [--poster PATH] [--fit cover|contain|stretch|tile|center] [--background #rrggbb]\n\
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

#[cfg(all(test, feature = "native-vulkan-renderer"))]
mod tests {
    use super::*;

    #[test]
    fn capture_frame_path_accepts_a_png_destination() {
        assert_eq!(
            parse_capture_frame_path(Some("/tmp/scene.png".to_owned())).unwrap(),
            PathBuf::from("/tmp/scene.png")
        );
    }

    #[test]
    fn capture_frame_path_requires_a_value() {
        assert_eq!(
            parse_capture_frame_path(None).unwrap_err(),
            "--capture-frame requires a path"
        );
    }

    #[test]
    fn capture_frame_number_is_positive_and_one_based() {
        assert_eq!(parse_capture_frame_number(Some("45".to_owned())), Ok(45));
        assert_eq!(
            parse_capture_frame_number(Some("0".to_owned())),
            Err("--capture-frame-number requires a positive frame number")
        );
    }
}
