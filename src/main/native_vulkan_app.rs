use std::{
    sync::{
        Arc,
        mpsc::{Receiver, Sender},
    },
    time::{Duration, Instant},
};

use crate::app_actions::apply_navigation_completion;
use crate::shell::window_semantics::{ShellWindowRole, apply_window_semantics};
use crate::shell::{
    selection::SelectionClick,
    shortcuts::{
        PathNavigationAction, is_activation_key, navigation_action_for_key,
        path_navigation_action_for_key,
    },
    transfer::{ShellAsyncNavigationCompletion, ShellNavigationHistoryUpdate},
};
use crate::vulkan_state::VulkanState;
use crate::windowing::{
    ActiveEventLoop, ApplicationHandler, ButtonSource, ControlFlow, ElementState, EventLoopProxy,
    Modifiers, MouseButton, MouseScrollDelta, PhysicalPosition, PhysicalSize, Window,
    WindowAttributes, WindowEvent, WindowId,
};
use crate::{
    FolderPreviewCacheStats, IconEngine, IconFrameBuilder, IconFrameConfig, IconFrameResources,
    ShellItemActivation, ShellScene, TextEngine, TextFrameBuilder, TextFrameResources,
    read_shell_entries_sync, scroll_delta_xy, view_point_from_physical_position, window_title,
};

/// Native Vulkan host for analytic chrome, sampled resident icons, and R8 text.
///
/// This deliberately creates no wgpu object. It is selected only by
/// `FIKA_VULKAN_RENDERER=1`; unsupported SVG/composite source generation stays
/// encoded until its corresponding Vulkan pipeline is available.
pub(crate) struct FikaNativeVulkanApp {
    scene: ShellScene,
    event_loop_proxy: EventLoopProxy,
    navigation_tx: Sender<ShellAsyncNavigationCompletion>,
    navigation_rx: Receiver<ShellAsyncNavigationCompletion>,
    navigation_generations: [u64; 2],
    modifiers: Modifiers,
    icon_engine: IconEngine,
    text_engine: TextEngine,
    // Drop before the Wayland window because its swapchain retains the surface.
    renderer: Option<VulkanState>,
    window: Option<Arc<Window>>,
}

impl FikaNativeVulkanApp {
    pub(crate) fn new(scene: ShellScene, event_loop_proxy: EventLoopProxy) -> Self {
        let (navigation_tx, navigation_rx) = std::sync::mpsc::channel();
        Self {
            scene,
            event_loop_proxy,
            navigation_tx,
            navigation_rx,
            navigation_generations: [0; 2],
            modifiers: Modifiers::default(),
            icon_engine: IconEngine::new(),
            text_engine: TextEngine::new(),
            renderer: None,
            window: None,
        }
    }

    fn create_window_and_renderer(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let attributes = WindowAttributes::default()
            .with_title(window_title(&self.scene))
            .with_transparent(true)
            .with_surface_size(PhysicalSize::new(1100, 720));
        let attributes = apply_window_semantics(event_loop, attributes, ShellWindowRole::Main);
        let window = event_loop
            .create_window(attributes)
            .map_err(|error| format!("create native Vulkan window: {error}"))?;
        window.set_blur(self.scene.background_blur);
        let renderer = VulkanState::new(Arc::clone(&window))?;
        let size = renderer.size();
        self.scene
            .set_scale_factor(window.scale_factor() as f32, size);
        self.scene.clamp_scroll(size);
        self.renderer = Some(renderer);
        self.window = Some(window);
        Ok(())
    }

    fn renderer_size(&self) -> Option<PhysicalSize<u32>> {
        self.renderer.as_ref().map(VulkanState::size)
    }

    fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn sync_window_title(&self) {
        if let Some(window) = self.window.as_ref() {
            window.set_title(&window_title(&self.scene));
        }
    }

