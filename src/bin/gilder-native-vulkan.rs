#[cfg(all(
    feature = "native-vulkan-renderer",
    feature = "native-vulkan-gst-video"
))]
use gilder::renderer::native_vulkan::NativeVulkanAudioOutputPolicy;
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
    #[cfg(feature = "native-vulkan-gst-video")]
    use gilder::renderer::native_vulkan::{
        NativeVulkanAudioClockProbeOptions, NativeVulkanH264VideoInputMode,
        NativeVulkanH265VideoInputMode, NativeVulkanVideoSessionCodec,
        native_vulkan_extract_av1_ready_prefix_for_vulkanalia,
        native_vulkan_extract_av1_sequence_header_for_vulkanalia,
        native_vulkan_extract_h264_parameter_sets_for_vulkanalia,
        native_vulkan_extract_h264_ready_prefix_for_vulkanalia,
        native_vulkan_extract_h265_parameter_sets_for_vulkanalia,
        native_vulkan_extract_h265_ready_prefix_for_vulkanalia, probe_native_vulkan_audio_clock,
        run_av1_ready_prefix_video, run_h264_ready_prefix_video, run_h265_first_frame_video,
        run_h265_ready_prefix_video, run_vulkanalia_ready_prefix_video,
    };
    use gilder::renderer::native_vulkan::{
        NativeVulkanOptions, NativeVulkanSurfaceProbeOptions, NativeVulkanVideoSessionSmokeOptions,
        backend_contract, capabilities, probe_vulkan_video_decode, probe_vulkan_video_session,
        probe_wayland_surface, run_clear, run_legacy_static_image, run_static_image, run_video,
        wallpaper_type_support_matrix,
    };
    use gilder::renderer::native_vulkan::{
        NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions,
        NativeVulkanVulkanaliaSceneLiteSolidQuadPresentOptions,
        NativeVulkanVulkanaliaSurfaceSwapchainProbeOptions,
        NativeVulkanVulkanaliaVideoPresentDeviceProbeOptions,
        NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
        NativeVulkanVulkanaliaVideoSessionBindSmokeOptions, probe_native_vulkan_vulkanalia_devices,
        probe_native_vulkan_vulkanalia_surface_swapchain,
        probe_native_vulkan_vulkanalia_video_present_device,
        probe_native_vulkan_vulkanalia_video_present_session,
        probe_native_vulkan_vulkanalia_video_session_bind,
        run_native_vulkan_vulkanalia_scene_lite_sampled_image_present,
        run_native_vulkan_vulkanalia_scene_lite_solid_quad_present,
    };
    use gilder::renderer::native_wayland::NativeWaylandLayer;
    use gilder::renderer::{StaticWallpaperPlan, VideoWallpaperPlan};
    use serde_json::json;
    use std::path::PathBuf;
    use std::time::Duration;

    let mut mode = NativeVulkanCliMode::All;
    let mut options = NativeVulkanOptions::default();
    let mut duration = Duration::from_secs(5);
    let mut audio_probe_duration = Duration::from_secs(10);
    #[cfg(feature = "native-vulkan-gst-video")]
    let mut audio_output_policy = NativeVulkanAudioOutputPolicy::Plan;
    let mut audio_clock_probe_with_video = false;
    let mut source = None::<PathBuf>;
    let mut poster = None::<PathBuf>;
    let mut fit = gilder::core::FitMode::Cover;
    let mut background = None::<String>;
    let mut loop_playback = true;
    let mut muted = true;
    let mut decoder_policy = gilder::config::VideoDecoderPolicy::HardwarePreferred;
    let mut start_offset_ms = 0u64;
    let mut allow_foreground_layer = false;
    let mut video_session_options = NativeVulkanVideoSessionSmokeOptions::default();
    let mut vulkanalia_create_empty_session_parameters = false;
    let mut vulkanalia_create_session_parameters = false;
    let mut ready_prefix_playback_frames = 0u32;
    let mut av1_ready_prefix_frames = 0u32;
    #[cfg(feature = "native-vulkan-gst-video")]
    let mut h264_video_input = NativeVulkanH264VideoInputMode::StreamingQueue;
    #[cfg(feature = "native-vulkan-gst-video")]
    let mut h265_video_input = NativeVulkanH265VideoInputMode::StreamingQueue;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--capabilities" => mode = NativeVulkanCliMode::Capabilities,
            "--contract" => mode = NativeVulkanCliMode::Contract,
            "--type-support" => mode = NativeVulkanCliMode::TypeSupport,
            "--probe-surface" => mode = NativeVulkanCliMode::ProbeSurface,
            "--probe-video" => mode = NativeVulkanCliMode::ProbeVideo,
            "--probe-vulkanalia" => mode = NativeVulkanCliMode::ProbeVulkanalia,
            "--probe-vulkanalia-swapchain" | "--probe-vulkanalia-surface" => {
                mode = NativeVulkanCliMode::ProbeVulkanaliaSwapchain
            }
            "--probe-vulkanalia-video-session" => {
                mode = NativeVulkanCliMode::ProbeVulkanaliaVideoSession
            }
            "--probe-vulkanalia-video-present" => {
                mode = NativeVulkanCliMode::ProbeVulkanaliaVideoPresent
            }
            "--probe-vulkanalia-video-present-session" => {
                mode = NativeVulkanCliMode::ProbeVulkanaliaVideoPresentSession
            }
            "--probe-video-session" => mode = NativeVulkanCliMode::ProbeVideoSession,
            "--probe-audio-clock" => mode = NativeVulkanCliMode::ProbeAudioClock,
            "--audio-clock-probe" => audio_clock_probe_with_video = true,
            "--audio-output" => {
                let value = args
                    .next()
                    .ok_or("--audio-output requires plan, clock-only, or auto")?;
                #[cfg(feature = "native-vulkan-gst-video")]
                {
                    audio_output_policy = NativeVulkanAudioOutputPolicy::parse_cli(&value)?;
                }
                #[cfg(not(feature = "native-vulkan-gst-video"))]
                {
                    let _ = value;
                    return Err("--audio-output requires native-vulkan-gst-video feature".into());
                }
            }
            "--run-h265-first-frame-video" => mode = NativeVulkanCliMode::RunH265FirstFrameVideo,
            "--run-h264-ready-prefix-video" => mode = NativeVulkanCliMode::RunH264ReadyPrefixVideo,
            "--run-h265-ready-prefix-video" => mode = NativeVulkanCliMode::RunH265ReadyPrefixVideo,
            "--run-av1-ready-prefix-video" => mode = NativeVulkanCliMode::RunAv1ReadyPrefixVideo,
            "--run-vulkanalia-ready-prefix-video" => {
                mode = NativeVulkanCliMode::RunVulkanaliaReadyPrefixVideo
            }
            "--allocate-video-images" => video_session_options.allocate_video_images = true,
            "--allocate-bitstream-buffer" => video_session_options.allocate_bitstream_buffer = true,
            "--create-empty-session-parameters" => {
                vulkanalia_create_empty_session_parameters = true
            }
            "--create-session-parameters" => vulkanalia_create_session_parameters = true,
            "--extract-bitstream" => {
                video_session_options.extract_bitstream = true;
                video_session_options.allocate_bitstream_buffer = true;
            }
            "--decode-first-frame" => {
                video_session_options.decode_first_frame = true;
                video_session_options.extract_bitstream = true;
                video_session_options.allocate_bitstream_buffer = true;
                video_session_options.allocate_video_images = true;
            }
            "--sample-decoded-first-frame" => {
                video_session_options.sample_decoded_first_frame = true;
                video_session_options.decode_first_frame = true;
                video_session_options.extract_bitstream = true;
                video_session_options.allocate_bitstream_buffer = true;
                video_session_options.allocate_video_images = true;
            }
            "--decode-h264-idr-prefix" => {
                let count = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--decode-h264-idr-prefix requires a count")?;
                video_session_options.decode_h264_idr_prefix_frames = count;
                video_session_options.h264_required_idr_prefix_access_units = count;
                video_session_options.extract_bitstream = true;
                video_session_options.allocate_bitstream_buffer = true;
                video_session_options.allocate_video_images = true;
            }
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
                av1_ready_prefix_frames = count;
                video_session_options.bitstream_extract_max_samples = video_session_options
                    .bitstream_extract_max_samples
                    .max(count);
                video_session_options.extract_bitstream = true;
                video_session_options.allocate_bitstream_buffer = true;
                video_session_options.allocate_video_images = true;
            }
            "--sample-h265-ready-prefix" => {
                video_session_options.sample_h265_ready_prefix_output = true;
                video_session_options.extract_bitstream = true;
                video_session_options.allocate_bitstream_buffer = true;
                video_session_options.allocate_video_images = true;
            }
            "--sample-h265-ready-prefix-sequence" => {
                video_session_options.sample_h265_ready_prefix_output = true;
                video_session_options.sample_h265_ready_prefix_sequence = true;
                video_session_options.extract_bitstream = true;
                video_session_options.allocate_bitstream_buffer = true;
                video_session_options.allocate_video_images = true;
            }
            "--run-clear" | "--run-vulkanalia-clear" => mode = NativeVulkanCliMode::RunClear,
            "--run-vulkanalia-scene-lite-solid-quad" => {
                mode = NativeVulkanCliMode::RunVulkanaliaSceneLiteSolidQuad
            }
            "--run-vulkanalia-scene-lite-sampled-image" => {
                mode = NativeVulkanCliMode::RunVulkanaliaSceneLiteSampledImage
            }
            "--run-vulkanalia-static" => mode = NativeVulkanCliMode::RunStatic,
            "--run-legacy-static" => mode = NativeVulkanCliMode::RunLegacyStatic,
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
                audio_probe_duration = duration;
            }
            "--audio-probe-duration" => {
                audio_probe_duration = args
                    .next()
                    .map(|value| value.parse::<u64>())
                    .transpose()?
                    .map(Duration::from_secs)
                    .ok_or("--audio-probe-duration requires seconds")?;
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
            "--poster" => {
                poster = Some(args.next().ok_or("--poster requires a path")?.into());
            }
            "--fit" => {
                let value = args.next().ok_or("--fit requires a value")?;
                fit = parse_fit_mode(&value)?;
            }
            "--background" => {
                background = Some(args.next().ok_or("--background requires #rrggbb")?);
            }
            "--loop" => loop_playback = true,
            "--no-loop" => loop_playback = false,
            "--muted" => muted = true,
            "--unmuted" => muted = false,
            "--decoder" => {
                let value = args.next().ok_or("--decoder requires a value")?;
                decoder_policy = parse_decoder_policy(&value)?;
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
            }
            "--height" => {
                video_session_options.height = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--height requires pixels")?;
            }
            "--bitstream-buffer-size" => {
                video_session_options.bitstream_buffer_size = args
                    .next()
                    .map(|value| value.parse::<u64>())
                    .transpose()?
                    .ok_or("--bitstream-buffer-size requires bytes")?;
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
            "--require-h264-idr-prefix" => {
                video_session_options.h264_required_idr_prefix_access_units = args
                    .next()
                    .map(|value| value.parse::<u32>())
                    .transpose()?
                    .ok_or("--require-h264-idr-prefix requires a count")?;
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
            "--h264-input" | "--h264-input-mode" => {
                let value = args.next().ok_or("--h264-input requires a value")?;
                #[cfg(feature = "native-vulkan-gst-video")]
                {
                    h264_video_input = parse_h264_video_input_mode(&value)?;
                }
                #[cfg(not(feature = "native-vulkan-gst-video"))]
                {
                    let _ = value;
                    return Err("--h264-input requires native-vulkan-gst-video feature".into());
                }
            }
            "--h265-input" | "--h265-input-mode" => {
                let value = args.next().ok_or("--h265-input requires a value")?;
                #[cfg(feature = "native-vulkan-gst-video")]
                {
                    h265_video_input = parse_h265_video_input_mode(&value)?;
                }
                #[cfg(not(feature = "native-vulkan-gst-video"))]
                {
                    let _ = value;
                    return Err("--h265-input requires native-vulkan-gst-video feature".into());
                }
            }
            "--start-offset-ms" => {
                start_offset_ms = args
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
                }
            )?)
        }
        NativeVulkanCliMode::ProbeVulkanaliaVideoSession => {
            let decode_h264_ready_prefix =
                video_session_options.decode_h264_ready_prefix_frames > 0;
            let decode_h265_ready_prefix =
                video_session_options.decode_h265_ready_prefix_frames > 0;
            let decode_av1_ready_prefix = av1_ready_prefix_frames > 0;
            let create_parameters = vulkanalia_create_session_parameters
                || decode_h264_ready_prefix
                || decode_h265_ready_prefix
                || decode_av1_ready_prefix;
            let (
                h264_parameter_sets,
                h265_parameter_sets,
                av1_sequence_header,
                h264_ready_prefix_decode,
                h265_ready_prefix_decode,
                av1_ready_prefix_decode,
            ) = if create_parameters {
                let source = source.clone().ok_or(
                    "--create-session-parameters/--decode-*-ready-prefix requires --source",
                )?;
                if !source.is_file() {
                    return Err(
                        format!("bitstream source does not exist: {}", source.display()).into(),
                    );
                }
                #[cfg(feature = "native-vulkan-gst-video")]
                {
                    match video_session_options.codec {
                        NativeVulkanVideoSessionCodec::H264High8 => {
                            if decode_h264_ready_prefix {
                                let ready_prefix =
                                    native_vulkan_extract_h264_ready_prefix_for_vulkanalia(
                                        source,
                                        video_session_options.bitstream_extract_max_samples,
                                        video_session_options.decode_h264_ready_prefix_frames,
                                    )?;
                                (
                                    Some(ready_prefix.parameter_sets.clone()),
                                    None,
                                    None,
                                    Some(ready_prefix),
                                    None,
                                    None,
                                )
                            } else {
                                let parameter_sets =
                                    native_vulkan_extract_h264_parameter_sets_for_vulkanalia(
                                        source,
                                        video_session_options.bitstream_extract_max_samples,
                                    )?;
                                (Some(parameter_sets), None, None, None, None, None)
                            }
                        }
                        NativeVulkanVideoSessionCodec::H265Main8
                        | NativeVulkanVideoSessionCodec::H265Main10 => {
                            if decode_h265_ready_prefix {
                                let ready_prefix =
                                    native_vulkan_extract_h265_ready_prefix_for_vulkanalia(
                                        source,
                                        video_session_options.codec,
                                        video_session_options.bitstream_extract_max_samples,
                                        video_session_options.decode_h265_ready_prefix_frames,
                                    )?;
                                (
                                    None,
                                    Some(ready_prefix.parameter_sets.clone()),
                                    None,
                                    None,
                                    Some(ready_prefix),
                                    None,
                                )
                            } else {
                                let parameter_sets =
                                    native_vulkan_extract_h265_parameter_sets_for_vulkanalia(
                                        source,
                                        video_session_options.codec,
                                        video_session_options.bitstream_extract_max_samples,
                                    )?;
                                (None, Some(parameter_sets), None, None, None, None)
                            }
                        }
                        NativeVulkanVideoSessionCodec::Av1Main8
                        | NativeVulkanVideoSessionCodec::Av1Main10 => {
                            if decode_av1_ready_prefix {
                                let ready_prefix =
                                    native_vulkan_extract_av1_ready_prefix_for_vulkanalia(
                                        source,
                                        video_session_options.codec,
                                        video_session_options.bitstream_extract_max_samples,
                                        av1_ready_prefix_frames,
                                    )?;
                                (
                                    None,
                                    None,
                                    Some(ready_prefix.sequence_header.clone()),
                                    None,
                                    None,
                                    Some(ready_prefix),
                                )
                            } else {
                                let sequence_header =
                                    native_vulkan_extract_av1_sequence_header_for_vulkanalia(
                                        source,
                                        video_session_options.codec,
                                        video_session_options.bitstream_extract_max_samples,
                                    )?;
                                (None, None, Some(sequence_header), None, None, None)
                            }
                        }
                    }
                }
                #[cfg(not(feature = "native-vulkan-gst-video"))]
                {
                    let _ = source;
                    return Err(
                        "--create-session-parameters requires native-vulkan-gst-video feature"
                            .into(),
                    );
                }
            } else {
                (None, None, None, None, None, None)
            };
            json!(probe_native_vulkan_vulkanalia_video_session_bind(
                NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
                    codec: video_session_options.codec,
                    width: video_session_options.width,
                    height: video_session_options.height,
                    allocate_video_images: video_session_options.allocate_video_images,
                    allocate_bitstream_buffer: video_session_options.allocate_bitstream_buffer,
                    bitstream_buffer_size: video_session_options.bitstream_buffer_size,
                    create_empty_session_parameters: vulkanalia_create_empty_session_parameters,
                    create_session_parameters: create_parameters,
                    h264_parameter_sets,
                    h265_parameter_sets,
                    av1_sequence_header,
                    h264_ready_prefix_decode,
                    h265_ready_prefix_decode,
                    av1_ready_prefix_decode,
                }
            )?)
        }
        NativeVulkanCliMode::ProbeVideoSession => {
            if video_session_options.sample_h265_ready_prefix_output
                && video_session_options.decode_h265_ready_prefix_frames == 0
            {
                return Err(
                    "--sample-h265-ready-prefix requires --decode-h265-ready-prefix N".into(),
                );
            }
            if video_session_options.extract_bitstream {
                let source = source
                    .clone()
                    .ok_or("--extract-bitstream requires --source")?;
                if !source.is_file() {
                    return Err(
                        format!("bitstream source does not exist: {}", source.display()).into(),
                    );
                }
                video_session_options.bitstream_source = Some(source);
            }
            json!(probe_vulkan_video_session(video_session_options)?)
        }
        NativeVulkanCliMode::ProbeAudioClock => {
            let source = source.ok_or("--probe-audio-clock requires --source")?;
            if !source.is_file() {
                return Err(
                    format!("audio probe source does not exist: {}", source.display()).into(),
                );
            }
            #[cfg(feature = "native-vulkan-gst-video")]
            {
                json!(probe_native_vulkan_audio_clock(
                    NativeVulkanAudioClockProbeOptions {
                        source,
                        duration: audio_probe_duration,
                    }
                )?)
            }
            #[cfg(not(feature = "native-vulkan-gst-video"))]
            {
                let _ = (source, audio_probe_duration);
                return Err("--probe-audio-clock requires native-vulkan-gst-video feature".into());
            }
        }
        NativeVulkanCliMode::RunClear => json!(run_clear(options, duration)?),
        NativeVulkanCliMode::RunVulkanaliaSceneLiteSolidQuad => {
            json!(run_native_vulkan_vulkanalia_scene_lite_solid_quad_present(
                NativeVulkanVulkanaliaSceneLiteSolidQuadPresentOptions {
                    host: options.host,
                    wait_configure_roundtrips: options.wait_configure_roundtrips,
                    duration,
                    target_max_fps: options.target_max_fps,
                    quad_color: options.clear_color,
                    geometry: None,
                }
            )?)
        }
        NativeVulkanCliMode::RunVulkanaliaSceneLiteSampledImage => {
            let source =
                source.ok_or("--run-vulkanalia-scene-lite-sampled-image requires --source")?;
            if !source.is_file() {
                return Err(
                    format!("sampled-image source does not exist: {}", source.display()).into(),
                );
            }
            json!(
                run_native_vulkan_vulkanalia_scene_lite_sampled_image_present(
                    NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions {
                        host: options.host,
                        wait_configure_roundtrips: options.wait_configure_roundtrips,
                        duration,
                        target_max_fps: options.target_max_fps,
                        source,
                        clear_color: options.clear_color,
                        fit: None,
                        geometry: None,
                    }
                )?
            )
        }
        NativeVulkanCliMode::RunStatic => {
            let source = source.ok_or("--run-static requires --source")?;
            if !source.is_file() {
                return Err(format!("static source does not exist: {}", source.display()).into());
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
        NativeVulkanCliMode::RunLegacyStatic => {
            let source = source.ok_or("--run-legacy-static requires --source")?;
            let output_name = options
                .host
                .output_name
                .clone()
                .unwrap_or_else(|| "native-vulkan".to_owned());
            json!(run_legacy_static_image(
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
        NativeVulkanCliMode::RunVideo => {
            let source = source.ok_or("--run-video requires --source")?;
            if !source.is_file() {
                return Err(format!("video source does not exist: {}", source.display()).into());
            }
            if let Some(poster) = poster.as_ref()
                && !poster.is_file()
            {
                return Err(format!("video poster does not exist: {}", poster.display()).into());
            }
            let output_name = options
                .host
                .output_name
                .clone()
                .unwrap_or_else(|| "native-vulkan".to_owned());
            let target_max_fps = options.target_max_fps;
            json!(run_video(
                options,
                duration,
                VideoWallpaperPlan {
                    output_name,
                    source,
                    poster,
                    fit,
                    loop_playback,
                    muted,
                    manifest_max_fps: None,
                    target_max_fps,
                    decoder_policy,
                    start_offset_ms,
                },
            )?)
        }
        NativeVulkanCliMode::RunH265FirstFrameVideo => {
            let source = source.ok_or("--run-h265-first-frame-video requires --source")?;
            if !source.is_file() {
                return Err(format!("video source does not exist: {}", source.display()).into());
            }
            #[cfg(feature = "native-vulkan-gst-video")]
            {
                json!(run_h265_first_frame_video(
                    options,
                    duration,
                    source,
                    video_session_options.width,
                    video_session_options.height,
                    fit,
                    video_session_options.bitstream_extract_max_samples,
                )?)
            }
            #[cfg(not(feature = "native-vulkan-gst-video"))]
            {
                let _ = (
                    options,
                    duration,
                    source,
                    video_session_options.width,
                    video_session_options.height,
                    fit,
                );
                return Err(
                    "--run-h265-first-frame-video requires native-vulkan-gst-video feature".into(),
                );
            }
        }
        NativeVulkanCliMode::RunH264ReadyPrefixVideo => {
            let source = source.ok_or("--run-h264-ready-prefix-video requires --source")?;
            if !source.is_file() {
                return Err(format!("video source does not exist: {}", source.display()).into());
            }
            if video_session_options.decode_h264_ready_prefix_frames == 0 {
                return Err(
                    "--run-h264-ready-prefix-video requires --decode-h264-ready-prefix N".into(),
                );
            }
            #[cfg(feature = "native-vulkan-gst-video")]
            {
                let playback_frames = if ready_prefix_playback_frames == 0 {
                    video_session_options.decode_h264_ready_prefix_frames
                } else {
                    ready_prefix_playback_frames
                };
                let audio_output_mode = audio_output_policy.resolve(muted);
                json!(run_h264_ready_prefix_video(
                    options,
                    source,
                    video_session_options.width,
                    video_session_options.height,
                    fit,
                    video_session_options.bitstream_extract_max_samples,
                    video_session_options.decode_h264_ready_prefix_frames,
                    h264_video_input,
                    playback_frames,
                    audio_clock_probe_with_video,
                    audio_output_mode,
                )?)
            }
            #[cfg(not(feature = "native-vulkan-gst-video"))]
            {
                let _ = (
                    options,
                    source,
                    video_session_options.width,
                    video_session_options.height,
                    fit,
                    video_session_options.bitstream_extract_max_samples,
                    video_session_options.decode_h264_ready_prefix_frames,
                    ready_prefix_playback_frames,
                    audio_clock_probe_with_video,
                );
                return Err(
                    "--run-h264-ready-prefix-video requires native-vulkan-gst-video feature".into(),
                );
            }
        }
        NativeVulkanCliMode::RunH265ReadyPrefixVideo => {
            let source = source.ok_or("--run-h265-ready-prefix-video requires --source")?;
            if !source.is_file() {
                return Err(format!("video source does not exist: {}", source.display()).into());
            }
            if video_session_options.decode_h265_ready_prefix_frames == 0 {
                return Err(
                    "--run-h265-ready-prefix-video requires --decode-h265-ready-prefix N".into(),
                );
            }
            #[cfg(feature = "native-vulkan-gst-video")]
            {
                let playback_frames = if ready_prefix_playback_frames == 0 {
                    video_session_options.decode_h265_ready_prefix_frames
                } else {
                    ready_prefix_playback_frames
                };
                let audio_output_mode = audio_output_policy.resolve(muted);
                json!(run_h265_ready_prefix_video(
                    options,
                    video_session_options.codec,
                    source,
                    video_session_options.width,
                    video_session_options.height,
                    fit,
                    video_session_options.bitstream_extract_max_samples,
                    video_session_options.decode_h265_ready_prefix_frames,
                    h265_video_input,
                    playback_frames,
                    audio_clock_probe_with_video,
                    audio_output_mode,
                )?)
            }
            #[cfg(not(feature = "native-vulkan-gst-video"))]
            {
                let _ = (
                    options,
                    video_session_options.codec,
                    source,
                    video_session_options.width,
                    video_session_options.height,
                    fit,
                    video_session_options.bitstream_extract_max_samples,
                    video_session_options.decode_h265_ready_prefix_frames,
                    ready_prefix_playback_frames,
                    audio_clock_probe_with_video,
                );
                return Err(
                    "--run-h265-ready-prefix-video requires native-vulkan-gst-video feature".into(),
                );
            }
        }
        NativeVulkanCliMode::RunAv1ReadyPrefixVideo => {
            let source = source.ok_or("--run-av1-ready-prefix-video requires --source")?;
            if !source.is_file() {
                return Err(format!("video source does not exist: {}", source.display()).into());
            }
            if av1_ready_prefix_frames == 0 {
                return Err(
                    "--run-av1-ready-prefix-video requires --decode-av1-ready-prefix N".into(),
                );
            }
            #[cfg(feature = "native-vulkan-gst-video")]
            {
                let playback_frames = if ready_prefix_playback_frames == 0 {
                    av1_ready_prefix_frames
                } else {
                    ready_prefix_playback_frames
                };
                let audio_output_mode = audio_output_policy.resolve(muted);
                json!(run_av1_ready_prefix_video(
                    options,
                    video_session_options.codec,
                    source,
                    video_session_options.width,
                    video_session_options.height,
                    fit,
                    video_session_options.bitstream_extract_max_samples,
                    av1_ready_prefix_frames,
                    playback_frames,
                    audio_clock_probe_with_video,
                    audio_output_mode,
                )?)
            }
            #[cfg(not(feature = "native-vulkan-gst-video"))]
            {
                let _ = (
                    options,
                    video_session_options.codec,
                    source,
                    video_session_options.width,
                    video_session_options.height,
                    fit,
                    av1_ready_prefix_frames,
                    ready_prefix_playback_frames,
                    audio_clock_probe_with_video,
                );
                return Err(
                    "--run-av1-ready-prefix-video requires native-vulkan-gst-video feature".into(),
                );
            }
        }
        NativeVulkanCliMode::RunVulkanaliaReadyPrefixVideo => {
            let source = source.ok_or("--run-vulkanalia-ready-prefix-video requires --source")?;
            if !source.is_file() {
                return Err(format!("video source does not exist: {}", source.display()).into());
            }
            #[cfg(feature = "native-vulkan-gst-video")]
            let ready_prefix_frames = match video_session_options.codec {
                NativeVulkanVideoSessionCodec::H264High8 => {
                    video_session_options.decode_h264_ready_prefix_frames
                }
                NativeVulkanVideoSessionCodec::H265Main8
                | NativeVulkanVideoSessionCodec::H265Main10 => {
                    video_session_options.decode_h265_ready_prefix_frames
                }
                NativeVulkanVideoSessionCodec::Av1Main8
                | NativeVulkanVideoSessionCodec::Av1Main10 => av1_ready_prefix_frames,
            };
            #[cfg(not(feature = "native-vulkan-gst-video"))]
            let ready_prefix_frames = 0u32;
            if ready_prefix_frames == 0 {
                return Err(
                    "--run-vulkanalia-ready-prefix-video requires --decode-h264-ready-prefix N, --decode-h265-ready-prefix N, or --decode-av1-ready-prefix N matching --video-codec"
                        .into(),
                );
            }
            #[cfg(feature = "native-vulkan-gst-video")]
            {
                let playback_frames = if ready_prefix_playback_frames == 0 {
                    ready_prefix_frames
                } else {
                    ready_prefix_playback_frames
                };
                let audio_output_mode = audio_output_policy.resolve(muted);
                json!(run_vulkanalia_ready_prefix_video(
                    options,
                    video_session_options.codec,
                    source,
                    video_session_options.width,
                    video_session_options.height,
                    fit,
                    video_session_options.bitstream_extract_max_samples,
                    ready_prefix_frames,
                    playback_frames,
                    audio_clock_probe_with_video,
                    audio_output_mode,
                )?)
            }
            #[cfg(not(feature = "native-vulkan-gst-video"))]
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
                    audio_clock_probe_with_video,
                );
                return Err(
                    "--run-vulkanalia-ready-prefix-video requires native-vulkan-gst-video feature"
                        .into(),
                );
            }
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

#[cfg(all(
    feature = "native-vulkan-renderer",
    feature = "native-vulkan-gst-video"
))]
fn parse_h264_video_input_mode(
    value: &str,
) -> Result<gilder::renderer::native_vulkan::NativeVulkanH264VideoInputMode, String> {
    use gilder::renderer::native_vulkan::NativeVulkanH264VideoInputMode;

    match value {
        "ready-prefix-spool" | "spool" | "ready-prefix" => Err(
            "H.264 ready-prefix spool input is no longer maintained; use streaming-queue"
                .to_owned(),
        ),
        "streaming-queue" | "streaming" | "queue" => {
            Ok(NativeVulkanH264VideoInputMode::StreamingQueue)
        }
        other => Err(format!("unsupported H.264 video input mode: {other}")),
    }
}

#[cfg(all(
    feature = "native-vulkan-renderer",
    feature = "native-vulkan-gst-video"
))]
fn parse_h265_video_input_mode(
    value: &str,
) -> Result<gilder::renderer::native_vulkan::NativeVulkanH265VideoInputMode, String> {
    use gilder::renderer::native_vulkan::NativeVulkanH265VideoInputMode;

    match value {
        "ready-prefix-spool" | "spool" | "ready-prefix" => Err(
            "H.265 ready-prefix spool input is no longer maintained; use streaming-queue"
                .to_owned(),
        ),
        "streaming-queue" | "streaming" | "queue" => {
            Ok(NativeVulkanH265VideoInputMode::StreamingQueue)
        }
        other => Err(format!("unsupported H.265 video input mode: {other}")),
    }
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
    ProbeVideoSession,
    ProbeAudioClock,
    RunClear,
    RunVulkanaliaSceneLiteSolidQuad,
    RunVulkanaliaSceneLiteSampledImage,
    RunStatic,
    RunLegacyStatic,
    RunVideo,
    RunH265FirstFrameVideo,
    RunH264ReadyPrefixVideo,
    RunH265ReadyPrefixVideo,
    RunAv1ReadyPrefixVideo,
    RunVulkanaliaReadyPrefixVideo,
}

