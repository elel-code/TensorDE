#[cfg(feature = "native-wgpu-renderer")]
use std::io::Write;

#[cfg(feature = "native-wgpu-renderer")]
fn main() {
    if let Err(err) = run() {
        eprintln!("gilder-native-wgpu: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "native-wgpu-renderer"))]
fn main() {
    eprintln!("gilder-native-wgpu requires native-wgpu-renderer feature");
    std::process::exit(1);
}

#[cfg(feature = "native-wgpu-renderer")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use gilder::renderer::native_wayland::NativeWaylandLayer;
    use gilder::renderer::native_wgpu::{
        NativeWgpuColor, NativeWgpuOptions, NativeWgpuRenderMode, NativeWgpuSession,
    };
    use std::{path::PathBuf, time::Duration};

    let mut duration = Duration::from_secs(5);
    let mut target_fps = Some(60);
    let mut layer = NativeWaylandLayer::Bottom;
    let mut allow_foreground_layer = false;
    let mut output_name = None::<String>;
    let mut color = NativeWgpuColor::default();
    let mut render_mode = NativeWgpuRenderMode::Solid;
    let mut source = None::<PathBuf>;
    let mut fit = gilder::core::FitMode::Cover;
    let mut loop_playback = true;
    let mut decoder_policy = gilder::config::VideoDecoderPolicy::HardwarePreferred;
    let mut video_backend = NativeWgpuCliVideoBackend::Auto;
    let mut runtime_json = None::<std::path::PathBuf>;
    let mut runtime_jsonl = None::<std::path::PathBuf>;
    let mut runtime_interval = Duration::from_secs(1);

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--duration" => {
                duration = args
                    .next()
                    .map(|value| value.parse::<u64>())
                    .transpose()?
                    .map(Duration::from_secs)
                    .ok_or("--duration requires seconds")?;
            }
            "--target-fps" => {
                target_fps = args.next().map(|value| value.parse::<u32>()).transpose()?;
            }
            "--no-fps-limit" => target_fps = None,
            "--layer" => {
                let value = args.next().ok_or("--layer requires a value")?;
                layer = value.parse::<NativeWaylandLayer>()?;
            }
            "--allow-foreground-layer" => allow_foreground_layer = true,
            "--output-name" => {
                output_name = Some(args.next().ok_or("--output-name requires a value")?);
            }
            "--color" => {
                let value = args.next().ok_or("--color requires #rrggbb or r,g,b")?;
                color = parse_color(&value)?;
            }
            "--source" => {
                source = Some(args.next().ok_or("--source requires a path")?.into());
            }
            "--fit" => {
                let value = args.next().ok_or("--fit requires a value")?;
                fit = parse_fit_mode(&value)?;
            }
            "--loop" => loop_playback = true,
            "--no-loop" => loop_playback = false,
            "--decoder" => {
                let value = args.next().ok_or("--decoder requires a value")?;
                decoder_policy = parse_decoder_policy(&value)?;
            }
            "--video-backend" => {
                let value = args
                    .next()
                    .ok_or("--video-backend requires auto, cpu-upload, gpu-video, gst-gpu-video, or gst-dmabuf")?;
                video_backend = parse_video_backend(&value)?;
            }
            "--render-mode" => {
                let value = args.next().ok_or("--render-mode requires solid or pulse")?;
                render_mode = value.parse::<NativeWgpuRenderMode>()?;
            }
            "--animate-color" => render_mode = NativeWgpuRenderMode::Pulse,
            "--runtime-json" => {
                runtime_json = Some(args.next().ok_or("--runtime-json requires a path")?.into());
            }
            "--runtime-jsonl" => {
                runtime_jsonl = Some(args.next().ok_or("--runtime-jsonl requires a path")?.into());
            }
            "--runtime-interval-ms" => {
                runtime_interval = args
                    .next()
                    .map(|value| value.parse::<u64>())
                    .transpose()?
                    .map(Duration::from_millis)
                    .ok_or("--runtime-interval-ms requires milliseconds")?;
            }
            "-h" | "--help" => {
                print_usage();
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
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

    if let Some(source) = source {
        if !source.is_file() {
            return Err(format!("source does not exist: {}", source.display()).into());
        }
        return run_video(
            source,
            NativeWgpuOptions {
                namespace: "gilder-wallpaper-native-wgpu".to_owned(),
                layer,
                output_name,
                initial_color: color,
                render_mode,
            },
            fit,
            loop_playback,
            target_fps,
            decoder_policy,
            video_backend,
            duration,
            runtime_interval,
            runtime_json,
            runtime_jsonl,
        );
    }

    let mut session = NativeWgpuSession::connect(NativeWgpuOptions {
        namespace: "gilder-wallpaper-native-wgpu".to_owned(),
        layer,
        output_name,
        initial_color: color,
        render_mode,
    })?;
    let runtime_interval = runtime_interval.max(Duration::from_millis(100));
    let mut runtime_jsonl = runtime_jsonl
        .map(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(path)
                .map(std::io::BufWriter::new)
        })
        .transpose()?;
    run_session(
        &mut session,
        duration,
        target_fps,
        runtime_interval,
        runtime_jsonl.as_mut(),
    )?;

    if let Some(path) = runtime_json {
        let writer = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        serde_json::to_writer_pretty(writer, &session.snapshot())?;
    }
    Ok(())
}

#[cfg(feature = "native-wgpu-renderer")]
fn run_video(
    source: std::path::PathBuf,
    wayland: gilder::renderer::native_wgpu::NativeWgpuOptions,
    fit: gilder::core::FitMode,
    loop_playback: bool,
    target_fps: Option<u32>,
    decoder_policy: gilder::config::VideoDecoderPolicy,
    video_backend: NativeWgpuCliVideoBackend,
    duration: std::time::Duration,
    runtime_interval: std::time::Duration,
    runtime_json: Option<std::path::PathBuf>,
    runtime_jsonl: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    match resolve_video_backend(video_backend, &source) {
        NativeWgpuCliVideoBackend::GpuVideo => run_gpu_video(
            source,
            wayland,
            fit,
            loop_playback,
            target_fps,
            duration,
            runtime_interval,
            runtime_json,
            runtime_jsonl,
            NativeWgpuCliGpuVideoInputMode::AnnexBFile,
        ),
        NativeWgpuCliVideoBackend::GstGpuVideo => run_gpu_video(
            source,
            wayland,
            fit,
            loop_playback,
            target_fps,
            duration,
            runtime_interval,
            runtime_json,
            runtime_jsonl,
            NativeWgpuCliGpuVideoInputMode::GstH264ByteStream,
        ),
        NativeWgpuCliVideoBackend::CpuUpload => run_cpu_upload_video(
            source,
            wayland,
            fit,
            loop_playback,
            target_fps,
            decoder_policy,
            duration,
            runtime_interval,
            runtime_json,
            runtime_jsonl,
        ),
        NativeWgpuCliVideoBackend::GstDmabuf => run_gst_dmabuf_video(
            source,
            wayland,
            fit,
            loop_playback,
            target_fps,
            decoder_policy,
            duration,
            runtime_interval,
            runtime_json,
            runtime_jsonl,
        ),
        NativeWgpuCliVideoBackend::Auto => unreachable!("auto backend must be resolved"),
    }
}

#[cfg(feature = "native-wgpu-renderer")]
fn run_cpu_upload_video(
    source: std::path::PathBuf,
    wayland: gilder::renderer::native_wgpu::NativeWgpuOptions,
    fit: gilder::core::FitMode,
    loop_playback: bool,
    target_fps: Option<u32>,
    decoder_policy: gilder::config::VideoDecoderPolicy,
    duration: std::time::Duration,
    runtime_interval: std::time::Duration,
    runtime_json: Option<std::path::PathBuf>,
    runtime_jsonl: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "video-renderer")]
    {
        use gilder::renderer::native_wgpu::{NativeWgpuVideoOptions, NativeWgpuVideoSession};

        let mut session = NativeWgpuVideoSession::connect(NativeWgpuVideoOptions {
            wayland,
            source,
            fit,
            loop_playback,
            target_max_fps: target_fps,
            decoder_policy,
        })?;
        let runtime_interval = runtime_interval.max(std::time::Duration::from_millis(100));
        let mut runtime_jsonl = runtime_jsonl
            .map(|path| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(path)
                    .map(std::io::BufWriter::new)
            })
            .transpose()?;
        run_video_session(
            &mut session,
            duration,
            target_fps,
            runtime_interval,
            runtime_jsonl.as_mut(),
        )?;

        if let Some(path) = runtime_json {
            let writer = std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(path)?;
            serde_json::to_writer_pretty(writer, &session.snapshot())?;
        }
        return Ok(());
    }

    #[cfg(not(feature = "video-renderer"))]
    {
        let _ = (
            source,
            wayland,
            fit,
            loop_playback,
            target_fps,
            decoder_policy,
            duration,
            runtime_interval,
            runtime_json,
            runtime_jsonl,
        );
        Err("--source requires building with the video-renderer feature".into())
    }
}

