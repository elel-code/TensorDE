#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_clock_advances_from_pts_and_duration() {
        let mut clock = NativeVulkanAudioClock::new();

        assert_eq!(
            clock.advance(NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(1_000_000_000),
                duration_ns: Some(20_000_000),
                payload_bytes: 128,
                decoded_frames: 0,
                decoded_samples: 0,
                sample_rate_hz: None,
                channel_count: None,
                ..NativeVulkanAudioClockPacket::default()
            }),
            Some(20_000_000)
        );
        assert_eq!(
            clock.advance(NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(1_020_000_000),
                duration_ns: Some(20_000_000),
                payload_bytes: 128,
                decoded_frames: 0,
                decoded_samples: 0,
                sample_rate_hz: None,
                channel_count: None,
                ..NativeVulkanAudioClockPacket::default()
            }),
            Some(40_000_000)
        );
    }

    #[test]
    fn audio_clock_serial_reset_rebases_loop_without_reusing_stale_packets() {
        let mut clock = NativeVulkanAudioClock::new();
        clock.advance(NativeVulkanAudioClockPacket {
            serial: 0,
            pts_ns: Some(5_000_000_000),
            duration_ns: Some(10_000_000),
            payload_bytes: 64,
            decoded_frames: 0,
            decoded_samples: 0,
            sample_rate_hz: None,
            channel_count: None,
            ..NativeVulkanAudioClockPacket::default()
        });

        assert_eq!(
            clock.advance(NativeVulkanAudioClockPacket {
                serial: 1,
                pts_ns: Some(5_000_000_000),
                duration_ns: Some(10_000_000),
                payload_bytes: 64,
                decoded_frames: 0,
                decoded_samples: 0,
                sample_rate_hz: None,
                channel_count: None,
                ..NativeVulkanAudioClockPacket::default()
            }),
            Some(20_000_000)
        );
        assert_eq!(
            clock.advance(NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(5_010_000_000),
                duration_ns: Some(10_000_000),
                payload_bytes: 64,
                decoded_frames: 0,
                decoded_samples: 0,
                sample_rate_hz: None,
                channel_count: None,
                ..NativeVulkanAudioClockPacket::default()
            }),
            Some(20_000_000)
        );
        assert_eq!(clock.stale_dropped_packets, 1);
    }

    #[test]
    fn audio_packet_queue_is_bounded_and_retains_no_payload_bytes() {
        let mut queue = NativeVulkanAudioClockPacketQueue::new(1);
        queue.push(NativeVulkanAudioClockPacket {
            serial: 0,
            pts_ns: Some(0),
            duration_ns: Some(1_000_000),
            payload_bytes: 128,
            decoded_frames: 0,
            decoded_samples: 0,
            sample_rate_hz: None,
            channel_count: None,
            ..NativeVulkanAudioClockPacket::default()
        });
        queue.push(NativeVulkanAudioClockPacket {
            serial: 0,
            pts_ns: Some(1_000_000),
            duration_ns: Some(1_000_000),
            payload_bytes: 256,
            decoded_frames: 0,
            decoded_samples: 0,
            sample_rate_hz: None,
            channel_count: None,
            ..NativeVulkanAudioClockPacket::default()
        });

        assert_eq!(queue.queued_packets(), 1);
        assert_eq!(queue.overflow_dropped_packets, 1);
        assert_eq!(queue.retained_payload_bytes(), 0);
        assert_eq!(queue.max_payload_bytes, 256);
    }

    #[test]
    fn audio_runtime_snapshot_reports_clock_only_boundary() {
        let mut runtime = NativeVulkanAudioClockRuntime::new(
            NativeVulkanAudioOutputMode::ClockOnly,
            NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
        );
        runtime.set_audio_stream(2);
        runtime.push_and_advance(
            0,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(42_000_000),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                audio_signal_level_micros: 0,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );

        let snapshot = runtime.snapshot();

        assert_eq!(snapshot.output_mode, "clock-only");
        assert!(snapshot.audio_stream_found);
        assert_eq!(snapshot.audio_stream_index, Some(2));
        assert!(!snapshot.audible_output_started);
        assert_eq!(snapshot.audio_output_backend, "none");
        assert_eq!(snapshot.audio_output_sample_format, "none");
        assert_eq!(snapshot.audio_output_frames, 0);
        assert_eq!(snapshot.audio_output_samples, 0);
        assert_eq!(snapshot.audio_output_bytes, 0);
        assert_eq!(snapshot.audio_output_sample_rate_hz, None);
        assert_eq!(snapshot.audio_output_channel_count, None);
        assert_eq!(snapshot.audio_output_write_calls, 0);
        assert_eq!(snapshot.audio_output_write_waits, 0);
        assert_eq!(snapshot.audio_output_process_callbacks, 0);
        assert_eq!(snapshot.audio_output_buffer_errors, 0);
        assert_eq!(snapshot.audio_output_timeout_errors, 0);
        assert_eq!(snapshot.audio_output_xrun_count, 0);
        assert_eq!(snapshot.audio_output_state_changes, 0);
        assert_eq!(snapshot.audio_output_ready_state_changes, 0);
        assert_eq!(snapshot.audio_output_stream_state, "unconnected");
        assert!(!snapshot.audio_output_stream_ready);
        assert_eq!(
            snapshot.audio_output_lifecycle_model,
            "clock-only-no-output-stream-lifecycle"
        );
        assert_eq!(
            snapshot.audio_output_latency_policy,
            "clock-only-no-output-latency"
        );
        assert_eq!(snapshot.retained_payload_bytes, 0);
        assert_eq!(snapshot.retained_pcm_frame_bytes, 0);
        assert_eq!(snapshot.decoded_frames, 1);
        assert_eq!(snapshot.decoded_samples, 1008);
        assert_eq!(snapshot.audio_sample_rate_hz, Some(48_000));
        assert_eq!(snapshot.audio_channel_count, Some(2));
        assert_eq!(snapshot.clock_ns, Some(21_000_000));
        assert!(snapshot.video_master_clock_ready);
        assert_eq!(snapshot.video_master_start_clock_ns, Some(21_000_000));
        assert_eq!(snapshot.video_master_start_serial, Some(0));
        assert_eq!(snapshot.video_master_start_packet_index, Some(0));
        assert_eq!(snapshot.current_serial_start_clock_ns, Some(21_000_000));
        assert_eq!(snapshot.current_serial_start_serial, Some(0));
        assert_eq!(snapshot.current_serial_start_packet_index, Some(0));
        assert_eq!(snapshot.packets_head.len(), 1);
        assert_eq!(snapshot.packets_head[0].decoded_frames, 1);
        assert_eq!(snapshot.packets_head[0].decoded_samples, 1008);
    }

    #[test]
    fn audio_runtime_snapshot_reports_pipewire_output_boundary() {
        let event_channel = NativeVulkanAudioEventChannel::default();
        let mut runtime = NativeVulkanAudioClockRuntime::new(
            NativeVulkanAudioOutputMode::Auto,
            NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
        )
        .with_event_channel(Some(event_channel.clone()));
        runtime.set_audio_stream(2);
        runtime.push_and_advance(
            0,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(42_000_000),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                audio_signal_level_micros: 123_456,
                audio_spectrum: Some(StereoSpectrum64 {
                    left: [0.25; 64],
                    right: [0.75; 64],
                }),
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                output_frames: 1,
                output_samples: 1008,
                output_bytes: 8064,
                output_sample_rate_hz: Some(48_000),
                output_channel_count: Some(2),
                output_write_calls: 1,
                output_write_waits: 1,
                output_process_callbacks: 1,
                output_buffer_errors: 0,
                output_timeout_errors: 0,
                output_state_changes: 2,
                output_ready_state_changes: 1,
                output_stream_state: 3,
                output_stream_ready: true,
                ..NativeVulkanAudioClockPacket::default()
            },
        );

        let snapshot = runtime.snapshot();

        assert_eq!(snapshot.output_mode, "auto");
        assert!(snapshot.audible_output_started);
        assert_eq!(snapshot.audio_output_backend, "pipewire-f32le");
        assert_eq!(snapshot.audio_output_sample_format, "f32le-interleaved");
        assert_eq!(snapshot.audio_output_frames, 1);
        assert_eq!(snapshot.audio_output_samples, 1008);
        assert_eq!(snapshot.audio_output_bytes, 8064);
        assert_eq!(snapshot.audio_output_sample_rate_hz, Some(48_000));
        assert_eq!(snapshot.audio_output_channel_count, Some(2));
        assert_eq!(snapshot.audio_output_write_calls, 1);
        assert_eq!(snapshot.audio_output_write_waits, 1);
        assert_eq!(snapshot.audio_output_process_callbacks, 1);
        assert_eq!(snapshot.audio_output_buffer_errors, 0);
        assert_eq!(snapshot.audio_output_timeout_errors, 0);
        assert_eq!(snapshot.audio_output_xrun_count, 0);
        assert_eq!(snapshot.audio_output_state_changes, 2);
        assert_eq!(snapshot.audio_output_ready_state_changes, 1);
        assert_eq!(snapshot.audio_output_stream_state, "streaming");
        assert!(snapshot.audio_output_stream_ready);
        assert_eq!(snapshot.audio_signal_level_micros, 123_456);
        assert_eq!(snapshot.audio_signal_model, "decoded-f32le-frame-rms");
        assert_eq!(
            snapshot.audio_spectrum,
            StereoSpectrum64 {
                left: [0.25; 64],
                right: [0.75; 64],
            }
        );
        assert_eq!(
            snapshot.audio_spectrum_model,
            "decoded-f32-canonical-stereo64"
        );
        assert_eq!(
            snapshot.audio_output_lifecycle_model,
            "pipewire-thread-loop-stream-state-owned-by-audio-runtime"
        );
        assert_eq!(
            snapshot.audio_output_latency_policy,
            "bounded-pipewire-write-wait-with-zero-buffer-timeout-error-gate"
        );
        let audio_event = event_channel.capture(0, 0);
        assert!(audio_event.ready);
        assert_eq!(audio_event.sample_time_ns, 42_000_000);
        assert!(audio_event.spectrum.left[0] > 0.0);
        assert!(audio_event.spectrum.right[0] > audio_event.spectrum.left[0]);
    }

    #[test]
    fn audio_runtime_reports_playback_target_coverage() {
        let mut runtime = NativeVulkanAudioClockRuntime::new(
            NativeVulkanAudioOutputMode::Auto,
            NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
        );
        runtime.set_audio_stream(2);
        runtime.set_playback_target_clock_ns(Some(42_000_000));
        runtime.push_and_advance(
            0,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(0),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                audio_signal_level_micros: 0,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                output_frames: 1,
                output_samples: 1008,
                output_bytes: 4032,
                output_sample_rate_hz: Some(48_000),
                output_channel_count: Some(2),
                output_write_calls: 1,
                output_write_waits: 1,
                output_process_callbacks: 1,
                output_buffer_errors: 0,
                output_timeout_errors: 0,
                output_state_changes: 2,
                output_ready_state_changes: 1,
                output_stream_state: 3,
                output_stream_ready: true,
                ..NativeVulkanAudioClockPacket::default()
            },
        );

        let partial = runtime.snapshot();
        assert_eq!(partial.playback_target_clock_ns, Some(42_000_000));
        assert_eq!(partial.playback_covered_clock_ns, Some(21_000_000));
        assert_eq!(partial.playback_coverage_percent, 50);
        assert!(!partial.playback_target_reached);

        runtime.push_and_advance(
            1,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(21_000_000),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                audio_signal_level_micros: 0,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                output_frames: 1,
                output_samples: 1008,
                output_bytes: 4032,
                output_sample_rate_hz: Some(48_000),
                output_channel_count: Some(2),
                output_write_calls: 2,
                output_write_waits: 2,
                output_process_callbacks: 2,
                output_buffer_errors: 0,
                output_timeout_errors: 0,
                output_state_changes: 2,
                output_ready_state_changes: 1,
                output_stream_state: 3,
                output_stream_ready: true,
                ..NativeVulkanAudioClockPacket::default()
            },
        );

        let covered = runtime.snapshot();
        assert_eq!(covered.playback_covered_clock_ns, Some(42_000_000));
        assert_eq!(covered.playback_coverage_percent, 100);
        assert!(covered.playback_target_reached);
    }

    #[test]
    fn audio_runtime_accepts_metadata_only_fast_forward_without_payload_retention() {
        let mut runtime = NativeVulkanAudioClockRuntime::new(
            NativeVulkanAudioOutputMode::ClockOnly,
            NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
        );
        runtime.set_audio_stream(1);
        runtime.set_playback_target_clock_ns(Some(6_000_000_000));
        runtime.push_and_advance(
            0,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(0),
                duration_ns: Some(21_333_333),
                payload_bytes: 560,
                decoded_frames: 1,
                decoded_samples: 1024,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );
        runtime.push_and_advance(
            1,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: None,
                duration_ns: Some(5_978_666_667),
                payload_bytes: 0,
                decoded_frames: 1,
                decoded_samples: 286_976,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );

        let snapshot = runtime.snapshot();

        assert_eq!(snapshot.pushed_packets, 2);
        assert_eq!(snapshot.consumed_packets, 2);
        assert_eq!(snapshot.retained_payload_bytes, 0);
        assert_eq!(snapshot.max_payload_bytes, 560);
        assert_eq!(snapshot.playback_covered_clock_ns, Some(6_000_000_000));
        assert!(snapshot.playback_target_reached);
        assert_eq!(snapshot.video_master_start_clock_ns, Some(21_333_333));
        assert_eq!(snapshot.packets_head[1].payload_bytes, 0);
    }

    #[test]
    fn audio_runtime_video_master_start_uses_first_ready_clock_sample() {
        let mut runtime = NativeVulkanAudioClockRuntime::new(
            NativeVulkanAudioOutputMode::ClockOnly,
            NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
        );
        runtime.set_audio_stream(2);
        runtime.push_and_advance(
            0,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: None,
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );
        runtime.push_and_advance(
            1,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(0),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );
        runtime.push_and_advance(
            2,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(21_000_000),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );

        let snapshot = runtime.snapshot();

        assert_eq!(snapshot.clock_ns, Some(42_000_000));
        assert_eq!(snapshot.video_master_start_clock_ns, Some(21_000_000));
        assert_eq!(snapshot.video_master_start_serial, Some(0));
        assert_eq!(snapshot.video_master_start_packet_index, Some(1));
        assert_eq!(snapshot.current_serial_start_clock_ns, Some(21_000_000));
        assert_eq!(snapshot.current_serial_start_serial, Some(0));
        assert_eq!(snapshot.current_serial_start_packet_index, Some(1));
    }

    #[test]
    fn audio_runtime_current_serial_start_resets_on_new_serial() {
        let mut runtime = NativeVulkanAudioClockRuntime::new(
            NativeVulkanAudioOutputMode::ClockOnly,
            NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
        );
        runtime.set_audio_stream(2);
        runtime.push_and_advance(
            0,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(0),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );
        runtime.push_and_advance(
            3,
            NativeVulkanAudioClockPacket {
                serial: 1,
                pts_ns: Some(0),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );

        let snapshot = runtime.snapshot();

        assert_eq!(snapshot.clock_ns, Some(42_000_000));
        assert_eq!(snapshot.video_master_start_clock_ns, Some(21_000_000));
        assert_eq!(snapshot.video_master_start_serial, Some(0));
        assert_eq!(snapshot.video_master_start_packet_index, Some(0));
        assert_eq!(snapshot.current_serial_start_clock_ns, Some(42_000_000));
        assert_eq!(snapshot.current_serial_start_serial, Some(1));
        assert_eq!(snapshot.current_serial_start_packet_index, Some(3));
    }

    #[test]
    fn unattached_audio_clock_is_not_a_video_master() {
        let snapshot =
            native_vulkan_unattached_audio_clock_snapshot(NativeVulkanAudioOutputMode::ClockOnly);

        assert!(!snapshot.audio_stream_found);
        assert!(!snapshot.video_master_clock_ready);
        assert_eq!(snapshot.video_master_start_clock_ns, None);
        assert_eq!(snapshot.video_master_start_serial, None);
        assert_eq!(snapshot.video_master_start_packet_index, None);
        assert_eq!(snapshot.current_serial_start_clock_ns, None);
        assert_eq!(snapshot.current_serial_start_serial, None);
        assert_eq!(snapshot.current_serial_start_packet_index, None);
    }
}
