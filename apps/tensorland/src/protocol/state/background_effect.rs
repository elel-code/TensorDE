//! `ext-background-effect-v1` → scene `EffectStyle` bridge.
//!
//! The protocol is double-buffered on the surface. Tensor samples committed
//! blur state on `wl_surface.commit`, normalizes add/subtract operations once,
//! and stores a value-only [`BackdropBlur`] plus exact [`BackdropRegion`] on
//! the view (or layer merge path). Radius remains compositor policy.

use wayland_server::protocol::wl_surface::WlSurface;

use tensor_util::Rect;

use crate::scene::{BackdropBlur, BackdropRegion, EffectStyle};

use super::{RuntimeState, surfaces::surface_logical_size};

/// Compositor policy blur radius in logical pixels (protocol leaves algorithm
/// to the compositor). Kept small so damage expansion stays cheap until the
/// Vulkan blur pass lands.
pub(crate) const BACKGROUND_BLUR_RADIUS: u32 = 16;

pub(crate) const fn backdrop_blur_from_region(has_region: bool) -> Option<BackdropBlur> {
    if has_region {
        Some(BackdropBlur {
            radius: BACKGROUND_BLUR_RADIUS,
        })
    } else {
        None
    }
}

impl RuntimeState {
    /// Sync ECS view effects from the root surface's committed blur region.
    ///
    /// Returns `true` when the view's [`EffectStyle`] changed (caller should
    /// queue redraw). Cost is one cached-state read plus an equality check.
    pub(crate) fn sync_view_background_effect(&mut self, root: &WlSurface) -> bool {
        let Some(view_id) = self.view_for_surface(root) else {
            return false;
        };
        let Some(protocol_region) = self.protocol_globals.committed_background_region(root) else {
            return self
                .world
                .set_view_backdrop_effect(view_id, None, None)
                .unwrap_or(false);
        };
        let Some(window_geometry) = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(root))
            .map(|window| window.geometry())
        else {
            return false;
        };
        let size = crate::protocol::globals::compositor::with_states(root, |states| {
            surface_logical_size(states)
        })
        .or_else(|| {
            Some(tensor_util::Size::new(
                u32::try_from(window_geometry.size.w).ok()?,
                u32::try_from(window_geometry.size.h).ok()?,
            ))
        });
        let Some(size) = size.filter(|size| size.width > 0 && size.height > 0) else {
            return false;
        };
        let region = protocol_region.to_scene_region(
            Rect::new(0, 0, size.width, size.height),
            (
                window_geometry.loc.x.saturating_neg(),
                window_geometry.loc.y.saturating_neg(),
            ),
        );
        let blur = backdrop_blur_from_region(region.is_some());
        self.world
            .set_view_backdrop_effect(view_id, blur, region)
            .unwrap_or(false)
    }

    /// Effects for a layer / lock surface that is not an ECS view.
    pub(crate) fn layer_surface_background_effect(
        &self,
        surface: &WlSurface,
        bounds: Rect,
    ) -> (EffectStyle, Option<BackdropRegion>) {
        let region = self
            .protocol_globals
            .committed_background_region(surface)
            .and_then(|region| {
                region.to_scene_region(Rect::new(0, 0, bounds.width, bounds.height), (0, 0))
            });
        (
            EffectStyle {
                backdrop_blur: backdrop_blur_from_region(region.is_some()),
                ..EffectStyle::default()
            },
            region,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_area_disables_blur() {
        assert_eq!(backdrop_blur_from_region(false), None);
    }

    #[test]
    fn add_rect_enables_compositor_radius() {
        assert_eq!(
            backdrop_blur_from_region(true),
            Some(BackdropBlur {
                radius: BACKGROUND_BLUR_RADIUS
            })
        );
    }
}
