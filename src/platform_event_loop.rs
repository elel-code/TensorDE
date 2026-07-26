impl EventLoop {
    pub fn new() -> Result<Self, RuntimeError> {
        let runtime = PlatformBackend::connect()?;
        let wake = runtime.wake_handle();
        let shared = Arc::new(LoopShared {
            wake,
            commands: Mutex::new(Vec::new()),
            synthetic_events: Mutex::new(Vec::new()),
        });
        Ok(Self {
            active: ActiveEventLoop {
                runtime: Rc::new(RefCell::new(runtime)),
                shared,
                windows: Rc::new(RefCell::new(HashMap::new())),
                primary_surface: Cell::new(None),
                dnd_transfers: RefCell::new(HashMap::new()),
                dnd_sources: RefCell::new(HashMap::new()),
                next_async_serial: Cell::new(1),
                control_flow: Cell::new(ControlFlow::Wait),
                exiting: Cell::new(false),
                dmabuf_default_feedback: RefCell::new(None),
                dmabuf_surface_feedback: RefCell::new(HashMap::new()),
            },
        })
    }

    pub fn create_proxy(&self) -> EventLoopProxy {
        EventLoopProxy {
            wake: self.active.shared.wake.clone(),
        }
    }

    pub fn set_control_flow(&self, control_flow: ControlFlow) {
        self.active.set_control_flow(control_flow);
    }

    pub fn run_app<A: ApplicationHandler>(self, mut app: A) -> Result<(), RuntimeError> {
        app.can_create_surfaces(&self.active);
        let mut runtime_events = Vec::new();
        while !self.active.exiting.get() {
            self.process_commands();
            {
                let mut runtime = self.active.runtime.borrow_mut();
                runtime.drain_events_into(&mut runtime_events);
            }
            for event in runtime_events.drain(..) {
                self.dispatch_runtime_event(&mut app, event)?;
                if self.active.exiting.get() {
                    break;
                }
            }
            self.dispatch_synthetic_events(&mut app);
            self.process_commands();
            self.dispatch_ready_redraws(&mut app);
            if self.active.exiting.get() {
                break;
            }

            app.about_to_wait(&self.active);
            self.process_commands();
            self.dispatch_ready_redraws(&mut app);
            if self.active.exiting.get() {
                break;
            }

            let timeout = if self.has_ready_redraw() {
                Some(Duration::ZERO)
            } else {
                match self.active.control_flow.get() {
                    ControlFlow::Poll => Some(Duration::ZERO),
                    ControlFlow::Wait => None,
                    ControlFlow::WaitUntil(deadline) => {
                        Some(deadline.saturating_duration_since(Instant::now()))
                    }
                }
            };
            self.active.runtime.borrow_mut().dispatch(timeout)?;
            app.proxy_wake_up(&self.active);
        }
        self.process_commands();
        Ok(())
    }

    fn process_commands(&self) {
        let commands = {
            let mut commands = self
                .active
                .shared
                .commands
                .lock()
                .expect("Wayland command queue mutex poisoned");
            std::mem::take(&mut *commands)
        };
        let mut runtime = self.active.runtime.borrow_mut();
        for command in commands {
            let result = match command {
                RuntimeCommand::SetTitle(surface, title) => runtime.set_title(surface, title),
                RuntimeCommand::SetMinSize(surface, size) => runtime.set_min_size(surface, size),
                RuntimeCommand::SetMaxSize(surface, size) => runtime.set_max_size(surface, size),
                RuntimeCommand::SetBlur(surface, state) => match runtime.set_blur(surface, state) {
                    Err(RuntimeError::Unsupported(_)) => Ok(()),
                    result => result,
                },
                RuntimeCommand::SetCursor(icon) => runtime
                    .set_cursor(runtime_cursor_icon(icon))
                    .or_else(|error| match error {
                        RuntimeError::Unsupported(_) => Ok(()),
                        error => Err(error),
                    }),
                RuntimeCommand::SetIme(surface, state) => state
                    .map(|state| {
                        let scale_factor = self
                            .window(surface)
                            .map(|window| window.scale_factor())
                            .unwrap_or(1.0);
                        runtime_text_input_state(state, scale_factor)
                    })
                    .transpose()
                    .and_then(|state| runtime.set_text_input_state(surface, state))
                    .or_else(|error| match error {
                        RuntimeError::Unsupported(_) => Ok(()),
                        error => Err(error),
                    }),
                RuntimeCommand::RequestUserAttention(surface) => runtime
                    .request_user_attention(surface)
                    .or_else(|error| match error {
                        RuntimeError::Unsupported(_) => Ok(()),
                        error => Err(error),
                    }),
                RuntimeCommand::ArmFrame(surface) => {
                    // Arm presentation feedback only when the global exists;
                    // always keep wl_surface.frame for pacing.
                    if runtime.capabilities().presentation {
                        let _ = runtime.request_presentation_feedback(surface);
                    }
                    runtime
                        .request_frame(surface)
                        .and_then(|()| runtime.commit(surface))
                }
                RuntimeCommand::Destroy(surface) => {
                    self.active.windows.borrow_mut().remove(&surface);
                    self.active
                        .dmabuf_surface_feedback
                        .borrow_mut()
                        .remove(&surface);
                    runtime.destroy_surface(surface).map(|_| ())
                }
            };
            if let Err(error) = result
                && !matches!(error, RuntimeError::SurfaceNotFound(_))
            {
                eprintln!("[fika-wayland] runtime command failed: {error}");
            }
        }
    }

    fn dispatch_runtime_event<A: ApplicationHandler>(
        &self,
        app: &mut A,
        event: Event,
    ) -> Result<(), RuntimeError> {
        match event {
            Event::Surface(event) => self.dispatch_surface_event(app, event),
            Event::LayerSurface(_) | Event::Output(_) | Event::Seat(_) => Ok(()),
            Event::Activation(_) => Ok(()),
            Event::PointerConstraint(_) | Event::RelativePointer(_) => Ok(()),
            Event::PointerGesture(event) => {
                self.dispatch_pointer_gesture_event(app, event);
                Ok(())
            }
            Event::TextInput(event) => {
                self.dispatch_text_input_event(app, event);
                Ok(())
            }
            Event::Pointer(event) => {
                let Some(window) = self.window(event.surface) else {
                    return Ok(());
                };
                let scale = window.scale_factor();
                let position = PhysicalPosition::new(
                    event.position.0 * scale,
                    event.position.1 * scale,
                );
                let event = match event.kind {
                    PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                        WindowEvent::PointerMoved { position }
                    }
                    PointerEventKind::Leave => WindowEvent::PointerLeft {},
                    PointerEventKind::Press { button, .. } => WindowEvent::PointerButton {
                        state: ElementState::Pressed,
                        position,
                        button: linux_button(button),
                    },
                    PointerEventKind::Release { button, .. } => WindowEvent::PointerButton {
                        state: ElementState::Released,
                        position,
                        button: linux_button(button),
                    },
                    PointerEventKind::Axis {
                        horizontal,
                        vertical,
                        ..
                    } => WindowEvent::MouseWheel {
                        delta: map_pointer_axis_to_scroll_delta(horizontal, vertical, scale),
                    },
                };
                app.window_event(&self.active, window.id(), event);
                Ok(())
            }
            Event::Keyboard(event) => {
                match event {
                    KeyboardEvent::Key {
                        surface,
                        state,
                        raw_code,
                        keysym,
                        text,
                        ..
                    } => {
                        if self.window(surface).is_some() {
                            app.window_event(
                                &self.active,
                                surface,
                                WindowEvent::KeyboardInput {
                                    event: translate_key_event(state, raw_code, keysym, text),
                                    is_synthetic: false,
                                },
                            );
                        }
                    }
                    KeyboardEvent::Modifiers { surface, modifiers } => {
                        if self.window(surface).is_some() {
                            app.window_event(
                                &self.active,
                                surface,
                                WindowEvent::ModifiersChanged(modifiers.into()),
                            );
                        }
                    }
                    KeyboardEvent::Enter { .. } | KeyboardEvent::Leave { .. } => {}
                }
                Ok(())
            }
            Event::Touch(_) => Ok(()),
            Event::Dmabuf(event) => {
                // Cache feedback for import negotiation; present stays RWH/swapchain.
                match event {
                    wayland_client_runtime::DmabufEvent::Feedback {
                        surface,
                        feedback,
                    } => {
                        let surface_id = match surface {
                            Some(id) => {
                                self.active
                                    .dmabuf_surface_feedback
                                    .borrow_mut()
                                    .insert(id, feedback.clone());
                                Some(id)
                            }
                            None => {
                                *self.active.dmabuf_default_feedback.borrow_mut() =
                                    Some(feedback.clone());
                                None
                            }
                        };
                        // Verbose format table only when FIKA_LOG / FIKA_WGPU_LOG is set.
                        // App-level readiness still logs via dmabuf_feedback_updated.
                        if platform_verbose_log_enabled() {
                            let pick =
                                crate::shell::render::dmabuf::pick_import_format(&feedback);
                            let scope = match surface_id {
                                Some(id) => format!("surface={id:?}"),
                                None => "default".to_string(),
                            };
                            eprintln!(
                                "[fika-wgpu] dmabuf-feedback {scope} main_device=0x{:x} formats={} tranches={} pick={pick:?}",
                                feedback.main_device(),
                                feedback.formats().len(),
                                feedback.tranches().len(),
                            );
                        }
                        app.dmabuf_feedback_updated(&self.active, surface_id);
                    }
                    wayland_client_runtime::DmabufEvent::BufferCreated { id } => {
                        if platform_verbose_log_enabled() {
                            eprintln!("[fika-wgpu] dmabuf-buffer-created id={id:?}");
                        }
                    }
                    wayland_client_runtime::DmabufEvent::BufferFailed => {
                        if platform_verbose_log_enabled() {
                            eprintln!("[fika-wgpu] dmabuf-buffer-failed");
                        }
                    }
                    wayland_client_runtime::DmabufEvent::BufferReleased { id } => {
                        if platform_verbose_log_enabled() {
                            eprintln!("[fika-wgpu] dmabuf-buffer-released id={id:?}");
                        }
                    }
                }
                Ok(())
            }
            Event::Dnd(event) => {
                self.dispatch_dnd_event(app, event);
                Ok(())
            }
        }
    }

    fn dispatch_text_input_event<A: ApplicationHandler>(
        &self,
        app: &mut A,
        event: RuntimeTextInputEvent,
    ) {
        let (surface, event) = match event {
            RuntimeTextInputEvent::Entered { surface } => (surface, ImeEvent::Enabled),
            RuntimeTextInputEvent::Left { surface } => (surface, ImeEvent::Disabled),
            RuntimeTextInputEvent::Done(done) => (
                done.surface,
                ImeEvent::Done {
                    serial: done.serial,
                    delete_surrounding: done.delete_surrounding.map(|delete| {
                        ImeDeleteSurrounding {
                            before_bytes: delete.before_bytes,
                            after_bytes: delete.after_bytes,
                        }
                    }),
                    commit: done.commit,
                    preedit: done.preedit.map(|preedit| ImePreedit {
                        text: preedit.text,
                        cursor_range: preedit.cursor_range,
                    }),
                },
            ),
        };
        if self.window(surface).is_some() {
            app.window_event(&self.active, surface, WindowEvent::Ime(event));
        }
    }

    fn dispatch_dnd_event<A: ApplicationHandler>(&self, app: &mut A, event: DndEvent) {
        match event {
            DndEvent::Enter {
                offer,
                surface,
                position,
                mime_types,
                source_actions: _,
            } => {
                let Some(window) = self.window(surface) else {
                    return;
                };
                let id = DataTransferId(offer.get());
                let hints = mime_types
                    .iter()
                    .filter_map(|mime| TypeHint::from_mime(mime))
                    .collect();
                self.active.dnd_transfers.borrow_mut().insert(
                    id,
                    ActiveDndTransfer {
                        offer,
                        window: surface,
                        hints,
                        dropped: false,
                        read_complete: false,
                    },
                );
                app.window_event(
                    &self.active,
                    surface,
                    WindowEvent::DragEntered {
                        id,
                        position: Some(scale_dnd_position(position, window.scale_factor())),
                    },
                );
            }
            DndEvent::Motion {
                offer,
                surface,
                position,
            } => {
                let Some(window) = self.window(surface) else {
                    return;
                };
                app.window_event(
                    &self.active,
                    surface,
                    WindowEvent::DragPosition {
                        id: DataTransferId(offer.get()),
                        position: scale_dnd_position(position, window.scale_factor()),
                    },
                );
            }
            DndEvent::Leave { offer, surface } => {
                let id = DataTransferId(offer.get());
                let dropped = self
                    .active
                    .dnd_transfers
                    .borrow()
                    .get(&id)
                    .is_some_and(|transfer| transfer.dropped);
                if !dropped {
                    self.active.dnd_transfers.borrow_mut().remove(&id);
                    if self.window(surface).is_some() {
                        app.window_event(
                            &self.active,
                            surface,
                            WindowEvent::DragLeft { id },
                        );
                    }
                    if let Err(error) = self
                        .active
                        .runtime
                        .borrow_mut()
                        .discard_dnd_offer(offer)
                    {
                        eprintln!("[fika-wayland] discard DnD offer failed: {error}");
                    }
                }
            }
            DndEvent::Drop {
                offer,
                surface,
                action: _,
            } => {
                let id = DataTransferId(offer.get());
                if let Some(transfer) = self.active.dnd_transfers.borrow_mut().get_mut(&id) {
                    transfer.dropped = true;
                }
                if self.window(surface).is_some() {
                    app.window_event(
                        &self.active,
                        surface,
                        WindowEvent::DragDropped { id },
                    );
                }
                self.finish_dnd_if_ready(id);
            }
            DndEvent::SourceDropped { source, action }
            | DndEvent::SourceFinished { source, action } => {
                if let Some(window) = self.active.dnd_sources.borrow_mut().remove(&source) {
                    app.window_event(
                        &self.active,
                        window,
                        WindowEvent::OutgoingDragDropped {
                            id: DataTransferId(source.get()),
                            action: action.map(platform_dnd_action),
                        },
                    );
                }
            }
            DndEvent::SourceCancelled { source } => {
                if let Some(window) = self.active.dnd_sources.borrow_mut().remove(&source) {
                    app.window_event(
                        &self.active,
                        window,
                        WindowEvent::OutgoingDragCanceled {
                            id: DataTransferId(source.get()),
                        },
                    );
                }
            }
        }
    }

    fn dispatch_synthetic_events<A: ApplicationHandler>(&self, app: &mut A) {
        let events = {
            let mut events = self
                .active
                .shared
                .synthetic_events
                .lock()
                .expect("Wayland synthetic event queue mutex poisoned");
            std::mem::take(&mut *events)
        };
        for synthetic in events {
            app.window_event(
                &self.active,
                synthetic.window,
                synthetic.event,
            );
            if let Some(offer) = synthetic.completed_offer {
                let id = DataTransferId(offer.get());
                if let Some(transfer) = self.active.dnd_transfers.borrow_mut().get_mut(&id) {
                    transfer.read_complete = true;
                }
                self.finish_dnd_if_ready(id);
            }
        }
    }

    fn finish_dnd_if_ready(&self, id: DataTransferId) {
        let ready = self
            .active
            .dnd_transfers
            .borrow()
            .get(&id)
            .is_some_and(|transfer| transfer.dropped && transfer.read_complete);
        if !ready {
            return;
        }
        let Some(transfer) = self.active.dnd_transfers.borrow_mut().remove(&id) else {
            return;
        };
        if let Err(error) = self
            .active
            .runtime
            .borrow_mut()
            .finish_dnd_offer(transfer.offer)
        {
            eprintln!("[fika-wayland] finish DnD offer failed: {error}");
        }
    }

    fn dispatch_pointer_gesture_event<A: ApplicationHandler>(
        &self,
        app: &mut A,
        event: PointerGestureEvent,
    ) {
        // Hold is unused. Pinch → zoom; multi-finger swipe → history navigation.
        match event {
            PointerGestureEvent::Pinch(pinch) => {
                let surface = pinch.surface();
                let Some(window) = self.window(surface) else {
                    return;
                };
                let gesture = match pinch {
                    PointerPinchEvent::Begin { .. } => PinchGesture::Begin,
                    PointerPinchEvent::Update { scale, .. } => PinchGesture::Update { scale },
                    PointerPinchEvent::End { cancelled, .. } => PinchGesture::End { cancelled },
                };
                app.window_event(
                    &self.active,
                    window.id(),
                    WindowEvent::PinchGesture(gesture),
                );
            }
            PointerGestureEvent::Swipe(swipe) => {
                let surface = swipe.surface();
                let Some(window) = self.window(surface) else {
                    return;
                };
                let gesture = match swipe {
                    PointerSwipeEvent::Begin { fingers, .. } => SwipeGesture::Begin { fingers },
                    PointerSwipeEvent::Update {
                        delta: (dx, dy), ..
                    } => SwipeGesture::Update {
                        delta_x: dx,
                        delta_y: dy,
                    },
                    PointerSwipeEvent::End { cancelled, .. } => SwipeGesture::End { cancelled },
                };
                app.window_event(
                    &self.active,
                    window.id(),
                    WindowEvent::SwipeGesture(gesture),
                );
            }
            PointerGestureEvent::Hold(_) => {}
        }
    }

    fn dispatch_surface_event<A: ApplicationHandler>(
        &self,
        app: &mut A,
        event: SurfaceEvent,
    ) -> Result<(), RuntimeError> {
        match event {
            SurfaceEvent::Configure {
                surface,
                suggested_size,
                ..
            } => {
                let Some(window) = self.window(surface) else {
                    return Ok(());
                };
                let fractional_scale = self
                    .active
                    .runtime
                    .borrow()
                    .capabilities()
                    .fractional_scale;
                let (physical, logical, changed) = {
                    let mut state = window
                        .state
                        .lock()
                        .expect("Wayland window state mutex poisoned");
                    let logical = LogicalSize::new(
                        suggested_size.width.unwrap_or(state.logical_size.width),
                        suggested_size.height.unwrap_or(state.logical_size.height),
                    );
                    let physical = logical_to_physical_rounded(logical, state.scale_factor);
                    let changed = !state.configured || physical != state.physical_size;
                    state.logical_size = logical;
                    state.physical_size = physical;
                    state.configured = true;
                    state.redraw_requested = true;
                    (physical, logical, changed)
                };
                {
                    let mut runtime = self.active.runtime.borrow_mut();
                    runtime.set_window_geometry(surface, LogicalPosition::ZERO, logical)?;
                    if fractional_scale {
                        runtime.set_buffer_scale(surface, 1)?;
                        runtime.set_viewport_destination(surface, Some(logical))?;
                    } else {
                        let scale_factor = window
                            .state
                            .lock()
                            .expect("Wayland window state mutex poisoned")
                            .scale_factor;
                        runtime.set_buffer_scale(surface, integer_buffer_scale(scale_factor))?;
                    }
                    runtime.commit(surface)?;
                }
                if changed {
                    app.window_event(
                        &self.active,
                        surface,
                        WindowEvent::SurfaceResized(physical),
                    );
                }
                Ok(())
            }
            SurfaceEvent::ScaleFactorChanged { surface, factor } => {
                let Some(window) = self.window(surface) else {
                    return Ok(());
                };
                let factor = normalize_wayland_scale_factor(factor);
                let logical = {
                    let mut state = window
                        .state
                        .lock()
                        .expect("Wayland window state mutex poisoned");
                    state.scale_factor = factor;
                    state.physical_size =
                        logical_to_physical_rounded(state.logical_size, state.scale_factor);
                    state.redraw_requested = true;
                    state.logical_size
                };
                {
                    let mut runtime = self.active.runtime.borrow_mut();
                    if runtime.capabilities().fractional_scale {
                        runtime.set_buffer_scale(surface, 1)?;
                        runtime.set_viewport_destination(surface, Some(logical))?;
                    } else {
                        runtime.set_buffer_scale(surface, integer_buffer_scale(factor))?;
                    }
                    runtime.commit(surface)?;
                }
                app.window_event(
                    &self.active,
                    surface,
                    WindowEvent::ScaleFactorChanged {
                        scale_factor: factor,
                    },
                );
                Ok(())
            }
            SurfaceEvent::CloseRequested { surface } | SurfaceEvent::PopupDone { surface } => {
                if self.window(surface).is_some() {
                    app.window_event(&self.active, surface, WindowEvent::CloseRequested);
                }
                Ok(())
            }
            SurfaceEvent::Frame { surface, .. }
            | SurfaceEvent::Presented { surface, .. }
            | SurfaceEvent::PresentationDiscarded { surface } => {
                // Runtime clears protocol-side pending flags; wake so the loop
                // re-evaluates `has_ready_redraw` under ControlFlow::Wait.
                if let Some(window) = self.window(surface) {
                    let state = window
                        .state
                        .lock()
                        .expect("Wayland window state mutex poisoned");
                    if state.configured && state.redraw_requested {
                        drop(state);
                        self.active.shared.wake.wake();
                    }
                }
                Ok(())
            }
            SurfaceEvent::PopupConfigure { .. }
            | SurfaceEvent::OutputEnter { .. }
            | SurfaceEvent::OutputLeave { .. } => Ok(()),
        }
    }

    fn dispatch_ready_redraws<A: ApplicationHandler>(&self, app: &mut A) {
        // Collect ready surface ids while holding the runtime borrow, then
        // drop it before delivering RedrawRequested (handlers may command the
        // runtime).
        let ready_ids = {
            let runtime = self.active.runtime.borrow();
            let windows = self
                .active
                .windows
                .borrow()
                .values()
                .filter_map(Weak::upgrade)
                .collect::<Vec<_>>();
            let mut ready = Vec::new();
            for window in windows {
                let mut state = window
                    .state
                    .lock()
                    .expect("Wayland window state mutex poisoned");
                if state.configured
                    && state.redraw_requested
                    && !runtime.is_frame_pending(window.id())
                {
                    state.redraw_requested = false;
                    ready.push(window.id());
                }
            }
            ready
        };
        for id in ready_ids {
            app.window_event(&self.active, id, WindowEvent::RedrawRequested);
        }
    }

    fn has_ready_redraw(&self) -> bool {
        let runtime = self.active.runtime.borrow();
        self.active
            .windows
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .any(|window| {
                let state = window
                    .state
                    .lock()
                    .expect("Wayland window state mutex poisoned");
                state.configured
                    && state.redraw_requested
                    && !runtime.is_frame_pending(window.id())
            })
    }

    fn window(&self, id: SurfaceId) -> Option<Arc<WaylandWindow>> {
        self.active.windows.borrow().get(&id).and_then(Weak::upgrade)
    }
}

/// Verbose platform diagnostics (`FIKA_LOG` / `FIKA_WGPU_LOG`), same policy as
/// the main binary's `fika_log!` but usable from the platform module.
fn platform_verbose_log_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let on = |name: &str| {
            std::env::var_os(name).is_some_and(|value| {
                let value = value.to_string_lossy();
                let value = value.trim().to_ascii_lowercase();
                !matches!(value.as_str(), "" | "0" | "false" | "no" | "off")
            })
        };
        on("FIKA_LOG") || on("FIKA_WGPU_LOG")
    })
}
