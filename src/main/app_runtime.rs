struct FikaWgpuApp {
    scene: ShellScene,
    mime_applications: MimeApplicationCache,
    settings_path: PathBuf,
    event_loop_proxy: EventLoopProxy,
    directory_watchers: ShellDirectoryWatcherRuntime,
    async_task_tx: Sender<ShellAsyncTaskResult>,
    async_task_rx: Receiver<ShellAsyncTaskResult>,
    navigation_generations: [u64; 2],
    active_task_controllers: HashMap<ShellTaskId, OperationController>,
    active_task_base_details: HashMap<ShellTaskId, String>,
    next_task_id: ShellTaskId,
    modifiers: Modifiers,
    /// Active touchpad pinch session for icon/view zoom (main window).
    pinch_zoom: Option<PinchZoomTracker>,
    /// Active multi-finger swipe session for history navigation (main window).
    swipe_nav: Option<SwipeNavigationTracker>,
    incoming_dnd_transfer: Option<IncomingDndTransfer>,
    outgoing_dnd_transfer: Option<OutgoingDndTransfer>,
    outgoing_dnd_start_failed: bool,
    // Drop order matters: renderer owns a surface tied to the window handle.
    renderer: Option<WgpuState>,
    dialog_windows: ShellDialogWindows,
    text_input: FikaTextInputRuntime,
    settings_dialog: ShellSettingsDialogState,
    clipboard: Option<ShellClipboard>,
    window: Option<Arc<WaylandWindow>>,
    cursor_icon: CursorIcon,
    pending_redraw_frames: u8,
    pending_render_reason: Option<&'static str>,
    next_animation_redraw: Option<Instant>,
    last_location_text_caret_dirty_value: u64,
    last_open_with_text_caret_dirty_value: u64,
    auto_cycle_views: bool,
    next_auto_cycle: Instant,
    autosmoke_zoom_actions: VecDeque<ZoomAction>,
    next_autosmoke_zoom: Instant,
    autosmoke_zoom_interval: Duration,
    autosmoke_zoom_allow_pending_redraw: bool,
    autosmoke_scroll_actions: VecDeque<AutosmokeScrollAction>,
    next_autosmoke_scroll: Instant,
    autosmoke_scroll_interval: Duration,
    autosmoke_scroll_allow_pending_redraw: bool,
    dialog_lifecycle_smoke: Option<DialogLifecycleSmoke>,
}
#[derive(Clone, Debug)]
struct IncomingDndTransfer {
    id: DataTransferId,
    fetch_serial: Option<AsyncRequestSerial>,
    paths: Option<Vec<PathBuf>>,
    data_received: bool,
    local_source: Option<ShellInternalDragSource>,
    last_position: Option<PhysicalPosition<f64>>,
    drop_pending: bool,
}
impl IncomingDndTransfer {
    fn new(
        id: DataTransferId,
        position: Option<PhysicalPosition<f64>>,
        paths: Option<Vec<PathBuf>>,
        local_source: Option<ShellInternalDragSource>,
    ) -> Self {
        Self {
            id,
            fetch_serial: None,
            paths,
            data_received: false,
            local_source,
            last_position: position,
            drop_pending: false,
        }
    }
}
#[derive(Clone, Debug)]
struct OutgoingDndTransfer {
    id: DataTransferId,
    paths: Vec<PathBuf>,
    source: ShellInternalDragSource,
}
include!("app_controller/window_lifecycle.rs");
include!("app_controller/dialog_windows.rs");
include!("app_controller/text_input.rs");
include!("app_controller/async_operations.rs");
include!("app_controller/async_tasks.rs");
include!("app_controller/settings_window.rs");
impl ApplicationHandler for FikaWgpuApp {
    fn proxy_wake_up(&mut self, event_loop: &ActiveEventLoop) {
        self.drive_directory_watchers(event_loop);
        if let Some(deadline) = self.directory_watchers.next_reload_deadline() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }

