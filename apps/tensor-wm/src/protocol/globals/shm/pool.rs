#![allow(unsafe_code)]

use std::{
    cell::Cell,
    ffi::c_void,
    os::fd::OwnedFd,
    ptr,
    sync::{
        LazyLock, OnceLock, RwLock,
        mpsc::{Sender, channel},
    },
};

use rustix::{
    fs::fstat,
    mm::{self, MapFlags, MremapFlags, ProtFlags},
    runtime::Signal,
};
use signal_hook_registry::SigId;

static DROP_POOL: LazyLock<Sender<PoolStorage>> = LazyLock::new(|| {
    let (sender, receiver) = channel();
    std::thread::Builder::new()
        .name("tensor-shm-drop".to_owned())
        .spawn(move || {
            while let Ok(pool) = receiver.recv() {
                drop(pool);
            }
        })
        .expect("failed to start SHM cleanup thread");
    sender
});

static SIGBUS_HANDLER: OnceLock<Result<SigId, std::io::Error>> = OnceLock::new();

#[derive(Clone, Copy, Default)]
struct ActiveMapping {
    address: usize,
    len: usize,
    faulted: bool,
}

thread_local! {
    static ACTIVE_MAPPING: Cell<ActiveMapping> = const { Cell::new(ActiveMapping {
        address: 0,
        len: 0,
        faulted: false,
    }) };
}

#[derive(Debug, thiserror::Error)]
pub(super) enum PoolCreateError {
    #[error("SIGBUS guard installation failed")]
    Signal,
    #[error("backing file is smaller than the requested mapping")]
    FileTooSmall,
    #[error("shared mapping failed")]
    Map,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum PoolResizeError {
    #[error("SHM pools cannot shrink")]
    Shrink,
    #[error("backing file was not extended before resize")]
    FileTooSmall,
    #[error("mapping resize failed")]
    Remap,
}

#[derive(Debug)]
pub(super) struct Pool {
    storage: Option<PoolStorage>,
}

#[derive(Debug)]
struct PoolStorage {
    mapping: RwLock<Mapping>,
    fd: OwnedFd,
}

#[derive(Debug)]
struct Mapping {
    address: *mut c_void,
    len: usize,
}

// Mapping access and movement are serialized by PoolStorage::mapping.
unsafe impl Send for Mapping {}
unsafe impl Sync for Mapping {}

impl Pool {
    pub(super) fn new(fd: OwnedFd, size: usize) -> Result<Self, PoolCreateError> {
        install_sigbus_handler()?;
        let _ = &*DROP_POOL;
        if file_size(&fd).filter(|actual| *actual >= size).is_none() {
            return Err(PoolCreateError::FileTooSmall);
        }
        let address = unsafe {
            mm::mmap(
                ptr::null_mut(),
                size,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                &fd,
                0,
            )
        }
        .map_err(|_| PoolCreateError::Map)?;
        Ok(Self {
            storage: Some(PoolStorage {
                mapping: RwLock::new(Mapping { address, len: size }),
                fd,
            }),
        })
    }

    pub(super) fn size(&self) -> usize {
        self.storage()
            .mapping
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len
    }

    pub(super) fn resize(&self, size: usize) -> Result<(), PoolResizeError> {
        let storage = self.storage();
        if file_size(&storage.fd)
            .filter(|actual| *actual >= size)
            .is_none()
        {
            return Err(PoolResizeError::FileTooSmall);
        }
        let mut mapping = storage
            .mapping
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if size < mapping.len {
            return Err(PoolResizeError::Shrink);
        }
        if size == mapping.len {
            return Ok(());
        }
        let address =
            unsafe { mm::mremap(mapping.address, mapping.len, size, MremapFlags::MAYMOVE) }
                .map_err(|_| PoolResizeError::Remap)?;
        mapping.address = address;
        mapping.len = size;
        Ok(())
    }

    pub(super) fn with_data<T>(
        &self,
        offset: usize,
        len: usize,
        callback: impl FnOnce(*const u8) -> T,
    ) -> Result<T, ()> {
        let mapping = self
            .storage()
            .mapping
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let ptr = mapping.range(offset, len)?;
        let access = MappingAccess::begin(&mapping);
        let value = callback(ptr.cast_const());
        access.finish().map(|()| value)
    }

    pub(super) fn with_data_mut<T>(
        &self,
        offset: usize,
        len: usize,
        callback: impl FnOnce(*mut u8) -> T,
    ) -> Result<T, ()> {
        let mapping = self
            .storage()
            .mapping
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let ptr = mapping.range(offset, len)?;
        let access = MappingAccess::begin(&mapping);
        let value = callback(ptr);
        access.finish().map(|()| value)
    }

    /// Copy validated rows out of the shared mapping while the SIGBUS guard is
    /// active. The destination is tightly packed so Vulkan uploads never copy
    /// client-provided row padding.
    pub(super) fn copy_rows(
        &self,
        offset: usize,
        source_stride: usize,
        row_bytes: usize,
        rows: usize,
        destination: &mut [u8],
    ) -> Result<(), ()> {
        let source_len = source_stride.checked_mul(rows).ok_or(())?;
        let destination_len = row_bytes.checked_mul(rows).ok_or(())?;
        if destination.len() != destination_len || row_bytes > source_stride {
            return Err(());
        }
        self.with_data(offset, source_len, |source| {
            for row in 0..rows {
                let source_offset = row
                    .checked_mul(source_stride)
                    .expect("validated SHM row offset");
                let destination_offset = row
                    .checked_mul(row_bytes)
                    .expect("validated upload row offset");
                unsafe {
                    source.add(source_offset).copy_to_nonoverlapping(
                        destination.as_mut_ptr().add(destination_offset),
                        row_bytes,
                    );
                }
            }
        })
    }