    fn queue_navigation(
        &mut self,
        pane: crate::ShellPaneId,
        target_path: std::path::PathBuf,
        history: ShellNavigationHistoryUpdate,
        reason: &'static str,
    ) {
        let Some(size) = self.renderer_size() else {
            return;
        };
        let pane = self.scene.normalized_pane_id(pane);
        let Some(source_path) = self.scene.pane_state(pane).map(|state| state.path.clone()) else {
            return;
        };
        if source_path == target_path {
            if self.scene.cancel_pane_navigation(pane) {
                self.navigation_generations[pane.index()] =
                    self.navigation_generations[pane.index()].wrapping_add(1);
                self.sync_window_title();
                self.request_redraw();
            }
            return;
        }
        if self
            .scene
            .pending_pane_navigation_matches(pane, &target_path)
            || !self
                .scene
                .begin_pane_navigation(pane, target_path.clone(), size)
        {
            return;
        }

        let generation = self.navigation_generations[pane.index()].wrapping_add(1);
        self.navigation_generations[pane.index()] = generation;
        let completion_tx = self.navigation_tx.clone();
        let event_loop_proxy = self.event_loop_proxy.clone();
        let listing_target = target_path.clone();
        let spawn = std::thread::Builder::new()
            .name("fika-vulkan-directory-list".to_string())
            .spawn(move || {
                let result = read_shell_entries_sync(&listing_target);
                let _ = completion_tx.send(ShellAsyncNavigationCompletion {
                    generation,
                    pane,
                    source_path,
                    target_path,
                    history,
                    reason,
                    result,
                });
                event_loop_proxy.wake_up();
            });
        if let Err(error) = spawn {
            let _ = self.scene.cancel_pane_navigation(pane);
            eprintln!("[fika-vulkan] start directory listing failed: {error}");
        }
        self.sync_window_title();
        self.request_redraw();
    }

    fn perform_path_navigation(&mut self, action: PathNavigationAction) {
        let pane = self.scene.active_pane();
        let target = match action {
            PathNavigationAction::Back => self
                .scene
                .pane_history(pane)
                .back
                .last()
                .cloned()
                .map(|path| (path, ShellNavigationHistoryUpdate::Back)),
            PathNavigationAction::Forward => self
                .scene
                .pane_history(pane)
                .forward
                .last()
                .cloned()
                .map(|path| (path, ShellNavigationHistoryUpdate::Forward)),
            PathNavigationAction::Parent => self
                .scene
                .parent_directory_path_for_pane(pane)
                .map(|path| (path, ShellNavigationHistoryUpdate::Push)),
        };
        if let Some((path, history)) = target {
            self.queue_navigation(pane, path, history, action.reason());
        }
    }

    fn drain_navigation_completions(&mut self) -> bool {
        let Some(size) = self.renderer_size() else {
            return false;
        };
        let mut changed = false;
        while let Ok(completion) = self.navigation_rx.try_recv() {
            changed |= apply_navigation_completion(
                &mut self.scene,
                &self.navigation_generations,
                completion,
                size,
            );
        }
        if changed {
            self.sync_window_title();
            self.request_redraw();
        }
        changed
    }

    fn pointer_moved(&mut self, position: PhysicalPosition<f64>) {
        let Some(size) = self.renderer_size() else {
            return;
        };
        if self
            .scene
            .set_pointer(view_point_from_physical_position(position), size)
        {
            self.request_redraw();
        }
    }

    fn pointer_left(&mut self) {
        if self.scene.clear_pointer() {
            self.request_redraw();
        }
    }

