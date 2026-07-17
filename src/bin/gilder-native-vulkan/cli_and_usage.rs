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

fn parse_capture_frame_count(value: Option<String>) -> Result<u64, &'static str> {
    value
        .ok_or("--capture-frame-count requires a positive count")?
        .parse::<u64>()
        .ok()
        .filter(|count| *count > 0)
        .ok_or("--capture-frame-count requires a positive count")
}

fn parse_capture_frame_step(value: Option<String>) -> Result<u64, &'static str> {
    value
        .ok_or("--capture-frame-step requires a positive step")?
        .parse::<u64>()
        .ok()
        .filter(|step| *step > 0)
        .ok_or("--capture-frame-step requires a positive step")
}

fn parse_capture_frame_downscale(value: Option<String>) -> Result<u32, &'static str> {
    value
        .ok_or("--capture-frame-downscale requires a positive divisor")?
        .parse::<u32>()
        .ok()
        .filter(|downscale| *downscale > 0)
        .ok_or("--capture-frame-downscale requires a positive divisor")
}

fn parse_capture_frame_reference(value: Option<String>) -> Result<PathBuf, &'static str> {
    value
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or("--capture-frame-reference requires a path")
}

fn parse_capture_frame_time_step(value: Option<String>) -> Result<f32, &'static str> {
    value
        .ok_or("--capture-frame-time-step requires positive seconds")?
        .parse::<f32>()
        .ok()
        .filter(|step| step.is_finite() && *step > 0.0)
        .ok_or("--capture-frame-time-step requires positive seconds")
}

fn parse_capture_frame_region(
    value: Option<String>,
) -> Result<(u32, u32, u32, u32), &'static str> {
    let value = value.ok_or("--capture-frame-region requires X,Y,WIDTH,HEIGHT")?;
    let values = value
        .split(',')
        .map(str::trim)
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "--capture-frame-region requires X,Y,WIDTH,HEIGHT")?;
    match values.as_slice() {
        [x, y, width, height] if *width > 0 && *height > 0 => {
            Ok((*x, *y, *width, *height))
        }
        _ => Err("--capture-frame-region requires X,Y,WIDTH,HEIGHT"),
    }
}

