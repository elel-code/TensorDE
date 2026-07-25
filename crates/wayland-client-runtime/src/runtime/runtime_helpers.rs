fn has_global(globals: &GlobalList, interface: &str) -> bool {
    globals
        .contents()
        .with_list(|list| list.iter().any(|global| global.interface == interface))
}

fn window_decorations(preference: DecorationPreference) -> WindowDecorations {
    match preference {
        DecorationPreference::Server => WindowDecorations::RequestServer,
        DecorationPreference::Client => WindowDecorations::ClientOnly,
        DecorationPreference::None => WindowDecorations::None,
    }
}

fn apply_toplevel_attributes(toplevel: &xdg_toplevel::XdgToplevel, value: &ToplevelAttributes) {
    toplevel.set_title(value.title.clone());
    toplevel.set_app_id(value.app_id.clone());
    let min = value.min_size.unwrap_or_default();
    let max = value.max_size.unwrap_or_default();
    toplevel.set_min_size(u32_to_i32(min.width), u32_to_i32(min.height));
    toplevel.set_max_size(u32_to_i32(max.width), u32_to_i32(max.height));
}

fn validate_positioner(positioner: &PopupPositioner) -> Result<(), RuntimeError> {
    if positioner.size.is_empty() {
        return Err(RuntimeError::InvalidPositioner(
            "popup size must be non-zero",
        ));
    }
    if positioner.anchor_rect.is_empty() {
        return Err(RuntimeError::InvalidPositioner(
            "anchor rectangle must be non-zero",
        ));
    }
    if positioner.parent_size.is_some_and(LogicalSize::is_empty) {
        return Err(RuntimeError::InvalidPositioner(
            "parent size must be non-zero",
        ));
    }
    Ok(())
}

fn u32_to_i32(value: u32) -> i32 {
    value.min(i32::MAX as u32) as i32
}

fn map_dnd_actions(actions: DndActions) -> WlDndAction {
    let mut mapped = WlDndAction::empty();
    if actions.contains(DndActions::COPY) {
        mapped |= WlDndAction::Copy;
    }
    if actions.contains(DndActions::MOVE) {
        mapped |= WlDndAction::Move;
    }
    if actions.contains(DndActions::ASK) {
        mapped |= WlDndAction::Ask;
    }
    mapped
}

fn map_dnd_action(action: DndAction) -> WlDndAction {
    match action {
        DndAction::Copy => WlDndAction::Copy,
        DndAction::Move => WlDndAction::Move,
        DndAction::Ask => WlDndAction::Ask,
    }
}

fn dnd_actions(actions: WlDndAction) -> DndActions {
    let mut mapped = DndActions::empty();
    if actions.contains(WlDndAction::Copy) {
        mapped |= DndActions::COPY;
    }
    if actions.contains(WlDndAction::Move) {
        mapped |= DndActions::MOVE;
    }
    if actions.contains(WlDndAction::Ask) {
        mapped |= DndActions::ASK;
    }
    mapped
}

fn dnd_action(action: WlDndAction) -> Option<DndAction> {
    if action.contains(WlDndAction::Ask) {
        Some(DndAction::Ask)
    } else if action.contains(WlDndAction::Move) {
        Some(DndAction::Move)
    } else if action.contains(WlDndAction::Copy) {
        Some(DndAction::Copy)
    } else {
        None
    }
}

fn map_cursor_icon(icon: CursorIcon) -> SctkCursorIcon {
    match icon {
        CursorIcon::ColResize => SctkCursorIcon::ColResize,
        CursorIcon::Default => SctkCursorIcon::Default,
        CursorIcon::Pointer => SctkCursorIcon::Pointer,
        CursorIcon::Text => SctkCursorIcon::Text,
    }
}

