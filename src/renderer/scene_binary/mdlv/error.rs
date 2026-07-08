//! MDLV reader error boundary.
//!
//! Reference:
//! - `reverse-engineered/docs/mdl-format.md`

use std::path::Path;

use crate::renderer::RendererPlanError;

pub(super) fn mdlv_error(path: &Path, message: &str) -> RendererPlanError {
    RendererPlanError::PackageLoad(format!(
        "failed to read MDLV raw scene geometry {}: {message}",
        path.display()
    ))
}
