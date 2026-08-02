//! Device-local buffers with bounded host-side upload bookkeeping.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::{
    Buffer, BufferDescriptor, BufferUsages, Error, MemoryAllocator, MemoryLocation, Result,
    UploadBatch,
};

/// Creation parameters for a [`DynamicBuffer`].
///
/// The resulting allocation always includes `TRANSFER_DST`; callers provide
/// the consumer usage, such as `VERTEX_BUFFER`, `INDEX_BUFFER`,
/// `STORAGE_BUFFER`, or `INDIRECT_BUFFER`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicBufferDescriptor {
    pub label: Option<String>,
    /// Initial allocation capacity in bytes. It is rounded up to Vulkan's
    /// four-byte transfer-copy granularity.
    pub initial_capacity: u64,
    /// Consumer-visible usages. `TRANSFER_DST` is added automatically.
    pub usage: BufferUsages,
}

impl Default for DynamicBufferDescriptor {
    fn default() -> Self {
        Self {
            label: None,
            initial_capacity: 256,
            usage: BufferUsages::VERTEX,
        }
    }
}

/// Result of one [`DynamicBuffer::upload`] request.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicBufferUpload {
    /// Number of source bytes copied this frame. Zero means the unchanged
    /// content cache avoided a transfer command.
    pub bytes_written: u64,
    /// Whether this request replaced the device-local allocation.
    pub reallocated: bool,
}

/// A device-local buffer that grows geometrically and skips unchanged uploads.
///
/// Replacements are safe while earlier submissions are in flight: transfer and
/// vertex/index bind recording retain the prior [`Buffer`] through its
/// submission timeline.
pub struct DynamicBuffer {
    allocator: MemoryAllocator,
    descriptor: DynamicBufferDescriptor,
    buffer: Buffer,
    capacity: u64,
    content_size: u64,
    content_hash: Option<u64>,
}

impl std::fmt::Debug for DynamicBuffer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DynamicBuffer")
            .field("label", &self.descriptor.label)
            .field("capacity", &self.capacity)
            .field("content_size", &self.content_size)
            .field("usage", &self.descriptor.usage)
            .finish_non_exhaustive()
    }
}

impl DynamicBuffer {
    pub fn new(allocator: &MemoryAllocator, descriptor: DynamicBufferDescriptor) -> Result<Self> {
        if descriptor.usage.is_empty() {
            return Err(Error::Validation(
                "dynamic buffer usage must name at least one consumer role".into(),
            ));
        }
        let capacity = transfer_aligned_capacity(descriptor.initial_capacity.max(4))?;
        let buffer = create_buffer(allocator, &descriptor, capacity)?;
        Ok(Self {
            allocator: allocator.clone(),
            descriptor,
            buffer,
            capacity,
            content_size: 0,
            content_hash: None,
        })
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub const fn capacity(&self) -> u64 {
        self.capacity
    }

    pub const fn content_size(&self) -> u64 {
        self.content_size
    }

    pub const fn is_empty(&self) -> bool {
        self.content_size == 0
    }

    /// Stages `data` into the device-local allocation when it changed.
    ///
    /// The byte length must satisfy Vulkan's four-byte copy granularity.
    /// Empty data clears the logical content without issuing a GPU command.
    pub fn upload(
        &mut self,
        uploads: &mut UploadBatch<'_>,
        data: &[u8],
    ) -> Result<DynamicBufferUpload> {
        let size = u64::try_from(data.len())
            .map_err(|_| Error::Validation("dynamic buffer data exceeds u64".into()))?;
        if !size.is_multiple_of(4) {
            return Err(Error::Validation(
                "dynamic buffer data must have a four-byte length".into(),
            ));
        }
        let hash = hash_data(data);
        if self.content_size == size && self.content_hash == Some(hash) {
            return Ok(DynamicBufferUpload::default());
        }
        if data.is_empty() {
            self.content_size = 0;
            self.content_hash = Some(hash);
            return Ok(DynamicBufferUpload::default());
        }

        let replacement = (size > self.capacity)
            .then(|| {
                let capacity = next_capacity(size)?;
                let buffer = create_buffer(&self.allocator, &self.descriptor, capacity)?;
                Ok::<_, Error>((capacity, buffer))
            })
            .transpose()?;
        let destination = replacement
            .as_ref()
            .map_or(&self.buffer, |(_, buffer)| buffer);
        unsafe { uploads.write_buffer(destination, 0, data)? };
        let reallocated = replacement.is_some();
        if let Some((capacity, buffer)) = replacement {
            self.capacity = capacity;
            self.buffer = buffer;
        }
        self.content_size = size;
        self.content_hash = Some(hash);
        Ok(DynamicBufferUpload {
            bytes_written: size,
            reallocated,
        })
    }
}

fn create_buffer(
    allocator: &MemoryAllocator,
    descriptor: &DynamicBufferDescriptor,
    capacity: u64,
) -> Result<Buffer> {
    allocator.create_buffer(&BufferDescriptor {
        label: descriptor.label.clone(),
        size: capacity,
        usage: descriptor.usage | BufferUsages::COPY_DESTINATION,
        memory: MemoryLocation::Device,
    })
}

fn transfer_aligned_capacity(size: u64) -> Result<u64> {
    size.checked_add(3)
        .map(|size| size & !3)
        .filter(|size| *size != 0)
        .ok_or_else(|| Error::Validation("dynamic buffer capacity overflows".into()))
}

fn next_capacity(required: u64) -> Result<u64> {
    transfer_aligned_capacity(
        required
            .checked_next_power_of_two()
            .ok_or_else(|| Error::Validation("dynamic buffer capacity overflows".into()))?,
    )
}

fn hash_data(data: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::{hash_data, next_capacity, transfer_aligned_capacity};

    #[test]
    fn capacity_is_copy_aligned_and_grows_geometrically() {
        assert_eq!(transfer_aligned_capacity(1).unwrap(), 4);
        assert_eq!(transfer_aligned_capacity(8).unwrap(), 8);
        assert_eq!(next_capacity(24).unwrap(), 32);
        assert_eq!(next_capacity(33).unwrap(), 64);
    }

    #[test]
    fn content_hash_tracks_bytes_and_length() {
        assert_eq!(hash_data(&[1, 2, 3]), hash_data(&[1, 2, 3]));
        assert_ne!(hash_data(&[1, 2, 3]), hash_data(&[1, 2, 4]));
        assert_ne!(hash_data(&[1, 2, 3]), hash_data(&[1, 2, 3, 0]));
    }
}
