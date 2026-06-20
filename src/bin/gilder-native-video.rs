#[cfg(all(feature = "native-wayland-renderer", feature = "video-renderer"))]
fn main() {
    if let Err(err) = run() {
        eprintln!("gilder-native-video: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(all(feature = "native-wayland-renderer", feature = "video-renderer")))]
fn main() {
    eprintln!("gilder-native-video requires native-wayland-renderer and video-renderer features");
    std::process::exit(1);
}

#[cfg(all(feature = "native-wayland-renderer", feature = "video-renderer"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use gilder::config::VideoDecoderPolicy;
    use gilder::core::FitMode;
    use gilder::renderer::native_wayland::{
        NativeWaylandLayer, NativeWaylandVideoOptions, NativeWaylandVideoPipeline,
        NativeWaylandVideoSession,
    };
    use std::{
        fs::OpenOptions,
        io::Write,
        path::PathBuf,
        time::{Duration, Instant},
    };

    let mut source = None;
    let mut duration = None;
    let mut target_max_fps = Some(240);
    let mut muted = true;
    let mut loop_playback = true;
    let mut decoder_policy = VideoDecoderPolicy::HardwarePreferred;
    let mut sink_throttle = false;
    let mut layer = NativeWaylandLayer::Bottom;
    let mut allow_foreground_layer = false;
    let mut opaque_region = true;
    let mut input_passthrough = true;
    let mut pipeline = NativeWaylandVideoPipeline::AppsinkDmabufPresent;
    let mut allow_legacy_waylandsink = false;
    let mut fit = FitMode::Cover;
    let mut output_name = None::<String>;
    let mut runtime_json = None::<PathBuf>;
    let mut runtime_interval = Duration::from_secs(1);
    let mut debug_visible_frame = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--source" => source = args.next().map(PathBuf::from),
            "--duration" => {
                duration = args
                    .next()
                    .map(|value| value.parse::<u64>())
                    .transpose()?
                    .map(Duration::from_secs)
            }
            "--target-max-fps" => {
                target_max_fps = args.next().map(|value| value.parse::<u32>()).transpose()?
            }
            "--no-fps-limit" => target_max_fps = None,
            "--sink-throttle" => sink_throttle = true,
            "--no-sink-throttle" => sink_throttle = false,
            "--muted" => muted = true,
            "--unmuted" => muted = false,
            "--loop" => loop_playback = true,
            "--no-loop" => loop_playback = false,
            "--decoder" => {
                let value = args.next().ok_or("--decoder requires a value")?;
                decoder_policy = parse_decoder_policy(&value)?;
            }
            "--layer" => {
                let value = args.next().ok_or("--layer requires a value")?;
                layer = value.parse::<NativeWaylandLayer>()?;
            }
            "--allow-foreground-layer" => allow_foreground_layer = true,
            "--opaque-region" => opaque_region = true,
            "--no-opaque-region" => opaque_region = false,
            "--input-passthrough" => input_passthrough = true,
            "--no-input-passthrough" => input_passthrough = false,
            "--pipeline" => {
                let value = args.next().ok_or("--pipeline requires a value")?;
                pipeline = value.parse::<NativeWaylandVideoPipeline>()?;
            }
            "--allow-legacy-waylandsink" => allow_legacy_waylandsink = true,
            "--fit" => {
                let value = args.next().ok_or("--fit requires a value")?;
                fit = parse_fit_mode(&value)?;
            }
            "--output-name" => {
                output_name = Some(args.next().ok_or("--output-name requires a value")?);
            }
            "--runtime-json" => runtime_json = args.next().map(PathBuf::from),
            "--runtime-interval-ms" => {
                runtime_interval = args
                    .next()
                    .map(|value| value.parse::<u64>())
                    .transpose()?
                    .map(Duration::from_millis)
                    .ok_or("--runtime-interval-ms requires a value")?;
            }
            "--debug-visible-frame" => debug_visible_frame = true,
            "-h" | "--help" => {
                print_usage();
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }

    let source = source.ok_or("--source is required")?;
    if !source.is_file() {
        return Err(format!("source does not exist: {}", source.display()).into());
    }
    if !allow_foreground_layer
        && matches!(layer, NativeWaylandLayer::Top | NativeWaylandLayer::Overlay)
    {
        return Err(format!(
            "--layer {} covers normal application windows; pass --allow-foreground-layer for foreground debug",
            layer.as_str()
        )
        .into());
    }
    if pipeline.uses_legacy_waylandsink() && !allow_legacy_waylandsink {
        return Err(
            "--pipeline playbin/playbin3 is the deprecated playbin+waylandsink path; pass --allow-legacy-waylandsink only for explicit comparison runs"
                .into(),
        );
    }

    let runtime_interval = runtime_interval.max(Duration::from_millis(100));
    let mut runtime_json = runtime_json
        .map(|path| {
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(path)
        })
        .transpose()?;

    let mut session = NativeWaylandVideoSession::new(
        &source,
        NativeWaylandVideoOptions {
            host: gilder::renderer::native_wayland::NativeWaylandHostOptions {
                namespace: "gilder-wallpaper-native-video".to_owned(),
                layer,
                output_name: output_name.clone(),
                opaque_region,
                input_passthrough,
            },
            output_name: output_name.unwrap_or_else(|| "native-wayland".to_owned()),
            fit,
            muted,
            loop_playback,
            target_max_fps,
            sink_throttle,
            decoder_policy,
            start_offset_ms: 0,
            pipeline,
            debug_visible_frame,
        },
    )?;
    session.play()?;

    let started = Instant::now();
    let mut next_runtime_sample = started;
    loop {
        session.tick()?;
        let now = Instant::now();
        if now >= next_runtime_sample {
            if let Some(writer) = runtime_json.as_mut() {
                serde_json::to_writer(&mut *writer, &session.runtime_sample_snapshot())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
            }
            next_runtime_sample = now + runtime_interval;
        }
        if let Some(duration) = duration
            && started.elapsed() >= duration
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    Ok(())
}

#[cfg(all(feature = "native-wayland-renderer", feature = "video-renderer"))]
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

#[cfg(all(feature = "native-wayland-renderer", feature = "video-renderer"))]
fn parse_fit_mode(value: &str) -> Result<gilder::core::FitMode, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "cover" => Ok(gilder::core::FitMode::Cover),
        "contain" => Ok(gilder::core::FitMode::Contain),
        "stretch" => Ok(gilder::core::FitMode::Stretch),
        "center" => Ok(gilder::core::FitMode::Center),
        other => Err(format!("unsupported fit mode: {other}").into()),
    }
}

#[cfg(all(feature = "native-wayland-renderer", feature = "video-renderer"))]
fn print_usage() {
    println!(
        "usage: gilder-native-video --source <path> [--duration <seconds>] [--target-max-fps <fps>|--no-fps-limit] [--sink-throttle] [--decoder auto|hardware-preferred|hardware-required|software] [--pipeline appsink-dmabuf-present|appsink-mmap-probe|appsink-probe|explicit-h264-gl|playbin|playbin3] [--allow-legacy-waylandsink] [--fit cover|contain|stretch|center] [--layer background|bottom|top|overlay] [--allow-foreground-layer] [--output-name <wl_output-name>] [--runtime-json <path>] [--debug-visible-frame]"
    );
}