    fn dmabuf_feedback_updated(
        &mut self,
        event_loop: &ActiveEventLoop,
        surface: Option<WindowId>,
    ) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        // Prefer the surface from the event; fall back to main window.
        let surface = surface.or_else(|| self.window.as_ref().map(|w| w.id()));
        renderer.sync_icon_dmabuf_plan(event_loop, surface);
        renderer.log_dmabuf_readiness(event_loop, surface, "feedback");
    }

    fn can_create_surfaces(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title(window_title(&self.scene))
            .with_transparent(true)
            .with_surface_size(PhysicalSize::new(1100, 720));
        let attrs = apply_window_platform_semantics(event_loop, attrs, ShellWindowRole::Main);

        let window = match event_loop.create_window(attrs) {
            Ok(window) => window,
            Err(error) => {
                fika_log!("[fika-wgpu] window create failed: {error}");
                self.exit_event_loop(event_loop, "main-window-create-failed");
                return;
            }
        };

        window.set_blur(self.scene.background_blur);
        let clipboard = ShellClipboard::new(event_loop);
        fika_log!(
            "[fika-wgpu] clipboard-ready backend={}",
            clipboard.backend()
        );
        self.clipboard = Some(clipboard);
        self.window = Some(window);

        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.drive_directory_watchers(event_loop);
        self.drain_async_task_results(event_loop);
        self.sync_text_input_states();
        let progress_changed = self.refresh_active_task_progress();
        let animation_pruned = self.scene.prune_finished_animations();
        if (progress_changed || animation_pruned)
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
        }
        if progress_changed {
            self.request_dialog_redraw(ShellDialogWindowKind::TaskDetail);
        }
        if self.auto_cycle_views && Instant::now() >= self.next_auto_cycle {
            self.next_auto_cycle = Instant::now() + AUTO_CYCLE_INTERVAL;
            if let Some(renderer) = self.renderer.as_ref() {
                let next = self.scene.active_view_mode().next();
                if self.scene.set_view_mode(next, renderer.size) {
                    self.pending_redraw_frames = VIEW_SWITCH_REDRAW_FRAMES;
                    if let Some(window) = self.window.as_ref() {
                        window.set_title(&window_title(&self.scene));
                        window.request_redraw();
                    }
                    self.prewarm_current_scene_caches("auto-cycle");
                    self.render_now(event_loop, "auto-cycle", true);
                }
            }
        }
        if let Some(renderer) = self.renderer.as_ref()
            && renderer.frame_count > 0
        {
            let size = renderer.size;
            self.drive_autosmoke_zoom(size);
            self.drive_autosmoke_scroll(size);
        }
        self.drive_dialog_lifecycle_autosmoke(event_loop);
        self.drain_dialog_window_deferred_closes();

        let now = Instant::now();
        let animation_active = self.scene.animation_active();
        let next_scene_animation_deadline = self.scene.next_animation_frame_deadline();
        let animation_redraw_due = schedule_animation_redraw(
            animation_active,
            &mut self.next_animation_redraw,
            now,
            next_scene_animation_deadline,
        );
        let next_text_caret_deadline = self.scene.next_text_caret_blink_deadline();
        let location_caret_active = self.scene.location_text_caret_active();
        let location_caret_dirty_value = self.scene.location_text_caret_dirty_value();
        let location_caret_blink_due = location_caret_active
            && location_caret_dirty_value != self.last_location_text_caret_dirty_value;
        self.last_location_text_caret_dirty_value = if location_caret_active {
            location_caret_dirty_value
        } else {
            0
        };
        let open_with_caret_active = self.scene.open_with_text_caret_active();
        let open_with_caret_dirty_value = self.scene.open_with_text_caret_dirty_value();
        let open_with_caret_blink_due = open_with_caret_active
            && open_with_caret_dirty_value != self.last_open_with_text_caret_dirty_value;
        self.last_open_with_text_caret_dirty_value = if open_with_caret_active {
            open_with_caret_dirty_value
        } else {
            0
        };
        let dialog_redraw_requested =
            open_with_caret_blink_due && self.request_open_with_dialog_redraw();
        let needs_redraw = self.renderer.as_ref().is_some_and(|renderer| {
            renderer.frame_count == 0
                || renderer.rendered_view_switches != self.scene.view_switches
                || self.pending_redraw_frames > 0
                || animation_redraw_due
                || location_caret_blink_due
        });
        let next_autosmoke_deadline = [
            (!self.autosmoke_zoom_actions.is_empty()).then_some(self.next_autosmoke_zoom),
            (!self.autosmoke_scroll_actions.is_empty()).then_some(self.next_autosmoke_scroll),
        ]
        .into_iter()
        .flatten()
        .min();
        let next_idle_deadline = [
            self.auto_cycle_views.then_some(self.next_auto_cycle),
            next_autosmoke_deadline,
            self.dialog_windows.next_deferred_close_deadline(),
            self.next_animation_redraw,
            next_scene_animation_deadline,
            next_text_caret_deadline,
            self.directory_watchers.next_reload_deadline(),
        ]
        .into_iter()
        .flatten()
        .min();

        if needs_redraw && let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }

        if needs_redraw || dialog_redraw_requested {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else if !self.active_task_controllers.is_empty() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(100),
            ));
        } else if let Some(deadline) = next_idle_deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.trace_window_event(window_id, &event);
        if let WindowEvent::Ime(event) = &event {
            self.handle_text_input_event(window_id, event.clone());
            return;
        }
        if let Some(kind) = self.dialog_windows.window_kind_for_id(window_id) {
            match kind {
                ShellDialogWindowKind::Create => {
                    self.create_dialog_window_event(event_loop, event);
                    return;
                }
                ShellDialogWindowKind::OpenWith => {
                    self.open_with_dialog_window_event(event_loop, event);
                    return;
                }
                ShellDialogWindowKind::Properties => {
                    self.properties_dialog_window_event(event_loop, event);
                    return;
                }
                ShellDialogWindowKind::Rename => {
                    self.rename_dialog_window_event(event_loop, event);
                    return;
                }
                ShellDialogWindowKind::Settings => {
                    self.settings_dialog_window_event(event_loop, event);
                    return;
                }
                ShellDialogWindowKind::TaskDetail => {
                    self.task_detail_dialog_window_event(event_loop, event);
                    return;
                }
                ShellDialogWindowKind::TrashConflict => {
                    self.trash_conflict_dialog_window_event(event_loop, event);
                    return;
                }
            }
        }
        let Some(main_window_id) = self.window.as_ref().map(|window| window.id()) else {
            return;
        };
        if main_window_id != window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.close_main_window_from_window_manager_request(
                    event_loop,
                    "main-close-requested",
                );
            }
            WindowEvent::SurfaceResized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    let previous_size = renderer.size;
                    let animate_reflow = renderer.frame_count > 0;
                    renderer.resize(size);
                    if animate_reflow {
                        self.scene
                            .reflow_pane_items_after_window_resize(previous_size, renderer.size);
                    } else {
                        self.scene.clamp_scroll(renderer.size);
                    }
                } else {
                    self.scene.clamp_scroll(size);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor } => {
                if let Some(window) = self.window.as_ref() {
                    let surface_size = window.surface_size();
                    if let Some(renderer) = self.renderer.as_mut() {
                        let previous_size = renderer.size;
                        let previous_rects = (renderer.frame_count > 0).then(|| {
                            self.scene
                                .visible_item_rects_by_path_for_open_panes(previous_size)
                        });
                        renderer.resize(surface_size);
                        let next_size = renderer.size;
                        let scale_changed = self
                            .scene
                            .set_scale_factor(scale_factor as f32, next_size);
                        if scale_changed || previous_size != next_size {
                            if let Some(previous_rects) = previous_rects {
                                self.scene.start_item_reflow_transitions_for_panes(
                                    previous_rects,
                                    next_size,
                                );
                            } else {
                                self.scene.clamp_scroll(next_size);
                            }
                        }
                    } else {
                        self.scene
                            .set_scale_factor(scale_factor as f32, surface_size);
                        self.scene.clamp_scroll(surface_size);
                    }
                    window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                self.handle_main_keyboard_input(event_loop, &event);
            }
            WindowEvent::PointerMoved { position, .. } => {
                self.handle_main_pointer_moved(event_loop, position);
            }
            WindowEvent::PointerLeft { .. } => {
                self.handle_main_pointer_left();
            }
            WindowEvent::DragEntered { id, position } => {
                let outcome = self.external_drag_entered(event_loop, id, position);
                self.apply_window_action_outcome(outcome);
            }
            WindowEvent::DragPosition { id, position, .. } => {
                let outcome = self.external_drag_position(event_loop, id, position);
                self.apply_window_action_outcome(outcome);
            }
            WindowEvent::DragDropped { id, .. } => {
                let outcome = self.external_drag_dropped(id);
                self.apply_window_action_outcome(outcome);
            }
            WindowEvent::DragLeft { id } => {
                let outcome = self.external_drag_left(id);
                self.apply_window_action_outcome(outcome);
            }
            WindowEvent::DataTransferReceived { id, serial, value } => {
                let outcome = self.external_drag_data_received(event_loop, id, serial, value);
                self.apply_window_action_outcome(outcome);
            }
            WindowEvent::OutgoingDragDropped { id, action } => {
                let outcome = self.outgoing_drag_dropped(id, action);
                self.apply_window_action_outcome(outcome);
            }
            WindowEvent::OutgoingDragCanceled { id } => {
                let outcome = self.outgoing_drag_canceled(id);
                self.apply_window_action_outcome(outcome);
            }
            WindowEvent::PointerButton {
                state,
                position,
                button,
                ..
            } => {
                self.handle_main_pointer_button(event_loop, state, position, button);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_main_mouse_wheel(delta);
            }
            WindowEvent::PinchGesture(gesture) => {
                self.handle_main_pinch_gesture(gesture);
            }
            WindowEvent::SwipeGesture(gesture) => {
                self.handle_main_swipe_gesture(event_loop, gesture);
            }
            WindowEvent::RedrawRequested => {
                let force_log = self.pending_redraw_frames > 0;
                let reason = self.pending_render_reason.take().unwrap_or(if force_log {
                    "switch-redraw"
                } else {
                    "redraw"
                });
                self.render_now(event_loop, reason, force_log);
            }
            _ => {}
        }
    }
}

fn schedule_animation_redraw(
    active: bool,
    scheduled: &mut Option<Instant>,
    now: Instant,
    next_deadline: Option<Instant>,
) -> bool {
    if !active {
        *scheduled = None;
        return false;
    }
    if scheduled.is_some_and(|deadline| deadline > now) {
        return false;
    }
    *scheduled = Some(next_deadline.unwrap_or(now + Duration::from_millis(16)));
    true
}
