#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConstraintAdjustments, LogicalRect};

    #[test]
    fn positioner_rejects_zero_sized_geometry() {
        let positioner = PopupPositioner {
            size: LogicalSize::new(0, 20),
            ..PopupPositioner::default()
        };
        assert!(matches!(
            validate_positioner(&positioner),
            Err(RuntimeError::InvalidPositioner(_))
        ));
    }

    #[test]
    fn positioner_preserves_all_constraint_bits() {
        let all = ConstraintAdjustments::all();
        let mapped = map_constraints(all);
        assert!(mapped.contains(xdg_positioner::ConstraintAdjustment::SlideX));
        assert!(mapped.contains(xdg_positioner::ConstraintAdjustment::SlideY));
        assert!(mapped.contains(xdg_positioner::ConstraintAdjustment::FlipX));
        assert!(mapped.contains(xdg_positioner::ConstraintAdjustment::FlipY));
        assert!(mapped.contains(xdg_positioner::ConstraintAdjustment::ResizeX));
        assert!(mapped.contains(xdg_positioner::ConstraintAdjustment::ResizeY));
    }

    #[test]
    fn blur_region_keeps_surface_local_rectangles() {
        let region = BlurRegion::Rectangles(vec![LogicalRect::new(4, 8, 120, 40)]);
        assert_eq!(
            region,
            BlurRegion::Rectangles(vec![LogicalRect::new(4, 8, 120, 40)])
        );
    }

    #[test]
    fn ext_background_effect_requires_the_dynamic_blur_capability() {
        assert!(!supports_ext_background_blur(None));
        assert!(!supports_ext_background_blur(Some(
            BackgroundEffectCapability::empty()
        )));
        assert!(supports_ext_background_blur(Some(
            BackgroundEffectCapability::Blur
        )));
    }

    #[test]
    fn fractional_scale_keeps_wl_surface_buffer_scale_at_one() {
        assert!(matches!(validate_buffer_scale(1, true, 3), Ok(true)));
        assert!(matches!(validate_buffer_scale(2, false, 3), Ok(true)));
        assert!(validate_buffer_scale(0, false, 3).is_err());
        assert!(validate_buffer_scale(2, true, 3).is_err());
    }

    #[test]
    fn old_wl_surface_never_receives_the_v3_buffer_scale_request() {
        assert!(matches!(validate_buffer_scale(1, false, 1), Ok(false)));
        assert!(matches!(validate_buffer_scale(1, false, 2), Ok(false)));
        assert!(validate_buffer_scale(2, false, 2).is_err());
    }

    #[test]
    fn viewport_destination_rejects_zero_sized_axes() {
        assert!(validate_viewport_destination(None).is_ok());
        assert!(validate_viewport_destination(Some(LogicalSize::new(1, 1))).is_ok());
        assert!(validate_viewport_destination(Some(LogicalSize::new(0, 1))).is_err());
        assert!(validate_viewport_destination(Some(LogicalSize::new(1, 0))).is_err());
    }

    #[test]
    fn activation_targets_are_limited_to_toplevel_roles() {
        let surface = SurfaceId(8);
        assert!(validate_activation_target(surface, SurfaceKind::Toplevel).is_ok());
        assert!(validate_activation_target(surface, SurfaceKind::Dialog).is_ok());
        assert!(matches!(
            validate_activation_target(surface, SurfaceKind::Popup),
            Err(RuntimeError::InvalidActivationTarget(id)) if id == surface
        ));
        assert!(matches!(
            validate_activation_target(surface, SurfaceKind::Layer),
            Err(RuntimeError::InvalidActivationTarget(id)) if id == surface
        ));
    }

    #[test]
    fn activation_request_ids_are_nonzero_and_wrap_safely() {
        let mut next = 1;
        assert_eq!(take_activation_request_id(&mut next).get(), 1);
        assert_eq!(take_activation_request_id(&mut next).get(), 2);

        next = u64::MAX;
        assert_eq!(take_activation_request_id(&mut next).get(), u64::MAX);
        assert_eq!(next, 1);
    }

    #[test]
    fn user_attention_requests_coalesce_per_surface() {
        let first = SurfaceId(3);
        let second = SurfaceId(7);
        let mut pending = HashSet::new();

        assert!(begin_attention_request(&mut pending, first));
        assert!(!begin_attention_request(&mut pending, first));
        assert!(begin_attention_request(&mut pending, second));
        assert_eq!(pending, HashSet::from([first, second]));
    }

    #[test]
    fn popup_touch_grab_expires_when_its_touch_point_ends() {
        let mut objects = SeatObjects::default();
        objects.touch_points.insert(5, SurfaceId(2), 55);

        assert!(is_current_popup_grab(
            &objects,
            InputSerialSource::TouchDown,
            55
        ));
        objects.touch_points.remove(5);
        assert!(!is_current_popup_grab(
            &objects,
            InputSerialSource::TouchDown,
            55
        ));
    }

    #[test]
    fn nested_popups_are_removed_before_their_parents() {
        let root = SurfaceId(1);
        let popup = SurfaceId(2);
        let nested_popup = SurfaceId(3);
        let dialog = SurfaceId(4);
        let children = HashMap::from([(root, vec![popup, dialog]), (popup, vec![nested_popup])]);
        let mut order = Vec::new();

        collect_post_order(&children, root, &mut order);

        assert_eq!(order, vec![nested_popup, popup, dialog, root]);
    }

    #[test]
    fn dnd_actions_round_trip_all_protocol_bits() {
        let actions = DndActions::COPY | DndActions::MOVE | DndActions::ASK;
        assert_eq!(dnd_actions(map_dnd_actions(actions)), actions);
        assert_eq!(dnd_action(map_dnd_action(DndAction::Copy)), Some(DndAction::Copy));
        assert_eq!(dnd_action(map_dnd_action(DndAction::Move)), Some(DndAction::Move));
        assert_eq!(dnd_action(map_dnd_action(DndAction::Ask)), Some(DndAction::Ask));
    }

    #[test]
    fn cursor_icons_cover_fika_runtime_vocabulary() {
        assert_eq!(map_cursor_icon(CursorIcon::Default), SctkCursorIcon::Default);
        assert_eq!(map_cursor_icon(CursorIcon::Pointer), SctkCursorIcon::Pointer);
        assert_eq!(map_cursor_icon(CursorIcon::Text), SctkCursorIcon::Text);
        assert_eq!(
            map_cursor_icon(CursorIcon::ColResize),
            SctkCursorIcon::ColResize
        );
    }

    #[test]
    fn selection_seat_uses_newest_focused_data_device_serial() {
        let input = |serial, order| Some(SelectionSerial { serial, order });
        let candidates = [
            (1, true, true, input(10, 2)),
            (2, false, true, input(20, 8)),
            (3, true, false, input(30, 9)),
            (4, true, true, input(40, 7)),
        ];

        assert_eq!(select_selection_seat(candidates), Some((4, 40)));
    }
}
