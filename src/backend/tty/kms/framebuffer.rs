//! Direct GBM dma-buf import and DRM framebuffer ownership.
//!
//! The import shape and source-metadata override are adapted from Smithay's
//! `backend::drm::gbm` implementation. See `LICENSES/Smithay-MIT.txt`.

use std::os::fd::{AsFd, BorrowedFd};

use drm::{
    buffer::PlanarBuffer,
    control::{Device as _, FbCmd2Flags, framebuffer},
};
use gbm::{BufferObject, BufferObjectFlags, Device as GbmDevice, Format, Modifier};
use smithay::backend::drm::DrmDeviceFd;
use thiserror::Error;
use tracing::{trace, warn};

use crate::render::ExportedDmabuf;

const MAX_PLANES: usize = 4;

pub(super) fn framebuffer_from_dmabuf(
    drm: &DrmDeviceFd,
    gbm: &GbmDevice<DrmDeviceFd>,
    dmabuf: &ExportedDmabuf,
) -> Result<ScanoutFramebuffer, FramebufferError> {
    let metadata = ImportMetadata::new(dmabuf)?;
    let bo: BufferObject<()> = gbm
        .import_buffer_object_from_dma_buf_with_modifiers(
            metadata.plane_count,
            metadata.handles,
            metadata.size.0,
            metadata.size.1,
            metadata.format,
            BufferObjectFlags::SCANOUT,
            metadata.pitches.map(|pitch| pitch as i32),
            metadata.offsets.map(|offset| offset as i32),
            metadata.modifier,
        )
        .map_err(FramebufferError::Import)?;
    let buffer = ImportedBuffer {
        bo: &bo,
        size: metadata.size,
        format: opaque_format(metadata.format),
        modifier: metadata.modifier,
        pitches: metadata.pitches,
        offsets: metadata.offsets,
    };
    let handle = drm
        .add_planar_framebuffer(&buffer, FbCmd2Flags::MODIFIERS)
        .map_err(FramebufferError::Add)?;
    Ok(ScanoutFramebuffer {
        handle,
        drm: drm.clone(),
    })
}

#[derive(Debug)]
pub(super) struct ScanoutFramebuffer {
    handle: framebuffer::Handle,
    drm: DrmDeviceFd,
}

impl ScanoutFramebuffer {
    #[inline]
    pub(super) fn handle(&self) -> framebuffer::Handle {
        self.handle
    }
}

impl AsRef<framebuffer::Handle> for ScanoutFramebuffer {
    #[inline]
    fn as_ref(&self) -> &framebuffer::Handle {
        &self.handle
    }
}

impl Drop for ScanoutFramebuffer {
    fn drop(&mut self) {
        trace!(framebuffer = ?self.handle, "destroying Tensor scanout framebuffer");
        if let Err(error) = self.drm.destroy_framebuffer(self.handle) {
            warn!(framebuffer = ?self.handle, %error, "failed to destroy Tensor scanout framebuffer");
        }
    }
}

struct ImportMetadata<'a> {
    plane_count: u32,
    handles: [Option<BorrowedFd<'a>>; MAX_PLANES],
    size: (u32, u32),
    format: Format,
    modifier: Modifier,
    pitches: [u32; MAX_PLANES],
    offsets: [u32; MAX_PLANES],
}

impl<'a> ImportMetadata<'a> {
    fn new(dmabuf: &'a ExportedDmabuf) -> Result<Self, FramebufferError> {
        let plane_count = dmabuf.planes.len();
        if !(1..=MAX_PLANES).contains(&plane_count) {
            return Err(FramebufferError::PlaneCount(plane_count));
        }
        if dmabuf.format.modifier.is_invalid() {
            return Err(FramebufferError::ImplicitModifier);
        }
        let format = Format::try_from(dmabuf.format.code.raw())
            .map_err(|_| FramebufferError::Fourcc(dmabuf.format.code.raw()))?;
        let mut handles = [None; MAX_PLANES];
        let mut pitches = [0; MAX_PLANES];
        let mut offsets = [0; MAX_PLANES];
        for (index, plane) in dmabuf.planes.iter().enumerate() {
            i32::try_from(plane.stride).map_err(|_| FramebufferError::PlaneMetadata {
                plane: index,
                field: "stride",
                value: plane.stride,
            })?;
            i32::try_from(plane.offset).map_err(|_| FramebufferError::PlaneMetadata {
                plane: index,
                field: "offset",
                value: plane.offset,
            })?;
            handles[index] = Some(plane.fd.as_fd());
            pitches[index] = plane.stride;
            offsets[index] = plane.offset;
        }
        Ok(Self {
            plane_count: plane_count as u32,
            handles,
            size: (dmabuf.size.width, dmabuf.size.height),
            format,
            modifier: Modifier::from(dmabuf.format.modifier.raw()),
            pitches,
            offsets,
        })
    }
}

