use tensor_util::{Rect, Size};
use vulkan_renderer::TextureFormat;

/// Stable protocol-to-renderer identity for one bounded output capture.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct OutputCaptureId(u64);

impl OutputCaptureId {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }
}

/// Value-only frame-side tap request in output-local physical pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OutputCaptureRequest {
    pub(crate) id: OutputCaptureId,
    pub(crate) region: Rect,
    /// Whether software cursor overlays belong in the captured pixels.
    pub(crate) draw_cursors: bool,
}

impl OutputCaptureRequest {
    pub(crate) const fn extent(self) -> Size {
        Size::new(self.region.width, self.region.height)
    }

    pub(crate) const fn tap_before_software_cursors(self) -> bool {
        !self.draw_cursors
    }
}

/// Completed mapped readback, consumed only on the compositor thread.
#[derive(Debug)]
pub(crate) struct OutputCapturePixels {
    pub(crate) id: OutputCaptureId,
    pub(crate) size: Size,
    pub(crate) format: TextureFormat,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug)]
pub(crate) enum OutputCaptureResult {
    Ready(OutputCapturePixels),
    Failed { id: OutputCaptureId, reason: String },
}

impl OutputCaptureResult {
    pub(crate) const fn id(&self) -> OutputCaptureId {
        match self {
            Self::Ready(pixels) => pixels.id,
            Self::Failed { id, .. } => *id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_cursor_option_selects_the_capture_boundary() {
        let request = OutputCaptureRequest {
            id: OutputCaptureId::new(1),
            region: Rect::new(0, 0, 64, 64),
            draw_cursors: false,
        };
        assert!(request.tap_before_software_cursors());
        assert!(
            !OutputCaptureRequest {
                draw_cursors: true,
                ..request
            }
            .tap_before_software_cursors()
        );
    }
}
