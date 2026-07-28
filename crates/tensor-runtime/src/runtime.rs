//! Sized io_uring runtime construction for Tensor completion services.

use std::io;

use compio::{
    driver::{DriverType, ProactorBuilder},
    runtime::Runtime,
};

/// Small rings still need enough entries for cancellation and wake operations.
const MIN_RING_CAPACITY: u32 = 8;

/// Build an io_uring-only Compio runtime sized for a fixed operation budget.
///
/// `concurrent_ops` is the maximum number of kernel operations a service can
/// keep submitted at once. The SQ size is rounded up once during construction;
/// no runtime growth, fallback driver, or readiness registry is involved.
pub fn io_uring_runtime(concurrent_ops: usize) -> io::Result<Runtime> {
    let requested = u32::try_from(concurrent_ops.max(1)).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "io_uring operation budget exceeds u32",
        )
    })?;
    let capacity = requested
        .checked_next_power_of_two()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "io_uring capacity overflow"))?
        .max(MIN_RING_CAPACITY);

    let mut proactor = ProactorBuilder::new();
    proactor.capacity(capacity).driver_type(DriverType::IoUring);
    let mut runtime = Runtime::builder();
    runtime.with_proactor(proactor);
    runtime.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_completion_service_uses_a_live_io_uring_runtime() {
        io_uring_runtime(2).unwrap().block_on(async {});
    }
}
