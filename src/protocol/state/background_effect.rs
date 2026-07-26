//! `ext-background-effect-v1` → scene `EffectStyle` bridge.
//!
//! The protocol is double-buffered on the surface. Tensor samples committed
//! blur state on `wl_surface.commit` and stores only value-only
//! [`BackdropBlur`] on the view (or layer merge path). Radius is compositor
//! policy; region emptiness is evaluated without allocating.

use smithay::wayland::{
    background_effect::BackgroundEffectSurfaceCachedState,
    compositor::{RegionAttributes, with_states},
};
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
pub(crate) fn backdrop_blur_from_region(region: Option<&RegionAttributes>) -> Option<BackdropBlur> {
    let region = region?;
    if !region_has_area(region) {
        return None;
    }
    Some(BackdropBlur {
        radius: BACKGROUND_BLUR_RADIUS,
    })
}

fn region_has_area(region: &RegionAttributes) -> bool {
    // Presence of any Add rect is enough to enable sampling. Subtract-only
    // regions never contribute area; full geometry is compositor-side later.
    use smithay::wayland::compositor::RectangleKind;
    region.rects.iter().any(|(kind, rect)| {
        matches!(kind, RectangleKind::Add) && rect.size.w > 0 && rect.size.h > 0
    })
}

/// Committed blur state for a surface (after double-buffer apply).
pub(crate) fn committed_backdrop_blur(surface: &WlSurface) -> Option<BackdropBlur> {
    with_states(surface, |states| {
        let mut cached = states
            .cached_state
            .get::<BackgroundEffectSurfaceCachedState>();
        backdrop_blur_from_region(cached.current().blur_region.as_ref())
    })
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
        let blur = committed_backdrop_blur(root);
        if effects.backdrop_blur == blur {
            return false;
        }
        effects.backdrop_blur = blur;
        self.world
            .set_view_effects(view_id, effects)
            .unwrap_or(false)
    }

    /// Effects for a layer / lock surface that is not an ECS view.
    pub(crate) fn layer_surface_effects(surface: &WlSurface) -> EffectStyle {
        EffectStyle {
            backdrop_blur: committed_backdrop_blur(surface),
            ..EffectStyle::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::{
        utils::{Logical, Rectangle},
        wayland::compositor::RectangleKind,
    };

    #[test]
    fn null_and_empty_regions_disable_blur() {
        assert_eq!(backdrop_blur_from_region(None), None);
        assert_eq!(
            backdrop_blur_from_region(Some(&RegionAttributes { rects: vec![] })),
            None
        );
    }

    #[test]
    fn add_rect_enables_compositor_radius() {
        let region = RegionAttributes {
            rects: vec![(
                RectangleKind::Add,
                Rectangle::<i32, Logical>::new((0, 0).into(), (100, 50).into()),
            )],
        };
        assert_eq!(
            backdrop_blur_from_region(Some(&region)),
            Some(BackdropBlur {
                radius: BACKGROUND_BLUR_RADIUS
            })
        );
    }

    #[test]
    fn zero_size_add_does_not_enable_blur() {
        let region = RegionAttributes {
            rects: vec![(
                RectangleKind::Add,
                Rectangle::<i32, Logical>::new((0, 0).into(), (0, 10).into()),
            )],
        };
        assert_eq!(backdrop_blur_from_region(Some(&region)), None);
    }
}