struct ImportedBuffer<'a> {
    bo: &'a BufferObject<()>,
    size: (u32, u32),
    format: Format,
    modifier: Modifier,
    pitches: [u32; MAX_PLANES],
    offsets: [u32; MAX_PLANES],
}

impl PlanarBuffer for ImportedBuffer<'_> {
    #[inline]
    fn size(&self) -> (u32, u32) {
        self.size
    }

    #[inline]
    fn format(&self) -> Format {
        self.format
    }

    #[inline]
    fn modifier(&self) -> Option<Modifier> {
        Some(self.modifier)
    }

    #[inline]
    fn pitches(&self) -> [u32; MAX_PLANES] {
        self.pitches
    }

    #[inline]
    fn handles(&self) -> [Option<drm::buffer::Handle>; MAX_PLANES] {
        PlanarBuffer::handles(self.bo)
    }

    #[inline]
    fn offsets(&self) -> [u32; MAX_PLANES] {
        self.offsets
    }
}

#[inline]
fn opaque_format(format: Format) -> Format {
    match format {
        Format::Argb8888 => Format::Xrgb8888,
        Format::Abgr8888 => Format::Xbgr8888,
        Format::Argb2101010 => Format::Xrgb2101010,
        Format::Abgr2101010 => Format::Xbgr2101010,
        format => format,
    }
}

#[derive(Debug, Error)]
pub(super) enum FramebufferError {
    #[error("dma-buf has {0} planes; GBM supports one through four")]
    PlaneCount(usize),
    #[error("scanout dma-buf has no explicit DRM modifier")]
    ImplicitModifier,
    #[error("unsupported DRM fourcc {0:#x}")]
    Fourcc(u32),
    #[error("dma-buf plane {plane} {field} {value} exceeds the GBM import ABI")]
    PlaneMetadata {
        plane: usize,
        field: &'static str,
        value: u32,
    },
    #[error("GBM dma-buf import failed: {0}")]
    Import(std::io::Error),
    #[error("DRM ADDFB2 with explicit modifiers failed: {0}")]
    Add(std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        os::fd::{AsRawFd, OwnedFd},
        sync::Arc,
    };

    use tensor_host::{DrmFormat, Fourcc, Modifier as HostModifier};
    use tensor_util::Size;

    use super::*;
    use crate::render::{DmabufPlane, DrmNodeId};

    fn dmabuf(code: Fourcc, modifier: HostModifier, planes: &[(u32, u32)]) -> ExportedDmabuf {
        let fd: OwnedFd = File::open("/dev/null").unwrap().into();
        let fd = Arc::new(fd);
        ExportedDmabuf {
            size: Size::new(1920, 1080),
            format: DrmFormat::new(code, modifier),
            node: Some(DrmNodeId::new(226, 128)),
            planes: planes
                .iter()
                .map(|&(offset, stride)| DmabufPlane {
                    fd: Arc::clone(&fd),
                    offset,
                    stride,
                })
                .collect(),
        }
    }

    #[test]
    fn import_metadata_preserves_source_plane_layout_and_borrows_fds() {
        let dmabuf = dmabuf(
            Fourcc::NV12,
            HostModifier::from_raw(9),
            &[(64, 2048), (4096, 1024)],
        );
        let metadata = ImportMetadata::new(&dmabuf).unwrap();

        assert_eq!(metadata.plane_count, 2);
        assert_eq!(metadata.pitches, [2048, 1024, 0, 0]);
        assert_eq!(metadata.offsets, [64, 4096, 0, 0]);
        assert_eq!(
            metadata.handles[0].unwrap().as_raw_fd(),
            dmabuf.planes[0].fd.as_raw_fd()
        );
    }

    #[test]
    fn import_metadata_rejects_implicit_and_overflowing_layouts() {
        let implicit = dmabuf(Fourcc::XRGB8888, HostModifier::INVALID, &[(0, 256)]);
        assert!(matches!(
            ImportMetadata::new(&implicit),
            Err(FramebufferError::ImplicitModifier)
        ));

        let overflow = dmabuf(Fourcc::XRGB8888, HostModifier::LINEAR, &[(0, u32::MAX)]);
        assert!(matches!(
            ImportMetadata::new(&overflow),
            Err(FramebufferError::PlaneMetadata {
                plane: 0,
                field: "stride",
                value: u32::MAX,
            })
        ));
    }

    #[test]
    fn scanout_framebuffer_uses_opaque_output_formats() {
        assert_eq!(opaque_format(Format::Argb8888), Format::Xrgb8888);
        assert_eq!(opaque_format(Format::Abgr8888), Format::Xbgr8888);
        assert_eq!(opaque_format(Format::Argb2101010), Format::Xrgb2101010);
        assert_eq!(opaque_format(Format::Xrgb8888), Format::Xrgb8888);
    }
}