#[cfg(all(feature = "native-wgpu-renderer", feature = "native-wgpu-gst-dmabuf"))]
fn run_gst_dmabuf_video(
    source: std::path::PathBuf,
    wayland: gilder::renderer::native_wgpu::NativeWgpuOptions,
    fit: gilder::core::FitMode,
    loop_playback: bool,
    target_fps: Option<u32>,
    decoder_policy: gilder::config::VideoDecoderPolicy,
    duration: std::time::Duration,
    runtime_interval: std::time::Duration,
    runtime_json: Option<std::path::PathBuf>,
    runtime_jsonl: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    use gilder::renderer::native_wgpu::{NativeWgpuGstDmabufOptions, NativeWgpuGstDmabufSession};

    let mut session = NativeWgpuGstDmabufSession::connect(NativeWgpuGstDmabufOptions {
        wayland,
        source,
        fit,
        loop_playback,
        target_max_fps: target_fps,
        decoder_policy,
    })?;
    let runtime_interval = runtime_interval.max(std::time::Duration::from_millis(100));
    let mut runtime_jsonl = runtime_jsonl
        .map(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(path)
                .map(std::io::BufWriter::new)
        })
        .transpose()?;
    run_gst_dmabuf_video_session(
        &mut session,
        duration,
        target_fps,
        runtime_interval,
        runtime_jsonl.as_mut(),
    )?;

    if let Some(path) = runtime_json {
        let writer = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        serde_json::to_writer_pretty(writer, &session.snapshot())?;
    }
    Ok(())
}