    fn storage(&self) -> &PoolStorage {
        self.storage.as_ref().expect("SHM pool storage is live")
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        if let Some(storage) = self.storage.take() {
            let _ = DROP_POOL.send(storage);
        }
    }
}

impl Mapping {
    fn range(&self, offset: usize, len: usize) -> Result<*mut u8, ()> {
        if offset
            .checked_add(len)
            .filter(|end| *end <= self.len)
            .is_none()
        {
            return Err(());
        }
        Ok(unsafe { self.address.cast::<u8>().add(offset) })
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        if !self.address.is_null() {
            let _ = unsafe { mm::munmap(self.address, self.len) };
        }
    }
}

struct MappingAccess {
    finished: bool,
}

impl MappingAccess {
    fn begin(mapping: &Mapping) -> Self {
        ACTIVE_MAPPING.with(|active| {
            assert_eq!(active.get().address, 0, "recursive SHM mapping access");
            active.set(ActiveMapping {
                address: mapping.address as usize,
                len: mapping.len,
                faulted: false,
            });
        });
        Self { finished: false }
    }

    fn finish(mut self) -> Result<(), ()> {
        self.finished = true;
        let faulted = ACTIVE_MAPPING.with(|active| {
            let faulted = active.get().faulted;
            active.set(ActiveMapping::default());
            faulted
        });
        (!faulted).then_some(()).ok_or(())
    }
}

impl Drop for MappingAccess {
    fn drop(&mut self) {
        if !self.finished {
            ACTIVE_MAPPING.with(|active| active.set(ActiveMapping::default()));
        }
    }
}

fn install_sigbus_handler() -> Result<(), PoolCreateError> {
    let result = SIGBUS_HANDLER.get_or_init(|| unsafe {
        signal_hook_registry::register_sigaction(Signal::BUS.as_raw(), |info| {
            let fault = info.si_addr() as usize;
            let handled = ACTIVE_MAPPING.with(|active| {
                let mut mapping = active.get();
                let contains = mapping.address != 0
                    && fault >= mapping.address
                    && fault < mapping.address.saturating_add(mapping.len);
                if contains {
                    let replaced = mm::mmap_anonymous(
                        mapping.address as *mut c_void,
                        mapping.len,
                        ProtFlags::READ | ProtFlags::WRITE,
                        MapFlags::PRIVATE | MapFlags::FIXED,
                    )
                    .is_ok();
                    if !replaced {
                        std::process::abort();
                    }
                    mapping.faulted = true;
                    active.set(mapping);
                }
                contains
            });
            if !handled {
                std::process::abort();
            }
        })
    });
    result
        .as_ref()
        .map(|_| ())
        .map_err(|_| PoolCreateError::Signal)
}

fn file_size(fd: &OwnedFd) -> Option<usize> {
    let size = fstat(fd).ok()?.st_size;
    usize::try_from(size).ok()
}

#[cfg(test)]
mod tests {
    use std::os::fd::AsFd;

    use rustix::{
        fs::{MemfdFlags, ftruncate, memfd_create},
        io::dup,
    };

    use super::*;

    fn pool(size: usize) -> (Pool, OwnedFd) {
        let client = memfd_create("tensor-shm-pool-test", MemfdFlags::CLOEXEC).unwrap();
        ftruncate(&client, u64::try_from(size).unwrap()).unwrap();
        let server = dup(client.as_fd()).unwrap();
        (Pool::new(server, size).unwrap(), client)
    }

    #[test]
    fn mapping_access_borrows_the_requested_subrange_without_copying() {
        let (pool, _client) = pool(4096);
        pool.with_data_mut(128, 4, |ptr| unsafe {
            ptr.copy_from_nonoverlapping([1, 2, 3, 4].as_ptr(), 4);
        })
        .unwrap();
        let value = pool
            .with_data(128, 4, |ptr| unsafe {
                [
                    ptr.read(),
                    ptr.add(1).read(),
                    ptr.add(2).read(),
                    ptr.add(3).read(),
                ]
            })
            .unwrap();
        assert_eq!(value, [1, 2, 3, 4]);
    }

    #[test]
    fn row_copy_drops_source_padding() {
        let (pool, _client) = pool(4096);
        pool.with_data_mut(128, 12, |ptr| unsafe {
            ptr.copy_from_nonoverlapping([1, 2, 3, 4, 99, 99, 5, 6, 7, 8, 88, 88].as_ptr(), 12);
        })
        .unwrap();
        let mut destination = [0; 8];
        pool.copy_rows(128, 6, 4, 2, &mut destination).unwrap();
        assert_eq!(destination, [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn resize_is_grow_only_and_preserves_existing_bytes() {
        let (pool, client) = pool(4096);
        pool.with_data_mut(17, 1, |ptr| unsafe { ptr.write(0x5a) })
            .unwrap();
        ftruncate(&client, 8192).unwrap();
        pool.resize(8192).unwrap();
        assert_eq!(pool.size(), 8192);
        assert_eq!(
            pool.with_data(17, 1, |ptr| unsafe { ptr.read() }).unwrap(),
            0x5a
        );
        assert!(matches!(pool.resize(4096), Err(PoolResizeError::Shrink)));
    }

    #[test]
    fn truncated_backing_file_becomes_a_protocol_error_instead_of_crashing() {
        let (pool, client) = pool(4096);
        ftruncate(&client, 0).unwrap();
        let access = pool.with_data(1024, 1, |ptr| unsafe { ptr.read_volatile() });
        assert!(access.is_err());
    }
}
