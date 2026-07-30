use std::time::Instant;

use crate::windowing::{
    ActiveEventLoop, ButtonSource, CursorIcon, ElementState, PhysicalPosition, PhysicalSize,
};

use super::outcome::{ShellActionEffect, ShellActionOutcome};
use super::pointer_route::{
    MainLeftPointerButtonIntent, MainLeftPointerButtonRoute, MainLeftPointerButtonRouteSnapshot,
    MainPointerButtonIntent, MainPointerButtonSnapshot, MainPointerMoveIntent,
    MainPointerMoveSnapshot, main_left_pointer_button_route, main_pointer_button_intent,
    main_pointer_move_intent,
};
use crate::ui::animation::ShellAnimationKind;
use crate::ui::selection::SelectionClick;
use crate::{
    FikaApp, ShellItemActivation, ShellPlaceActivation, view_point_from_physical_position,
};

impl FikaApp {
    pub(crate) fn handle_main_pointer_moved(
        &mut self,
        event_loop: &ActiveEventLoop,
        position: PhysicalPosition<f64>,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let point = view_point_from_physical_position(position);
        let intent = main_pointer_move_intent(self.main_pointer_move_snapshot());
        let outcome = self.dispatch_main_pointer_move_intent(intent, point, size);
        self.start_outgoing_drag_if_needed(event_loop);
        self.apply_window_action_outcome(outcome);
    }

    pub(crate) fn handle_main_pointer_left(&mut self) {
        self.set_window_cursor(CursorIcon::Default);
        let changed = self.scene.clear_pointer();
        self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
    }

    pub(crate) fn handle_main_pointer_button(
        &mut self,
        event_loop: &ActiveEventLoop,
        state: ElementState,
        position: PhysicalPosition<f64>,
        button: ButtonSource,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let point = view_point_from_physical_position(position);
        let Some(mouse_button) = button.mouse_button() else {
            return;
        };

        let intent =
            main_pointer_button_intent(state, mouse_button, self.main_pointer_button_snapshot());
        let effect =
            self.dispatch_main_pointer_button_intent(event_loop, intent, state, point, size);
        self.apply_action_effect(event_loop, effect);
    }

    fn main_pointer_button_snapshot(&self) -> MainPointerButtonSnapshot {
        MainPointerButtonSnapshot::default()
    }

    fn main_pointer_move_snapshot(&self) -> MainPointerMoveSnapshot {
        MainPointerMoveSnapshot::default()
    }

    fn dispatch_main_pointer_move_intent(
        &mut self,
        intent: MainPointerMoveIntent,
        point: crate::ViewPoint,
        size: PhysicalSize<u32>,
    ) -> ShellActionOutcome {
        match intent {
            MainPointerMoveIntent::TaskDetailModal => {
                self.set_window_cursor(CursorIcon::Default);
                ShellActionOutcome::None
            }
            MainPointerMoveIntent::ScenePointer => {
                let role_update = self.scene.scrollbar_drag_visible_role_update_kind();
                let changed = self.scene.set_pointer(point, size);
                if changed && let Some(kind) = role_update {
                    self.visible_role_updates.schedule(kind, Instant::now());
                }
                self.update_window_cursor_for_scene(size);
                // Hover (and other pointer-driven motion) may start hover ease;
                // queue the registry budget so presentation survives until the
                // animation runtime is polled again in about_to_wait.
                ShellActionOutcome::redraw_if(changed)
                    .with_animation_if(changed, ShellAnimationKind::Hover)
            }
        }
    }

    fn dispatch_main_pointer_button_intent(
        &mut self,
        event_loop: &ActiveEventLoop,
        intent: MainPointerButtonIntent,
        state: ElementState,
        point: crate::ViewPoint,
        size: PhysicalSize<u32>,
    ) -> ShellActionEffect {
        match intent {
            MainPointerButtonIntent::TrashConflict
            | MainPointerButtonIntent::TaskDetail
            | MainPointerButtonIntent::PropertiesOverlay => ShellActionOutcome::None.into(),
            MainPointerButtonIntent::MouseNavigation(action) => {
                self.perform_path_navigation(event_loop, action);
                ShellActionOutcome::None.into()
            }
            MainPointerButtonIntent::ContextMenu => {
                let changed =
                    self.scene
                        .open_context_menu_with_cache(point, size, &self.mime_applications);
                ShellActionOutcome::redraw_if(changed).into()
            }
            MainPointerButtonIntent::PrimaryPaste => {
                // Middle-click: async primary selection paste (same worker path
                // as Ctrl+V; never blocks the event loop on pipe I/O).
                self.load_primary_text_for_paste();
                ShellActionOutcome::None.into()
            }
            MainPointerButtonIntent::Left => {
                let route = main_left_pointer_button_route(
                    state,
                    self.main_left_pointer_button_snapshot(point, size),
                );
                self.apply_main_left_pointer_button_route(event_loop, state, point, size, route)
            }
            MainPointerButtonIntent::Ignore => ShellActionOutcome::None.into(),
        }
    }

