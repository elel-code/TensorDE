#[cfg(feature = "native-vulkan-renderer")]
use gilder::renderer::native_vulkan::NativeVulkanClearColor;

#[cfg(feature = "native-vulkan-renderer")]
fn main() {
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

#[cfg(feature = "native-vulkan-renderer")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use gilder::renderer::StaticWallpaperPlan;
    use gilder::renderer::native_vulkan::{
        NativeVulkanOptions, NativeVulkanSurfaceProbeOptions, backend_contract, capabilities,
        probe_wayland_surface, run_clear, run_static_image, wallpaper_type_support_matrix,
    };
    use gilder::renderer::native_wayland::NativeWaylandLayer;
    use serde_json::json;
    use std::path::PathBuf;
    use std::time::Duration;

    let mut mode = NativeVulkanCliMode::All;
    let mut options = NativeVulkanOptions::default();
    let mut duration = Duration::from_secs(5);
    let mut source = None::<PathBuf>;
    let mut fit = gilder::core::FitMode::Cover;
    let mut background = None::<String>;
    let mut allow_foreground_layer = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--capabilities" => mode = NativeVulkanCliMode::Capabilities,
            "--contract" => mode = NativeVulkanCliMode::Contract,
            "--type-support" => mode = NativeVulkanCliMode::TypeSupport,
            "--probe-surface" => mode = NativeVulkanCliMode::ProbeSurface,
            "--run-clear" => mode = NativeVulkanCliMode::RunClear,
            "--run-static" => mode = NativeVulkanCliMode::RunStatic,
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
            }
            "--target-fps" => {
                options.target_max_fps =
                    args.next().map(|value| value.parse::<u32>()).transpose()?;
            }
            "--no-fps-limit" => options.target_max_fps = None,
            "--color" => {
                let value = args.next().ok_or("--color requires #rrggbb or r,g,b")?;
                options.clear_color = parse_color(&value)?;
            }
            "--source" => {
                source = Some(args.next().ok_or("--source requires a path")?.into());
            }
            "--fit" => {
                let value = args.next().ok_or("--fit requires a value")?;
                fit = parse_fit_mode(&value)?;
            }
            "--background" => {
                background = Some(args.next().ok_or("--background requires #rrggbb")?);
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
        NativeVulkanCliMode::RunClear => json!(run_clear(options, duration)?),
        NativeVulkanCliMode::RunStatic => {
            let source = source.ok_or("--run-static requires --source")?;
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
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
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
fn parse_fit_mode(value: &str) -> Result<gilder::core::FitMode, String> {
    match value {
        "cover" => Ok(gilder::core::FitMode::Cover),
        "contain" => Ok(gilder::core::FitMode::Contain),
        "stretch" => Ok(gilder::core::FitMode::Stretch),
        "tile" => Ok(gilder::core::FitMode::Tile),
        "center" => Ok(gilder::core::FitMode::Center),
        other => Err(format!("unsupported fit mode: {other}")),
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
    RunClear,
    RunStatic,
}

#[cfg(feature = "native-vulkan-renderer")]
fn print_usage() {
    println!(
        "Usage: gilder-native-vulkan [--json|--capabilities|--contract|--type-support|--probe-surface|--run-clear|--run-static]\n\
\n\
Print native Vulkan spike capabilities and backend contract.\n\
--probe-surface creates a layer-shell Wayland surface and VK_KHR_wayland_surface, then exits.\n\
--run-clear creates a Vulkan device/swapchain, clears frames, presents, then prints runtime JSON.\n\
--run-static decodes --source, fits it to the swapchain, copies it through Vulkan, presents, then prints runtime JSON.\n\
Options: [--output-name NAME] [--layer background|bottom|top|overlay] [--wait-roundtrips N]\n\
         [--duration SECONDS] [--target-fps FPS|--no-fps-limit] [--color #rrggbb|r,g,b]\n\
         [--source PATH] [--fit cover|contain|stretch|tile|center] [--background #rrggbb]"
    );
}
