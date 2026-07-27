use std::{
    os::fd::OwnedFd,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use tensor_host::{DrmFormat, Fourcc, Modifier};
use wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_buffer_params_v1::{
    self, ZwpLinuxBufferParamsV1,
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, Resource, WEnum, protocol::wl_buffer::WlBuffer,
};

use crate::protocol::dispatch::DispatchDelegate;

use super::{DmabufBuffer, DmabufImportHandler};

const MAX_PLANES: usize = 4;

#[derive(Debug)]
struct PendingPlane {
    fd: OwnedFd,
    offset: u32,
    stride: u32,
}

#[derive(Debug)]
struct PendingState {
    planes: [Option<PendingPlane>; MAX_PLANES],
    modifier: Option<Modifier>,
    _sampling_device: Option<u64>,
}

impl Default for PendingState {
    fn default() -> Self {
        Self {
            planes: std::array::from_fn(|_| None),
            modifier: None,
            _sampling_device: None,
        }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct DmabufParamsData {
    used: AtomicBool,
    formats: Arc<[DrmFormat]>,
    pending: Mutex<PendingState>,
}

impl DmabufParamsData {
    pub(super) fn new(formats: Arc<[DrmFormat]>) -> Self {
        Self {
            used: AtomicBool::new(false),
            formats,
            pending: Mutex::new(PendingState::default()),
        }
    }

    fn ensure_unused(&self, params: &ZwpLinuxBufferParamsV1) -> bool {
        if !self.used.load(Ordering::Relaxed) {
            return true;
        }
        params.post_error(
            zwp_linux_buffer_params_v1::Error::AlreadyUsed,
            "buffer parameters were already used".to_owned(),
        );
        false
    }

    fn build(
        &self,
        params: &ZwpLinuxBufferParamsV1,
        width: i32,
        height: i32,
        format: u32,
        flags: WEnum<zwp_linux_buffer_params_v1::Flags>,
    ) -> Option<DmabufBuffer> {
        if self
            .used
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            self.ensure_unused(params);
            return None;
        }
        if width <= 0 || height <= 0 {
            params.post_error(
                zwp_linux_buffer_params_v1::Error::InvalidDimensions,
                format!("invalid dma-buf dimensions {width}x{height}"),
            );
            return None;
        }
        let size = tensor_util::Size::new(width as u32, height as u32);
        let mut pending = self.pending.lock().unwrap();
        let modifier = pending.modifier.unwrap_or(Modifier::INVALID);
        let format = DrmFormat::new(Fourcc::from_raw(format), modifier);
        let supports_code = self.formats.iter().any(|known| known.code == format.code);
        let supports_pair = self.formats.contains(&format);
        if !supports_code || (params.version() >= 4 && !supports_pair) {
            params.post_error(
                zwp_linux_buffer_params_v1::Error::InvalidFormat,
                format!(
                    "unsupported dma-buf format {}/{:#x}",
                    format.code, format.modifier.0
                ),
            );
            return None;
        }

        let mut planes = Vec::with_capacity(MAX_PLANES);
        let mut missing = false;
        for (index, plane) in pending.planes.iter_mut().enumerate() {
            let Some(plane) = plane.take() else {
                missing = true;
                continue;
            };
            if missing {
                params.post_error(
                    zwp_linux_buffer_params_v1::Error::Incomplete,
                    format!("dma-buf plane {index} follows a missing plane"),
                );
                return None;
            }
            if !validate_bounds(&plane, height as u32) {
                params.post_error(
                    zwp_linux_buffer_params_v1::Error::OutOfBounds,
                    format!("dma-buf plane {index} exceeds its fd bounds"),
                );
                return None;
            }
            planes.push(crate::render::DmabufPlane {
                fd: plane.fd,
                offset: plane.offset,
                stride: plane.stride,
            });
        }
        if planes.len() != 1 {
            params.post_error(
                zwp_linux_buffer_params_v1::Error::Incomplete,
                format!(
                    "dma-buf has {} planes; the advertised RGB import contract requires one",
                    planes.len()
                ),
            );
            return None;
        }
        let flags: u32 = flags.into();
        Some(DmabufBuffer::new(
            crate::render::Dmabuf {
                size,
                format,
                node: None,
                planes,
            },
            flags,
        ))
    }
}

impl<D> DispatchDelegate<ZwpLinuxBufferParamsV1, D> for DmabufParamsData
where
    D: Dispatch<WlBuffer, DmabufBuffer>,
    D: DmabufImportHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        client: &Client,
        params: &ZwpLinuxBufferParamsV1,
        request: zwp_linux_buffer_params_v1::Request,
        display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwp_linux_buffer_params_v1::Request::Destroy => {}
            zwp_linux_buffer_params_v1::Request::Add {
                fd,
                plane_idx,
                offset,
                stride,
                modifier_hi,
                modifier_lo,
            } => {
                if !self.ensure_unused(params) {
                    return;
                }
                let Ok(index) = usize::try_from(plane_idx) else {
                    unreachable!("u32 fits usize on supported Linux targets");
                };
                if index >= MAX_PLANES {
                    params.post_error(
                        zwp_linux_buffer_params_v1::Error::PlaneIdx,
                        format!("dma-buf plane index {plane_idx} is out of bounds"),
                    );
                    return;
                }
                let modifier =
                    Modifier::from_raw((u64::from(modifier_hi) << 32) | u64::from(modifier_lo));
                let mut pending = self.pending.lock().unwrap();
                if pending.planes[index].is_some() {
                    params.post_error(
                        zwp_linux_buffer_params_v1::Error::PlaneSet,
                        format!("dma-buf plane index {plane_idx} was already set"),
                    );
                    return;
                }
                if params.version() >= 5 && pending.modifier.is_some_and(|known| known != modifier)
                {
                    params.post_error(
                        zwp_linux_buffer_params_v1::Error::InvalidFormat,
                        "dma-buf planes use different modifiers".to_owned(),
                    );
                    return;
                }
                pending.modifier.get_or_insert(modifier);
                pending.planes[index] = Some(PendingPlane { fd, offset, stride });
            }
            zwp_linux_buffer_params_v1::Request::SetSamplingDevice { device } => {
                if !self.ensure_unused(params) {
                    return;
                }
                let Ok(raw) = <[u8; std::mem::size_of::<u64>()]>::try_from(device.as_slice())
                else {
                    params.post_error(
                        zwp_linux_buffer_params_v1::Error::InvalidDevTSize,
                        format!("sampling device has {} bytes, expected 8", device.len()),
                    );
                    return;
                };
                self.pending.lock().unwrap()._sampling_device = Some(u64::from_ne_bytes(raw));
            }
            zwp_linux_buffer_params_v1::Request::Create {
                width,
                height,
                format,
                flags,
            } => {
                let Some(buffer) = self.build(params, width, height, format, flags) else {
                    return;
                };
                let size = buffer.size();
                let id = match state.import_dmabuf(&buffer) {
                    Ok(id) => id,
                    Err(error) => {
                        tracing::warn!(%error, "client linux-dmabuf import failed");
                        params.failed();
                        return;
                    }
                };
                match client.create_resource::<WlBuffer, DmabufBuffer, D>(display, 1, buffer) {
                    Ok(resource) => {
                        if !state.register_dmabuf_buffer(&resource, id, size) {
                            state.release_dmabuf_import(id);
                            params.post_error(
                                zwp_linux_buffer_params_v1::Error::InvalidWlBuffer,
                                "dma-buf resource identity collision".to_owned(),
                            );
                            return;
                        }
                        params.created(&resource);
                    }
                    Err(error) => {
                        state.release_dmabuf_import(id);
                        tracing::warn!(%error, "client disappeared during linux-dmabuf import");
                    }
                }
            }
            zwp_linux_buffer_params_v1::Request::CreateImmed {
                buffer_id,
                width,
                height,
                format,
                flags,
            } => {
                let Some(buffer) = self.build(params, width, height, format, flags) else {
                    return;
                };
                let size = buffer.size();
                let id = match state.import_dmabuf(&buffer) {
                    Ok(id) => id,
                    Err(error) => {
                        tracing::warn!(%error, "immediate client linux-dmabuf import failed");
                        params.post_error(
                            zwp_linux_buffer_params_v1::Error::InvalidWlBuffer,
                            "renderer rejected immediate dma-buf import".to_owned(),
                        );
                        return;
                    }
                };
                let resource = data_init.init(buffer_id, buffer);
                if !state.register_dmabuf_buffer(&resource, id, size) {
                    state.release_dmabuf_import(id);
                    params.post_error(
                        zwp_linux_buffer_params_v1::Error::InvalidWlBuffer,
                        "dma-buf resource identity collision".to_owned(),
                    );
                }
            }
            _ => unreachable!(),
        }
    }
}

