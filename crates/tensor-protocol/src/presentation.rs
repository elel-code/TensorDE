//! Value-only surface presentation semantics.

/// Client hint committed for one surface.
///
/// This is a hint rather than a presentation guarantee. Product policy and
/// KMS capability checks decide whether an asynchronous flip is safe.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SurfacePresentationHint {
    #[default]
    Vsync,
    Async,
}
