use std::{os::fd::OwnedFd, sync::Arc};

use tensor_host::DrmFormat;
use tensor_util::Size;

use super::DrmNodeId;

/// One dma-buf plane with an adapter-independent fd holder.
#[derive(Clone, Debug)]
pub(crate) struct DmabufPlane<F> {
    pub(crate) fd: F,
    pub(crate) offset: u32,
    pub(crate) stride: u32,
}

/// Renderer-facing dma-buf description.
///
/// Protocol and KMS adapters convert their native objects at the edge. The
/// renderer sees only dimensions, host format values, plane layout, and fds.
#[derive(Clone, Debug)]
pub(crate) struct Dmabuf<F> {
    pub(crate) size: Size,
    pub(crate) format: DrmFormat,
    pub(crate) node: Option<DrmNodeId>,
    pub(crate) planes: Vec<DmabufPlane<F>>,
}

pub(crate) type ExportedDmabuf = Dmabuf<Arc<OwnedFd>>;
