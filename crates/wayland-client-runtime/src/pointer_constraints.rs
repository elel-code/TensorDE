//! Pointer constraint public types. 

use crate::geometry::LogicalRect;
use crate::SurfaceId;

/// Desired constraint for a pointer focused on a surface.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PointerConstraint {
    #[default]
    None,
    Confined,
    Locked,
}

/// Declarative pointer protocol state retained for one surface.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct PointerCaptureState {
    /// Confinement or lock requested while a pointer focuses the surface.
    pub constraint: PointerConstraint,
    /// Emit high-frequency relative motion while the surface is focused.
    /// Locked pointers always emit relative motion, regardless of this flag.
    pub relative_motion: bool,
    /// Region in which the constraint may activate.
    pub region: PointerConstraintRegion,
}

/// Surface-local region used by a pointer lock or confinement.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub enum PointerConstraintRegion {
    /// Use the surface's current input region, as represented by a NULL region
    /// in pointer-constraints-v1.
    #[default]
    SurfaceInput,
    /// Use the union of these surface-local rectangles. An empty vector is an
    /// intentionally empty region.
    Rectangles(Vec<LogicalRect>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PointerConstraintError {
    #[error("pointer constraint rectangle {index} has an empty dimension")]
    EmptyRectangle { index: usize },
    #[error("pointer constraint rectangle {index} exceeds Wayland integer limits")]
    RectangleTooLarge { index: usize },
}

/// A compositor transition for a pointer constraint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PointerConstraintEvent {
    pub surface: SurfaceId,
    pub constraint: PointerConstraint,
    pub active: bool,
}

/// Unaccelerated and accelerated motion from `zwp_relative_pointer_v1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RelativePointerEvent {
    pub surface: SurfaceId,
    /// Monotonic timestamp supplied by the compositor, in microseconds.
    pub time_micros: u64,
    pub delta: (f64, f64),
    pub delta_unaccelerated: (f64, f64),
    /// Seat that owns the relative-pointer stream, when known.
    pub seat: Option<crate::SeatId>,
}

#[allow(dead_code)]
const fn wants_relative_pointer(capture: &PointerCaptureState) -> bool {
    capture.relative_motion || matches!(capture.constraint, PointerConstraint::Locked)
}

#[allow(dead_code)]
pub(crate) fn validate_pointer_capture_state(
    state: &PointerCaptureState,
) -> Result<(), PointerConstraintError> {
    let PointerConstraintRegion::Rectangles(rectangles) = &state.region else {
        return Ok(());
    };
    for (index, rectangle) in rectangles.iter().enumerate() {
        if rectangle.is_empty() {
            return Err(PointerConstraintError::EmptyRectangle { index });
        }
        if rectangle.size.width > i32::MAX as u32 || rectangle.size.height > i32::MAX as u32 {
            return Err(PointerConstraintError::RectangleTooLarge { index });
        }
    }
    Ok(())
}