#[cfg(all(
    feature = "native-wgpu-renderer",
    not(feature = "native-wgpu-gst-dmabuf")
))]
fn run_gst_dmabuf_video(
    source: std::path::PathBuf,
    wayland: gilder::renderer::native_wgpu::NativeWgpuOptions,
    fit: gilder::core::FitMode,
    loop_playback: bool,
    target_fps: Option<u32>,
    decoder_policy: gilder::config::VideoDecoderPolicy,
    duration: std::time::Duration,
    runtime_interval: std::time::Duration,
    runtime_json: Option<std::path::PathBuf>,
    runtime_jsonl: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = (
        source,
        wayland,
        fit,
        loop_playback,
        target_fps,
        decoder_policy,
        duration,
        runtime_interval,
        runtime_json,
        runtime_jsonl,
    );
    Err("--video-backend gst-dmabuf requires building with native-wgpu-gst-dmabuf".into())
}

#[cfg(all(feature = "native-wgpu-renderer", feature = "native-wgpu-gpu-video"))]
fn run_gpu_video(
    source: std::path::PathBuf,
    wayland: gilder::renderer::native_wgpu::NativeWgpuOptions,
    fit: gilder::core::FitMode,
    loop_playback: bool,
    target_fps: Option<u32>,
    duration: std::time::Duration,
    runtime_interval: std::time::Duration,
    runtime_json: Option<std::path::PathBuf>,
    runtime_jsonl: Option<std::path::PathBuf>,
    input_mode: NativeWgpuCliGpuVideoInputMode,
) -> Result<(), Box<dyn std::error::Error>> {
    use gilder::renderer::native_wgpu::{
        NativeWgpuGpuVideoInputMode, NativeWgpuGpuVideoOptions, NativeWgpuGpuVideoSession,
    };
    let input_mode = match input_mode {
        NativeWgpuCliGpuVideoInputMode::AnnexBFile => NativeWgpuGpuVideoInputMode::AnnexBFile,
        NativeWgpuCliGpuVideoInputMode::GstH264ByteStream => {
            NativeWgpuGpuVideoInputMode::GstH264ByteStream
        }
    };

    let mut session = NativeWgpuGpuVideoSession::connect(NativeWgpuGpuVideoOptions {
        wayland,
        source,
        fit,
        loop_playback,
        input_mode,
    })?;
    let runtime_interval = runtime_interval.max(std::time::Duration::from_millis(100));
    let mut runtime_jsonl = runtime_jsonl
        .map(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(path)
                .map(std::io::BufWriter::new)
        })
        .transpose()?;
    run_gpu_video_session(
        &mut session,
        duration,
        target_fps,
        runtime_interval,
        runtime_jsonl.as_mut(),
    )?;

    if let Some(path) = runtime_json {
        let writer = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        serde_json::to_writer_pretty(writer, &session.snapshot())?;
    }
    Ok(())
}

