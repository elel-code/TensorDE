//! Native desktop shell built on the shared Wayland and Vulkan standards.
//!
//! The shell owns desktop-level surfaces. Application chrome belongs to the
//! applications themselves and is deliberately outside this crate.

mod layout;
mod model;
mod runtime;
mod surfaces;

pub use layout::{ShellLayout, ShellLayoutError};
pub use model::{ShellComponent, ShellModel};
pub use runtime::ShellRuntime;
pub use surfaces::{SurfacePlan, surface_plan};
pub use vulkan_renderer as gpu;