fn prepare_dnd_icon_surface(
    state: &mut RuntimeState,
    queue_handle: &QueueHandle<RuntimeState>,
    icon: DndIcon,
) -> Result<DndIconSurface, RuntimeError> {
    let (rgba, width, height, buffer_scale, offset) = icon.into_parts();
    let mut pool = SlotPool::new(rgba.len(), &state.shm)
        .map_err(|error| RuntimeError::Protocol(error.to_string()))?;
    let stride = i32::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| RuntimeError::Protocol("DnD icon stride overflow".to_string()))?;
    let (buffer, canvas) = pool
        .create_buffer(
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Argb8888,
        )
        .map_err(|error| RuntimeError::Protocol(error.to_string()))?;
    copy_rgba_to_premultiplied_argb8888(&rgba, canvas);

    let surface = state.compositor.create_surface(queue_handle);
    surface.set_buffer_scale(buffer_scale);
    if let Err(error) = buffer.attach_to(&surface) {
        surface.destroy();
        return Err(RuntimeError::Protocol(error.to_string()));
    }
    if offset != LogicalPosition::ZERO {
        if surface.version() >= 5 {
            surface.offset(offset.x, offset.y);
        } else {
            surface.attach(Some(buffer.wl_buffer()), offset.x, offset.y);
        }
    }
    surface.damage(0, 0, i32::MAX, i32::MAX);
    Ok(DndIconSurface {
        surface,
        _buffer: buffer,
    })
}

fn map_anchor(value: PopupAnchor) -> xdg_positioner::Anchor {
    match value {
        PopupAnchor::None => xdg_positioner::Anchor::None,
        PopupAnchor::Top => xdg_positioner::Anchor::Top,
        PopupAnchor::Bottom => xdg_positioner::Anchor::Bottom,
        PopupAnchor::Left => xdg_positioner::Anchor::Left,
        PopupAnchor::Right => xdg_positioner::Anchor::Right,
        PopupAnchor::TopLeft => xdg_positioner::Anchor::TopLeft,
        PopupAnchor::BottomLeft => xdg_positioner::Anchor::BottomLeft,
        PopupAnchor::TopRight => xdg_positioner::Anchor::TopRight,
        PopupAnchor::BottomRight => xdg_positioner::Anchor::BottomRight,
    }
}

fn map_gravity(value: Gravity) -> xdg_positioner::Gravity {
    match value {
        Gravity::None => xdg_positioner::Gravity::None,
        Gravity::Top => xdg_positioner::Gravity::Top,
        Gravity::Bottom => xdg_positioner::Gravity::Bottom,
        Gravity::Left => xdg_positioner::Gravity::Left,
        Gravity::Right => xdg_positioner::Gravity::Right,
        Gravity::TopLeft => xdg_positioner::Gravity::TopLeft,
        Gravity::BottomLeft => xdg_positioner::Gravity::BottomLeft,
        Gravity::TopRight => xdg_positioner::Gravity::TopRight,
        Gravity::BottomRight => xdg_positioner::Gravity::BottomRight,
    }
}

fn map_constraints(value: crate::ConstraintAdjustments) -> xdg_positioner::ConstraintAdjustment {
    let mut result = xdg_positioner::ConstraintAdjustment::empty();
    if value.contains(crate::ConstraintAdjustments::SLIDE_X) {
        result |= xdg_positioner::ConstraintAdjustment::SlideX;
    }
    if value.contains(crate::ConstraintAdjustments::SLIDE_Y) {
        result |= xdg_positioner::ConstraintAdjustment::SlideY;
    }
    if value.contains(crate::ConstraintAdjustments::FLIP_X) {
        result |= xdg_positioner::ConstraintAdjustment::FlipX;
    }
    if value.contains(crate::ConstraintAdjustments::FLIP_Y) {
        result |= xdg_positioner::ConstraintAdjustment::FlipY;
    }
    if value.contains(crate::ConstraintAdjustments::RESIZE_X) {
        result |= xdg_positioner::ConstraintAdjustment::ResizeX;
    }
    if value.contains(crate::ConstraintAdjustments::RESIZE_Y) {
        result |= xdg_positioner::ConstraintAdjustment::ResizeY;
    }
    result
}