#[cfg(all(
    feature = "native-wgpu-renderer",
    not(feature = "native-wgpu-gpu-video")
))]
fn run_gpu_video(
    source: std::path::PathBuf,
    wayland: gilder::renderer::native_wgpu::NativeWgpuOptions,
    fit: gilder::core::FitMode,
    loop_playback: bool,
    target_fps: Option<u32>,
    duration: std::time::Duration,
    runtime_interval: std::time::Duration,
    runtime_json: Option<std::path::PathBuf>,
    runtime_jsonl: Option<std::path::PathBuf>,
    input_mode: NativeWgpuCliGpuVideoInputMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = (
        source,
        wayland,
        fit,
        loop_playback,
        target_fps,
        duration,
        runtime_interval,
        runtime_json,
        runtime_jsonl,
        input_mode,
    );
    Err("--video-backend gpu-video requires building with native-wgpu-gpu-video".into())
}

#[cfg(all(feature = "native-wgpu-renderer", feature = "video-renderer"))]
fn run_video_session(
    session: &mut gilder::renderer::native_wgpu::NativeWgpuVideoSession,
    duration: std::time::Duration,
    target_fps: Option<u32>,
    runtime_interval: std::time::Duration,
    runtime_jsonl: Option<&mut std::io::BufWriter<std::fs::File>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    let mut last_runtime_sample = std::time::Instant::now();
    let mut runtime_jsonl = runtime_jsonl;
    let frame_interval = target_fps
        .filter(|fps| *fps > 0)
        .map(|fps| std::time::Duration::from_secs_f64(1.0 / f64::from(fps)));

    write_video_runtime_sample(session, &mut runtime_jsonl)?;
    while started.elapsed() < duration && !session.is_closed() {
        let frame_started = std::time::Instant::now();
        session.tick()?;
        if last_runtime_sample.elapsed() >= runtime_interval {
            write_video_runtime_sample(session, &mut runtime_jsonl)?;
            last_runtime_sample = std::time::Instant::now();
        }
        if let Some(interval) = frame_interval
            && let Some(remaining) = interval.checked_sub(frame_started.elapsed())
        {
            std::thread::sleep(remaining);
        }
    }
    write_video_runtime_sample(session, &mut runtime_jsonl)?;
    Ok(())
}

