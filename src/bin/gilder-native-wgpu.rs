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
    use gilder::renderer::native_wgpu::{NativeWgpuColor, NativeWgpuOptions, NativeWgpuSession};
    use std::time::Duration;

    let mut duration = Duration::from_secs(5);
    let mut target_fps = Some(60);
    let mut layer = NativeWaylandLayer::Bottom;
    let mut allow_foreground_layer = false;
    let mut output_name = None::<String>;
    let mut color = NativeWgpuColor::default();
    let mut runtime_json = None::<std::path::PathBuf>;

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
            "--runtime-json" => {
                runtime_json = Some(args.next().ok_or("--runtime-json requires a path")?.into());
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

    let mut session = NativeWgpuSession::connect(NativeWgpuOptions {
        namespace: "gilder-wallpaper-native-wgpu".to_owned(),
        layer,
        output_name,
        initial_color: color,
    })?;
    session.run_for(duration, target_fps)?;

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
fn print_usage() {
    println!(
        "usage: gilder-native-wgpu [--duration <seconds>] [--target-fps <fps>|--no-fps-limit] [--layer background|bottom|top|overlay] [--allow-foreground-layer] [--output-name <name>] [--color #rrggbb|r,g,b] [--runtime-json <path>]"
    );
}
