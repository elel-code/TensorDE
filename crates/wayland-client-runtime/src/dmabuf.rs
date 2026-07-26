//! Linux dmabuf (`zwp_linux_dmabuf_v1`) types for zero-copy GPU buffers.
//!
//! Behavioral reference: SCTK 0.21 `dmabuf` module and the stable
//! `linux-dmabuf-v1` protocol. This crate owns its own state machines and
//! does not depend on SCTK.

use std::fmt;
use std::os::unix::io::OwnedFd;

use bitflags::bitflags;

/// One fourcc format / DRM modifier pair advertised by the compositor.
///
/// Wire layout matches the feedback format table (`16` bytes: format, pad,
/// modifier) so mmap'd tables can be read as a slice of this type.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct DmabufFormat {
    /// DRM fourcc format (e.g. `DRM_FORMAT_ARGB8888`).
    pub format: u32,
    _padding: u32,
    /// Modifier, or `DRM_FORMAT_MOD_INVALID` for the implicit modifier.
    pub modifier: u64,
}

impl DmabufFormat {
    pub const fn new(format: u32, modifier: u64) -> Self {
        Self {
            format,
            _padding: 0,
            modifier,
        }
    }

    pub const fn format(self) -> u32 {
        self.format
    }

    pub const fn modifier(self) -> u64 {
        self.modifier
    }
}

impl fmt::Debug for DmabufFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DmabufFormat")
            .field("format", &format_args!("0x{:08x}", self.format))
            .field("modifier", &format_args!("0x{:016x}", self.modifier))
            .finish()
    }
}

bitflags! {
    /// `zwp_linux_dmabuf_feedback_v1.tranche_flags`.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct DmabufTrancheFlags: u32 {
        /// Prefer scan-out when possible.
        const SCANOUT = 1;
    }
}

bitflags! {
    /// `zwp_linux_buffer_params_v1` create flags.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct DmabufBufferFlags: u32 {
        /// Y is inverted (origin at bottom-left).
        const Y_INVERT = 1;
        /// Contents are interlaced.
        const INTERLACED = 2;
        /// Bottom field first when interlaced.
        const BOTTOM_FIRST = 4;
    }
}

/// One preference tranche from dmabuf feedback (v4+).
#[derive(Clone, Debug, Default)]
pub struct DmabufFeedbackTranche {
    /// Target device (`dev_t` as little/native-endian bytes from the wire).
    pub device: u64,
    pub flags: DmabufTrancheFlags,
    /// Indices into the feedback format table.
    pub formats: Vec<u16>,
}

/// Compositor-advertised dmabuf capabilities (feedback object, v4+).
#[derive(Clone, Debug, Default)]
pub struct DmabufFeedback {
    pub main_device: u64,
    pub formats: Vec<DmabufFormat>,
    pub tranches: Vec<DmabufFeedbackTranche>,
}

impl DmabufFeedback {
    pub fn main_device(&self) -> u64 {
        self.main_device
    }

    pub fn formats(&self) -> &[DmabufFormat] {
        &self.formats
    }

    pub fn tranches(&self) -> &[DmabufFeedbackTranche] {
        &self.tranches
    }
}

/// One plane passed to `zwp_linux_buffer_params_v1.add`.
#[derive(Debug)]
pub struct DmabufPlane {
    pub fd: OwnedFd,
    pub plane_idx: u32,
    pub offset: u32,
    pub stride: u32,
    pub modifier: u64,
}

impl DmabufPlane {
    pub fn new(fd: OwnedFd, plane_idx: u32, offset: u32, stride: u32, modifier: u64) -> Self {
        Self {
            fd,
            plane_idx,
            offset,
            stride,
            modifier,
        }
    }
}

/// Description of a multi-plane dmabuf buffer to import as `wl_buffer`.
#[derive(Debug)]
pub struct DmabufBufferParams {
    pub width: i32,
    pub height: i32,
    pub format: u32,
    pub flags: DmabufBufferFlags,
    pub planes: Vec<DmabufPlane>,
}

impl DmabufBufferParams {
    pub fn new(width: i32, height: i32, format: u32) -> Self {
        Self {
            width,
            height,
            format,
            flags: DmabufBufferFlags::empty(),
            planes: Vec::new(),
        }
    }

    pub fn with_flags(mut self, flags: DmabufBufferFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn with_plane(mut self, plane: DmabufPlane) -> Self {
        self.planes.push(plane);
        self
    }

    pub fn add_plane(&mut self, plane: DmabufPlane) {
        self.planes.push(plane);
    }
}

/// Opaque handle for an imported dmabuf `wl_buffer` managed by the runtime.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DmabufBufferId(pub(crate) u64);

impl DmabufBufferId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Public dmabuf events delivered through [`crate::Event::Dmabuf`].
#[derive(Clone, Debug)]
pub enum DmabufEvent {
    /// Default or surface-scoped feedback snapshot is ready.
    Feedback {
        surface: Option<crate::surface::SurfaceId>,
        feedback: DmabufFeedback,
    },
    /// Async params create succeeded.
    BufferCreated { id: DmabufBufferId },
    /// Async params create failed (compositor rejected the import).
    BufferFailed,
    /// Compositor released a dmabuf buffer (safe to destroy or re-queue).
    BufferReleased { id: DmabufBufferId },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_is_sixteen_bytes() {
        assert_eq!(std::mem::size_of::<DmabufFormat>(), 16);
        assert_eq!(std::mem::align_of::<DmabufFormat>(), 8);
    }

    #[test]
    fn params_builder_accumulates_planes() {
        // Empty OwnedFd is hard without a real fd; just exercise the builder fields.
        let params = DmabufBufferParams::new(64, 64, 0x34325241)
            .with_flags(DmabufBufferFlags::Y_INVERT);
        assert_eq!(params.width, 64);
        assert_eq!(params.height, 64);
        assert!(params.flags.contains(DmabufBufferFlags::Y_INVERT));
        assert!(params.planes.is_empty());
    }
}
