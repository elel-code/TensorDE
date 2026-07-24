use tensor_util::Rect;

/// A compositor-owned cursor primitive already expressed in one output's
/// physical coordinate space. It deliberately contains no protocol object or
/// Vulkan handle: the protocol boundary decides visibility and placement,
/// while the renderer owns the final overlay draw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CursorOverlay {
    pub(crate) destination: Rect,
    pub(crate) clip: Rect,
}

impl CursorOverlay {
    pub(crate) fn new(destination: Rect, viewport: Rect) -> Option<Self> {
        let clip = destination.intersection(viewport)?;
        Some(Self { destination, clip })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_overlay_keeps_its_hotspot_origin_and_clips_to_output() {
        let viewport = Rect::new(0, 0, 100, 80);
        let overlay = CursorOverlay::new(Rect::new(-3, 70, 24, 24), viewport).unwrap();

        assert_eq!(overlay.destination, Rect::new(-3, 70, 24, 24));
        assert_eq!(overlay.clip, Rect::new(0, 70, 21, 10));
    }

    #[test]
    fn cursor_overlay_is_absent_when_its_destination_misses_output() {
        assert_eq!(
            CursorOverlay::new(Rect::new(100, 0, 24, 24), Rect::new(0, 0, 100, 80)),
            None
        );
    }
}
