use std::fmt;
use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd};
use std::sync::Arc;

use vulkanalia::{
    prelude::v1_4::*,
    vk::{self, KhrExternalSemaphoreFdExtensionDeviceCommands},
};

use crate::backend::DeviceOwner;
use crate::{Backend, Error, PipelineStages, Result, SemaphoreWait};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BinarySemaphoreDescriptor {
    pub label: Option<String>,
}

/// Device-owned binary semaphore for swapchain acquire/submit/present chains.
pub struct BinarySemaphore {
    owner: Arc<DeviceOwner>,
    raw: vk::Semaphore,
    label: Option<String>,
    sync_fd_exportable: bool,
}

impl BinarySemaphore {
    pub const fn raw(&self) -> vk::Semaphore {
        self.raw
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn wait(&self, stages: PipelineStages) -> Result<SemaphoreWait> {
        if stages.is_empty() {
            return Err(Error::Validation(
                "binary semaphore wait stage mask must be non-empty".into(),
            ));
        }
        Ok(SemaphoreWait {
            semaphore: self.raw,
            value: 0,
            stages: stages.to_vk(),
        })
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }

    /// Exports the semaphore payload as a Linux `SYNC_FD`.
    ///
    /// # Safety
    ///
    /// The semaphore must have a pending or completed signal operation and
    /// must not be concurrently accessed by another host operation.
    pub unsafe fn export_sync_fd(&self) -> Result<OwnedFd> {
        if !self.sync_fd_exportable {
            return Err(Error::Validation(
                "binary semaphore was not created for SYNC_FD export".into(),
            ));
        }
        let info = vk::SemaphoreGetFdInfoKHR::builder()
            .semaphore(self.raw)
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
        let raw = unsafe { self.owner.device.get_semaphore_fd_khr(&info) }
            .map_err(|source| Error::vulkan("vkGetSemaphoreFdKHR(SYNC_FD)", source))?;
        if raw < 0 {
            return Err(Error::Validation(
                "vkGetSemaphoreFdKHR returned an invalid file descriptor".into(),
            ));
        }
        Ok(unsafe { OwnedFd::from_raw_fd(raw) })
    }
}

impl fmt::Debug for BinarySemaphore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BinarySemaphore")
            .field("raw", &self.raw)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl Drop for BinarySemaphore {
    fn drop(&mut self) {
        unsafe { self.owner.device.destroy_semaphore(self.raw, None) };
    }
}

impl Backend {
    pub fn create_binary_semaphore(
        &self,
        descriptor: &BinarySemaphoreDescriptor,
    ) -> Result<BinarySemaphore> {
        let owner = self.shared_owner();
        let raw = unsafe {
            owner
                .device
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
        }
        .map_err(|source| Error::vulkan("vkCreateSemaphore(binary)", source))?;
        Ok(BinarySemaphore {
            owner,
            raw,
            label: descriptor.label.clone(),
            sync_fd_exportable: false,
        })
    }

    /// Creates a binary semaphore whose signal payload can be exported as a
    /// Linux `SYNC_FD` for Wayland explicit sync or DRM/KMS.
    pub fn create_exportable_sync_fd_semaphore(
        &self,
        descriptor: &BinarySemaphoreDescriptor,
    ) -> Result<BinarySemaphore> {
        if !self
            .features()
            .contains(crate::Features::EXTERNAL_SEMAPHORE_SYNC_FD)
        {
            return Err(Error::Validation(
                "EXTERNAL_SEMAPHORE_SYNC_FD was not enabled on this Device".into(),
            ));
        }
        let owner = self.shared_owner();
        let mut export = vk::ExportSemaphoreCreateInfo::builder()
            .handle_types(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
        let create = vk::SemaphoreCreateInfo::builder().push_next(&mut export);
        let raw = unsafe { owner.device.create_semaphore(&create, None) }
            .map_err(|source| Error::vulkan("vkCreateSemaphore(export SYNC_FD)", source))?;
        Ok(BinarySemaphore {
            owner,
            raw,
            label: descriptor.label.clone(),
            sync_fd_exportable: true,
        })
    }

    /// Imports a Linux `SYNC_FD` payload into a temporary binary semaphore.
    /// Vulkan takes ownership of `fd` only after a successful import.
    pub fn import_sync_fd_semaphore(
        &self,
        descriptor: &BinarySemaphoreDescriptor,
        fd: OwnedFd,
    ) -> Result<BinarySemaphore> {
        if !self
            .features()
            .contains(crate::Features::EXTERNAL_SEMAPHORE_SYNC_FD)
        {
            return Err(Error::Validation(
                "EXTERNAL_SEMAPHORE_SYNC_FD was not enabled on this Device".into(),
            ));
        }
        let owner = self.shared_owner();
        let raw = unsafe {
            owner
                .device
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
        }
        .map_err(|source| Error::vulkan("vkCreateSemaphore(import SYNC_FD)", source))?;
        let fd = fd.into_raw_fd();
        let import = vk::ImportSemaphoreFdInfoKHR::builder()
            .semaphore(raw)
            .flags(vk::SemaphoreImportFlags::TEMPORARY)
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
            .fd(fd);
        if let Err(source) = unsafe { owner.device.import_semaphore_fd_khr(&import) } {
            unsafe {
                owner.device.destroy_semaphore(raw, None);
                drop(OwnedFd::from_raw_fd(fd));
            }
            return Err(Error::vulkan("vkImportSemaphoreFdKHR(SYNC_FD)", source));
        }
        Ok(BinarySemaphore {
            owner,
            raw,
            label: descriptor.label.clone(),
            sync_fd_exportable: false,
        })
    }
}