    fn pointer_button(
        &mut self,
        state: ElementState,
        position: PhysicalPosition<f64>,
        button: ButtonSource,
    ) {
        let Some(size) = self.renderer_size() else {
            return;
        };
        match (button.mouse_button(), state) {
            (Some(MouseButton::Back), ElementState::Pressed) => {
                self.perform_path_navigation(PathNavigationAction::Back);
                return;
            }
            (Some(MouseButton::Forward), ElementState::Pressed) => {
                self.perform_path_navigation(PathNavigationAction::Forward);
                return;
            }
            (Some(MouseButton::Left), _) => {}
            _ => return,
        }
        let point = view_point_from_physical_position(position);
        let changed = match state {
            ElementState::Pressed => {
                if let Some(activation) =
                    self.scene
                        .item_activation_for_press(point, size, Instant::now())
                {
                    match activation {
                        ShellItemActivation::Directory { pane, path } => {
                            self.queue_navigation(
                                pane,
                                path,
                                ShellNavigationHistoryUpdate::Push,
                                "native-double-click-directory",
                            );
                        }
                        ShellItemActivation::File(_) => {}
                    }
                    return;
                }
                self.scene.begin_pane_pointer(
                    SelectionClick {
                        point,
                        extend: self.modifiers.state().shift_key(),
                        toggle: self.modifiers.state().control_key()
                            || self.modifiers.state().meta_key(),
                    },
                    size,
                )
            }
            ElementState::Released => self.scene.end_pane_pointer(point, size),
        };
        if changed {
            self.request_redraw();
        }
    }

    fn mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let Some(size) = self.renderer_size() else {
            return;
        };
        let (delta_x, delta_y) = scroll_delta_xy(delta, self.scene.ui_scale());
        if self.scene.scroll_by_delta(delta_x, delta_y, size) {
            self.request_redraw();
        }
    }

    fn keyboard_input(&mut self, event: &crate::windowing::KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        let Some(size) = self.renderer_size() else {
            return;
        };
        if let Some(action) =
            path_navigation_action_for_key(&event.logical_key, self.modifiers.state().alt_key())
        {
            self.perform_path_navigation(action);
            return;
        }
        if is_activation_key(&event.logical_key) {
            if let Some((pane, path)) = self.scene.selected_directory_path() {
                self.queue_navigation(
                    pane,
                    path,
                    ShellNavigationHistoryUpdate::Push,
                    "native-activate-directory",
                );
            }
            return;
        }
        if let Some(action) = navigation_action_for_key(&event.logical_key)
            && self
                .scene
                .navigate(action, self.modifiers.state().shift_key(), size)
        {
            self.request_redraw();
        }
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let (Some(window), Some(renderer)) = (self.window.as_ref(), self.renderer.as_mut()) else {
            return Ok(());
        };
        let size = renderer.size();
        let mut layouts = self.scene.prepare_frame_projection_layouts(size);
        self.scene
            .update_visible_slot_pools_for_projection_layouts(&mut layouts);
        let projections = self.scene.pane_projections_from_layouts(layouts);
        let layers = self
            .scene
            .build_native_frame_layers(size, projections.projections());
        self.text_engine.begin_frame();
        let text_pixels = self.text_engine.take_staging_pixels();
        let mut text_builder = TextFrameBuilder::new(
            TextFrameResources::from_engine(&mut self.text_engine),
            size,
            self.scene.ui_scale(),
            text_pixels,
        );
        let resident_icons = renderer.icon_resident_index();
        let mut icon_builder = IconFrameBuilder::new(
            IconFrameResources::from_engine(&mut self.icon_engine, resident_icons),
            IconFrameConfig {
                surface_size: size,
                ui_scale: self.scene.ui_scale(),
                sync_resolve_budget: crate::shell::prewarm::icon_sync_resolve_budget_for_frame(
                    if renderer.frame_count() == 0 {
                        "startup"
                    } else {
                        "native-vulkan-frame"
                    },
                ),
                folder_preview_cache: FolderPreviewCacheStats {
                    ready_entries: self.scene.folder_preview_roles.borrow().ready_len(),
                    ready_bytes: self.scene.folder_preview_roles.borrow().ready_bytes(),
                },
            },
        );
        self.scene
            .push_native_frame_text(&mut text_builder, projections.projections(), size);
        self.scene.push_native_frame_icons(
            &mut icon_builder,
            projections.projections(),
            size,
            text_builder.file_manager_midline_shift(),
        );
        let mut text_frame = text_builder.finish();
        let mut icon_frame = icon_builder.finish();
        drop(projections);
        let result = renderer.present_layers(
            event_loop,
            window,
            [0.0, 0.0, 0.0, 0.0],
            layers.as_refs(),
            &mut icon_frame,
            &mut text_frame,
        );
        self.text_engine.staging_pixels = std::mem::take(&mut text_frame.pixels);
        self.text_engine.staging_pixels.clear();
        self.text_engine.trim_caches();
        result
    }

    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), String> {
        let Some(renderer) = self.renderer.as_mut() else {
            self.scene.clamp_scroll(size);
            return Ok(());
        };
        let previous_size = renderer.size();
        let animate_reflow = renderer.frame_count() > 0;
        renderer.resize(size)?;
        let next_size = renderer.size();
        if animate_reflow {
            self.scene
                .reflow_pane_items_after_window_resize(previous_size, next_size);
        } else {
            self.scene.clamp_scroll(next_size);
        }
        Ok(())
    }

    fn scale_factor_changed(&mut self, scale_factor: f64) -> Result<(), String> {
        let Some(window) = self.window.as_ref() else {
            return Ok(());
        };
        let size = window.surface_size();
        self.resize(size)?;
        self.scene.set_scale_factor(scale_factor as f32, size);
        self.scene.clamp_scroll(size);
        Ok(())
    }

    fn exit(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(renderer) = self.renderer.as_ref()
            && let Err(error) = renderer.wait_idle("native-renderer-shutdown")
        {
            eprintln!("[fika-vulkan] shutdown wait failed: {error}");
        }
        self.renderer = None;
        self.window = None;
        event_loop.exit();
    }
}