#[cfg(all(feature = "native-wgpu-renderer", feature = "video-renderer"))]
fn write_video_runtime_sample(
    session: &gilder::renderer::native_wgpu::NativeWgpuVideoSession,
    writer: &mut Option<&mut std::io::BufWriter<std::fs::File>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(writer) = writer.as_mut() else {
        return Ok(());
    };
    serde_json::to_writer(&mut **writer, &session.snapshot())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[cfg(all(feature = "native-wgpu-renderer", feature = "native-wgpu-gst-dmabuf"))]
fn run_gst_dmabuf_video_session(
    session: &mut gilder::renderer::native_wgpu::NativeWgpuGstDmabufSession,
    duration: std::time::Duration,
    target_fps: Option<u32>,
    runtime_interval: std::time::Duration,
    runtime_jsonl: Option<&mut std::io::BufWriter<std::fs::File>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    let mut last_runtime_sample = std::time::Instant::now();
    let mut runtime_jsonl = runtime_jsonl;
    let frame_interval = target_fps
        .filter(|fps| *fps > 0)
        .map(|fps| std::time::Duration::from_secs_f64(1.0 / f64::from(fps)));

    write_gst_dmabuf_video_runtime_sample(session, &mut runtime_jsonl)?;
    while started.elapsed() < duration && !session.is_closed() {
        let frame_started = std::time::Instant::now();
        session.tick()?;
        if last_runtime_sample.elapsed() >= runtime_interval {
            write_gst_dmabuf_video_runtime_sample(session, &mut runtime_jsonl)?;
            last_runtime_sample = std::time::Instant::now();
        }
        if let Some(interval) = frame_interval
            && let Some(remaining) = interval.checked_sub(frame_started.elapsed())
        {
            std::thread::sleep(remaining);
        }
    }
    write_gst_dmabuf_video_runtime_sample(session, &mut runtime_jsonl)?;
    Ok(())
}

#[cfg(all(feature = "native-wgpu-renderer", feature = "native-wgpu-gst-dmabuf"))]
fn write_gst_dmabuf_video_runtime_sample(
    session: &gilder::renderer::native_wgpu::NativeWgpuGstDmabufSession,
    writer: &mut Option<&mut std::io::BufWriter<std::fs::File>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(writer) = writer.as_mut() else {
        return Ok(());
    };
    serde_json::to_writer(&mut **writer, &session.snapshot())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[cfg(all(feature = "native-wgpu-renderer", feature = "native-wgpu-gpu-video"))]
fn run_gpu_video_session(
    session: &mut gilder::renderer::native_wgpu::NativeWgpuGpuVideoSession,
    duration: std::time::Duration,
    target_fps: Option<u32>,
    runtime_interval: std::time::Duration,
    runtime_jsonl: Option<&mut std::io::BufWriter<std::fs::File>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    let mut last_runtime_sample = std::time::Instant::now();
    let mut runtime_jsonl = runtime_jsonl;
    let frame_interval = target_fps
        .filter(|fps| *fps > 0)
        .map(|fps| std::time::Duration::from_secs_f64(1.0 / f64::from(fps)));

    write_gpu_video_runtime_sample(session, &mut runtime_jsonl)?;
    while started.elapsed() < duration && !session.is_closed() {
        let frame_started = std::time::Instant::now();
        session.tick()?;
        if last_runtime_sample.elapsed() >= runtime_interval {
            write_gpu_video_runtime_sample(session, &mut runtime_jsonl)?;
            last_runtime_sample = std::time::Instant::now();
        }
        if let Some(interval) = frame_interval
            && let Some(remaining) = interval.checked_sub(frame_started.elapsed())
        {
            std::thread::sleep(remaining);
        }
    }
    write_gpu_video_runtime_sample(session, &mut runtime_jsonl)?;
    Ok(())
}

#[cfg(all(feature = "native-wgpu-renderer", feature = "native-wgpu-gpu-video"))]
fn write_gpu_video_runtime_sample(
    session: &gilder::renderer::native_wgpu::NativeWgpuGpuVideoSession,
    writer: &mut Option<&mut std::io::BufWriter<std::fs::File>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(writer) = writer.as_mut() else {
        return Ok(());
    };
    serde_json::to_writer(&mut **writer, &session.snapshot())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[cfg(feature = "native-wgpu-renderer")]
fn run_session(
    session: &mut gilder::renderer::native_wgpu::NativeWgpuSession,
    duration: std::time::Duration,
    target_fps: Option<u32>,
    runtime_interval: std::time::Duration,
    runtime_jsonl: Option<&mut std::io::BufWriter<std::fs::File>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    let mut last_runtime_sample = std::time::Instant::now();
    let mut runtime_jsonl = runtime_jsonl;
    let frame_interval = target_fps
        .filter(|fps| *fps > 0)
        .map(|fps| std::time::Duration::from_secs_f64(1.0 / f64::from(fps)));

    write_runtime_sample(session, &mut runtime_jsonl)?;
    while started.elapsed() < duration && !session.is_closed() {
        let frame_started = std::time::Instant::now();
        session.tick()?;
        if last_runtime_sample.elapsed() >= runtime_interval {
            write_runtime_sample(session, &mut runtime_jsonl)?;
            last_runtime_sample = std::time::Instant::now();
        }
        if let Some(interval) = frame_interval
            && let Some(remaining) = interval.checked_sub(frame_started.elapsed())
        {
            std::thread::sleep(remaining);
        }
    }
    write_runtime_sample(session, &mut runtime_jsonl)?;
    Ok(())
}

#[cfg(feature = "native-wgpu-renderer")]
fn write_runtime_sample(
    session: &gilder::renderer::native_wgpu::NativeWgpuSession,
    writer: &mut Option<&mut std::io::BufWriter<std::fs::File>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(writer) = writer.as_mut() else {
        return Ok(());
    };
    serde_json::to_writer(&mut **writer, &session.snapshot())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[cfg(feature = "native-wgpu-renderer")]
fn parse_color(value: &str) -> Result<gilder::renderer::native_wgpu::NativeWgpuColor, String> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() != 6 {
            return Err("--color hex form must be #rrggbb".to_owned());
        }
        let red = u8::from_str_radix(&hex[0..2], 16).map_err(|_| "invalid red hex".to_owned())?;
        let green =
            u8::from_str_radix(&hex[2..4], 16).map_err(|_| "invalid green hex".to_owned())?;
        let blue = u8::from_str_radix(&hex[4..6], 16).map_err(|_| "invalid blue hex".to_owned())?;
        return Ok(rgb_u8(red, green, blue));
    }

    let channels = value
        .split(',')
        .map(str::trim)
        .map(|channel| channel.parse::<u8>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "--color comma form must be r,g,b with 0..255 channels".to_owned())?;
    if channels.len() != 3 {
        return Err("--color comma form must be r,g,b".to_owned());
    }
    Ok(rgb_u8(channels[0], channels[1], channels[2]))
}

#[cfg(feature = "native-wgpu-renderer")]
fn rgb_u8(red: u8, green: u8, blue: u8) -> gilder::renderer::native_wgpu::NativeWgpuColor {
    gilder::renderer::native_wgpu::NativeWgpuColor {
        red: f64::from(red) / 255.0,
        green: f64::from(green) / 255.0,
        blue: f64::from(blue) / 255.0,
        alpha: 1.0,
    }
}

#[cfg(feature = "native-wgpu-renderer")]
fn parse_fit_mode(value: &str) -> Result<gilder::core::FitMode, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "cover" => Ok(gilder::core::FitMode::Cover),
        "contain" => Ok(gilder::core::FitMode::Contain),
        "stretch" => Ok(gilder::core::FitMode::Stretch),
        "center" => Ok(gilder::core::FitMode::Center),
        other => Err(format!("unsupported fit mode: {other}").into()),
    }
}

