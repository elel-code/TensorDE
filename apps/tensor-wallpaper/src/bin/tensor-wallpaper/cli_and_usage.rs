#[cfg(feature = "rendering-device")]
fn write_json_report<T: serde::Serialize>(report: &T) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write as _;
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer_pretty(&mut stdout, report)?;
    stdout.write_all(b"\n")?;
    Ok(())
}

#[cfg(feature = "rendering-device")]
fn parse_color(value: &str) -> Result<RenderingDeviceClearColor, Box<dyn std::error::Error>> {
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() != 6 {
            return Err("hex color must be #rrggbb".into());
        }
        let r = u8::from_str_radix(&hex[0..2], 16)? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16)? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16)? as f32 / 255.0;
        return Ok(RenderingDeviceClearColor { r, g, b, a: 1.0 });
    }

    let parts = value
        .split(',')
        .map(|part| part.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()?;
    match parts.as_slice() {
        [r, g, b] => Ok(RenderingDeviceClearColor {
            r: *r,
            g: *g,
            b: *b,
            a: 1.0,
        }),
        [r, g, b, a] => Ok(RenderingDeviceClearColor {
            r: *r,
            g: *g,
            b: *b,
            a: *a,
        }),
        _ => Err("color must be #rrggbb, r,g,b, or r,g,b,a".into()),
    }
}

#[cfg(feature = "rendering-device")]
fn parse_scene_pointer_position(value: Option<String>) -> Result<[f64; 2], &'static str> {
    const ERROR: &str = "--scene-pointer-position requires finite normalized X,Y in [0,1]";
    let value = value.ok_or(ERROR)?;
    let values = value
        .split(',')
        .map(str::trim)
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ERROR)?;
    match values.as_slice() {
        [x, y]
            if x.is_finite()
                && y.is_finite()
                && (0.0..=1.0).contains(x)
                && (0.0..=1.0).contains(y) =>
        {
            Ok([*x, *y])
        }
        _ => Err(ERROR),
    }
}

#[cfg(all(feature = "rendering-device", feature = "video"))]
fn parse_scene_video_source(
    argument: String,
) -> Result<tensor_wallpaper::renderer::rendering_device::RenderingDeviceSceneVideoSource, String> {
    let mut fields = argument.splitn(3, ':');
    let media_instance = fields
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "--scene-video requires MEDIA_INSTANCE:CODEC:PATH".to_owned())?
        .parse::<u32>()
        .map_err(|_| "--scene-video MEDIA_INSTANCE must be a u32".to_owned())?;
    let codec = fields
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "--scene-video requires MEDIA_INSTANCE:CODEC:PATH".to_owned())?
        .parse::<tensor_wallpaper::renderer::rendering_device::RenderingDeviceVideoSessionCodec>()?;
    let source = fields
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "--scene-video PATH cannot be empty".to_owned())?;
    Ok(tensor_wallpaper::renderer::rendering_device::RenderingDeviceSceneVideoSource {
        media_instance,
        source: source.into(),
        codec,
        loop_playback: true,
    })
}

#[cfg(feature = "rendering-device")]
fn insert_scene_property_override(
    overrides: &mut serde_json::Map<String, serde_json::Value>,
    argument: Option<String>,
) -> Result<(), String> {
    let argument = argument.ok_or_else(|| {
        "--scene-property requires an exact NAME=JSON argument".to_owned()
    })?;
    let (name, raw_value) = argument.split_once('=').ok_or_else(|| {
        "--scene-property requires an exact NAME=JSON argument".to_owned()
    })?;
    if name.is_empty() {
        return Err("--scene-property NAME cannot be empty".to_owned());
    }
    if overrides.contains_key(name) {
        return Err(format!(
            "duplicate --scene-property for exact property name {name:?}"
        ));
    }
    let value = serde_json::from_str(raw_value).map_err(|error| {
        format!("--scene-property {name:?} has invalid JSON value: {error}")
    })?;
    overrides.insert(name.to_owned(), value);
    Ok(())
}

