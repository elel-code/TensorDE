use std::any::Any;
use std::fmt;
use std::sync::Arc;

use vulkanalia::vk::{self, Handle};

use crate::{Backend, Error, Result, SemaphoreWait};

/// Metadata for a decoder/host-owned timeline semaphore from this logical
/// device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalTimelineSemaphoreDescriptor {
    pub label: Option<String>,
    pub semaphore: vk::Semaphore,
}

impl ExternalTimelineSemaphoreDescriptor {
    fn validate(&self) -> Result<()> {
        if self.semaphore == vk::Semaphore::null() {
            return Err(Error::Validation(
                "external timeline semaphore must not be null".into(),
            ));
        }
        Ok(())
    }
}

/// A non-owning timeline semaphore handle that retains its decoder/host lease.
/// Dropping this object never destroys the Vulkan semaphore.
#[derive(Clone)]
pub struct RetainedExternalTimelineSemaphore {
    inner: Arc<RetainedExternalTimelineSemaphoreInner>,
}

impl fmt::Debug for RetainedExternalTimelineSemaphore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetainedExternalTimelineSemaphore")
            .field("label", &self.inner.label)
            .field("semaphore", &self.inner.semaphore)
            .finish_non_exhaustive()
    }
}

impl RetainedExternalTimelineSemaphore {
    pub fn raw(&self) -> vk::Semaphore {
        self.inner.semaphore
    }

    pub fn label(&self) -> Option<&str> {
        self.inner.label.as_deref()
    }

    /// Creates a queue wait for a positive external timeline value.
    ///
    /// The retained semaphore object must remain alive until the submission
    /// completes. Use [`crate::CommandEncoder::retain_resource`] to attach this
    /// object to the command buffer that records the wait when the caller does
    /// not otherwise retain it through that timeline value.
    pub fn wait(&self, value: u64, stages: vk::PipelineStageFlags2) -> Result<SemaphoreWait> {
        if value == 0 {
            return Err(Error::Validation(
                "external timeline semaphore wait value must be non-zero".into(),
            ));
        }
        if stages.is_empty() {
            return Err(Error::Validation(
                "external timeline semaphore wait stages must be non-empty".into(),
            ));
        }
        Ok(SemaphoreWait {
            semaphore: self.inner.semaphore,
            value,
            stages,
        })
    }
}

impl crate::SubmissionResource for RetainedExternalTimelineSemaphore {
    fn submission_lease(&self) -> crate::SubmissionLease {
        crate::SubmissionLease::new(Arc::clone(&self.inner))
    }
}

struct RetainedExternalTimelineSemaphoreInner {
    _owner: Arc<crate::backend::DeviceOwner>,
    semaphore: vk::Semaphore,
    label: Option<String>,
    _host_lease: Arc<dyn Any + Send + Sync>,
}

impl Backend {
    /// Retains a decoder/host timeline semaphore without taking Vulkan object
    /// ownership.
    ///
    /// # Safety
    ///
    /// The raw handle must name a timeline semaphore created from this exact
    /// logical device and remain valid until `host_lease` is dropped.
    pub unsafe fn retain_external_timeline_semaphore<T>(
        &self,
        descriptor: &ExternalTimelineSemaphoreDescriptor,
        host_lease: Arc<T>,
    ) -> Result<RetainedExternalTimelineSemaphore>
    where
        T: Any + Send + Sync,
    {
        retain_external_timeline_semaphore_for_owner(self.shared_owner(), descriptor, host_lease)
    }
}

pub(crate) fn retain_external_timeline_semaphore_for_owner<T>(
    owner: Arc<crate::backend::DeviceOwner>,
    descriptor: &ExternalTimelineSemaphoreDescriptor,
    host_lease: Arc<T>,
) -> Result<RetainedExternalTimelineSemaphore>
where
    T: Any + Send + Sync,
{
    descriptor.validate()?;
    Ok(RetainedExternalTimelineSemaphore {
        inner: Arc::new(RetainedExternalTimelineSemaphoreInner {
            _owner: owner,
            semaphore: descriptor.semaphore,
            label: descriptor.label.clone(),
            _host_lease: host_lease,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_timeline_descriptor_requires_a_handle() {
        let mut descriptor = ExternalTimelineSemaphoreDescriptor {
            label: None,
            semaphore: vk::Semaphore::null(),
        };
        assert!(descriptor.validate().is_err());
        descriptor.semaphore = vk::Semaphore::from_raw(9);
        assert!(descriptor.validate().is_ok());
    }
}