fn validate_bounds(plane: &PendingPlane, height: u32) -> bool {
    if plane.stride == 0 {
        return false;
    }
    let Some(end) = plane
        .stride
        .checked_mul(height)
        .and_then(|size| size.checked_add(plane.offset))
    else {
        return false;
    };
    let Ok(size) = rustix::fs::seek(&plane.fd, rustix::fs::SeekFrom::End(0)) else {
        return true;
    };
    let _ = rustix::fs::seek(&plane.fd, rustix::fs::SeekFrom::Start(0));
    plane.offset as u64 <= size
        && plane
            .offset
            .checked_add(plane.stride)
            .is_some_and(|row| u64::from(row) <= size)
        && u64::from(end) <= size
}

#[cfg(test)]
mod tests {
    use rustix::fs::{MemfdFlags, ftruncate, memfd_create};

    use super::*;

    #[test]
    fn plane_bounds_use_checked_arithmetic_and_rustix_fd_size() {
        let fd = memfd_create("tensor-dmabuf-plane-test", MemfdFlags::CLOEXEC).unwrap();
        ftruncate(&fd, 4096).unwrap();
        let plane = PendingPlane {
            fd,
            offset: 0,
            stride: 64,
        };
        assert!(validate_bounds(&plane, 64));
        assert!(!validate_bounds(&plane, 65));

        let zero_stride = PendingPlane {
            fd: memfd_create("tensor-dmabuf-zero-stride-test", MemfdFlags::CLOEXEC).unwrap(),
            offset: 0,
            stride: 0,
        };
        assert!(!validate_bounds(&zero_stride, 1));
    }
}
