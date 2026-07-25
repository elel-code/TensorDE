impl ProvidesRegistryState for RuntimeState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers!(OutputState, SeatState);
}

impl OutputHandler for RuntimeState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if let Some(info) = output_info(&self.output_state, &output) {
            self.events.push(Event::Output(OutputEvent::Added(info)));
        }
    }

    fn update_output(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        if let Some(info) = output_info(&self.output_state, &output) {
            self.events
                .push(Event::Output(OutputEvent::Updated(info)));
        }
    }

    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        if let Some(info) = output_info(&self.output_state, &output) {
            self.events
                .push(Event::Output(OutputEvent::Removed(info.id)));
        }
    }
}

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for RuntimeState {
    fn event(
        _: &mut Self,
        _: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        unreachable!("zwlr_layer_shell_v1 has no events")
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, LayerSurfaceData> for RuntimeState {
    fn event(
        state: &mut Self,
        role: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        data: &LayerSurfaceData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(event) = handle_layer_event(role, event) else {
            return;
        };
        let Some(surface) = state.surface_id(data.wl_surface()) else {
            return;
        };
        match event {
            LayerProtocolEvent::Configure {
                suggested_size,
                serial,
            } => state
                .events
                .push(Event::LayerSurface(LayerSurfaceEvent::Configure {
                    surface,
                    suggested_size,
                    serial,
                })),
            LayerProtocolEvent::Closed => {
                if let Some(layer) = state
                    .surfaces
                    .get(&surface)
                    .and_then(|shared| shared.protocol.layer_surface())
                {
                    layer.mark_closed();
                }
                state
                    .events
                    .push(Event::LayerSurface(LayerSurfaceEvent::Closed {
                        surface,
                    }));
            }
        }
    }
}

impl BackgroundEffectHandler for RuntimeState {
    fn background_effect_state(&mut self) -> &mut BackgroundEffectState {
        &mut self.background_effect_state
    }

    fn update_capabilities(&mut self) {}
}

impl CompositorHandler for RuntimeState {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if let Some(surface) = self.surface_id(surface) {
            if self
                .surfaces
                .get(&surface)
                .is_some_and(|shared| shared.fractional_scale.is_some())
            {
                return;
            }
            self.events
                .push(Event::Surface(SurfaceEvent::ScaleFactorChanged {
                    surface,
                    factor: f64::from(factor),
                }));
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        time: u32,
    ) {
        if let Some(surface) = self.surface_id(surface) {
            self.events
                .push(Event::Surface(SurfaceEvent::Frame { surface, time }));
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

fn toplevel_state(configure: &WindowConfigure) -> ToplevelState {
    let mut state = ToplevelState::empty();
    state.set(ToplevelState::MAXIMIZED, configure.is_maximized());
    state.set(ToplevelState::FULLSCREEN, configure.is_fullscreen());
    state.set(ToplevelState::RESIZING, configure.is_resizing());
    state.set(ToplevelState::ACTIVATED, configure.is_activated());
    state.set(ToplevelState::TILED_LEFT, configure.is_tiled_left());
    state.set(ToplevelState::TILED_RIGHT, configure.is_tiled_right());
    state.set(ToplevelState::TILED_TOP, configure.is_tiled_top());
    state.set(ToplevelState::TILED_BOTTOM, configure.is_tiled_bottom());
    state.set(
        ToplevelState::SUSPENDED,
        configure
            .state
            .contains(smithay_client_toolkit::reexports::csd_frame::WindowState::SUSPENDED),
    );
    state
}

fn push_toplevel_configure(
    state: &mut RuntimeState,
    surface: &wl_surface::WlSurface,
    configure: WindowConfigure,
    serial: u32,
) {
    let Some(surface) = state.surface_id(surface) else {
        return;
    };
    let suggested_size = SuggestedSize::new(
        configure.new_size.0.map(|value| value.get()),
        configure.new_size.1.map(|value| value.get()),
    );
    state
        .events
        .push(Event::Surface(SurfaceEvent::Configure {
            surface,
            suggested_size,
            state: toplevel_state(&configure),
            serial,
        }));
}

impl WindowHandler for RuntimeState {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        if let Some(surface) = self.surface_id(window.wl_surface()) {
            self.events
                .push(Event::Surface(SurfaceEvent::CloseRequested { surface }));
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        serial: u32,
    ) {
        push_toplevel_configure(self, window.wl_surface(), configure, serial);
    }
}

impl DialogHandler for RuntimeState {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, dialog: &Dialog) {
        if let Some(surface) = self.surface_id(dialog.wl_surface()) {
            self.events
                .push(Event::Surface(SurfaceEvent::CloseRequested { surface }));
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        dialog: &Dialog,
        configure: WindowConfigure,
        serial: u32,
    ) {
        push_toplevel_configure(self, dialog.wl_surface(), configure, serial);
    }
}

impl PopupHandler for RuntimeState {
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        popup: &Popup,
        configure: PopupConfigure,
    ) {
        let Some(surface) = self.surface_id(popup.wl_surface()) else {
            return;
        };
        let kind = match configure.kind {
            ConfigureKind::Initial => PopupConfigureKind::Initial,
            ConfigureKind::Reactive => PopupConfigureKind::Reactive,
            ConfigureKind::Reposition { token } => PopupConfigureKind::Reposition { token },
            _ => PopupConfigureKind::Reactive,
        };
        self.events
            .push(Event::Surface(SurfaceEvent::PopupConfigure {
                surface,
                position: LogicalPosition::new(configure.position.0, configure.position.1),
                size: LogicalSize::new(
                    configure.width.max(0) as u32,
                    configure.height.max(0) as u32,
                ),
                serial: configure.serial,
                kind,
            }));
    }

    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        if let Some(surface) = self.surface_id(popup.wl_surface()) {
            self.events
                .push(Event::Surface(SurfaceEvent::PopupDone { surface }));
        }
    }
}

