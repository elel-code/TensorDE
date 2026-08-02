#[cfg(feature = "native-vulkan-renderer")]
use gilder::renderer::native_vulkan::NativeVulkanClearColor;
#[cfg(feature = "native-vulkan-renderer")]
use std::path::PathBuf;
#[cfg(feature = "native-vulkan-renderer")]
use std::time::Duration;

#[cfg(feature = "native-vulkan-renderer")]
const DEFAULT_SCENE_RUN_DURATION: Option<Duration> = None;

#[cfg(feature = "native-vulkan-renderer")]
#[path = "gilder-native-vulkan/scene_backend_plan_report.rs"]
mod scene_backend_plan_report;

#[cfg(feature = "native-vulkan-renderer")]
fn main() {
    #[cfg(target_os = "linux")]
    native_vulkan_allocator_env_bootstrap();

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

#[cfg(target_os = "linux")]
fn native_vulkan_sync_executable_after_rebuild(executable: &std::path::Path) {
    let Ok(file) = std::fs::File::open(executable) else {
        return;
    };
    let _ = file.sync_all();
}

#[cfg(target_os = "linux")]
fn native_vulkan_allocator_env_bootstrap() {
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
    needs_reexec |= !native_vulkan_glibc_tcache_disabled();

    if !needs_reexec {
        if std::env::var_os(EXE_SYNCED).as_deref() != Some(std::ffi::OsStr::new("1"))
            && let Ok(executable) = std::env::current_exe()
        {
            native_vulkan_sync_executable_after_rebuild(&executable);
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
    native_vulkan_sync_executable_after_rebuild(&executable);
    let mut command = std::process::Command::new(executable);
    command.args(std::env::args_os().skip(1));
    command.env(BOOTSTRAPPED, "1");
    command.env(EXE_SYNCED, "1");
    for (name, value) in REQUIRED_ENV {
        command.env(name, value);
    }
    command.env(
        "GLIBC_TUNABLES",
        native_vulkan_glibc_tunables_with_tcache_disabled(),
    );

    use std::os::unix::process::CommandExt;
    let err = command.exec();
    eprintln!("gilder-native-vulkan: failed to exec allocator-bootstrapped process: {err}");
    std::process::exit(127);
}

#[cfg(target_os = "linux")]
fn native_vulkan_glibc_tcache_disabled() -> bool {
    std::env::var("GLIBC_TUNABLES").ok().is_some_and(|value| {
        value
            .split(':')
            .any(|entry| entry == "glibc.malloc.tcache_count=0")
    })
}

#[cfg(target_os = "linux")]
fn native_vulkan_glibc_tunables_with_tcache_disabled() -> String {
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
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use gilder::engine::scene::{RenderingServer, SceneStorage};
    use gilder::renderer::native_vulkan::{
        NativeVulkanOptions, NativeVulkanSceneRunOptions, backend_contract, capabilities,
        native_vulkan_video_duration_playback_frames, native_vulkan_video_playback_frame_count,
        run_scene_with_options, wallpaper_type_support_matrix,
    };
    #[cfg(feature = "native-vulkan-video")]
    use gilder::renderer::native_vulkan::{
        NativeVulkanSharedVideoPresentOptions, NativeVulkanVideoSessionCodec,
        run_native_vulkan_shared_video_present,
    };
    use gilder::renderer::native_wayland::{
        NativeWaylandFractionalScaleRounding, NativeWaylandLayer,
    };
    use scene_backend_plan_report::scene_backend_plan_report;
    use serde_json::{Map, json};
    let mut mode = NativeVulkanCliMode::All;
    let mut options = NativeVulkanOptions::default();
    let mut duration = DEFAULT_SCENE_RUN_DURATION;
    let mut source = None::<PathBuf>;
    let mut vulkan_device = None::<String>;
    let mut vulkan_device_preference = None::<String>;
    let mut scene_surface_width = None::<u32>;
    let mut scene_surface_height = None::<u32>;
    let mut scene_gpu_timing = false;
    let mut scene_semantic_diagnostics = false;
    let mut scene_pointer_replay_normalized = None::<[f64; 2]>;
    let mut scene_user_property_overrides = Map::new();
    #[cfg(feature = "native-vulkan-video")]
    let mut scene_video_sources = Vec::new();
    #[cfg(not(feature = "native-vulkan-video"))]
    let scene_video_sources = Vec::new();
    let mut scene_clear_color_override = None::<NativeVulkanClearColor>;
    let mut allow_foreground_layer = false;
    #[cfg(feature = "native-vulkan-video")]
    let mut video_codec = NativeVulkanVideoSessionCodec::H265Main8;
    let mut video_playback_frames = 0u32;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--capabilities" => mode = NativeVulkanCliMode::Capabilities,
            "--contract" => mode = NativeVulkanCliMode::Contract,
            "--type-support" => mode = NativeVulkanCliMode::TypeSupport,
            "--scene-backend-plan" => mode = NativeVulkanCliMode::SceneBackendPlan,
            "--run-scene" => mode = NativeVulkanCliMode::RunScene,
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
                duration = Some(
                    args.next()
                        .map(|value| value.parse::<u64>())
                        .transpose()?
                        .map(Duration::from_secs)
                        .ok_or("--duration requires seconds")?,
                );
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
            "--scene-pointer-position" => {
                scene_pointer_replay_normalized = Some(parse_scene_pointer_position(args.next())?);
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
            "--scene-semantic-diagnostics" => scene_semantic_diagnostics = true,
            "--scene-video" => {
                #[cfg(feature = "native-vulkan-video")]
                scene_video_sources.push(parse_scene_video_source(
                    args.next()
                        .ok_or("--scene-video requires MEDIA_INSTANCE:CODEC:PATH")?,
                )?);
                #[cfg(not(feature = "native-vulkan-video"))]
                return Err("--scene-video requires native-vulkan-video feature".into());
            }
            "--poster" | "--loop" | "--no-loop" | "--decoder" | "--start-offset-ms" => {
                return Err(format!("{arg} is not supported by the native Vulkan CLI").into());
            }
            "--text" | "--text-color" | "--font-size" | "--path-data" | "--path-fill-rule"
            | "--stroke-color" | "--stroke-width" | "--scene-time-ms" | "--snapshot-time-ms"
            | "--scene-root" => {
                return Err(format!("{arg} is not supported by the native Vulkan CLI").into());
            }
            "--scene-shader-artifact-root" => {
                return Err(
                    "--scene-shader-artifact-root was removed; scene shaders are engine built-ins"
                        .into(),
                );
            }
            "--scene-property" => {
                insert_scene_property_override(&mut scene_user_property_overrides, args.next())?;
            }
            "--muted" | "--unmuted" | "--audio-clock-probe" | "--audio-output" => {
                return Err(
                    "native video audio output and audio-master-clock controls were removed with the raw video route; the renderer-owned video path currently accepts video timing only"
                        .into(),
                );
            }
            "--video-codec" => {
                #[cfg(feature = "native-vulkan-video")]
                {
                    let value = args.next().ok_or("--video-codec requires a value")?;
                    video_codec = value.parse()?;
                }
                #[cfg(not(feature = "native-vulkan-video"))]
                {
                    let _ = args.next().ok_or("--video-codec requires a value")?;
                    return Err("--video-codec requires native-vulkan-video feature".into());
                }
            }
            "--playback-frames" => {
                video_playback_frames = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--playback-frames requires a count")?;
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
    if scene_gpu_timing && mode != NativeVulkanCliMode::RunScene {
        return Err("--gpu-timing requires --run-scene".into());
    }
    if scene_semantic_diagnostics && mode != NativeVulkanCliMode::RunScene {
        return Err("--scene-semantic-diagnostics requires --run-scene".into());
    }
    if !scene_user_property_overrides.is_empty()
        && !matches!(
            mode,
            NativeVulkanCliMode::RunScene | NativeVulkanCliMode::SceneBackendPlan
        )
    {
        return Err("--scene-property requires --run-scene or --scene-backend-plan".into());
    }
    let scene_surface_extent =
        parse_scene_surface_extent(scene_surface_width, scene_surface_height)?;

    let duration_playback_frames = duration.and_then(|duration| {
        native_vulkan_video_duration_playback_frames(duration, options.target_max_fps)
    });
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
        NativeVulkanCliMode::SceneBackendPlan => {
            let source = source.ok_or("--scene-backend-plan requires --source <file.gscene>")?;
            if !source.is_file() {
                return Err(format!("scene source does not exist: {}", source.display()).into());
            }
            let file = std::fs::File::open(&source)?;
            let storage = SceneStorage::from_binary_reader(file)?;
            let semantic_frame = RenderingServer::new(&storage)
                .semantic_world()?
                .resolve_frame_with_user_properties_at(0.0, &scene_user_property_overrides)?;
            json!(scene_backend_plan_report(
                &storage,
                &semantic_frame,
                scene_surface_extent,
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
                    user_property_overrides: scene_user_property_overrides,
                    pointer_events: true,
                    pointer_replay_normalized: scene_pointer_replay_normalized,
                    clear_color_override: scene_clear_color_override,
                    surface_extent: scene_surface_extent,
                    gpu_timing: scene_gpu_timing,
                    semantic_diagnostics: scene_semantic_diagnostics,
                    video_sources: scene_video_sources,
                },
            )?)
        }
        NativeVulkanCliMode::RunVideo => {
            let source = source.ok_or("--run-video requires --source")?;
            if !source.is_file() {
                return Err(format!("video source does not exist: {}", source.display()).into());
            }
            let playback_frame_count = native_vulkan_video_playback_frame_count(
                video_playback_frames,
                duration_playback_frames,
            );
            #[cfg(feature = "native-vulkan-video")]
            {
                json!(run_native_vulkan_shared_video_present(
                    NativeVulkanSharedVideoPresentOptions {
                        host: options.host,
                        wait_configure_roundtrips: options.wait_configure_roundtrips,
                        source,
                        codec: video_codec,
                        playback_frame_count,
                        target_max_fps: options.target_max_fps,
                        clear_color: options.clear_color,
                    },
                )?)
            }
            #[cfg(not(feature = "native-vulkan-video"))]
            {
                let _ = (options, source, playback_frame_count);
                return Err(
                    "--run-video FFmpeg Vulkan HW decode route requires native-vulkan-video feature"
                        .into(),
                );
            }
        }
    };
    write_json_report(&report)?;
    Ok(())
}

include!("gilder-native-vulkan/cli_and_usage.rs");
