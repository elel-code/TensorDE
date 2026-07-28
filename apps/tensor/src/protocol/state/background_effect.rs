//! `ext-background-effect-v1` → scene `EffectStyle` bridge.
//!
//! The protocol is double-buffered on the surface. Tensor samples committed
//! blur state on `wl_surface.commit` and stores only value-only
//! [`BackdropBlur`] on the view (or layer merge path). Radius is compositor
//! policy; region emptiness is evaluated without allocating.

use wayland_server::protocol::wl_surface::WlSurface;

use crate::scene::{BackdropBlur, EffectStyle};

use super::RuntimeState;

/// Compositor policy blur radius in logical pixels (protocol leaves algorithm
/// to the compositor). Kept small so damage expansion stays cheap until the
/// Vulkan blur pass lands.
pub(crate) const BACKGROUND_BLUR_RADIUS: u32 = 16;

/// Map a committed blur region into a scene backdrop effect.
///
/// - `None` / empty region → no backdrop sampling (protocol initial + null).
/// - Any non-empty region → enable blur at [`BACKGROUND_BLUR_RADIUS`].
///
/// Precise region clips are reserved for the future GPU blur pass; damage
/// already expands via [`SceneNode::samples_background`].
pub(crate) const fn backdrop_blur_from_area(has_area: bool) -> Option<BackdropBlur> {
    if has_area {
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
        let Some(mut effects) = self.world.view_effects(view_id) else {
            return false;
        };
        let blur =
            backdrop_blur_from_area(self.protocol_globals.committed_background_has_area(root));
        if effects.backdrop_blur == blur {
            return false;
        }
        effects.backdrop_blur = blur;
        self.world
            .set_view_effects(view_id, effects)
            .unwrap_or(false)
    }

    /// Effects for a layer / lock surface that is not an ECS view.
    pub(crate) fn layer_surface_effects(&self, surface: &WlSurface) -> EffectStyle {
        EffectStyle {
            backdrop_blur: backdrop_blur_from_area(
                self.protocol_globals.committed_background_has_area(surface),
            ),
            ..EffectStyle::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_area_disables_blur() {
        assert_eq!(backdrop_blur_from_area(false), None);
    }

    #[test]
    fn add_rect_enables_compositor_radius() {
        assert_eq!(
            backdrop_blur_from_area(true),
            Some(BackdropBlur {
                radius: BACKGROUND_BLUR_RADIUS
            })
        );
    }
}
