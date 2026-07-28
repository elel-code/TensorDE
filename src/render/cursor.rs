use tensor_protocol::SurfaceSampleTransform;
use tensor_util::Rect;

use crate::ecs::SurfaceBufferId;

pub(crate) const MAX_CURSOR_OVERLAYS: usize = 65;

/// A compositor-owned cursor primitive already expressed in one output's
/// physical coordinate space. It deliberately contains no protocol object or
/// Vulkan handle: the protocol boundary decides visibility and placement,
/// while the renderer owns the final overlay draw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CursorOverlay {
    pub(crate) source: u64,
    pub(crate) destination: Rect,
    pub(crate) clip: Rect,
    pub(crate) texture: Option<CursorTexture>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CursorTexture {
    pub(crate) buffer_id: SurfaceBufferId,
    pub(crate) sample_transform: SurfaceSampleTransform,
}

impl CursorOverlay {
    const EMPTY: Self = Self {
        source: 0,
        destination: Rect::new(0, 0, 0, 0),
        clip: Rect::new(0, 0, 0, 0),
        texture: None,
    };

    pub(crate) fn new(source: u64, destination: Rect, viewport: Rect) -> Option<Self> {
        let clip = destination.intersection(viewport)?;
        Some(Self {
            source,
            destination,
            clip,
            texture: None,
        })
    }

    pub(crate) fn with_texture(mut self, texture: CursorTexture) -> Self {
        self.texture = Some(texture);
        self
    }
}

/// Fixed-capacity cursor batch for one output frame. Slot zero is available
/// for the core pointer; the remaining slots cover Tensor's 64 tablet tools.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CursorOverlays {
    entries: [CursorOverlay; MAX_CURSOR_OVERLAYS],
    len: usize,
}

impl Default for CursorOverlays {
    fn default() -> Self {
        Self {
            entries: [CursorOverlay::EMPTY; MAX_CURSOR_OVERLAYS],
            len: 0,
        }
    }
}

impl CursorOverlays {
    pub(crate) fn push(&mut self, overlay: CursorOverlay) -> bool {
        if self.len > 0 && self.entries[self.len - 1].source >= overlay.source {
            return false;
        }
        let Some(slot) = self.entries.get_mut(self.len) else {
            return false;
        };
        *slot = overlay;
        self.len += 1;
        true
    }

    pub(crate) fn as_slice(&self) -> &[CursorOverlay] {
        &self.entries[..self.len]
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_overlay_keeps_its_hotspot_origin_and_clips_to_output() {
        let viewport = Rect::new(0, 0, 100, 80);
        let overlay = CursorOverlay::new(0, Rect::new(-3, 70, 24, 24), viewport).unwrap();

        assert_eq!(overlay.destination, Rect::new(-3, 70, 24, 24));
        assert_eq!(overlay.clip, Rect::new(0, 70, 21, 10));
    }

    #[test]
    fn cursor_overlay_is_absent_when_its_destination_misses_output() {
        assert_eq!(
            CursorOverlay::new(0, Rect::new(100, 0, 24, 24), Rect::new(0, 0, 100, 80)),
            None
        );
    }

    #[test]
    fn cursor_batch_has_fixed_capacity_and_borrowed_contents() {
        let mut cursors = CursorOverlays::default();
        for x in 0..MAX_CURSOR_OVERLAYS {
            assert!(
                cursors.push(
                    CursorOverlay::new(
                        x as u64,
                        Rect::new(x as i32, 0, 1, 1),
                        Rect::new(0, 0, MAX_CURSOR_OVERLAYS as u32, 1),
                    )
                    .unwrap()
                )
            );
        }
        assert!(!cursors.push(CursorOverlay::EMPTY));
        assert_eq!(cursors.as_slice().len(), MAX_CURSOR_OVERLAYS);
    }
}
