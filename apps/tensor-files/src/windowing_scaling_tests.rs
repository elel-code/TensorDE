mod scaling_tests {
    use super::*;

    #[test]
    fn physical_key_uses_linux_evdev_codes_without_xkb_offset() {
        // wl_keyboard / SCTK raw_code values are already Linux keycodes.
        assert_eq!(physical_key(46), PhysicalKey::Code(KeyCode::KeyC));
        assert_eq!(physical_key(38), PhysicalKey::Code(KeyCode::KeyL));
        assert_eq!(physical_key(30), PhysicalKey::Code(KeyCode::KeyA));
        assert_eq!(physical_key(47), PhysicalKey::Code(KeyCode::KeyV));
        assert_eq!(physical_key(45), PhysicalKey::Code(KeyCode::KeyX));
        // X11-style keycodes (evdev + 8) must not silently remap onto neighbors.
        assert!(matches!(physical_key(54), PhysicalKey::Unidentified(_)));
    }

    #[test]
    fn fractional_scale_rounds_toplevel_sizes_half_away_from_zero() {
        let logical = LogicalSize::new(801, 641);

        assert_eq!(
            logical_to_physical_rounded(logical, 1.25),
            PhysicalSize::new(1001, 801)
        );
        assert_eq!(
            logical_to_physical_rounded(logical, 1.5),
            PhysicalSize::new(1202, 962)
        );
        assert_eq!(
            logical_to_physical_rounded(LogicalSize::new(800, 640), 0.75),
            PhysicalSize::new(600, 480)
        );
    }

    #[test]
    fn physical_size_requests_use_the_fractional_scale() {
        assert_eq!(
            physical_to_logical_rounded(PhysicalSize::new(1001, 801), 1.25),
            LogicalSize::new(801, 641)
        );
        assert_eq!(
            physical_to_logical_rounded(PhysicalSize::new(1202, 962), 1.5),
            LogicalSize::new(801, 641)
        );
    }

    #[test]
    fn repeated_same_size_configure_skips_surface_state_commit_and_resize() {
        let logical = LogicalSize::new(847, 1015);
        let physical = PhysicalSize::new(1271, 1523);
        let mut state = WindowState {
            logical_size: logical,
            physical_size: physical,
            scale_factor: 1.5,
            configured: true,
            redraw_requested: false,
            destroy_requested: false,
        };

        let (next_physical, surface_state_changed, resized) =
            apply_configured_logical_size(&mut state, logical);

        assert_eq!(next_physical, physical);
        assert!(!surface_state_changed);
        assert!(!resized);
        assert!(state.redraw_requested);
    }

    #[test]
    fn initial_and_resized_configures_update_surface_state() {
        let initial_logical = LogicalSize::new(847, 1015);
        let mut state = WindowState {
            logical_size: initial_logical,
            physical_size: PhysicalSize::new(1271, 1523),
            scale_factor: 1.5,
            configured: false,
            redraw_requested: false,
            destroy_requested: false,
        };

        let (_, surface_state_changed, resized) =
            apply_configured_logical_size(&mut state, initial_logical);
        assert!(surface_state_changed);
        assert!(resized);

        let resized_logical = LogicalSize::new(900, 700);
        let (physical, surface_state_changed, resized) =
            apply_configured_logical_size(&mut state, resized_logical);
        assert_eq!(physical, PhysicalSize::new(1350, 1050));
        assert!(surface_state_changed);
        assert!(resized);
    }

    #[test]
    fn native_surface_destroy_request_is_idempotent() {
        let mut state = WindowState {
            logical_size: LogicalSize::new(800, 640),
            physical_size: PhysicalSize::new(1200, 960),
            scale_factor: 1.5,
            configured: true,
            redraw_requested: false,
            destroy_requested: false,
        };

        assert!(state.mark_destroy_requested());
        assert!(!state.mark_destroy_requested());
    }

    #[test]
    fn pointer_axis_prefers_value120_steps_over_continuous_pixels() {
        let horizontal = PointerAxisValue::default();
        let vertical = PointerAxisValue {
            continuous: 12.0,
            value120: 30,
            discrete: 1,
            ..Default::default()
        };
        assert_eq!(
            map_pointer_axis_to_scroll_delta(horizontal, vertical, 1.25),
            MouseScrollDelta::LineDelta { x: 0.0, y: -0.25 }
        );
    }

    #[test]
    fn pointer_axis_falls_back_to_scaled_continuous_pixels() {
        let horizontal = PointerAxisValue {
            continuous: -2.0,
            ..Default::default()
        };
        let vertical = PointerAxisValue {
            continuous: 4.0,
            ..Default::default()
        };
        assert_eq!(
            map_pointer_axis_to_scroll_delta(horizontal, vertical, 1.5),
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(3.0, -6.0))
        );
    }

    #[test]
    fn pointer_axis_uses_deprecated_discrete_when_value120_is_absent() {
        let vertical = PointerAxisValue {
            discrete: -2,
            continuous: 8.0,
            ..Default::default()
        };
        assert_eq!(
            map_pointer_axis_to_scroll_delta(PointerAxisValue::default(), vertical, 2.0),
            MouseScrollDelta::LineDelta { x: 0.0, y: 2.0 }
        );
    }
}
