use bitflags::bitflags;

use crate::{LogicalPosition, SurfaceId};

pub use crate::data_transfer::{MimePayload as DndMimePayload, TransferReadPipe as DndReadPipe};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DndOfferId(pub(crate) u64);

impl DndOfferId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DndSourceId(pub(crate) u64);

impl DndSourceId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
    pub struct DndActions: u8 {
        const COPY = 1 << 0;
        const MOVE = 1 << 1;
        const ASK = 1 << 2;
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DndAction {
    Copy,
    Move,
    Ask,
}

/// A drag icon backed by a Linux dmabuf.
///
/// Pixel dimensions are buffer coordinates. `offset` is expressed in logical
/// surface coordinates relative to the drag hotspot.
#[derive(Debug)]
pub struct DndIcon {
    buffer: crate::dmabuf::DmabufBufferParams,
    width: u32,
    height: u32,
    buffer_scale: i32,
    offset: LogicalPosition,
}

impl DndIcon {
    pub fn from_dmabuf(
        params: crate::dmabuf::DmabufBufferParams,
        buffer_scale: i32,
        offset: LogicalPosition,
    ) -> Result<Self, &'static str> {
        if params.width <= 0 || params.height <= 0 {
            return Err("DnD icon dimensions must be non-zero");
        }
        if params.planes.is_empty() {
            return Err("DnD icon dmabuf requires at least one plane");
        }
        if buffer_scale < 1 {
            return Err("DnD icon buffer scale must be at least one");
        }
        let width = params.width as u32;
        let height = params.height as u32;
        if !width.is_multiple_of(buffer_scale as u32) || !height.is_multiple_of(buffer_scale as u32)
        {
            return Err("DnD icon dimensions must be divisible by its buffer scale");
        }
        Ok(Self {
            buffer: params,
            width,
            height,
            buffer_scale,
            offset,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn buffer_scale(&self) -> i32 {
        self.buffer_scale
    }

    pub const fn offset(&self) -> LogicalPosition {
        self.offset
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        crate::dmabuf::DmabufBufferParams,
        u32,
        u32,
        i32,
        LogicalPosition,
    ) {
        (
            self.buffer,
            self.width,
            self.height,
            self.buffer_scale,
            self.offset,
        )
    }
}

#[derive(Clone, Debug)]
pub enum DndEvent {
    Enter {
        offer: DndOfferId,
        surface: SurfaceId,
        position: LogicalPosition,
        mime_types: Vec<String>,
        source_actions: DndActions,
    },
    Motion {
        offer: DndOfferId,
        surface: SurfaceId,
        position: LogicalPosition,
    },
    Leave {
        offer: DndOfferId,
        surface: SurfaceId,
    },
    Drop {
        offer: DndOfferId,
        surface: SurfaceId,
        action: Option<DndAction>,
    },
    /// The compositor accepted the drop. The source and drag icon remain alive
    /// until [`DndEvent::SourceFinished`] or [`DndEvent::SourceCancelled`].
    SourceDropped {
        source: DndSourceId,
        action: Option<DndAction>,
    },
    /// The destination completed the transfer and source resources were released.
    SourceFinished {
        source: DndSourceId,
        action: Option<DndAction>,
    },
    /// The drag was cancelled and source resources were released.
    SourceCancelled { source: DndSourceId },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_payload_rejects_empty_type_and_keeps_owned_bytes() {
        assert!(DndMimePayload::new("", b"value".as_slice()).is_err());

        let payload = DndMimePayload::new("text/uri-list", b"file:///tmp/a".as_slice())
            .expect("valid MIME payload");
        assert_eq!(payload.mime(), "text/uri-list");
        assert_eq!(payload.bytes().as_ref(), b"file:///tmp/a");
    }

    #[test]
    fn drag_icon_requires_valid_dmabuf_dimensions_planes_and_scale() {
        let offset = LogicalPosition::new(-8, -4);
        assert!(matches!(
            DndIcon::from_dmabuf(crate::DmabufBufferParams::new(0, 2, 0), 1, offset),
            Err("DnD icon dimensions must be non-zero")
        ));
        assert!(matches!(
            DndIcon::from_dmabuf(crate::DmabufBufferParams::new(2, 2, 0), 1, offset),
            Err("DnD icon dmabuf requires at least one plane")
        ));
    }
}
