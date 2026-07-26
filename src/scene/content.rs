pub use tensor_protocol::{
    ContentRevision, SurfaceContent, SurfaceLayer, SurfaceTransform, SurfaceUvTransform,
};

/// Index range into `SceneSnapshot`'s flat surface-content table.
///
/// Keeping this range on a scene node avoids embedding protocol-owned trees in
/// the node itself. Subsurfaces and popups can be appended to the same table
/// without changing the renderer's stable view ordering.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContentSpan {
    start: u32,
    len: u32,
}

impl ContentSpan {
    pub(crate) fn new(start: usize, len: usize) -> Option<Self> {
        Some(Self {
            start: u32::try_from(start).ok()?,
            len: u32::try_from(len).ok()?,
        })
    }

    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    pub const fn len(self) -> usize {
        self.len as usize
    }

    pub(crate) fn range(self) -> Option<std::ops::Range<usize>> {
        let start = self.start as usize;
        start.checked_add(self.len as usize).map(|end| start..end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_spans_reject_unrepresentable_tables() {
        assert_eq!(ContentSpan::new(3, 2).unwrap().range(), Some(3..5));
        assert!(ContentSpan::new(usize::MAX, 1).is_none());
    }
}