#[cfg(feature = "native-wgpu-renderer")]
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

#[cfg(feature = "native-wgpu-renderer")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeWgpuCliVideoBackend {
    Auto,
    CpuUpload,
    GpuVideo,
    GstGpuVideo,
    GstDmabuf,
}

#[cfg(feature = "native-wgpu-renderer")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeWgpuCliGpuVideoInputMode {
    AnnexBFile,
    GstH264ByteStream,
}

#[cfg(feature = "native-wgpu-renderer")]
fn parse_video_backend(
    value: &str,
) -> Result<NativeWgpuCliVideoBackend, Box<dyn std::error::Error>> {
    match value {
        "auto" => Ok(NativeWgpuCliVideoBackend::Auto),
        "cpu-upload" | "cpu" | "appsink" => Ok(NativeWgpuCliVideoBackend::CpuUpload),
        "gpu-video" | "gpu" | "vulkan-video" => Ok(NativeWgpuCliVideoBackend::GpuVideo),
        "gst-gpu-video" | "gstreamer-gpu-video" | "gst-vulkan-video" => {
            Ok(NativeWgpuCliVideoBackend::GstGpuVideo)
        }
        "gst-dmabuf" | "dmabuf" | "gstreamer-dmabuf" => Ok(NativeWgpuCliVideoBackend::GstDmabuf),
        other => Err(format!(
            "unsupported video backend: {other}; expected auto, cpu-upload, gpu-video, gst-gpu-video, or gst-dmabuf"
        )
        .into()),
    }
}

#[cfg(feature = "native-wgpu-renderer")]
fn resolve_video_backend(
    backend: NativeWgpuCliVideoBackend,
    source: &std::path::Path,
) -> NativeWgpuCliVideoBackend {
    if !matches!(backend, NativeWgpuCliVideoBackend::Auto) {
        return backend;
    }
    if cfg!(feature = "native-wgpu-gpu-video") && is_annex_b_h264_source(source) {
        NativeWgpuCliVideoBackend::GpuVideo
    } else if cfg!(feature = "native-wgpu-gst-gpu-video") {
        NativeWgpuCliVideoBackend::GstGpuVideo
    } else {
        NativeWgpuCliVideoBackend::CpuUpload
    }
}

#[cfg(feature = "native-wgpu-renderer")]
fn is_annex_b_h264_source(source: &std::path::Path) -> bool {
    source
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(|extension| matches!(extension.to_ascii_lowercase().as_str(), "h264" | "264"))
        .unwrap_or(false)
}

#[cfg(feature = "native-wgpu-renderer")]
fn print_usage() {
    println!(
        "usage: gilder-native-wgpu [--source <path>] [--video-backend auto|cpu-upload|gpu-video|gst-gpu-video|gst-dmabuf] [--duration <seconds>] [--target-fps <fps>|--no-fps-limit] [--layer background|bottom|top|overlay] [--allow-foreground-layer] [--output-name <name>] [--color #rrggbb|r,g,b] [--fit cover|contain|stretch|center] [--decoder auto|hardware-preferred|hardware-required|software] [--loop|--no-loop] [--render-mode solid|pulse] [--animate-color] [--runtime-json <path>] [--runtime-jsonl <path>] [--runtime-interval-ms <ms>]"
    );
}