#[cfg(feature = "native-vulkan-renderer")]
fn parse_capture_scene_graph(value: Option<String>) -> Result<u32, &'static str> {
    value
        .ok_or("--capture-scene-graph requires a graph index")?
        .parse::<u32>()
        .map_err(|_| "--capture-scene-graph requires a graph index")
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
--run-scene reads --source file.gscene and runs the selected Vulkan scene present policy.\n\
--capture-frame PATH writes a completed --run-scene frame directly from the Vulkan swapchain as an RGBA8 PNG.\n\
--capture-frame-number N selects the 1-based submitted frame captured by --capture-frame; the default is 1.\n\
--capture-frame-count N captures N submitted frames; sequence files append the zero-padded frame number to PATH.\n\
--capture-frame-step N samples every Nth submitted frame in a sequence; the default is 1.\n\
--capture-frame-downscale N keeps full-resolution rendering but stores every Nth readback pixel in each axis.\n\
--capture-frame-region X,Y,WIDTH,HEIGHT copies only that swapchain region before optional CPU downscale.\n\
--capture-frame-reference PATH compares the captured sequence with matching deterministic reference PNGs in-process.\n\
--capture-frame-time-step SECONDS advances scene time deterministically for every submitted capture run frame.\n\
--capture-scene-graph N isolates one RenderingDevice graph in a captured frame; it is rejected without --capture-frame.\n\
--scene-pointer-position X,Y replays a normalized wallpaper-surface pointer position for deterministic scene diagnostics.\n\
--surface-width/--surface-height override the automatic authored-scene extent (falling back to the Wayland buffer extent) and must be provided together.\n\
--gpu-timing enables top-of-pipe to bottom-of-pipe Vulkan timestamp queries for --run-scene diagnostics.\n\
--vulkan-device SELECTOR strictly selects index:N, name:TEXT, uuid:HEX, or pci:DOMAIN:BUS:DEVICE.FUNCTION for every Vulkan route.\n\
--vulkan-device-preference defaults to discrete; integrated and enumeration are explicit alternatives when no selector is set.\n\
--run-video selects the FFmpeg Vulkan HW decode mainline and requires AV_PIX_FMT_VULKAN/AVVkFrame before descriptor-heap present.\n\
--run-vulkanalia-ready-prefix-video runs the legacy Vulkanalia Vulkan Video compatibility route and prints runtime JSON.\n\
Options: [--output-name NAME] [--layer background|bottom|top|overlay] [--parent-mapping-buffer|--no-parent-mapping-buffer] [--fractional-scale-rounding ceil|nearest|floor] [--wait-roundtrips N]\n\
         [--duration SECONDS] [--target-fps FPS|--no-fps-limit] [--color #rrggbb|r,g,b] [--capture-frame PATH] [--capture-frame-number N] [--capture-frame-count N] [--capture-frame-step N] [--capture-frame-downscale N] [--capture-frame-region X,Y,WIDTH,HEIGHT] [--capture-frame-reference PATH] [--capture-frame-time-step SECONDS] [--capture-scene-graph N]\n\
         [--scene-pointer-position X,Y] [--surface-width PX --surface-height PX] [--gpu-timing]\n\
         [--vulkan-device SELECTOR] [--vulkan-device-preference discrete|integrated|enumeration]\n\
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

    #[test]
    fn capture_frame_count_is_positive() {
        assert_eq!(parse_capture_frame_count(Some("120".to_owned())), Ok(120));
        assert_eq!(
            parse_capture_frame_count(Some("0".to_owned())),
            Err("--capture-frame-count requires a positive count")
        );
    }

    #[test]
    fn capture_frame_sequence_sampling_values_are_positive() {
        assert_eq!(parse_capture_frame_step(Some("5".to_owned())), Ok(5));
        assert_eq!(
            parse_capture_frame_step(Some("0".to_owned())),
            Err("--capture-frame-step requires a positive step")
        );
        assert_eq!(parse_capture_frame_downscale(Some("3".to_owned())), Ok(3));
        assert_eq!(
            parse_capture_frame_downscale(Some("0".to_owned())),
            Err("--capture-frame-downscale requires a positive divisor")
        );
        assert_eq!(
            parse_capture_frame_time_step(Some("0.016666667".to_owned())),
            Ok(0.016666668)
        );
        assert!(parse_capture_frame_time_step(Some("0".to_owned())).is_err());
        assert!(parse_capture_frame_time_step(Some("nan".to_owned())).is_err());
    }

    #[test]
    fn capture_frame_reference_requires_a_path() {
        assert_eq!(
            parse_capture_frame_reference(Some("/tmp/authored.png".to_owned())),
            Ok(PathBuf::from("/tmp/authored.png"))
        );
        assert!(parse_capture_frame_reference(None).is_err());
    }

    #[test]
    fn capture_frame_region_requires_four_coordinates_and_positive_extent() {
        assert_eq!(
            parse_capture_frame_region(Some("12,34,640,360".to_owned())),
            Ok((12, 34, 640, 360))
        );
        assert!(parse_capture_frame_region(Some("12,34,0,360".to_owned())).is_err());
        assert!(parse_capture_frame_region(Some("12,34,640".to_owned())).is_err());
    }

    #[test]
    fn capture_scene_graph_accepts_zero_based_graph_index() {
        assert_eq!(parse_capture_scene_graph(Some("0".to_owned())), Ok(0));
        assert_eq!(
            parse_capture_scene_graph(Some("graph".to_owned())),
            Err("--capture-scene-graph requires a graph index")
        );
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
