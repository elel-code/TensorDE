//! Core `wl_shm` helpers: anonymous file + pool + ARGB8888 buffer.

use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsFd, FromRawFd, OwnedFd};
use std::os::unix::io::AsRawFd;

use wayland_client::protocol::{wl_buffer, wl_shm, wl_shm_pool};
use wayland_client::QueueHandle;

/// Create a sealed memfd-backed file of at least `size` bytes.
pub fn create_memfd(size: usize) -> io::Result<File> {
    let name = c"fika-wl-shm";
    let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let file = File::from(owned);
    file.set_len(size as u64)?;
    // Best-effort seals so the compositor can map safely.
    let _ = unsafe {
        libc::fcntl(
            file.as_raw_fd(),
            libc::F_ADD_SEALS,
            libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL,
        )
    };
    Ok(file)
}

/// Fill an ARGB8888 buffer with a solid color.
///
/// `argb` is `[a, r, g, b]` each 0..=255. Stored as little-endian ARGB8888
/// (byte order B,G,R,A) which is what `wl_shm::Format::Argb8888` expects on LE.
pub fn fill_argb8888(file: &mut File, width: u32, height: u32, argb: [u8; 4]) -> io::Result<()> {
    let [a, r, g, b] = argb;
    let bgra = [b, g, r, a];
    let mut writer = io::BufWriter::new(file);
    for _ in 0..(width * height) {
        writer.write_all(&bgra)?;
    }
    writer.flush()?;
    Ok(())
}

/// Create a single-buffer pool attachment for a solid-color surface.
pub fn create_solid_buffer<State: 'static>(
    shm: &wl_shm::WlShm,
    qh: &QueueHandle<State>,
    width: u32,
    height: u32,
    argb: [u8; 4],
) -> io::Result<(File, wl_shm_pool::WlShmPool, wl_buffer::WlBuffer)>
where
    State: wayland_client::Dispatch<wl_shm_pool::WlShmPool, ()>
        + wayland_client::Dispatch<wl_buffer::WlBuffer, ()>,
{
    let stride = width.saturating_mul(4);
    let size = stride.saturating_mul(height) as usize;
    let mut file = create_memfd(size.max(4))?;
    fill_argb8888(&mut file, width, height, argb)?;
    let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
    let buffer = pool.create_buffer(
        0,
        width as i32,
        height as i32,
        stride as i32,
        wl_shm::Format::Argb8888,
        qh,
        (),
    );
    Ok((file, pool, buffer))
}
