#[cfg(feature = "rendering-device")]
use tensor_wallpaper::renderer::rendering_device::RenderingDeviceClearColor;
#[cfg(feature = "rendering-device")]
use std::path::PathBuf;
#[cfg(feature = "rendering-device")]
use std::time::Duration;

#[cfg(feature = "rendering-device")]
const DEFAULT_SCENE_RUN_DURATION: Option<Duration> = None;

#[cfg(feature = "rendering-device")]
#[path = "tensor-wallpaper/scene_execution_plan_report.rs"]
mod scene_execution_plan_report;

#[cfg(feature = "rendering-device")]
fn main() {
    if let Err(err) = run() {
        eprintln!("tensor-wallpaper: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "rendering-device"))]
fn main() {
    eprintln!("tensor-wallpaper requires rendering-device feature");
    std::process::exit(1);
}

#[cfg(feature = "rendering-device")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use tensor_wallpaper::engine::scene::{RenderingServer, SceneStorage};
    use tensor_wallpaper::renderer::rendering_device::{
        RenderingDeviceOptions, RenderingDeviceSceneRunOptions, capabilities,
        rendering_device_video_duration_playback_frames,
        rendering_device_video_playback_frame_count, run_scene_with_options,
        wallpaper_kind_support_matrix, wallpaper_presentation_contract,
    };
    #[cfg(feature = "video")]
    use tensor_wallpaper::renderer::rendering_device::{
        RenderingDeviceSharedVideoPresentOptions, RenderingDeviceVideoSessionCodec,
        run_rendering_device_shared_video_present,
    };
    use tensor_wallpaper::renderer::wayland::{
        WaylandFractionalScaleRounding, WaylandLayer,
    };
    use scene_execution_plan_report::scene_execution_plan_report;
    use serde_json::{Map, json};
    let mut mode = RenderingDeviceCliMode::All;
    let mut options = RenderingDeviceOptions::default();
    let mut duration = DEFAULT_SCENE_RUN_DURATION;
    let mut source = None::<PathBuf>;
    let mut render_device_selector = None::<String>;
    let mut render_device_preference = None::<String>;
    let mut scene_surface_width = None::<u32>;
    let mut scene_surface_height = None::<u32>;
    let mut scene_gpu_timing = false;
    let mut scene_semantic_diagnostics = false;
    let mut scene_pointer_replay_normalized = None::<[f64; 2]>;
    let mut scene_user_property_overrides = Map::new();
    #[cfg(feature = "video")]
    let mut scene_video_sources = Vec::new();
    #[cfg(not(feature = "video"))]
    let scene_video_sources = Vec::new();
    let mut scene_clear_color_override = None::<RenderingDeviceClearColor>;
    let mut allow_foreground_layer = false;
    #[cfg(feature = "video")]
    let mut video_codec = RenderingDeviceVideoSessionCodec::H265Main8;
    let mut video_playback_frames = 0u32;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--capabilities" => mode = RenderingDeviceCliMode::Capabilities,
            "--contract" => mode = RenderingDeviceCliMode::Contract,
            "--wallpaper-kind-support" => mode = RenderingDeviceCliMode::TypeSupport,
            "--scene-execution-plan" => mode = RenderingDeviceCliMode::SceneExecutionPlan,
            "--run-scene" => mode = RenderingDeviceCliMode::RunScene,
            "--run-video" => mode = RenderingDeviceCliMode::RunVideo,
            "--json" => mode = RenderingDeviceCliMode::All,
            "--output-name" => {
                options.host.output_name =
                    Some(args.next().ok_or("--output-name requires a value")?);
            }
            "--layer" => {
                let value = args.next().ok_or("--layer requires a value")?;
                options.host.layer = value.parse::<WaylandLayer>()?;
            }
            "--fractional-scale-rounding" => {
                let value = args
                    .next()
                    .ok_or("--fractional-scale-rounding requires ceil, nearest, or floor")?;
                options.host.fractional_scale_rounding =
                    value.parse::<WaylandFractionalScaleRounding>()?;
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
            "--render-device" => {
                render_device_selector = Some(args.next().ok_or("--render-device requires a selector")?);
            }
            "--render-device-preference" => {
                let value = args
                    .next()
                    .ok_or("--render-device-preference requires a value")?;
                if !matches!(value.as_str(), "discrete" | "integrated" | "enumeration") {
                    return Err(
                        "--render-device-preference requires discrete, integrated, or enumeration"
                            .into(),
                    );
                }
                render_device_preference = Some(value);
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
                #[cfg(feature = "video")]
                scene_video_sources.push(parse_scene_video_source(
                    args.next()
                        .ok_or("--scene-video requires MEDIA_INSTANCE:CODEC:PATH")?,
                )?);
                #[cfg(not(feature = "video"))]
                return Err("--scene-video requires video feature".into());
            }
            "--poster" | "--loop" | "--no-loop" | "--decoder" | "--start-offset-ms" => {
                return Err(format!("{arg} is not supported by tensor-wallpaper").into());
            }
            "--text" | "--text-color" | "--font-size" | "--path-data" | "--path-fill-rule"
            | "--stroke-color" | "--stroke-width" | "--scene-time-ms" | "--snapshot-time-ms"
            | "--scene-root" => {
                return Err(format!("{arg} is not supported by tensor-wallpaper").into());
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
                    "direct video audio output and audio-master-clock controls were removed with the raw video route; the renderer-owned video path currently accepts video timing only"
                        .into(),
                );
            }
            "--video-codec" => {
                #[cfg(feature = "video")]
                {
                    let value = args.next().ok_or("--video-codec requires a value")?;
                    video_codec = value.parse()?;
                }
                #[cfg(not(feature = "video"))]
                {
                    let _ = args.next().ok_or("--video-codec requires a value")?;
                    return Err("--video-codec requires video feature".into());
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
            WaylandLayer::Top | WaylandLayer::Overlay
        )
    {
        return Err(format!(
            "--layer {} covers normal application windows; pass --allow-foreground-layer for foreground debug",
            options.host.layer.as_str()
        )
        .into());
    }
    if scene_gpu_timing && mode != RenderingDeviceCliMode::RunScene {
        return Err("--gpu-timing requires --run-scene".into());
    }
    if scene_semantic_diagnostics && mode != RenderingDeviceCliMode::RunScene {
        return Err("--scene-semantic-diagnostics requires --run-scene".into());
    }
    if !scene_user_property_overrides.is_empty()
        && !matches!(
            mode,
            RenderingDeviceCliMode::RunScene | RenderingDeviceCliMode::SceneExecutionPlan
        )
    {
        return Err("--scene-property requires --run-scene or --scene-execution-plan".into());
    }
    let scene_surface_extent =
        parse_scene_surface_extent(scene_surface_width, scene_surface_height)?;

    let duration_playback_frames = duration.and_then(|duration| {
        rendering_device_video_duration_playback_frames(duration, options.target_max_fps)
    });
    apply_render_device_selector_cli_environment(
        render_device_selector.as_deref(),
        render_device_preference.as_deref(),
    )?;
    let report = match mode {
        RenderingDeviceCliMode::All => {
            json!({ "capabilities": capabilities(), "presentation_contract": wallpaper_presentation_contract() })
        }
        RenderingDeviceCliMode::Capabilities => json!(capabilities()),
        RenderingDeviceCliMode::Contract => json!(wallpaper_presentation_contract()),
        RenderingDeviceCliMode::TypeSupport => json!(wallpaper_kind_support_matrix()),
        RenderingDeviceCliMode::SceneExecutionPlan => {
            let source = source.ok_or("--scene-execution-plan requires --source <file.gscene>")?;
            if !source.is_file() {
                return Err(format!("scene source does not exist: {}", source.display()).into());
            }
            let file = std::fs::File::open(&source)?;
            let storage = SceneStorage::from_binary_reader(file)?;
            let semantic_frame = RenderingServer::new(&storage)
                .semantic_world()?
                .resolve_frame_with_user_properties_at(0.0, &scene_user_property_overrides)?;
            json!(scene_execution_plan_report(
                &storage,
                &semantic_frame,
                scene_surface_extent,
            )?)
        }
        RenderingDeviceCliMode::RunScene => {
            let source = source.ok_or("--run-scene requires --source <file.gscene>")?;
            if !source.is_file() {
                return Err(format!("scene source does not exist: {}", source.display()).into());
            }
            json!(run_scene_with_options(
                options,
                duration,
                source,
                RenderingDeviceSceneRunOptions {
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
        RenderingDeviceCliMode::RunVideo => {
            let source = source.ok_or("--run-video requires --source")?;
            if !source.is_file() {
                return Err(format!("video source does not exist: {}", source.display()).into());
            }
            let playback_frame_count = rendering_device_video_playback_frame_count(
                video_playback_frames,
                duration_playback_frames,
            );
            #[cfg(feature = "video")]
            {
                json!(run_rendering_device_shared_video_present(
                    RenderingDeviceSharedVideoPresentOptions {
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
            #[cfg(not(feature = "video"))]
            {
                let _ = (options, source, playback_frame_count);
                return Err("--run-video GPU video decode route requires video feature".into());
            }
        }
    };
    write_json_report(&report)?;
    Ok(())
}

include!("tensor-wallpaper/cli_and_usage.rs");