impl ApplicationHandler for FikaNativeVulkanApp {
    fn proxy_wake_up(&mut self, _event_loop: &ActiveEventLoop) {
        self.drain_navigation_completions();
    }

    fn can_create_surfaces(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(error) = self.create_window_and_renderer(event_loop) {
            eprintln!("[fika-vulkan] native renderer startup failed: {error}");
            self.exit(event_loop);
            return;
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.drain_navigation_completions();
        let animations_finished = self.scene.prune_finished_animations();
        if animations_finished {
            self.request_redraw();
        }
        let icon_work_pending = self.icon_engine.resolver.has_visible_pending()
            || self.icon_engine.thumbnails.has_visible_pending();
        if self.scene.animation_active() || icon_work_pending {
            self.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                self.scene
                    .next_animation_frame_deadline()
                    .unwrap_or_else(|| Instant::now() + Duration::from_millis(16)),
            ));
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
        if self.window.as_ref().map(|window| window.id()) != Some(window_id) {
            return;
        }
        let redraw_after_resize = matches!(
            &event,
            WindowEvent::SurfaceResized(_) | WindowEvent::ScaleFactorChanged { .. }
        );
        let outcome = match event {
            WindowEvent::CloseRequested => {
                self.exit(event_loop);
                return;
            }
            WindowEvent::SurfaceResized(size) => self.resize(size),
            WindowEvent::ScaleFactorChanged { scale_factor } => {
                self.scale_factor_changed(scale_factor)
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                Ok(())
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                self.keyboard_input(&event);
                Ok(())
            }
            WindowEvent::PointerMoved { position, .. } => {
                self.pointer_moved(position);
                Ok(())
            }
            WindowEvent::PointerLeft { .. } => {
                self.pointer_left();
                Ok(())
            }
            WindowEvent::PointerButton {
                state,
                position,
                button,
                ..
            } => {
                self.pointer_button(state, position, button);
                Ok(())
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.mouse_wheel(delta);
                Ok(())
            }
            WindowEvent::RedrawRequested => self.render(event_loop),
            _ => Ok(()),
        };
        if let Err(error) = outcome {
            eprintln!("[fika-vulkan] native renderer frame failed: {error}");
            self.exit(event_loop);
            return;
        }
        if redraw_after_resize && let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}
