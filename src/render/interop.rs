#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeInteropCapabilities {
    pub external_memory_fd: bool,
    pub dma_buf_memory: bool,
    pub drm_format_modifier: bool,
    pub foreign_queue_family: bool,
    pub external_semaphore_fd: bool,
    pub sync_fd_semaphore: bool,
}

impl NativeInteropCapabilities {
    pub const fn is_complete(self) -> bool {
        self.external_memory_fd
            && self.dma_buf_memory
            && self.drm_format_modifier
            && self.foreign_queue_family
            && self.external_semaphore_fd
            && self.sync_fd_semaphore
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_native_interop_capability_is_required() {
        let required = NativeInteropCapabilities {
            external_memory_fd: true,
            dma_buf_memory: true,
            drm_format_modifier: true,
            foreign_queue_family: true,
            external_semaphore_fd: true,
            sync_fd_semaphore: true,
        };
        assert!(required.is_complete());

        for incomplete in [
            NativeInteropCapabilities {
                external_memory_fd: false,
                ..required
            },
            NativeInteropCapabilities {
                dma_buf_memory: false,
                ..required
            },
            NativeInteropCapabilities {
                drm_format_modifier: false,
                ..required
            },
            NativeInteropCapabilities {
                foreign_queue_family: false,
                ..required
            },
            NativeInteropCapabilities {
                external_semaphore_fd: false,
                ..required
            },
            NativeInteropCapabilities {
                sync_fd_semaphore: false,
                ..required
            },
        ] {
            assert!(!incomplete.is_complete());
        }
    }
}
