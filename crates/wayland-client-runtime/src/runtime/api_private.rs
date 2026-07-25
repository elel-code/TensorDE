impl Runtime {
    fn insert_surface(
        &mut self,
        protocol: ProtocolSurface,
        parent: Option<Arc<SurfaceShared>>,
        kind: SurfaceKind,
    ) -> SurfaceId {
        let id = SurfaceId(self.state.next_surface_id);
        self.state.next_surface_id += 1;
        let protocol_id = protocol.wl_surface().id();
        let parent_id = parent.as_ref().map(|parent| parent.id);
        let fractional_scale = self
            .state
            .fractional_scale_manager
            .as_ref()
            .map(|manager| manager.create_surface(protocol.wl_surface(), &self.queue_handle));
        let shared = Arc::new(SurfaceShared {
            blur: Default::default(),
            fractional_scale,
            pointer_capture: Default::default(),
            text_input: Default::default(),
            toplevel_icon: Default::default(),
            protocol,
            parent,
            connection: self.connection.clone(),
            id,
            kind,
        });
        self.state.surface_ids.insert(protocol_id, id);
        self.state.surfaces.insert(id, shared);
        if let Some(parent_id) = parent_id {
            self.state.children.entry(parent_id).or_default().push(id);
        }
        id
    }

    fn surface_shared(&self, surface: SurfaceId) -> Result<Arc<SurfaceShared>, RuntimeError> {
        self.state
            .surfaces
            .get(&surface)
            .cloned()
            .ok_or(RuntimeError::SurfaceNotFound(surface))
    }

    fn activation_manager(&self) -> Result<&ActivationManager, RuntimeError> {
        self.state
            .xdg_activation
            .as_ref()
            .ok_or(RuntimeError::Unsupported("xdg-activation-v1"))
    }

    fn validate_activation_serial(&self, serial: &InputSerial) -> Result<(), RuntimeError> {
        let same_connection =
            serial.seat.backend().upgrade().as_ref() == Some(&self.connection.backend());
        if same_connection {
            Ok(())
        } else {
            Err(RuntimeError::ForeignActivationSerial)
        }
    }

    fn parent_toplevel(&self, parent: SurfaceId) -> Result<Arc<SurfaceShared>, RuntimeError> {
        let shared = self.surface_shared(parent)?;
        if shared.protocol.xdg_toplevel().is_none() {
            return Err(RuntimeError::InvalidParent(parent));
        }
        Ok(shared)
    }

    fn request_toplevel_interaction(
        &self,
        surface: SurfaceId,
        interaction: ToplevelInteraction,
    ) -> Result<(), RuntimeError> {
        let shared = self.surface_shared(surface)?;
        let toplevel = shared
            .protocol
            .xdg_toplevel()
            .ok_or(RuntimeError::InvalidToplevelInteractionTarget(surface))?;
        let candidates = self.state.seats.values().filter_map(|objects| {
            let pointer = objects.pointer.as_ref()?.pointer();
            let seat = pointer.data::<PointerData<()>>()?.seat().clone();
            Some((
                seat,
                objects.pointer_session.focus(),
                true,
                objects.pointer_presses.latest_for_surface(surface),
            ))
        });
        let (seat, press) = select_active_pointer_press(surface, candidates)
            .ok_or(RuntimeError::InvalidToplevelInteractionSerial)?;
        interaction.send(toplevel, &seat, press.serial);
        Ok(())
    }

    fn resolve_output(&self, id: OutputId) -> Result<wl_output::WlOutput, RuntimeError> {
        self.state
            .output_state
            .outputs()
            .find(|output| {
                self.state
                    .output_state
                    .info(output)
                    .is_some_and(|info| info.id == id.get())
            })
            .ok_or(RuntimeError::OutputNotFound(id))
    }

    fn make_positioner(&self, value: &PopupPositioner) -> Result<XdgPositioner, RuntimeError> {
        let positioner = XdgPositioner::new(&self.state.xdg_shell)?;
        positioner.set_size(u32_to_i32(value.size.width), u32_to_i32(value.size.height));
        positioner.set_anchor_rect(
            value.anchor_rect.origin.x,
            value.anchor_rect.origin.y,
            u32_to_i32(value.anchor_rect.size.width),
            u32_to_i32(value.anchor_rect.size.height),
        );
        positioner.set_anchor(map_anchor(value.anchor));
        positioner.set_gravity(map_gravity(value.gravity));
        positioner.set_constraint_adjustment(map_constraints(value.constraints));
        positioner.set_offset(value.offset.x, value.offset.y);
        if positioner.version() >= 3 {
            if value.reactive {
                positioner.set_reactive();
            }
            if let Some(parent_size) = value.parent_size {
                positioner.set_parent_size(
                    u32_to_i32(parent_size.width),
                    u32_to_i32(parent_size.height),
                );
            }
            if let Some(serial) = value.parent_configure {
                positioner.set_parent_configure(serial);
            }
        }
        Ok(positioner)
    }
}