    fn main_left_pointer_button_snapshot(
        &self,
        point: crate::ViewPoint,
        size: PhysicalSize<u32>,
    ) -> MainLeftPointerButtonRouteSnapshot {
        MainLeftPointerButtonRouteSnapshot {
            scrollbar_dragging: self.scene.is_scrollbar_dragging(),
            context_menu_open: self.scene.is_context_menu_open(),
            drop_menu_open: self.scene.is_drop_menu_open(),
            task_detail_area_hit: self
                .scene
                .task_detail_area_contains_screen_point(point, size),
            overflow_button_hit: self
                .scene
                .overflow_button_contains_screen_point(point, size),
            split_view_button_hit: self.scene.split_view_button_at_screen_point(point, size),
            places_toggle_hit: self.scene.places_toggle_contains_screen_point(point, size),
            scrollbar_drag_hit: self.scene.scrollbar_drag_hit_at_screen_point(point, size),
            path_bar_hit: self.scene.path_bar_contains_screen_point(point, size),
            toolbar_navigation: self
                .scene
                .path_navigation_action_at_screen_point(point, size),
            view_mode: self.scene.view_mode_at_screen_point(point, size),
            place_pointer_target_hit: self.scene.place_pointer_target_at_screen_point(point, size),
            place_pointer_active: self.scene.place_pointer_active(),
        }
    }

    fn apply_main_left_pointer_button_route(
        &mut self,
        event_loop: &ActiveEventLoop,
        state: ElementState,
        point: crate::ViewPoint,
        size: PhysicalSize<u32>,
        route: MainLeftPointerButtonRoute,
    ) -> ShellActionEffect {
        let location_blur_changed = if route.should_blur_location(state) {
            self.scene.close_location_draft_if_outside(point, size)
        } else {
            false
        };

        match route.intent {
            MainLeftPointerButtonIntent::EndScrollbarDrag => {
                let role_update = self.scene.scrollbar_drag_visible_role_update_kind();
                let changed = self.scene.end_scrollbar_drag(point, size);
                if changed && let Some(kind) = role_update {
                    self.visible_role_updates.schedule(kind, Instant::now());
                }
                self.update_window_cursor_for_scene(size);
                ShellActionOutcome::redraw_if(changed).into()
            }
            MainLeftPointerButtonIntent::ContextMenu => {
                self.activate_or_close_context_menu(event_loop, point, size)
            }
            MainLeftPointerButtonIntent::DropMenu => self.activate_or_close_drop_menu(point, size),
            MainLeftPointerButtonIntent::OpenTaskDetail => {
                let changed = self
                    .scene
                    .open_task_detail_dialog_at_screen_point(point, size)
                    .unwrap_or(false);
                if changed {
                    self.ensure_task_detail_dialog_window(event_loop);
                }
                ShellActionOutcome::redraw_if(changed)
                    .with_redraw_if(location_blur_changed)
                    .into()
            }
            MainLeftPointerButtonIntent::ToggleSplitView => {
                self.toggle_split_view_from_toolbar(event_loop);
                ShellActionOutcome::None.into()
            }
            MainLeftPointerButtonIntent::OpenSettings => {
                self.ensure_settings_dialog_window(event_loop);
                ShellActionOutcome::redraw_if(location_blur_changed).into()
            }
            MainLeftPointerButtonIntent::TogglePlaces => {
                let changed = self.toggle_user_places_visibility(size);
                self.update_window_cursor_for_scene(size);
                ShellActionOutcome::redraw_if(changed)
                    .with_redraw_if(location_blur_changed)
                    .into()
            }
            MainLeftPointerButtonIntent::BeginScrollbarDrag => {
                let zoom_changes = self.scene.zoom_changes;
                let changed = self
                    .scene
                    .begin_scrollbar_drag(point, size)
                    .unwrap_or(false);
                if changed && let Some(kind) = self.scene.scrollbar_drag_visible_role_update_kind()
                {
                    self.visible_role_updates.schedule(kind, Instant::now());
                } else if self.scene.zoom_changes != zoom_changes {
                    self.visible_role_updates.schedule(
                        crate::ui::prewarm::VisibleRoleUpdateKind::IconSize,
                        Instant::now(),
                    );
                }
                self.update_window_cursor_for_scene(size);
                ShellActionOutcome::redraw_if(changed)
                    .with_redraw_if(location_blur_changed)
                    .into()
            }
            MainLeftPointerButtonIntent::PathBar => {
                let changed = self.scene.activate_path_bar_at_screen_point(point, size);
                ShellActionOutcome::redraw_if(changed).into()
            }
            MainLeftPointerButtonIntent::ToolbarNavigation(action) => {
                self.perform_path_navigation(event_loop, action);
                ShellActionOutcome::None.into()
            }
            MainLeftPointerButtonIntent::ViewMode(view_mode) => {
                let changed = self.set_user_view_mode(view_mode, size);
                ShellActionOutcome::present_if(changed, "mode-click").into()
            }
            MainLeftPointerButtonIntent::BeginPlacePointer => {
                self.reset_outgoing_drag_tracking();
                let changed = self.scene.begin_place_pointer(point, size).unwrap_or(false);
                ShellActionOutcome::redraw_if(changed)
                    .with_redraw_if(location_blur_changed)
                    .into()
            }
            MainLeftPointerButtonIntent::EndPlacePointer => {
                self.end_place_pointer(event_loop, point, size, location_blur_changed)
            }
            MainLeftPointerButtonIntent::ItemActivationCheck => {
                if let Some(activation) =
                    self.scene
                        .item_activation_for_press(point, size, Instant::now())
                {
                    return self.perform_item_activation(activation);
                }
                self.apply_pane_pointer(state, point, size, location_blur_changed)
                    .into()
            }
            MainLeftPointerButtonIntent::PanePointer => self
                .apply_pane_pointer(state, point, size, location_blur_changed)
                .into(),
        }
    }

