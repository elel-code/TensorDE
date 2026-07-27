//! Tensor-owned core `wl_shm` wire adapter and zero-copy buffer access.
//!
//! Validation and SIGBUS recovery follow Smithay's mature implementation. See
//! `LICENSES/Smithay-MIT.txt`.

mod pool;

use std::sync::Arc;

use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, New, Resource, WEnum,
    backend::{ClientId, GlobalId},
    protocol::{
        wl_buffer::{self, WlBuffer},
        wl_shm::{self, WlShm},
        wl_shm_pool::{self, WlShmPool},
    },
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

use pool::{Pool, PoolCreateError, PoolResizeError};

const VERSION: u32 = 2;
const FORMATS: [wl_shm::Format; 2] = [wl_shm::Format::Argb8888, wl_shm::Format::Xrgb8888];

pub(crate) struct ShmProtocol {
    _global: GlobalId,
}

impl ShmProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        let global = display.create_global::<RuntimeState, WlShm, _>(VERSION, ShmGlobalData);
        Self { _global: global }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct ShmGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct ShmData;

#[derive(Debug)]
pub(in crate::protocol) struct ShmPoolData {
    pool: Arc<Pool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::protocol) struct ShmBufferMetadata {
    pub(in crate::protocol) width: i32,
    pub(in crate::protocol) height: i32,
    pub(in crate::protocol) stride: i32,
    pub(in crate::protocol) format: wl_shm::Format,
}