#[cfg(feature = "rendering-device")]
fn parse_scene_surface_extent(
    width: Option<u32>,
    height: Option<u32>,
) -> Result<Option<(u32, u32)>, &'static str> {
    match (width, height) {
        (None, None) => Ok(None),
        (Some(width), Some(height)) if width > 0 && height > 0 => Ok(Some((width, height))),
        _ => Err("--surface-width and --surface-height must be positive and used together"),
    }
}

#[cfg(feature = "rendering-device")]
#[allow(unsafe_code)]
fn apply_render_device_selector_cli_environment(
    selector: Option<&str>,
    preference: Option<&str>,
) -> Result<(), &'static str> {
    if selector.is_some_and(|value| value.trim().is_empty()) {
        return Err("--render-device selector cannot be empty");
    }
    // The CLI has not started renderer, audio, or presentation threads yet.
    // Setting these process variables here makes one immutable selection policy
    // visible to every Vulkan route, including video decode and scene present.
    unsafe {
        if let Some(selector) = selector {
            std::env::set_var("TENSOR_WALLPAPER_RENDER_DEVICE", selector);
        }
        if let Some(preference) = preference {
            std::env::set_var("TENSOR_WALLPAPER_RENDER_DEVICE_PREFERENCE", preference);
        }
    }
    Ok(())
}

#[cfg(feature = "rendering-device")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderingDeviceCliMode {
    All,
    Capabilities,
    Contract,
    TypeSupport,
    SceneExecutionPlan,
    RunScene,
    RunVideo,
}

#[cfg(feature = "rendering-device")]
fn print_usage() {
    println!(
        "Usage: tensor-wallpaper [--json|--capabilities|--contract|--wallpaper-kind-support|--scene-execution-plan|--run-scene|--run-video]\n\
\n\
Print renderer capabilities and the wallpaper presentation contract.\n\
--scene-execution-plan reads --source file.gscene and prints the typed scene storage/resource/pipeline/command execution plan, then exits.\n\
--playback-frames N bounds the deterministic renderer-owned decoded-frame presentation run.\n\
--run-scene reads --source file.gscene and runs until the surface closes; --duration sets an explicit second limit.\n\
--scene-video MEDIA_INSTANCE:CODEC:PATH supplies one exact external VideoFrame source to --run-scene; repeat for distinct media instances.\n\
--scene-pointer-position X,Y replays a normalized wallpaper-surface pointer position for deterministic scene diagnostics.\n\
--scene-semantic-diagnostics appends final retained script deltas, resolved object and puppet-bone state, and draw activation to --run-scene JSON.\n\
--scene-property NAME=JSON overrides one exact, case-sensitive authored scene user property for --run-scene or --scene-execution-plan; repeat for distinct names.\n\
--surface-width/--surface-height provide a deterministic physical presentation extent for offline plans or captures and must be provided together; normal Wayland scene runs use the live configured buffer extent.\n\
--gpu-timing enables top-of-pipe to bottom-of-pipe Vulkan timestamp queries for --run-scene diagnostics.\n\
--render-device SELECTOR strictly selects index:N, name:TEXT, uuid:HEX, or pci:DOMAIN:BUS:DEVICE.FUNCTION for every Vulkan route.\n\
--render-device-preference defaults to discrete; integrated and enumeration are explicit alternatives when no selector is set.\n\
--run-video uses renderer-owned FFmpeg Vulkan decode with opaque plane leases, descriptor-heap sampling, and a retained presentation transaction.\n\
Options: [--output-name NAME] [--layer background|bottom|top|overlay] [--fractional-scale-rounding ceil|nearest|floor] [--wait-roundtrips N]\n\
         [--duration SECONDS] [--target-fps FPS|--no-fps-limit] [--color #rrggbb|r,g,b]\n\
         [--scene-pointer-position X,Y] [--scene-property NAME=JSON] [--surface-width PX --surface-height PX] [--gpu-timing]\n\
         [--scene-video MEDIA_INSTANCE:CODEC:PATH]...\n\
         [--scene-semantic-diagnostics]\n\
         [--render-device SELECTOR] [--render-device-preference discrete|integrated|enumeration]\n\
         [--source PATH]\n\
         [--video-codec h264|h265|h265-main-10|av1|av1-main-10] [--playback-frames N]"
    );
}

