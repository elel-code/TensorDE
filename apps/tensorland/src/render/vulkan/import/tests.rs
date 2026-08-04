use std::{fs::File, os::fd::OwnedFd};

use tensor_host::{DrmFormat, Modifier};
use tensor_util::Size;

use super::*;
use crate::render::DmabufPlane;

fn dmabuf(size: Size, planes: usize, modifier: Modifier, stride: u32) -> Dmabuf<OwnedFd> {
    Dmabuf {
        size,
        format: DrmFormat::new(Fourcc::XRGB8888, modifier),
        node: None,
        planes: (0..planes)
            .map(|_| DmabufPlane {
                fd: File::open("/dev/null").unwrap().into(),
                offset: 0,
                stride,
            })
            .collect(),
    }
}

#[test]
fn client_import_shape_rejects_implicit_and_multi_plane_buffers() {
    assert!(matches!(
        validate_shape(&dmabuf(Size::new(64, 64), 1, Modifier::INVALID, 256)),
        Err(ClientImportError::ImplicitModifier)
    ));
    assert!(matches!(
        validate_shape(&dmabuf(Size::new(64, 64), 2, Modifier::from_raw(9), 256)),
        Err(ClientImportError::UnsupportedPlaneCount(2))
    ));
}

#[test]
fn client_import_shape_preserves_explicit_plane_layout() {
    assert_eq!(
        validate_shape(&dmabuf(Size::new(128, 72), 1, Modifier::from_raw(9), 512)).unwrap(),
        ImportShape {
            width: 128,
            height: 72,
            offset: 0,
            stride: 512,
        }
    );
}

#[test]
fn cache_release_uses_a_stable_buffer_id() {
    let mut cache = ClientImageCache::default();
    assert_eq!(cache.len(), 0);
    cache.release(SurfaceBufferId::new(1));
    assert_eq!(cache.len(), 0);
}

#[test]
fn shm_staging_size_uses_tightly_packed_rgba_rows() {
    assert_eq!(shm_staging_len(Size::new(640, 480)).unwrap(), 640 * 480 * 4);
    assert!(shm_staging_len(Size::new(u32::MAX, u32::MAX)).is_err());
}
