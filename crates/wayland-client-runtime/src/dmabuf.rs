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

    /// Resolve a tranche's format indices into concrete format/modifier pairs.
    ///
    /// Invalid indices are skipped (faulty compositors / truncated tables).
    pub fn tranche_formats(&self, tranche: &DmabufFeedbackTranche) -> Vec<DmabufFormat> {
        tranche
            .formats
            .iter()
            .filter_map(|&idx| self.formats.get(idx as usize).copied())
            .collect()
    }

    /// Formats in compositor preference order (tranche 0 first, then 1, …).
    ///
    /// Duplicates across tranches are kept once (first wins). When no tranches
    /// were advertised, returns the raw format table.
    pub fn preferred_formats(&self) -> Vec<DmabufFormat> {
        if self.tranches.is_empty() {
            return self.formats.clone();
        }
        let mut out = Vec::new();
        for tranche in &self.tranches {
            for fmt in self.tranche_formats(tranche) {
                if !out
                    .iter()
                    .any(|e: &DmabufFormat| e.format == fmt.format && e.modifier == fmt.modifier)
                {
                    out.push(fmt);
                }
            }
        }
        out
    }

    /// Formats from the first tranche that has the `SCANOUT` flag, if any.
    pub fn scanout_formats(&self) -> Vec<DmabufFormat> {
        self.tranches
            .iter()
            .find(|t| t.flags.contains(DmabufTrancheFlags::SCANOUT))
            .map(|t| self.tranche_formats(t))
            .unwrap_or_default()
    }

    /// Whether this feedback advertises the given format (any modifier).
    pub fn supports_format(&self, format: u32) -> bool {
        self.formats.iter().any(|f| f.format == format)
    }

    /// Whether this feedback advertises an exact format/modifier pair.
    pub fn supports_format_modifier(&self, format: u32, modifier: u64) -> bool {
        self.formats
            .iter()
            .any(|f| f.format == format && f.modifier == modifier)
    }

    /// Pick the first preferred format whose fourcc is in `candidates` (order
    /// of `candidates` is the caller's preference among equals).
    ///
    /// Searches tranche-ordered formats first, then falls back to the full
    /// table. Returns `None` if nothing matches.
    pub fn pick_format(&self, candidates: &[u32]) -> Option<DmabufFormat> {
        if candidates.is_empty() {
            return None;
        }
        let preferred = self.preferred_formats();
        for &cand in candidates {
            if let Some(fmt) = preferred.iter().find(|f| f.format == cand) {
                return Some(*fmt);
            }
        }
        for &cand in candidates {
            if let Some(fmt) = self.formats.iter().find(|f| f.format == cand) {
                return Some(*fmt);
            }
        }
        None
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

/// Common DRM fourcc values used with Wayland / Vulkan / wgpu.
///
/// These match the kernel `drm_fourcc.h` definitions and the values advertised
/// in `zwp_linux_dmabuf` format tables.
pub mod fourcc {
    /// `DRM_FORMAT_ARGB8888` — little-endian 32-bit A8R8G8B8.
    pub const ARGB8888: u32 = 0x3432_5241; // 'AR24'
    /// `DRM_FORMAT_XRGB8888`.
    pub const XRGB8888: u32 = 0x3432_5258; // 'XR24'
    /// `DRM_FORMAT_ABGR8888`.
    pub const ABGR8888: u32 = 0x3432_4241; // 'AB24'
    /// `DRM_FORMAT_XBGR8888`.
    pub const XBGR8888: u32 = 0x3432_4258; // 'XB24'
    /// `DRM_FORMAT_RGBA8888`.
    pub const RGBA8888: u32 = 0x3432_4152; // 'RA24'
    /// `DRM_FORMAT_BGRA8888` — matches wgpu `Bgra8Unorm` / Vulkan `B8G8R8A8`.
    pub const BGRA8888: u32 = 0x3432_4142; // 'BA24'
    /// `DRM_FORMAT_MOD_INVALID` — use the buffer's implicit modifier.
    pub const MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;
    /// `DRM_FORMAT_MOD_LINEAR`.
    pub const MOD_LINEAR: u64 = 0;
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
        let params =
            DmabufBufferParams::new(64, 64, 0x34325241).with_flags(DmabufBufferFlags::Y_INVERT);
        assert_eq!(params.width, 64);
        assert_eq!(params.height, 64);
        assert!(params.flags.contains(DmabufBufferFlags::Y_INVERT));
        assert!(params.planes.is_empty());
    }

    #[test]
    fn feedback_pick_and_tranche_order() {
        let feedback = DmabufFeedback {
            main_device: 1,
            formats: vec![
                DmabufFormat::new(fourcc::XRGB8888, fourcc::MOD_LINEAR),
                DmabufFormat::new(fourcc::ARGB8888, fourcc::MOD_LINEAR),
                DmabufFormat::new(fourcc::BGRA8888, 0x1234),
            ],
            tranches: vec![
                DmabufFeedbackTranche {
                    device: 1,
                    flags: DmabufTrancheFlags::SCANOUT,
                    formats: vec![2], // BGRA with custom mod first (scan-out)
                },
                DmabufFeedbackTranche {
                    device: 1,
                    flags: DmabufTrancheFlags::empty(),
                    formats: vec![1, 0], // ARGB then XRGB
                },
            ],
        };
        assert!(feedback.supports_format(fourcc::ARGB8888));
        assert!(feedback.supports_format_modifier(fourcc::BGRA8888, 0x1234));
        assert!(!feedback.supports_format_modifier(fourcc::BGRA8888, fourcc::MOD_LINEAR));

        let preferred = feedback.preferred_formats();
        assert_eq!(preferred[0].format, fourcc::BGRA8888);
        assert_eq!(preferred[1].format, fourcc::ARGB8888);

        let scanout = feedback.scanout_formats();
        assert_eq!(scanout.len(), 1);
        assert_eq!(scanout[0].format, fourcc::BGRA8888);

        // Caller prefers ARGB over BGRA among candidates.
        let picked = feedback
            .pick_format(&[fourcc::ARGB8888, fourcc::BGRA8888])
            .expect("pick");
        assert_eq!(picked.format, fourcc::ARGB8888);

        // Only BGRA is requested → tranche order still wins for that fourcc.
        let only_bgra = feedback.pick_format(&[fourcc::BGRA8888]).expect("bgra");
        assert_eq!(only_bgra.modifier, 0x1234);
    }
}
