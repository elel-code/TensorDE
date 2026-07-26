impl Runtime {
    pub fn connect(options: RuntimeOptions) -> Result<Self, RuntimeError> {
        let connection = Connection::connect_to_env()
            .map_err(|error| RuntimeError::Connect(error.to_string()))?;
        Self::from_connection(connection, options)
    }

    pub fn from_connection(
        connection: Connection,
        options: RuntimeOptions,
    ) -> Result<Self, RuntimeError> {
        let (globals, mut event_queue) = registry_queue_init(&connection)
            .map_err(|error| RuntimeError::Registry(error.to_string()))?;
        let queue_handle = event_queue.handle();
        let event_loop = CalloopEventLoop::<RuntimeState>::try_new()
            .map_err(|error| RuntimeError::EventLoop(error.to_string()))?;

        let compositor = CompositorState::bind(&globals, &queue_handle)
            .map_err(|error| RuntimeError::MissingGlobal(error.to_string()))?;
        let shm = Shm::bind(&globals, &queue_handle)
            .map_err(|error| RuntimeError::MissingGlobal(error.to_string()))?;
        let xdg_shell = XdgShell::bind(&globals, &queue_handle)
            .map_err(|error| RuntimeError::MissingGlobal(error.to_string()))?;
        let output_state = OutputState::new(&globals, &queue_handle);
        let seat_state = SeatState::new(&globals, &queue_handle);
        let background_effect_state = BackgroundEffectState::new(&globals, &queue_handle);
        let xdg_activation = ActivationManager::bind(&globals, &queue_handle).ok();
        let toplevel_icon_manager = ToplevelIconManager::bind(&globals, &queue_handle).ok();
        let layer_shell_manager = LayerShellManager::bind(&globals, &queue_handle).ok();
        let text_input_manager = TextInputManager::bind(&globals, &queue_handle).ok();
        let fractional_scale_manager = FractionalScaleManager::bind(&globals, &queue_handle).ok();
        let pointer_gesture_manager = PointerGestureManager::bind(&globals, &queue_handle).ok();
        let pointer_protocols = PointerProtocols::bind(
            &globals,
            &queue_handle,
            has_global(&globals, "zwp_pointer_constraints_v1"),
            has_global(&globals, "zwp_relative_pointer_manager_v1"),
        );
        let data_device_manager = DataDeviceManagerState::bind(&globals, &queue_handle)
            .map_err(|error| RuntimeError::MissingGlobal(error.to_string()))?;
        let capabilities = RuntimeCapabilities {
            xdg_dialog_v1: has_global(&globals, "xdg_wm_dialog_v1"),
            xdg_activation_v1: xdg_activation.is_some(),
            xdg_toplevel_icon_v1: toplevel_icon_manager.is_some(),
            layer_shell_v1: layer_shell_manager.is_some(),
            layer_shell_dynamic_layer: layer_shell_manager
                .as_ref()
                .is_some_and(|manager| manager.version() >= 2),
            layer_shell_on_demand_keyboard: layer_shell_manager
                .as_ref()
                .is_some_and(|manager| manager.version() >= 4),
            layer_shell_exclusive_edge: layer_shell_manager
                .as_ref()
                .is_some_and(|manager| manager.version() >= 5),
            text_input_v3: text_input_manager.is_some(),
            pointer_constraints_v1: pointer_protocols.has_constraints(),
            relative_pointer_v1: pointer_protocols.has_relative_pointer(),
            pointer_gestures_v1: pointer_gesture_manager.is_some(),
            pointer_gesture_hold_v1: pointer_gesture_manager
                .as_ref()
                .is_some_and(PointerGestureManager::supports_hold),
            popup_reposition: xdg_shell.xdg_wm_base().version() >= 3,
            ext_background_effect: false,
            fractional_scale: fractional_scale_manager.is_some(),
            cursor_shape: has_global(&globals, "wp_cursor_shape_manager_v1"),
        };

        let mut state = RuntimeState {
            registry_state: RegistryState::new(&globals),
            output_state,
            seat_state,
            background_effect_state,
            data_device_manager,
            compositor,
            shm,
            xdg_shell,
            xdg_activation,
            toplevel_icon_manager,
            layer_shell_manager,
            text_input_manager,
            fractional_scale_manager,
            pointer_gesture_manager,
            pointer_protocols,
            pointer_gesture_subscriptions: PointerGestureSubscriptions::default(),
            surfaces: HashMap::new(),
            surface_ids: HashMap::new(),
            children: HashMap::new(),
            seats: HashMap::new(),
            keyboard_focus: HashMap::new(),
            incoming_dnd: HashMap::new(),
            active_dnd_by_device: HashMap::new(),
            outgoing_dnd: HashMap::new(),
            selection_sources: HashMap::new(),
            pending_attention: HashSet::new(),
            events: EventBuffer::with_capacity(options.event_capacity),
            next_surface_id: 1,
            next_dnd_id: 1,
            next_input_order: 1,
            next_activation_request_id: 1,
        };

        // ext-background-effect-v1 advertises effect support in an event after
        // binding. Complete one roundtrip so capabilities are accurate when
        // `from_connection` returns.
        if has_global(&globals, "ext_background_effect_manager_v1")
            || state.toplevel_icon_manager.is_some()
        {
            event_queue
                .roundtrip(&mut state)
                .map_err(|error| RuntimeError::Registry(error.to_string()))?;
        }

        WaylandSource::new(connection.clone(), event_queue)
            .insert(event_loop.handle())
            .map_err(|error| RuntimeError::EventLoop(error.to_string()))?;
        let wake = WakeHandle::from_calloop(event_loop.get_signal());
        let display_readiness = crate::DisplayReadiness::from_as_fd(&connection).map_err(
            |error| RuntimeError::EventLoop(format!("display readiness: {error}")),
        )?;

        Ok(Self {
            connection,
            queue_handle,
            event_loop,
            state,
            wake,
            capabilities,
            display_readiness,
        })
    }

    pub fn capabilities(&self) -> RuntimeCapabilities {
        let mut capabilities = self.capabilities;
        capabilities.ext_background_effect =
            supports_ext_background_blur(self.state.background_effect_state.capabilities());
        capabilities
    }

    /// Metadata snapshots for outputs whose initial compositor description is complete.
    pub fn outputs(&self) -> Vec<OutputInfo> {
        self.state
            .output_state
            .outputs()
            .filter_map(|output| output_info(&self.state.output_state, &output))
            .collect()
    }

    /// Preferred square icon sizes advertised by the compositor, in logical
    /// pixels. An empty list means the compositor has no size preference or
    /// does not support xdg-toplevel-icon-v1.
    pub fn preferred_toplevel_icon_sizes(&self) -> Vec<u32> {
        self.state
            .toplevel_icon_manager
            .as_ref()
            .map(ToplevelIconManager::preferred_sizes)
            .unwrap_or_default()
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn wake_handle(&self) -> WakeHandle {
        self.wake.clone()
    }

    /// Wait for and dispatch protocol events. `None` waits indefinitely.
    ///
    /// Uses calloop (transitional). Prefer [`Self::wait_display_readable`] +
    /// [`Self::dispatch_pending`] on a Compio executor when integrating async.
    pub fn dispatch(&mut self, timeout: Option<Duration>) -> Result<(), RuntimeError> {
        self.event_loop
            .dispatch(timeout, &mut self.state)
            .map_err(|error| RuntimeError::EventLoop(error.to_string()))
    }

    /// Await readability on the Wayland display fd (Compio).
    ///
    /// Must run inside a Compio runtime. After this returns, call
    /// [`Self::dispatch_pending`] to process queued protocol messages without
    /// blocking on calloop's wait.
    pub async fn wait_display_readable(&self) -> Result<(), RuntimeError> {
        self.display_readiness
            .wait_readable()
            .await
            .map_err(|error| RuntimeError::EventLoop(error.to_string()))
    }

    /// Dispatch already-available protocol events without blocking.
    ///
    /// Intended for the Compio path: `wait_display_readable().await` then
    /// `dispatch_pending()`. Safe to call when no events are ready.
    pub fn dispatch_pending(&mut self) -> Result<(), RuntimeError> {
        self.event_loop
            .dispatch(Some(Duration::ZERO), &mut self.state)
            .map_err(|error| RuntimeError::EventLoop(error.to_string()))
    }

    pub fn drain_events(&mut self) -> impl Iterator<Item = Event> + '_ {
        self.state.events.drain()
    }

    /// Append all pending events to a reusable caller-owned batch.
    ///
    /// Unlike collecting [`Runtime::drain_events`], this lets event loops keep
    /// one allocation across dispatch iterations. Existing items in `target`
    /// are preserved before the newly drained events.
    pub fn drain_events_into(&mut self, target: &mut Vec<Event>) {
        self.state.events.drain_into(target);
    }

    pub fn create_toplevel(
        &mut self,
        attributes: ToplevelAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        let surface = self.state.compositor.create_surface(&self.queue_handle);
        let window = self.state.xdg_shell.create_window(
            surface,
            window_decorations(attributes.decorations),
            &self.queue_handle,
        );
        apply_toplevel_attributes(window.xdg_toplevel(), &attributes);
        window.commit();
        Ok(self.insert_surface(
            ProtocolSurface::Toplevel(window),
            None,
            SurfaceKind::Toplevel,
        ))
    }

    /// Create a layer surface and perform its required initial bufferless commit.
    ///
    /// Wait for [`Event::LayerSurface`] with
    /// [`LayerSurfaceEvent::Configure`] before attaching the first renderer
    /// buffer. All layer state is double-buffered with `wl_surface`.
    pub fn create_layer_surface(
        &mut self,
        attributes: LayerSurfaceAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        let manager = self
            .state
            .layer_shell_manager
            .as_ref()
            .ok_or(RuntimeError::Unsupported("layer-shell-v1"))?;
        let output = attributes
            .output
            .map(|output| self.resolve_output(output))
            .transpose()?;
        let surface = self.state.compositor.create_surface(&self.queue_handle);
        let layer =
            manager.create_surface(&self.queue_handle, surface, output.as_ref(), &attributes)?;
        let id = self.insert_surface(ProtocolSurface::Layer(layer), None, SurfaceKind::Layer);
        self.surface_shared(id)?.wl_surface().commit();
        Ok(id)
    }

    /// Create a parented toplevel and add xdg-dialog-v1 modality when available.
    ///
    /// If xdg-dialog-v1 is unavailable, the result remains a correctly parented
    /// transient toplevel and `capabilities().xdg_dialog_v1` is false.
    pub fn create_dialog(
        &mut self,
        parent: SurfaceId,
        attributes: DialogAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        let parent_shared = self.parent_toplevel(parent)?;
        let parent_toplevel = parent_shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidParent(parent))?;
        let surface = self.state.compositor.create_surface(&self.queue_handle);

        let protocol = if self.capabilities.xdg_dialog_v1 {
            let dialog = self.state.xdg_shell.create_dialog(
                surface,
                window_decorations(attributes.toplevel.decorations),
                &self.queue_handle,
                parent_toplevel,
            )?;
            apply_toplevel_attributes(dialog.xdg_toplevel(), &attributes.toplevel);
            dialog.set_modal(attributes.modal);
            dialog.commit();
            ProtocolSurface::NativeDialog(dialog)
        } else {
            let window = self.state.xdg_shell.create_window(
                surface,
                window_decorations(attributes.toplevel.decorations),
                &self.queue_handle,
            );
            apply_toplevel_attributes(window.xdg_toplevel(), &attributes.toplevel);
            window.xdg_toplevel().set_parent(Some(parent_toplevel));
            window.commit();
            ProtocolSurface::FallbackDialog(window)
        };

        Ok(self.insert_surface(protocol, Some(parent_shared), SurfaceKind::Dialog))
    }

    pub fn create_popup(
        &mut self,
        parent: SurfaceId,
        attributes: PopupAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        validate_positioner(&attributes.positioner)?;
        if attributes
            .grab
            .as_ref()
            .is_some_and(|serial| !serial.is_popup_grab())
        {
            return Err(RuntimeError::InvalidPopupGrab);
        }
        if let Some(serial) = attributes.grab.as_ref() {
            let same_connection =
                serial.seat.backend().upgrade().as_ref() == Some(&self.connection.backend());
            let current = self
                .state
                .seats
                .get(&serial.seat.id().protocol_id())
                .is_some_and(|objects| {
                    is_current_popup_grab(objects, serial.source(), serial.serial)
                });
            if !same_connection || !current {
                return Err(RuntimeError::ForeignOrStalePopupGrab);
            }
        }

        let parent_shared = self
            .state
            .surfaces
            .get(&parent)
            .cloned()
            .ok_or(RuntimeError::SurfaceNotFound(parent))?;
        let positioner = self.make_positioner(&attributes.positioner)?;
        let surface = self.state.compositor.create_surface(&self.queue_handle);
        let popup = Popup::from_surface(
            parent_shared.protocol.xdg_surface(),
            &positioner,
            &self.queue_handle,
            surface,
            &self.state.xdg_shell,
        )?;
        if let Some(layer) = parent_shared.protocol.layer_surface() {
            layer.role().get_popup(popup.xdg_popup());
        }
        if let Some(serial) = attributes.grab.as_ref() {
            popup.xdg_popup().grab(&serial.seat, serial.serial);
        }
        popup.commit();

        Ok(self.insert_surface(
            ProtocolSurface::Popup(popup),
            Some(parent_shared),
            SurfaceKind::Popup,
        ))
    }

    pub fn reposition_popup(
        &mut self,
        surface: SurfaceId,
        positioner: &PopupPositioner,
        token: u32,
    ) -> Result<(), RuntimeError> {
        validate_positioner(positioner)?;
        if !self.capabilities.popup_reposition {
            return Err(RuntimeError::Unsupported("xdg-popup reposition"));
        }
        let positioner = self.make_positioner(positioner)?;
        let shared = self.surface_shared(surface)?;
        let ProtocolSurface::Popup(popup) = &shared.protocol else {
            return Err(RuntimeError::InvalidParent(surface));
        };
        popup.reposition(&positioner, token);
        Ok(())
    }

    pub fn surface_handle(&self, surface: SurfaceId) -> Option<SurfaceHandle> {
        self.state
            .surfaces
            .get(&surface)
            .cloned()
            .map(SurfaceHandle::from_sctk)
    }

    pub fn request_frame(&self, surface: SurfaceId) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let wl_surface = shared.wl_surface();
        wl_surface.frame(&self.queue_handle, FrameCallbackData(wl_surface.clone()));
        Ok(())
    }

    pub fn commit(&self, surface: SurfaceId) -> Result<(), RuntimeError> {
        self.surface_shared(surface)?.wl_surface().commit();
        Ok(())
    }

    /// Request a compositor activation token associated with `surface`.
    ///
    /// Completion is asynchronous and reported as
    /// [`Event::Activation`] carrying [`ActivationEvent::TokenDone`].
    /// Supplying a recent input serial generally gives the compositor enough
    /// context to issue an effective token, but all request attributes are
    /// optional in the protocol.
    pub fn request_activation_token(
        &mut self,
        surface: SurfaceId,
        attributes: ActivationTokenAttributes,
    ) -> Result<ActivationRequestId, RuntimeError> {
        self.activation_manager()?;
        let shared = self.surface_shared(surface)?;
        if let Some(serial) = attributes.serial.as_ref() {
            self.validate_activation_serial(serial)?;
        }

        let request = take_activation_request_id(&mut self.state.next_activation_request_id);
        self.state
            .xdg_activation
            .as_ref()
            .expect("activation support checked above")
            .request_token(
                &self.queue_handle,
                ActivationTokenPurpose::Export { request, surface },
                shared.wl_surface(),
                attributes,
            );
        Ok(request)
    }

    /// Activate `surface` with a token received from this runtime or through
    /// an external channel such as `XDG_ACTIVATION_TOKEN`.
    pub fn activate_surface(
        &self,
        surface: SurfaceId,
        token: ActivationToken,
    ) -> Result<(), RuntimeError> {
        let activation = self.activation_manager()?;
        let shared = self.surface_shared(surface)?;
        validate_activation_target(surface, shared.kind)?;
        activation.activate(shared.wl_surface(), token);
        Ok(())
    }

    /// Ask the compositor to draw attention to `surface`.
    ///
    /// This mirrors winit's Wayland path: request a surface-associated token
    /// and activate the same surface when the token arrives. Repeated requests
    /// are coalesced while one is pending.
    pub fn request_user_attention(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        self.activation_manager()?;
        let shared = self.surface_shared(surface)?;
        validate_activation_target(surface, shared.kind)?;
        if !begin_attention_request(&mut self.state.pending_attention, surface) {
            return Ok(());
        }
        self.state
            .xdg_activation
            .as_ref()
            .expect("activation support checked above")
            .request_token(
                &self.queue_handle,
                ActivationTokenPurpose::Attention { surface },
                shared.wl_surface(),
                ActivationTokenAttributes::default(),
            );
        Ok(())
    }

    pub fn set_window_geometry(
        &self,
        surface: SurfaceId,
        origin: LogicalPosition,
        size: LogicalSize,
    ) -> Result<(), RuntimeError> {
        if size.is_empty() {
            return Err(RuntimeError::Protocol(
                "window geometry must have non-zero dimensions".to_string(),
            ));
        }
        self.surface_shared(surface)?
            .protocol
            .xdg_surface()
            .ok_or(RuntimeError::InvalidWindowGeometryTarget(surface))?
            .set_window_geometry(
                origin.x,
                origin.y,
                u32_to_i32(size.width),
                u32_to_i32(size.height),
            );
        Ok(())
    }

    pub fn set_title(
        &self,
        surface: SurfaceId,
        title: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let toplevel = shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidParent(surface))?;
        toplevel.set_title(title.into());
        Ok(())
    }

    pub fn set_app_id(
        &self,
        surface: SurfaceId,
        app_id: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let toplevel = shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidParent(surface))?;
        toplevel.set_app_id(app_id.into());
        Ok(())
    }

    /// Replace the complete double-buffered state of a layer surface.
    ///
    /// Equal state is ignored and only changed protocol fields are sent. Call
    /// [`Runtime::commit`] to apply the update atomically.
    pub fn set_layer_surface_state(
        &self,
        surface: SurfaceId,
        state: LayerSurfaceState,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let layer = shared
            .protocol
            .layer_surface()
            .ok_or(RuntimeError::InvalidLayerSurfaceTarget(surface))?;
        layer.apply_state(state)?;
        Ok(())
    }

    pub fn layer_surface_state(
        &self,
        surface: SurfaceId,
    ) -> Result<LayerSurfaceState, RuntimeError> {
        let shared = self.surface_shared(surface)?;
        shared
            .protocol
            .layer_surface()
            .map(|layer| layer.state())
            .ok_or(RuntimeError::InvalidLayerSurfaceTarget(surface))
    }

    /// Begin a compositor-driven move using the newest pointer press that is
    /// still held over this toplevel.
    ///
    /// Call this while handling a pointer press. Wayland rejects requests made
    /// without the press serial for the active implicit pointer grab.
    pub fn begin_interactive_move(&self, surface: SurfaceId) -> Result<(), RuntimeError> {
        self.request_toplevel_interaction(surface, ToplevelInteraction::Move)
    }

    /// Begin a compositor-driven resize using the newest pointer press that is
    /// still held over this toplevel.
    pub fn begin_interactive_resize(
        &self,
        surface: SurfaceId,
        edge: ResizeEdge,
    ) -> Result<(), RuntimeError> {
        self.request_toplevel_interaction(surface, ToplevelInteraction::Resize(edge))
    }

    /// Show the compositor's window menu at a surface-local logical position.
    ///
    /// Call this while handling a pointer press so the runtime can supply the
    /// active implicit-grab serial required by xdg-shell.
    pub fn show_window_menu(
        &self,
        surface: SurfaceId,
        position: LogicalPosition,
    ) -> Result<(), RuntimeError> {
        self.request_toplevel_interaction(surface, ToplevelInteraction::WindowMenu(position))
    }

    /// Set or clear the icon for an individual toplevel.
    ///
    /// Named icons follow the active XDG icon theme. Pixel icons are copied
    /// into immutable premultiplied ARGB8888 SHM buffers. The assignment is
    /// double-buffered and becomes visible on the next surface commit.
    pub fn set_toplevel_icon(
        &self,
        surface: SurfaceId,
        icon: Option<ToplevelIcon>,
    ) -> Result<(), RuntimeError> {
        let manager = self
            .state
            .toplevel_icon_manager
            .as_ref()
            .ok_or(RuntimeError::Unsupported("xdg-toplevel-icon-v1"))?;
        let shared = self.surface_shared(surface)?;
        let toplevel = shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidToplevelIconTarget(surface))?;
        let applied = manager
            .set_icon(&self.queue_handle, &self.state.shm, toplevel, icon)
            .map_err(RuntimeError::Protocol)?;
        *shared
            .toplevel_icon
            .lock()
            .expect("toplevel icon mutex poisoned") = applied;
        Ok(())
    }

    /// Enable, update, or disable text input for a managed surface.
    ///
    /// The desired state is retained even while no seat focuses the surface.
    /// On a text-input-v3 `enter`, it is atomically resent with `enable`; later
    /// updates are committed without resetting the active input method.
    pub fn set_text_input_state(
        &mut self,
        surface: SurfaceId,
        state: Option<TextInputState>,
    ) -> Result<(), RuntimeError> {
        if self.state.text_input_manager.is_none() {
            return Err(RuntimeError::Unsupported("zwp-text-input-v3"));
        }
        let shared = self.surface_shared(surface)?;
        {
            let mut desired = shared
                .text_input
                .lock()
                .expect("surface text input mutex poisoned");
            if *desired == state {
                return Ok(());
            }
            *desired = state.clone();
        }

        for text_input in self
            .state
            .seats
            .values_mut()
            .filter_map(|objects| objects.text_input.as_mut())
        {
            text_input.update(surface, state.as_ref());
        }
        Ok(())
    }

    pub fn set_min_size(
        &self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let toplevel = shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidParent(surface))?;
        let size = size.unwrap_or_default();
        toplevel.set_min_size(u32_to_i32(size.width), u32_to_i32(size.height));
        Ok(())
    }

    pub fn set_max_size(
        &self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let toplevel = shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidParent(surface))?;
        let size = size.unwrap_or_default();
        toplevel.set_max_size(u32_to_i32(size.width), u32_to_i32(size.height));
        Ok(())
    }

    /// Set the integer buffer scale used to interpret attached renderer buffers.
    pub fn set_buffer_scale(&self, surface: SurfaceId, factor: i32) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let wl_surface = shared.wl_surface();
        if validate_buffer_scale(
            factor,
            shared.fractional_scale.is_some(),
            wl_surface.version(),
        )? {
            wl_surface.set_buffer_scale(factor);
        }
        Ok(())
    }

    /// Set the surface-local destination size used by wp-viewporter.
    ///
    /// Fractional-scale clients should keep `wl_surface.buffer_scale` at one,
    /// render a buffer sized from the preferred scale, and set this destination
    /// to the unscaled logical surface size. The change takes effect on the
    /// next surface commit. `None` unsets the destination.
    pub fn set_viewport_destination(
        &self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        validate_viewport_destination(size)?;
        let shared = self.surface_shared(surface)?;
        let fractional_scale = shared
            .fractional_scale
            .as_ref()
            .ok_or(RuntimeError::Unsupported("wp-viewporter"))?;
        fractional_scale.set_destination(size);
        Ok(())
    }

    /// Set the surface-local compositor background blur request.
    ///
    /// ext-background-effect-v1 must advertise its dynamic blur capability.
    /// Effect state is double-buffered with `wl_surface`; call
    /// [`Runtime::commit`] (or commit the renderer's next buffer) to make the
    /// change visible.
    pub fn set_blur(&self, surface: SurfaceId, state: BlurState) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let wl_surface = shared.wl_surface();
        let mut current = shared.blur.lock().expect("blur state mutex poisoned");

        match state {
            BlurState::Disabled => {
                current.take();
            }
            BlurState::Enabled(region) => {
                if !self.capabilities().ext_background_effect {
                    return Err(RuntimeError::Unsupported("ext-background-effect-v1 blur"));
                }
                if current.is_none() {
                    *current = Some(ManagedBlur(
                        self.state
                            .background_effect_state
                            .get_background_effect(wl_surface, &self.queue_handle)?,
                    ));
                }

                let blur_region = Region::new(&self.state.compositor)?;
                match region {
                    BlurRegion::EntireSurface => {
                        // NULL explicitly disables blur in this protocol, so
                        // use an oversized region clipped by the compositor.
                        blur_region.add(0, 0, i32::MAX, i32::MAX);
                    }
                    BlurRegion::Rectangles(rectangles) => {
                        for rectangle in rectangles.into_iter().filter(|rect| !rect.is_empty()) {
                            blur_region.add(
                                rectangle.origin.x,
                                rectangle.origin.y,
                                u32_to_i32(rectangle.size.width),
                                u32_to_i32(rectangle.size.height),
                            );
                        }
                    }
                }
                current
                    .as_ref()
                    .expect("blur was initialized")
                    .0
                    .set_blur_region(Some(blur_region.wl_region()));
            }
        }
        Ok(())
    }

    /// Retain the pointer constraint and relative-motion policy for a surface.
    ///
    /// A constraint is created only for a seat whose pointer currently focuses
    /// the surface. It is destroyed on leave and recreated from this retained
    /// state on a later enter, preventing conflicting constraint objects from
    /// accumulating on the same pointer. Equal states are ignored.
    pub fn set_pointer_capture_state(
        &mut self,
        surface: SurfaceId,
        state: PointerCaptureState,
    ) -> Result<(), RuntimeError> {
        validate_pointer_capture_state(&state)?;
        if state.constraint != PointerConstraint::None
            && !self.state.pointer_protocols.has_constraints()
        {
            return Err(RuntimeError::Unsupported("zwp-pointer-constraints-v1"));
        }
        if state.relative_motion && !self.state.pointer_protocols.has_relative_pointer() {
            return Err(RuntimeError::Unsupported("zwp-relative-pointer-v1"));
        }
        let shared = self.surface_shared(surface)?;
        if *shared
            .pointer_capture
            .lock()
            .expect("surface pointer capture mutex poisoned")
            == state
        {
            return Ok(());
        }
        let region = if state.constraint == PointerConstraint::None {
            None
        } else {
            make_pointer_constraint_region(&self.state.compositor, &state.region)?
        };

        for objects in self.state.seats.values_mut() {
            let Some(pointer) = objects.pointer.as_ref() else {
                continue;
            };
            objects.pointer_session.sync_capture(
                PointerCaptureTarget::new(
                    surface,
                    shared.wl_surface(),
                    pointer.pointer(),
                    region.as_ref().map(Region::wl_region),
                ),
                &state,
                &self.state.pointer_protocols,
                &self.queue_handle,
            )?;
        }
        *shared
            .pointer_capture
            .lock()
            .expect("surface pointer capture mutex poisoned") = state;
        Ok(())
    }

    /// Change only the constraint part of a surface's retained pointer state.
    pub fn set_pointer_constraint(
        &mut self,
        surface: SurfaceId,
        constraint: PointerConstraint,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let mut state = shared
            .pointer_capture
            .lock()
            .expect("surface pointer capture mutex poisoned")
            .clone();
        state.constraint = constraint;
        self.set_pointer_capture_state(surface, state)
    }

    /// Subscribe or unsubscribe one focused surface from relative motion.
    ///
    /// Relative events are otherwise suppressed to avoid doubling the normal
    /// pointer event stream. A locked pointer always receives them.
    pub fn set_relative_pointer_enabled(
        &mut self,
        surface: SurfaceId,
        enabled: bool,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let mut state = shared
            .pointer_capture
            .lock()
            .expect("surface pointer capture mutex poisoned")
            .clone();
        state.relative_motion = enabled;
        self.set_pointer_capture_state(surface, state)
    }

    /// Subscribe or unsubscribe a surface from semantic touchpad gestures.
    ///
    /// Gesture protocol objects are created lazily for live pointer seats when
    /// the first surface subscribes and destroyed when the final subscription
    /// disappears. This keeps applications that do not consume gestures at
    /// zero per-seat protocol and event overhead.
    ///
    /// Disabling a surface immediately drops any in-progress route for that
    /// surface and does not synthesize an `End` event; the caller initiating
    /// the unsubscribe should clear its corresponding UI state.
    pub fn set_pointer_gestures_enabled(
        &mut self,
        surface: SurfaceId,
        enabled: bool,
    ) -> Result<(), RuntimeError> {
        self.surface_shared(surface)?;
        if enabled && self.state.pointer_gesture_manager.is_none() {
            return Err(RuntimeError::Unsupported("zwp-pointer-gestures-v1"));
        }
        let change = self
            .state
            .pointer_gesture_subscriptions
            .set(surface, enabled);
        if !enabled && change != GestureSubscriptionChange::Unchanged {
            self.state.clear_pointer_gesture_surface(surface);
        }
        self.state
            .apply_pointer_gesture_subscription_change(change, &self.queue_handle);
        Ok(())
    }

    /// Whether a surface currently subscribes to pointer gesture events.
    pub fn pointer_gestures_enabled(&self, surface: SurfaceId) -> Result<bool, RuntimeError> {
        self.surface_shared(surface)?;
        Ok(self.state.pointer_gesture_subscriptions.contains(surface))
    }

    /// Change only the activation region of a surface's retained constraint.
    ///
    /// Region updates on an existing constraint are double-buffered with the
    /// target `wl_surface`; call [`Runtime::commit`] to apply the change.
    pub fn set_pointer_constraint_region(
        &mut self,
        surface: SurfaceId,
        region: PointerConstraintRegion,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let mut state = shared
            .pointer_capture
            .lock()
            .expect("surface pointer capture mutex poisoned")
            .clone();
        state.region = region;
        self.set_pointer_capture_state(surface, state)
    }

    /// Set the restoration hint used when a compositor releases a locked pointer.
    ///
    /// This does not warp the pointer. The request is double-buffered with the
    /// target `wl_surface`; call [`Runtime::commit`] to apply it.
    pub fn set_locked_pointer_position_hint(
        &self,
        surface: SurfaceId,
        position: (f64, f64),
    ) -> Result<(), RuntimeError> {
        if !position.0.is_finite() || !position.1.is_finite() {
            return Err(RuntimeError::Protocol(
                "locked pointer position hint must be finite".to_string(),
            ));
        }
        let shared = self.surface_shared(surface)?;
        if shared
            .pointer_capture
            .lock()
            .expect("surface pointer capture mutex poisoned")
            .constraint
            != PointerConstraint::Locked
        {
            return Err(RuntimeError::PointerNotLocked(surface));
        }
        for objects in self.state.seats.values() {
            objects
                .pointer_session
                .set_locked_position_hint(surface, position);
        }
        Ok(())
    }

    pub fn set_cursor(&self, icon: CursorIcon) -> Result<(), RuntimeError> {
        for objects in self.state.seats.values() {
            let Some(pointer) = objects.pointer.as_ref() else {
                continue;
            };
            if pointer
                .pointer()
                .data::<PointerData<()>>()
                .and_then(PointerData::latest_enter_serial)
                .is_none()
            {
                continue;
            }
            pointer
                .set_cursor(&self.connection, map_cursor_icon(icon))
                .map_err(|error| RuntimeError::Protocol(error.to_string()))?;
        }
        Ok(())
    }

    /// Remove a surface and every descendant from the runtime in child-first order.
    /// Renderer-held [`SurfaceHandle`] leases may keep those protocol objects alive;
    /// each child lease holds its parent so the protocol destruction order remains valid.
    pub fn destroy_surface(&mut self, surface: SurfaceId) -> Result<Vec<SurfaceId>, RuntimeError> {
        if !self.state.surfaces.contains_key(&surface) {
            return Err(RuntimeError::SurfaceNotFound(surface));
        }
        let mut order = Vec::new();
        collect_post_order(&self.state.children, surface, &mut order);
        for id in &order {
            self.state.remove_surface(*id);
        }
        Ok(order)
    }

}
