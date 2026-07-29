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

#[cfg(feature = "native-vulkan-renderer")]
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

#[cfg(feature = "native-vulkan-renderer")]
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

#[cfg(feature = "native-vulkan-renderer")]
#[allow(unsafe_code)]
fn apply_vulkan_device_cli_environment(
    selector: Option<&str>,
    preference: Option<&str>,
) -> Result<(), &'static str> {
    if selector.is_some_and(|value| value.trim().is_empty()) {
        return Err("--vulkan-device selector cannot be empty");
    }
    // The CLI has not started renderer, audio, or presentation threads yet.
    // Setting these process variables here makes one immutable selection policy
    // visible to every Vulkan route, including video decode and scene present.
    unsafe {
        if let Some(selector) = selector {
            std::env::set_var("GILDER_VULKAN_DEVICE", selector);
        }
        if let Some(preference) = preference {
            std::env::set_var("GILDER_VULKAN_DEVICE_PREFERENCE", preference);
        }
    }
    Ok(())
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
    SceneBackendPlan,
    RunClear,
    RunStatic,
    RunScene,
    RunVideo,
}

#[cfg(feature = "native-vulkan-renderer")]
fn print_usage() {
    println!(
        "Usage: gilder-native-vulkan [--json|--capabilities|--contract|--type-support|--probe-surface|--probe-video|--probe-vulkanalia|--probe-vulkanalia-swapchain|--probe-vulkanalia-video-present|--scene-backend-plan|--run-clear|--run-static|--run-scene|--run-video]\n\
\n\
Print native Vulkan spike capabilities and backend contract.\n\
--probe-surface creates a layer-shell Wayland surface and VK_KHR_wayland_surface, then exits.\n\
--probe-video enumerates Vulkan Video decode extensions and queue families, then exits.\n\
--probe-vulkanalia enumerates the vulkanalia Vulkan 1.4 physical-device/video/external-memory gates, then exits.\n\
--probe-vulkanalia-swapchain creates a Wayland VkSurfaceKHR, Vulkanalia device, swapchain and swapchain image list, then exits.\n\
--probe-vulkanalia-video-present creates one Vulkanalia device with video-decode and graphics/present queues plus a Wayland swapchain, then exits.\n\
--scene-backend-plan reads --source file.gscene and prints the native Vulkan scene storage/pipeline/executor plan, then exits.\n\
--playback-frames N sets the FFmpeg Vulkan HW present frame budget.\n\
--run-clear uses the Vulkanalia Wayland swapchain runtime, clears frames with CmdPipelineBarrier2/QueueSubmit2, presents, then prints runtime JSON.\n\
--run-static uses Vulkanalia sampled-image dynamic rendering for static wallpapers with cover|contain|stretch|tile|center fit and background clear.\n\
--run-scene reads --source file.gscene and runs the selected Vulkan scene present policy.\n\
--scene-pointer-position X,Y replays a normalized wallpaper-surface pointer position for deterministic scene diagnostics.\n\
--scene-property NAME=JSON overrides one exact, case-sensitive authored scene user property for --run-scene or --scene-backend-plan; repeat for distinct names.\n\
--surface-width/--surface-height override the automatic authored-scene extent (falling back to the Wayland buffer extent) and must be provided together.\n\
--gpu-timing enables top-of-pipe to bottom-of-pipe Vulkan timestamp queries for --run-scene diagnostics.\n\
--vulkan-device SELECTOR strictly selects index:N, name:TEXT, uuid:HEX, or pci:DOMAIN:BUS:DEVICE.FUNCTION for every Vulkan route.\n\
--vulkan-device-preference defaults to discrete; integrated and enumeration are explicit alternatives when no selector is set.\n\
--run-video selects the FFmpeg Vulkan HW decode mainline and requires AV_PIX_FMT_VULKAN/AVVkFrame before descriptor-heap present.\n\
Options: [--output-name NAME] [--layer background|bottom|top|overlay] [--parent-mapping-buffer|--no-parent-mapping-buffer] [--fractional-scale-rounding ceil|nearest|floor] [--wait-roundtrips N]\n\
         [--duration SECONDS] [--target-fps FPS|--no-fps-limit] [--color #rrggbb|r,g,b]\n\
         [--scene-pointer-position X,Y] [--scene-property NAME=JSON] [--surface-width PX --surface-height PX] [--gpu-timing]\n\
         [--vulkan-device SELECTOR] [--vulkan-device-preference discrete|integrated|enumeration]\n\
         [--source PATH] [--fit cover|contain|stretch|tile|center] [--background #rrggbb]\n\
         [--muted|--unmuted] [--audio-output plan|clock-only|auto] [--audio-clock-probe]\n\
         [--video-codec h264|h265|h265-main-10|av1|av1-main-10] [--playback-frames N]"
    );
}

#[cfg(all(test, feature = "native-vulkan-renderer"))]
mod tests {
    use super::*;

    #[test]
    fn native_scene_defaults_to_background_and_uncapped_present() {
        use gilder::renderer::native_vulkan::NativeVulkanOptions;
        use gilder::renderer::native_wayland::NativeWaylandLayer;

        let options = NativeVulkanOptions::default();
        assert_eq!(options.host.layer, NativeWaylandLayer::Background);
        assert_eq!(options.target_max_fps, None);
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
