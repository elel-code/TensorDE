fn supports_ext_background_blur(capabilities: Option<BackgroundEffectCapability>) -> bool {
    capabilities.is_some_and(|value| value.contains(BackgroundEffectCapability::Blur))
}

/// Validate a buffer-scale update and report whether a wire request is needed.
///
/// wl_surface v1/v2 have an implicit, immutable scale of one. Treating one as
/// a no-op keeps those compositors usable without sending the v3 request that
/// would otherwise terminate the connection.
fn validate_buffer_scale(
    factor: i32,
    fractional_scale: bool,
    surface_version: u32,
) -> Result<bool, RuntimeError> {
    if factor < 1 {
        return Err(RuntimeError::Protocol(
            "buffer scale must be at least one".to_string(),
        ));
    }
    if fractional_scale && factor != 1 {
        return Err(RuntimeError::Protocol(
            "buffer scale must remain one while fractional scaling is active".to_string(),
        ));
    }
    if surface_version < 3 {
        if factor == 1 {
            return Ok(false);
        }
        return Err(RuntimeError::Unsupported(
            "integer buffer scaling on wl_surface versions below 3",
        ));
    }
    Ok(true)
}

fn validate_viewport_destination(size: Option<LogicalSize>) -> Result<(), RuntimeError> {
    if size.is_some_and(LogicalSize::is_empty) {
        return Err(RuntimeError::Protocol(
            "viewport destination must have non-zero dimensions".to_string(),
        ));
    }
    Ok(())
}

fn make_pointer_constraint_region(
    compositor: &CompositorState,
    region: &PointerConstraintRegion,
) -> Result<Option<Region>, RuntimeError> {
    let PointerConstraintRegion::Rectangles(rectangles) = region else {
        return Ok(None);
    };
    let wire_region = Region::new(compositor)?;
    for rectangle in rectangles {
        wire_region.add(
            rectangle.origin.x,
            rectangle.origin.y,
            rectangle.size.width as i32,
            rectangle.size.height as i32,
        );
    }
    Ok(Some(wire_region))
}

fn validate_activation_target(surface: SurfaceId, kind: SurfaceKind) -> Result<(), RuntimeError> {
    match kind {
        SurfaceKind::Toplevel | SurfaceKind::Dialog => Ok(()),
        SurfaceKind::Popup | SurfaceKind::Layer => {
            Err(RuntimeError::InvalidActivationTarget(surface))
        }
    }
}

fn take_activation_request_id(next: &mut u64) -> ActivationRequestId {
    let request = ActivationRequestId(*next);
    *next = next.wrapping_add(1).max(1);
    request
}

fn begin_attention_request(pending: &mut HashSet<SurfaceId>, surface: SurfaceId) -> bool {
    pending.insert(surface)
}