#[cfg(all(test, feature = "rendering-device"))]
mod tests {
    use super::*;

    #[test]
    fn scene_defaults_to_background_uncapped_and_surface_lifetime() {
        use tensor_wallpaper::renderer::rendering_device::RenderingDeviceOptions;
        use tensor_wallpaper::renderer::wayland::WaylandLayer;

        let options = RenderingDeviceOptions::default();
        assert_eq!(options.host.layer, WaylandLayer::Background);
        assert_eq!(options.target_max_fps, None);
        assert_eq!(DEFAULT_SCENE_RUN_DURATION, None);
    }

    #[test]
    fn scene_pointer_position_accepts_finite_normalized_surface_coordinates() {
        assert_eq!(
            parse_scene_pointer_position(Some("0, 0.5".to_owned())),
            Ok([0.0, 0.5])
        );
        assert_eq!(
            parse_scene_pointer_position(Some("1,1".to_owned())),
            Ok([1.0, 1.0])
        );
        assert!(parse_scene_pointer_position(None).is_err());
        assert!(parse_scene_pointer_position(Some("0.5".to_owned())).is_err());
        assert!(parse_scene_pointer_position(Some("-0.1,0.5".to_owned())).is_err());
        assert!(parse_scene_pointer_position(Some("0.5,1.1".to_owned())).is_err());
        assert!(parse_scene_pointer_position(Some("NaN,0.5".to_owned())).is_err());
        assert!(parse_scene_pointer_position(Some("inf,0.5".to_owned())).is_err());
    }

    #[test]
    fn scene_property_requires_exact_name_json_and_rejects_duplicate_keys() {
        let mut overrides = serde_json::Map::new();
        insert_scene_property_override(&mut overrides, Some("jia=false".to_owned())).unwrap();
        assert_eq!(overrides["jia"], serde_json::Value::Bool(false));
        assert!(
            insert_scene_property_override(&mut overrides, Some("jia=true".to_owned())).is_err()
        );
        assert!(insert_scene_property_override(&mut overrides, Some("jia".to_owned())).is_err());
        assert!(
            insert_scene_property_override(&mut serde_json::Map::new(), Some("=false".to_owned()))
                .is_err()
        );
        let mut exact = serde_json::Map::new();
        insert_scene_property_override(&mut exact, Some(" Jia =false".to_owned())).unwrap();
        assert!(exact.contains_key(" Jia "));
    }

    #[cfg(feature = "video")]
    #[test]
    fn scene_video_cli_preserves_exact_media_codec_and_path() {
        let source =
            parse_scene_video_source("7:h265-main-10:/tmp/video:segment.mkv".to_owned()).unwrap();
        assert_eq!(source.media_instance, 7);
        assert_eq!(
            source.codec,
            tensor_wallpaper::renderer::rendering_device::RenderingDeviceVideoSessionCodec::H265Main10
        );
        assert_eq!(source.source, PathBuf::from("/tmp/video:segment.mkv"));
        assert!(source.loop_playback);
    }

    #[test]
    fn scene_surface_extent_defaults_to_automatic_and_allows_a_paired_override() {
        assert_eq!(parse_scene_surface_extent(None, None), Ok(None));
        assert_eq!(
            parse_scene_surface_extent(Some(2561), Some(1601)),
            Ok(Some((2561, 1601)))
        );
        assert_eq!(
            parse_scene_surface_extent(Some(2561), None),
            Err("--surface-width and --surface-height must be positive and used together")
        );
        assert_eq!(
            parse_scene_surface_extent(Some(0), Some(1601)),
            Err("--surface-width and --surface-height must be positive and used together")
        );
    }
}
