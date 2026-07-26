use crate::platform::{ElementState, MouseButton};

use crate::shell::options::ShellViewMode;
use crate::shell::shortcuts::{PathNavigationAction, path_navigation_action_for_mouse_button};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct MainPointerMoveSnapshot {
    pub(super) task_detail_dialog_open: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MainPointerMoveIntent {
    TaskDetailModal,
    ScenePointer,
}

pub(super) fn main_pointer_move_intent(snapshot: MainPointerMoveSnapshot) -> MainPointerMoveIntent {
    if snapshot.task_detail_dialog_open {
        MainPointerMoveIntent::TaskDetailModal
    } else {
        MainPointerMoveIntent::ScenePointer
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct MainPointerButtonSnapshot {
    pub(super) trash_conflict_dialog_open: bool,
    pub(super) task_detail_dialog_open: bool,
    pub(super) properties_overlay_open: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MainPointerButtonIntent {
    TrashConflict,
    TaskDetail,
    PropertiesOverlay,
    MouseNavigation(PathNavigationAction),
    ContextMenu,
    /// Middle button: primary selection paste (when protocol available).
    PrimaryPaste,
    Left,
    Ignore,
}

pub(super) fn main_pointer_button_intent(
    state: ElementState,
    mouse_button: MouseButton,
    snapshot: MainPointerButtonSnapshot,
) -> MainPointerButtonIntent {
    if snapshot.trash_conflict_dialog_open {
        return MainPointerButtonIntent::TrashConflict;
    }
    if snapshot.task_detail_dialog_open {
        return MainPointerButtonIntent::TaskDetail;
    }
    if snapshot.properties_overlay_open {
        return MainPointerButtonIntent::PropertiesOverlay;
    }
    if state == ElementState::Pressed
        && let Some(action) = path_navigation_action_for_mouse_button(mouse_button)
    {
        return MainPointerButtonIntent::MouseNavigation(action);
    }
    if mouse_button == MouseButton::Right {
        return if state == ElementState::Pressed {
            MainPointerButtonIntent::ContextMenu
        } else {
            MainPointerButtonIntent::Ignore
        };
    }
    if mouse_button == MouseButton::Middle {
        return if state == ElementState::Pressed {
            MainPointerButtonIntent::PrimaryPaste
        } else {
            MainPointerButtonIntent::Ignore
        };
    }
    if mouse_button == MouseButton::Left {
        MainPointerButtonIntent::Left
    } else {
        MainPointerButtonIntent::Ignore
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct MainLeftPointerButtonRouteSnapshot {
    pub(super) scrollbar_dragging: bool,
    pub(super) context_menu_open: bool,
    pub(super) drop_menu_open: bool,
    pub(super) task_detail_area_hit: bool,
    pub(super) overflow_button_hit: bool,
    pub(super) split_view_button_hit: bool,
    pub(super) places_toggle_hit: bool,
    pub(super) scrollbar_drag_hit: bool,
    pub(super) path_bar_hit: bool,
    pub(super) toolbar_navigation: Option<PathNavigationAction>,
    pub(super) view_mode: Option<ShellViewMode>,
    pub(super) place_pointer_target_hit: bool,
    pub(super) place_pointer_active: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MainLeftPointerButtonRoute {
    pub(super) intent: MainLeftPointerButtonIntent,
    path_bar_hit: bool,
}

impl MainLeftPointerButtonRoute {
    pub(super) fn should_blur_location(self, state: ElementState) -> bool {
        state == ElementState::Pressed && !self.path_bar_hit
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MainLeftPointerButtonIntent {
    EndScrollbarDrag,
    ContextMenu,
    DropMenu,
    OpenTaskDetail,
    OpenSettings,
    ToggleSplitView,
    TogglePlaces,
    BeginScrollbarDrag,
    PathBar,
    ToolbarNavigation(PathNavigationAction),
    ViewMode(ShellViewMode),
    BeginPlacePointer,
    EndPlacePointer,
    ItemActivationCheck,
    PanePointer,
}

pub(super) fn main_left_pointer_button_route(
    state: ElementState,
    snapshot: MainLeftPointerButtonRouteSnapshot,
) -> MainLeftPointerButtonRoute {
    let path_bar_hit = state == ElementState::Pressed && snapshot.path_bar_hit;
    let intent = if state == ElementState::Released && snapshot.scrollbar_dragging {
        MainLeftPointerButtonIntent::EndScrollbarDrag
    } else if state == ElementState::Pressed && snapshot.context_menu_open {
        MainLeftPointerButtonIntent::ContextMenu
    } else if state == ElementState::Pressed && snapshot.drop_menu_open {
        MainLeftPointerButtonIntent::DropMenu
    } else if state == ElementState::Pressed && snapshot.task_detail_area_hit {
        MainLeftPointerButtonIntent::OpenTaskDetail
    } else if state == ElementState::Pressed && snapshot.overflow_button_hit {
        MainLeftPointerButtonIntent::OpenSettings
    } else if state == ElementState::Pressed && snapshot.split_view_button_hit {
        MainLeftPointerButtonIntent::ToggleSplitView
    } else if state == ElementState::Pressed && snapshot.places_toggle_hit {
        MainLeftPointerButtonIntent::TogglePlaces
    } else if state == ElementState::Pressed && snapshot.scrollbar_drag_hit {
        MainLeftPointerButtonIntent::BeginScrollbarDrag
    } else if path_bar_hit {
        MainLeftPointerButtonIntent::PathBar
    } else if state == ElementState::Pressed
        && let Some(action) = snapshot.toolbar_navigation
    {
        MainLeftPointerButtonIntent::ToolbarNavigation(action)
    } else if state == ElementState::Pressed
        && let Some(view_mode) = snapshot.view_mode
    {
        MainLeftPointerButtonIntent::ViewMode(view_mode)
    } else if state == ElementState::Pressed && snapshot.place_pointer_target_hit {
        MainLeftPointerButtonIntent::BeginPlacePointer
    } else if state == ElementState::Released && snapshot.place_pointer_active {
        MainLeftPointerButtonIntent::EndPlacePointer
    } else if state == ElementState::Pressed {
        MainLeftPointerButtonIntent::ItemActivationCheck
    } else {
        MainLeftPointerButtonIntent::PanePointer
    };
    MainLeftPointerButtonRoute {
        intent,
        path_bar_hit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_move_is_blocked_by_task_detail_modal() {
        assert_eq!(
            main_pointer_move_intent(MainPointerMoveSnapshot {
                task_detail_dialog_open: true,
            }),
            MainPointerMoveIntent::TaskDetailModal
        );
        assert_eq!(
            main_pointer_move_intent(MainPointerMoveSnapshot {
                task_detail_dialog_open: false,
            }),
            MainPointerMoveIntent::ScenePointer
        );
    }

    fn left_route(
        state: ElementState,
        snapshot: MainLeftPointerButtonRouteSnapshot,
    ) -> MainLeftPointerButtonIntent {
        main_left_pointer_button_route(state, snapshot).intent
    }

    #[test]
    fn main_pointer_modal_dialogs_precede_mouse_and_context_buttons() {
        let snapshot = MainPointerButtonSnapshot {
            trash_conflict_dialog_open: true,
            task_detail_dialog_open: true,
            properties_overlay_open: true,
        };
        assert_eq!(
            main_pointer_button_intent(ElementState::Pressed, MouseButton::Back, snapshot),
            MainPointerButtonIntent::TrashConflict
        );

        let snapshot = MainPointerButtonSnapshot {
            task_detail_dialog_open: true,
            properties_overlay_open: true,
            ..MainPointerButtonSnapshot::default()
        };
        assert_eq!(
            main_pointer_button_intent(ElementState::Pressed, MouseButton::Right, snapshot),
            MainPointerButtonIntent::TaskDetail
        );

        let snapshot = MainPointerButtonSnapshot {
            properties_overlay_open: true,
            ..MainPointerButtonSnapshot::default()
        };
        assert_eq!(
            main_pointer_button_intent(ElementState::Pressed, MouseButton::Back, snapshot),
            MainPointerButtonIntent::PropertiesOverlay
        );
    }

    #[test]
    fn main_pointer_mouse_navigation_precedes_button_specific_routes() {
        assert_eq!(
            main_pointer_button_intent(
                ElementState::Pressed,
                MouseButton::Back,
                MainPointerButtonSnapshot::default(),
            ),
            MainPointerButtonIntent::MouseNavigation(PathNavigationAction::Back)
        );
        assert_eq!(
            main_pointer_button_intent(
                ElementState::Pressed,
                MouseButton::Forward,
                MainPointerButtonSnapshot::default(),
            ),
            MainPointerButtonIntent::MouseNavigation(PathNavigationAction::Forward)
        );
        assert_eq!(
            main_pointer_button_intent(
                ElementState::Pressed,
                MouseButton::Right,
                MainPointerButtonSnapshot::default(),
            ),
            MainPointerButtonIntent::ContextMenu
        );
        assert_eq!(
            main_pointer_button_intent(
                ElementState::Released,
                MouseButton::Right,
                MainPointerButtonSnapshot::default(),
            ),
            MainPointerButtonIntent::Ignore
        );
        assert_eq!(
            main_pointer_button_intent(
                ElementState::Pressed,
                MouseButton::Left,
                MainPointerButtonSnapshot::default(),
            ),
            MainPointerButtonIntent::Left
        );
    }

    #[test]
    fn left_pointer_pressed_route_priority_is_stable() {
        let all_hits = MainLeftPointerButtonRouteSnapshot {
            context_menu_open: true,
            drop_menu_open: true,
            task_detail_area_hit: true,
            overflow_button_hit: true,
            split_view_button_hit: true,
            places_toggle_hit: true,
            scrollbar_drag_hit: true,
            path_bar_hit: true,
            toolbar_navigation: Some(PathNavigationAction::Parent),
            view_mode: Some(ShellViewMode::Details),
            place_pointer_target_hit: true,
            ..MainLeftPointerButtonRouteSnapshot::default()
        };
        assert_eq!(
            left_route(ElementState::Pressed, all_hits),
            MainLeftPointerButtonIntent::ContextMenu
        );

        let drop_hits = MainLeftPointerButtonRouteSnapshot {
            context_menu_open: false,
            ..all_hits
        };
        assert_eq!(
            left_route(ElementState::Pressed, drop_hits),
            MainLeftPointerButtonIntent::DropMenu
        );

        let task_hits = MainLeftPointerButtonRouteSnapshot {
            drop_menu_open: false,
            ..drop_hits
        };
        assert_eq!(
            left_route(ElementState::Pressed, task_hits),
            MainLeftPointerButtonIntent::OpenTaskDetail
        );

        let overflow_button_hits = MainLeftPointerButtonRouteSnapshot {
            task_detail_area_hit: false,
            ..task_hits
        };
        assert_eq!(
            left_route(ElementState::Pressed, overflow_button_hits),
            MainLeftPointerButtonIntent::OpenSettings
        );

        let split_hits = MainLeftPointerButtonRouteSnapshot {
            overflow_button_hit: false,
            ..overflow_button_hits
        };
        assert_eq!(
            left_route(ElementState::Pressed, split_hits),
            MainLeftPointerButtonIntent::ToggleSplitView
        );

        let places_hits = MainLeftPointerButtonRouteSnapshot {
            split_view_button_hit: false,
            ..split_hits
        };
        assert_eq!(
            left_route(ElementState::Pressed, places_hits),
            MainLeftPointerButtonIntent::TogglePlaces
        );

        let scrollbar_hits = MainLeftPointerButtonRouteSnapshot {
            places_toggle_hit: false,
            ..places_hits
        };
        assert_eq!(
            left_route(ElementState::Pressed, scrollbar_hits),
            MainLeftPointerButtonIntent::BeginScrollbarDrag
        );

        let path_hits = MainLeftPointerButtonRouteSnapshot {
            scrollbar_drag_hit: false,
            ..scrollbar_hits
        };
        assert_eq!(
            left_route(ElementState::Pressed, path_hits),
            MainLeftPointerButtonIntent::PathBar
        );
    }

    #[test]
    fn left_pointer_pressed_falls_through_toolbar_view_place_and_item() {
        assert_eq!(
            left_route(
                ElementState::Pressed,
                MainLeftPointerButtonRouteSnapshot {
                    toolbar_navigation: Some(PathNavigationAction::Parent),
                    view_mode: Some(ShellViewMode::Details),
                    place_pointer_target_hit: true,
                    ..MainLeftPointerButtonRouteSnapshot::default()
                },
            ),
            MainLeftPointerButtonIntent::ToolbarNavigation(PathNavigationAction::Parent)
        );
        assert_eq!(
            left_route(
                ElementState::Pressed,
                MainLeftPointerButtonRouteSnapshot {
                    view_mode: Some(ShellViewMode::Details),
                    place_pointer_target_hit: true,
                    ..MainLeftPointerButtonRouteSnapshot::default()
                },
            ),
            MainLeftPointerButtonIntent::ViewMode(ShellViewMode::Details)
        );
        assert_eq!(
            left_route(
                ElementState::Pressed,
                MainLeftPointerButtonRouteSnapshot {
                    place_pointer_target_hit: true,
                    ..MainLeftPointerButtonRouteSnapshot::default()
                },
            ),
            MainLeftPointerButtonIntent::BeginPlacePointer
        );
        assert_eq!(
            left_route(
                ElementState::Pressed,
                MainLeftPointerButtonRouteSnapshot::default(),
            ),
            MainLeftPointerButtonIntent::ItemActivationCheck
        );
    }

    #[test]
    fn left_pointer_released_prefers_drag_and_place_release_before_pane() {
        assert_eq!(
            left_route(
                ElementState::Released,
                MainLeftPointerButtonRouteSnapshot {
                    scrollbar_dragging: true,
                    place_pointer_active: true,
                    ..MainLeftPointerButtonRouteSnapshot::default()
                },
            ),
            MainLeftPointerButtonIntent::EndScrollbarDrag
        );
        assert_eq!(
            left_route(
                ElementState::Released,
                MainLeftPointerButtonRouteSnapshot {
                    place_pointer_active: true,
                    ..MainLeftPointerButtonRouteSnapshot::default()
                },
            ),
            MainLeftPointerButtonIntent::EndPlacePointer
        );
        assert_eq!(
            left_route(
                ElementState::Released,
                MainLeftPointerButtonRouteSnapshot::default(),
            ),
            MainLeftPointerButtonIntent::PanePointer
        );
    }

    #[test]
    fn path_bar_press_does_not_blur_location() {
        let route = main_left_pointer_button_route(
            ElementState::Pressed,
            MainLeftPointerButtonRouteSnapshot {
                path_bar_hit: true,
                ..MainLeftPointerButtonRouteSnapshot::default()
            },
        );
        assert_eq!(route.intent, MainLeftPointerButtonIntent::PathBar);
        assert!(!route.should_blur_location(ElementState::Pressed));

        let route = main_left_pointer_button_route(
            ElementState::Pressed,
            MainLeftPointerButtonRouteSnapshot::default(),
        );
        assert!(route.should_blur_location(ElementState::Pressed));
        assert!(!route.should_blur_location(ElementState::Released));
    }
}