#[cfg(feature = "native-vulkan-renderer")]
fn print_usage() {
    println!(
        "Usage: gilder-native-vulkan [--json|--capabilities|--contract|--type-support|--probe-surface|--probe-video|--probe-vulkanalia|--probe-vulkanalia-swapchain|--probe-vulkanalia-video-present|--probe-vulkanalia-video-present-session|--probe-vulkanalia-video-session|--probe-video-session|--probe-audio-clock|--run-clear|--run-vulkanalia-clear|--run-vulkanalia-scene-lite-solid-quad|--run-vulkanalia-scene-lite-sampled-image|--run-static|--run-vulkanalia-static|--run-legacy-static|--run-video|--run-h265-first-frame-video|--run-h264-ready-prefix-video|--run-h265-ready-prefix-video|--run-av1-ready-prefix-video|--run-vulkanalia-ready-prefix-video]\n\
\n\
Print native Vulkan spike capabilities and backend contract.\n\
--probe-surface creates a layer-shell Wayland surface and VK_KHR_wayland_surface, then exits.\n\
--probe-video enumerates Vulkan Video decode extensions and queue families, then exits.\n\
--probe-vulkanalia enumerates the vulkanalia Vulkan 1.4 physical-device/video/external-memory gates, then exits.\n\
--probe-vulkanalia-swapchain creates a Wayland VkSurfaceKHR, Vulkanalia device, swapchain and swapchain image list, then exits.\n\
--probe-vulkanalia-video-present creates one Vulkanalia device with video-decode and graphics/present queues plus a Wayland swapchain, then exits.\n\
--probe-vulkanalia-video-present-session creates one Vulkanalia video+present device, video session, sampled DPB/output image, and Wayland swapchain, then exits.\n\
--probe-vulkanalia-video-session creates and binds a Vulkanalia Vulkan Video session for --video-codec, then exits.\n\
--probe-video-session creates and binds a Vulkan Video H.264/H.265/AV1 decode session, then exits.\n\
--probe-audio-clock runs an explicit audio-only GStreamer clock probe for --source, then exits.\n\
--audio-clock-probe runs the explicit audio-only clock probe beside H.264 visible video and reports A/V drift.\n\
--audio-output plan|clock-only|auto selects plan-following, clock-only telemetry, or tee-to-autoaudiosink output for --audio-clock-probe.\n\
--allocate-video-images extends --probe-video-session and --probe-vulkanalia-video-session with codec-matching 2-plane 4:2:0 DPB/output sampled image allocation.\n\
--allocate-bitstream-buffer extends --probe-video-session and --probe-vulkanalia-video-session with a mapped VIDEO_DECODE_SRC bitstream buffer.\n\
--create-empty-session-parameters extends --probe-vulkanalia-video-session with an H.264/H.265 empty capacity VkVideoSessionParametersKHR smoke.\n\
--create-session-parameters extends --probe-vulkanalia-video-session with real H.264 SPS/PPS, H.265 VPS/SPS/PPS, or AV1 sequence-header VkVideoSessionParametersKHR creation from --source.\n\
--extract-bitstream extends --probe-video-session with parser/appsink encoded AU extraction and writes the selected AU to the bitstream buffer.\n\
--decode-first-frame extends --probe-video-session with a real H.264/H.265 IDR Vulkan Video command buffer submit.\n\
--sample-decoded-first-frame extends --decode-first-frame with NV12 shader sampling into an offscreen Vulkan color target.\n\
--decode-h264-idr-prefix N extends --probe-video-session with N H.264 IDR AU Vulkan Video decode submits and final-frame readback.\n\
--decode-h264-ready-prefix N extends --probe-video-session with N reference-ready H.264 AU Vulkan Video decode submits and final-frame readback.\n\
--decode-h265-ready-prefix N extends --probe-video-session with N ready H.265 AU Vulkan Video decode submits and final-frame readback.\n\
--decode-av1-ready-prefix N selects an AV1 ready TU window for direct visible AV1 decode/present.\n\
--sample-h265-ready-prefix extends --decode-h265-ready-prefix with final-frame NV12 shader sampling into an offscreen RGBA target.\n\
--sample-h265-ready-prefix-sequence samples each ready-prefix decoded frame before the next AU can overwrite its DPB/output layer.\n\
--playback-frames N repeats the ready-prefix AU window for N direct Vulkan Video decode/present frames.\n\
--audio-probe-duration N overrides the default 10s audio clock probe duration.\n\
--run-clear uses the Vulkanalia Wayland swapchain runtime, clears frames with CmdPipelineBarrier2/QueueSubmit2, presents, then prints runtime JSON.\n\
--run-vulkanalia-clear is an explicit alias for --run-clear.\n\
--run-vulkanalia-scene-lite-solid-quad uses Vulkanalia dynamic rendering to draw a retained scene-lite solid quad to the Wayland swapchain.\n\
--run-vulkanalia-scene-lite-sampled-image uses Vulkanalia dynamic rendering to upload --source once into a retained sampled image and draw it to the Wayland swapchain.\n\
--run-static and --run-vulkanalia-static use Vulkanalia sampled-image dynamic rendering for static wallpapers with cover|contain|stretch|tile|center fit and background clear.\n\
--run-legacy-static uses the old ash staging-copy static image path for comparison.\n\
--run-video accepts a video wallpaper plan, presents a poster/clear placeholder through native Vulkan, then prints video handoff telemetry.\n\
--run-h265-first-frame-video decodes the first H.265 IDR with Vulkan Video and samples the decoded NV12 image to the swapchain.\n\
--run-h264-ready-prefix-video decodes a ready H.264 AU prefix with Vulkan Video and samples each decoded NV12 layer to the swapchain.\n\
--h264-input streaming-queue selects bounded streaming packet queue input for H.264 visible video; ready-prefix spool is no longer maintained.\n\
--run-h265-ready-prefix-video decodes a ready H.265 AU prefix with Vulkan Video and samples each decoded NV12 layer to the swapchain.\n\
--h265-input streaming-queue selects bounded streaming packet queue input for H.265 visible video; ready-prefix spool is no longer maintained.\n\
--run-av1-ready-prefix-video decodes a ready AV1 TU prefix with Vulkan Video and samples each decoded layer to the swapchain.\n\
--run-vulkanalia-ready-prefix-video decodes a ready H.264/H.265/AV1 prefix through Vulkanalia CmdPipelineBarrier2/QueueSubmit2 and prints runtime JSON.\n\
Options: [--output-name NAME] [--layer background|bottom|top|overlay] [--wait-roundtrips N]\n\
         [--duration SECONDS] [--target-fps FPS|--no-fps-limit] [--color #rrggbb|r,g,b]\n\
         [--source PATH] [--poster PATH] [--fit cover|contain|stretch|tile|center] [--background #rrggbb]\n\
         [--loop|--no-loop] [--muted|--unmuted] [--audio-output plan|clock-only|auto] [--decoder auto|hardware-preferred|hardware-required|software]\n\
         [--video-codec h264|h265|h265-main-10|av1|av1-main-10] [--width PX] [--height PX]\n\
         [--allocate-video-images] [--allocate-bitstream-buffer] [--bitstream-buffer-size BYTES]\n\
         [--create-session-parameters] [--extract-bitstream] [--decode-first-frame] [--sample-decoded-first-frame] [--bitstream-samples N]\n\
         [--decode-h264-idr-prefix N] [--require-h264-idr-prefix N]\n\
         [--decode-h264-ready-prefix N] [--require-h264-ready-prefix N]\n\
         [--decode-h265-ready-prefix N] [--sample-h265-ready-prefix] [--sample-h265-ready-prefix-sequence]\n\
         [--decode-av1-ready-prefix N]\n\
         [--require-h265-ready-prefix N] [--playback-frames N]\n\
         [--start-offset-ms MS]"
    );
}