impl ShmBufferMetadata {
    fn byte_len(self) -> Option<usize> {
        usize::try_from(self.stride)
            .ok()?
            .checked_mul(usize::try_from(self.height).ok()?)
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct ShmBufferData {
    pool: Arc<Pool>,
    offset: usize,
    metadata: ShmBufferMetadata,
}

/// Keeps the pool mapping for a validated SHM buffer alive without copying pixels.
#[derive(Clone, Debug)]
pub(in crate::protocol) struct ShmBufferLease {
    _pool: Arc<Pool>,
    _offset: usize,
    metadata: ShmBufferMetadata,
}

impl ShmBufferLease {
    pub(in crate::protocol) fn metadata(&self) -> ShmBufferMetadata {
        self.metadata
    }

    #[cfg(test)]
    pub(in crate::protocol) fn with_contents<T>(
        &self,
        callback: impl FnOnce(*const u8, usize, ShmBufferMetadata) -> T,
    ) -> Result<T, BufferAccessError> {
        let len = self.metadata.byte_len().ok_or(BufferAccessError::BadMap)?;
        self._pool
            .with_data(self._offset, len, |ptr| callback(ptr, len, self.metadata))
            .map_err(|()| BufferAccessError::BadMap)
    }
}

impl ShmBufferData {
    pub(in crate::protocol) fn metadata(&self) -> ShmBufferMetadata {
        self.metadata
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(in crate::protocol) enum BufferAccessError {
    #[error("non-SHM buffer")]
    NotManaged,
    #[error("invalid client SHM mapping")]
    BadMap,
}

pub(in crate::protocol) fn shm_buffer(buffer: &WlBuffer) -> Option<&ShmBufferData> {
    buffer.data::<ShmBufferData>()
}

pub(in crate::protocol) fn lease_shm_buffer(buffer: &WlBuffer) -> Option<ShmBufferLease> {
    let data = shm_buffer(buffer)?;
    Some(ShmBufferLease {
        _pool: data.pool.clone(),
        _offset: data.offset,
        metadata: data.metadata,
    })
}

/// Borrow the exact byte range belonging to one SHM buffer without copying.
///
/// The pointer is valid only during the callback. The client may concurrently
/// mutate shared memory, so consumers must not retain it or create a Rust
/// reference that escapes the callback.
pub(in crate::protocol) fn with_buffer_contents<T>(
    buffer: &WlBuffer,
    callback: impl FnOnce(*const u8, usize, ShmBufferMetadata) -> T,
) -> Result<T, BufferAccessError> {
    let data = shm_buffer(buffer).ok_or(BufferAccessError::NotManaged)?;
    let len = data.metadata.byte_len().ok_or(BufferAccessError::BadMap)?;
    data.pool
        .with_data(data.offset, len, |ptr| callback(ptr, len, data.metadata))
        .map_err(|()| post_bad_map(buffer))
}

/// Mutably borrow the exact byte range belonging to one SHM buffer without
/// staging or copying.
pub(in crate::protocol) fn with_buffer_contents_mut<T>(
    buffer: &WlBuffer,
    callback: impl FnOnce(*mut u8, usize, ShmBufferMetadata) -> T,
) -> Result<T, BufferAccessError> {
    let data = shm_buffer(buffer).ok_or(BufferAccessError::NotManaged)?;
    let len = data.metadata.byte_len().ok_or(BufferAccessError::BadMap)?;
    data.pool
        .with_data_mut(data.offset, len, |ptr| callback(ptr, len, data.metadata))
        .map_err(|()| post_bad_map(buffer))
}

fn post_bad_map(buffer: &WlBuffer) -> BufferAccessError {
    buffer.post_error(
        wl_shm::Error::InvalidFd,
        "SHM pool backing file was truncated",
    );
    BufferAccessError::BadMap
}

pub(in crate::protocol) trait ShmHandler: 'static {
    fn shm_buffer_destroyed(&mut self, buffer: &WlBuffer);
}

impl ShmHandler for RuntimeState {
    fn shm_buffer_destroyed(&mut self, buffer: &WlBuffer) {
        self.protocol_globals
            .desktop_controls
            .shm_buffer_destroyed(buffer);
        #[cfg(feature = "tty")]
        self.buffer_destroyed(&buffer.id());
        #[cfg(not(feature = "tty"))]
        let _ = buffer;
    }
}

impl<D> GlobalDispatchDelegate<WlShm, D> for ShmGlobalData
where
    D: Dispatch<WlShm, ShmData>,
    D: 'static,
{
    fn bind(
        &self,
        _state: &mut D,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WlShm>,
        data_init: &mut DataInit<'_, D>,
    ) {
        let shm = data_init.init(resource, ShmData);
        for format in FORMATS {
            shm.format(format);
        }
    }
}

impl<D> DispatchDelegate<WlShm, D> for ShmData
where
    D: Dispatch<WlShmPool, ShmPoolData>,
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        shm: &WlShm,
        request: wl_shm::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wl_shm::Request::CreatePool { id, fd, size } => {
                let Some(size) = positive_size(size) else {
                    shm.post_error(
                        wl_shm::Error::InvalidStride,
                        "SHM pool size must be positive",
                    );
                    return;
                };
                match Pool::new(fd, size) {
                    Ok(pool) => {
                        data_init.init(
                            id,
                            ShmPoolData {
                                pool: Arc::new(pool),
                            },
                        );
                    }
                    Err(error) => post_pool_create_error(shm, error),
                }
            }
            wl_shm::Request::Release => {}
            _ => unreachable!(),
        }
    }
}

fn positive_size(size: i32) -> Option<usize> {
    usize::try_from(size).ok().filter(|size| *size > 0)
}

fn post_pool_create_error(shm: &WlShm, error: PoolCreateError) {
    shm.post_error(
        wl_shm::Error::InvalidFd,
        format!("failed to create SHM pool: {error}"),
    );
}