    fn apply_pane_pointer(
        &mut self,
        state: ElementState,
        point: crate::ViewPoint,
        size: PhysicalSize<u32>,
        location_blur_changed: bool,
    ) -> ShellActionOutcome {
        let changed = match state {
            ElementState::Pressed => {
                self.reset_outgoing_drag_tracking();
                let selection = SelectionClick {
                    point,
                    extend: self.modifiers.state().shift_key(),
                    toggle: self.modifiers.state().control_key()
                        || self.modifiers.state().meta_key(),
                };
                self.scene.begin_pane_pointer(selection, size)
            }
            ElementState::Released if self.outgoing_dnd_transfer.is_some() => false,
            ElementState::Released => self.scene.end_pane_pointer(point, size),
        };
        self.update_window_cursor_for_scene(size);
        ShellActionOutcome::redraw_if(changed).with_redraw_if(location_blur_changed)
    }

    fn activate_or_close_context_menu(
        &mut self,
        event_loop: &ActiveEventLoop,
        point: crate::ViewPoint,
        size: PhysicalSize<u32>,
    ) -> ShellActionEffect {
        let action = self
            .scene
            .activate_or_close_context_menu_command(point, size);
        if let Some(action) = action {
            self.perform_context_menu_action(event_loop, action);
            ShellActionOutcome::None.into()
        } else {
            ShellActionOutcome::Redraw.into()
        }
    }

    fn activate_or_close_drop_menu(
        &mut self,
        point: crate::ViewPoint,
        size: PhysicalSize<u32>,
    ) -> ShellActionEffect {
        let request = self.scene.activate_or_close_drop_menu_request(point, size);
        if let Some(request) = request {
            self.perform_drop_operation_request(request).into()
        } else {
            ShellActionOutcome::Redraw.into()
        }
    }

    fn end_place_pointer(
        &mut self,
        event_loop: &ActiveEventLoop,
        point: crate::ViewPoint,
        size: PhysicalSize<u32>,
        location_blur_changed: bool,
    ) -> ShellActionEffect {
        if self.outgoing_dnd_transfer.is_some() {
            return ShellActionOutcome::redraw_if(location_blur_changed).into();
        }
        let (changed, activation) = self.scene.end_place_pointer(point, size);
        if let Some(activation) = activation {
            return match activation {
                ShellPlaceActivation::Open { pane, path } => {
                    ShellActionEffect::load_path(pane, path, "place-open")
                }
                ShellPlaceActivation::DeviceAction(request) => {
                    self.perform_device_action_request(event_loop, request);
                    ShellActionOutcome::None.into()
                }
            };
        }
        ShellActionOutcome::redraw_if(changed)
            .with_redraw_if(location_blur_changed)
            .into()
    }

    fn perform_item_activation(&mut self, activation: ShellItemActivation) -> ShellActionEffect {
        match activation {
            ShellItemActivation::Directory { pane, path } => {
                ShellActionEffect::load_path(pane, path, "double-click-directory")
            }
            ShellItemActivation::File(request) => {
                self.launch_open_file_request(&request);
                ShellActionOutcome::None.into()
            }
        }
    }
}