impl<D> DispatchDelegate<WlShmPool, D> for ShmPoolData
where
    D: Dispatch<WlBuffer, ShmBufferData>,
    D: ShmHandler,
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        resource: &WlShmPool,
        request: wl_shm_pool::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wl_shm_pool::Request::CreateBuffer {
                id,
                offset,
                width,
                height,
                stride,
                format,
            } => {
                let format = match format {
                    WEnum::Value(format) if FORMATS.contains(&format) => format,
                    WEnum::Value(format) => {
                        resource.post_error(
                            wl_shm::Error::InvalidFormat,
                            format!("unsupported SHM format {format:?}"),
                        );
                        return;
                    }
                    WEnum::Unknown(format) => {
                        resource.post_error(
                            wl_shm::Error::InvalidFormat,
                            format!("unknown SHM format 0x{format:x}"),
                        );
                        return;
                    }
                };
                let pool_size = self.pool.size();
                let Ok((offset, metadata)) =
                    validate_buffer(pool_size, offset, width, height, stride, format)
                else {
                    resource.post_error(
                        wl_shm::Error::InvalidStride,
                        format!(
                            "invalid SHM buffer offset={offset}, size={width}x{height}, stride={stride}, pool={pool_size}"
                        ),
                    );
                    return;
                };
                data_init.init(
                    id,
                    ShmBufferData {
                        pool: Arc::clone(&self.pool),
                        offset,
                        metadata,
                    },
                );
            }
            wl_shm_pool::Request::Resize { size } => {
                let Some(size) = positive_size(size) else {
                    resource.post_error(wl_shm::Error::InvalidFd, "SHM pool size must be positive");
                    return;
                };
                if let Err(error) = self.pool.resize(size) {
                    post_pool_resize_error(resource, error);
                }
            }
            wl_shm_pool::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

fn validate_buffer(
    pool_size: usize,
    offset: i32,
    width: i32,
    height: i32,
    stride: i32,
    format: wl_shm::Format,
) -> Result<(usize, ShmBufferMetadata), ()> {
    let offset = usize::try_from(offset).map_err(|_| ())?;
    let width_bytes = width.checked_mul(4).filter(|value| *value > 0).ok_or(())?;
    if height <= 0 || stride < width_bytes {
        return Err(());
    }
    let byte_len = usize::try_from(stride)
        .map_err(|_| ())?
        .checked_mul(usize::try_from(height).map_err(|_| ())?)
        .ok_or(())?;
    if offset
        .checked_add(byte_len)
        .filter(|end| *end <= pool_size)
        .is_none()
    {
        return Err(());
    }
    Ok((
        offset,
        ShmBufferMetadata {
            width,
            height,
            stride,
            format,
        },
    ))
}

fn post_pool_resize_error(pool: &WlShmPool, error: PoolResizeError) {
    pool.post_error(
        wl_shm::Error::InvalidFd,
        format!("failed to resize SHM pool: {error}"),
    );
}

impl<D> DispatchDelegate<WlBuffer, D> for ShmBufferData
where
    D: ShmHandler,
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _buffer: &WlBuffer,
        request: wl_buffer::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wl_buffer::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut D, _client: ClientId, buffer: &WlBuffer) {
        state.shm_buffer_destroyed(buffer);
    }
}

delegate_global_dispatch!(RuntimeState, WlShm, ShmGlobalData);
delegate_dispatch!(RuntimeState, WlShm, ShmData);
delegate_dispatch!(RuntimeState, WlShmPool, ShmPoolData);
delegate_dispatch!(RuntimeState, WlBuffer, ShmBufferData);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_validation_uses_checked_full_stride_extent() {
        let (_, data) = validate_buffer(4096, 128, 16, 8, 80, wl_shm::Format::Argb8888).unwrap();
        assert_eq!(data.byte_len(), Some(640));
        assert!(validate_buffer(767, 128, 16, 8, 80, wl_shm::Format::Argb8888).is_err());
        assert!(validate_buffer(4096, -1, 16, 8, 80, wl_shm::Format::Argb8888).is_err());
        assert!(validate_buffer(4096, 0, 16, 8, 63, wl_shm::Format::Argb8888).is_err());
        assert!(
            validate_buffer(
                usize::MAX,
                0,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                wl_shm::Format::Argb8888,
            )
            .is_err()
        );
    }
}
