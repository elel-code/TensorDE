//! Persistently mapped, timeline-retired staging uploads.

use std::fmt;
use std::sync::Arc;

use vulkanalia::{prelude::v1_4::*, vk};

use crate::backend::DeviceOwner;
use crate::{
    Backend, BinarySemaphore, Buffer, BufferCopy, BufferDescriptor, BufferImageCopy,
    CommandEncoder, CommandEncoderDescriptor, Error, FrameToken, Image, MemoryAllocator,
    MemoryLocation, Queue, Result, SemaphoreWait,
};

mod texture;

pub use texture::{ImageDataLayout, ImageUpload, TexelBlockLayout};

/// Capacity policy for one reusable upload belt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadBeltDescriptor {
    /// Size of ordinary persistently mapped staging chunks.
    pub chunk_size: u64,
    /// Maximum retained chunk count. Exhaustion fails instead of growing
    /// process memory without a bound.
    pub max_chunks: usize,
    /// Hard upper bound for all retained staging-buffer bytes.
    pub max_bytes: u64,
    /// Minimum start alignment of every staged byte range.
    pub offset_alignment: u64,
}

impl Default for UploadBeltDescriptor {
    fn default() -> Self {
        Self {
            chunk_size: 4 * 1024 * 1024,
            max_chunks: 8,
            max_bytes: 32 * 1024 * 1024,
            offset_alignment: 256,
        }
    }
}

/// Observable retained-memory state for profiling and budget decisions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UploadBeltStats {
    pub chunk_count: usize,
    pub retained_bytes: u64,
    pub in_flight_chunks: usize,
}

/// Location of bytes staged by an [`UploadBatch`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadSlice {
    pub chunk_index: usize,
    pub offset: u64,
    pub size: u64,
}

struct UploadChunk {
    buffer: Buffer,
    cursor: u64,
    retire_after: u64,
}

/// Reusable upload memory whose chunks are recycled only after the device
/// timeline proves that their copy commands completed.
pub struct UploadBelt {
    owner: Arc<DeviceOwner>,
    allocator: MemoryAllocator,
    descriptor: UploadBeltDescriptor,
    chunks: Vec<UploadChunk>,
}

impl fmt::Debug for UploadBelt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UploadBelt")
            .field("descriptor", &self.descriptor)
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl Backend {
    /// Creates a bounded, persistently mapped upload belt using `allocator`.
    pub fn create_upload_belt(
        &self,
        allocator: &MemoryAllocator,
        descriptor: UploadBeltDescriptor,
    ) -> Result<UploadBelt> {
        validate_descriptor(descriptor)?;
        let owner = self.shared_owner();
        if !allocator.belongs_to(&owner) {
            return Err(Error::Validation(
                "upload allocator was created by a different Device".into(),
            ));
        }
        Ok(UploadBelt {
            owner,
            allocator: allocator.clone(),
            descriptor,
            chunks: Vec::new(),
        })
    }
}

impl UploadBelt {
    /// Starts one upload/graphics command buffer after reclaiming completed
    /// staging chunks. This performs one non-blocking timeline query.
    pub fn begin<'belt>(
        &'belt mut self,
        queue: &Queue,
        descriptor: &CommandEncoderDescriptor,
    ) -> Result<UploadBatch<'belt>> {
        if !Arc::ptr_eq(&self.owner, &queue.owner) {
            return Err(Error::Validation(
                "upload queue was created by a different Device".into(),
            ));
        }
        let completed = queue.completed_timeline()?;
        for chunk in &mut self.chunks {
            if chunk.retire_after != 0 && chunk.retire_after <= completed {
                chunk.cursor = 0;
                chunk.retire_after = 0;
            }
        }
        let encoder = CommandEncoder::new(Arc::clone(&self.owner), descriptor)?;
        Ok(UploadBatch {
            belt: self,
            encoder: Some(encoder),
            touched: Vec::new(),
            submitted: false,
        })
    }

    pub fn descriptor(&self) -> UploadBeltDescriptor {
        self.descriptor
    }

    pub fn stats(&self) -> UploadBeltStats {
        UploadBeltStats {
            chunk_count: self.chunks.len(),
            retained_bytes: self.chunks.iter().map(|chunk| chunk.buffer.size()).sum(),
            in_flight_chunks: self
                .chunks
                .iter()
                .filter(|chunk| chunk.retire_after != 0)
                .count(),
        }
    }

    /// Drops completed, idle staging chunks while retaining at most one
    /// ordinary chunk to avoid allocation churn.
    pub fn trim(&mut self, queue: &Queue) -> Result<usize> {
        if !Arc::ptr_eq(&self.owner, &queue.owner) {
            return Err(Error::Validation(
                "upload queue was created by a different Device".into(),
            ));
        }
        let completed = queue.completed_timeline()?;
        let before = self.chunks.len();
        let mut kept_idle = false;
        self.chunks.retain(|chunk| {
            if chunk.retire_after > completed {
                return true;
            }
            if !kept_idle && chunk.buffer.size() == self.descriptor.chunk_size {
                kept_idle = true;
                true
            } else {
                false
            }
        });
        Ok(before - self.chunks.len())
    }

    fn reserve(&mut self, size: u64) -> Result<(usize, u64, u64)> {
        let alignment = self.descriptor.offset_alignment.max(4);
        for (index, chunk) in self.chunks.iter_mut().enumerate() {
            if chunk.retire_after != 0 {
                continue;
            }
            let Some(offset) = align_up(chunk.cursor, alignment) else {
                continue;
            };
            if offset
                .checked_add(size)
                .is_some_and(|end| end <= chunk.buffer.size())
            {
                let previous = chunk.cursor;
                chunk.cursor = offset + size;
                return Ok((index, offset, previous));
            }
        }
        if self.chunks.len() >= self.descriptor.max_chunks {
            return Err(Error::Validation(format!(
                "upload belt exhausted its {}-chunk memory bound",
                self.descriptor.max_chunks
            )));
        }
        let minimum = size.max(self.descriptor.chunk_size);
        let chunk_size = minimum
            .checked_next_power_of_two()
            .ok_or_else(|| Error::Validation("upload chunk size overflows".into()))?;
        let retained_bytes = self
            .chunks
            .iter()
            .try_fold(0_u64, |total, chunk| total.checked_add(chunk.buffer.size()))
            .ok_or_else(|| Error::Validation("retained upload bytes overflow".into()))?;
        if retained_bytes
            .checked_add(chunk_size)
            .is_none_or(|total| total > self.descriptor.max_bytes)
        {
            return Err(Error::Validation(format!(
                "upload belt exhausted its {}-byte memory bound",
                self.descriptor.max_bytes
            )));
        }
        let buffer = self.allocator.create_buffer(&BufferDescriptor {
            label: Some(format!("upload-belt-chunk-{}", self.chunks.len())),
            size: chunk_size,
            usage: crate::BufferUsages::COPY_SOURCE,
            memory: MemoryLocation::Upload,
        })?;
        let index = self.chunks.len();
        self.chunks.push(UploadChunk {
            buffer,
            cursor: size,
            retire_after: 0,
        });
        Ok((index, 0, 0))
    }
}

impl Drop for UploadBelt {
    fn drop(&mut self) {
        let retire_after = self
            .chunks
            .iter()
            .map(|chunk| chunk.retire_after)
            .max()
            .unwrap_or(0);
        if retire_after == 0 {
            return;
        }
        let semaphores = [self.owner.timeline()];
        let values = [retire_after];
        let wait = vk::SemaphoreWaitInfo::builder()
            .semaphores(&semaphores)
            .values(&values);
        // Destruction is rare and must not release staging memory still read by
        // the GPU. Device-loss errors imply pending work has been terminated.
        let _ = unsafe { self.owner.device.wait_semaphores(&wait, u64::MAX) };
    }
}

/// One exclusive upload recording transaction.
///
/// Dropping an unsubmitted batch rolls all staging cursors back. Successful
/// submission marks touched chunks unavailable until its timeline completes.
pub struct UploadBatch<'belt> {
    belt: &'belt mut UploadBelt,
    encoder: Option<CommandEncoder>,
    touched: Vec<(usize, u64)>,
    submitted: bool,
}

impl fmt::Debug for UploadBatch<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UploadBatch")
            .field("touched_chunks", &self.touched.len())
            .field("submitted", &self.submitted)
            .finish_non_exhaustive()
    }
}

impl UploadBatch<'_> {
    /// Allows rendering and barriers to be recorded in the same command buffer
    /// as uploads, avoiding a second queue submission.
    pub fn encoder_mut(&mut self) -> &mut CommandEncoder {
        self.encoder.as_mut().expect("live upload batch encoder")
    }

    /// Stages and records one buffer upload.
    ///
    /// # Safety
    ///
    /// The caller must provide the required transfer-to-consumer barrier. The
    /// destination and staging source are retained through submission completion.
    pub unsafe fn write_buffer(
        &mut self,
        destination: &Buffer,
        destination_offset: u64,
        data: &[u8],
    ) -> Result<UploadSlice> {
        if data.is_empty() {
            return Err(Error::Validation("upload data must be non-empty".into()));
        }
        let size = u64::try_from(data.len())
            .map_err(|_| Error::Validation("upload size exceeds u64".into()))?;
        if !destination_offset.is_multiple_of(4) || !size.is_multiple_of(4) {
            return Err(Error::Validation(
                "buffer upload offset and size must be multiples of four bytes".into(),
            ));
        }
        let slice = self.stage(data)?;
        let copy = BufferCopy {
            source_offset: slice.offset,
            destination_offset,
            size,
        };
        let source = &self.belt.chunks[slice.chunk_index].buffer;
        unsafe {
            self.encoder
                .as_mut()
                .expect("live upload batch encoder")
                .copy_buffer_to_buffer(source, destination, &[copy])?;
        }
        Ok(slice)
    }

    /// Stages and records one buffer-to-image upload. `copy.buffer_offset` is
    /// ignored and replaced by the belt allocation. The caller controls exact
    /// Vulkan texel/block packing through the remaining fields.
    ///
    /// # Safety
    ///
    /// `data` must cover the copy region's complete texel/block footprint, and
    /// the caller must transition the image into and out of `layout` as
    /// required. The image and staging source are retained automatically.
    pub unsafe fn write_image(
        &mut self,
        image: &Image,
        layout: vk::ImageLayout,
        mut copy: BufferImageCopy,
        data: &[u8],
    ) -> Result<UploadSlice> {
        if data.is_empty() {
            return Err(Error::Validation("upload data must be non-empty".into()));
        }
        let slice = self.stage(data)?;
        copy.buffer_offset = slice.offset;
        let source = &self.belt.chunks[slice.chunk_index].buffer;
        unsafe {
            self.encoder
                .as_mut()
                .expect("live upload batch encoder")
                .copy_buffer_to_image(source, image, layout, &[copy])?;
        }
        Ok(slice)
    }

    /// Validates texel/block packing, stages only the required byte footprint,
    /// and records one buffer-to-image copy.
    ///
    /// # Safety
    ///
    /// The caller must transition the image into and out of `layout` for the
    /// declared access. The image and staging source are retained automatically.
    pub unsafe fn write_image_data(
        &mut self,
        image: &Image,
        layout: vk::ImageLayout,
        upload: ImageUpload,
        data: &[u8],
    ) -> Result<UploadSlice> {
        let validated = upload.validate(image, data.len())?;
        let required = usize::try_from(validated.required_bytes)
            .map_err(|_| Error::Validation("image upload footprint exceeds usize".into()))?;
        let slice = self.stage(&data[..required])?;
        let mut copy = validated.copy;
        copy.buffer_offset = slice.offset;
        let source = &self.belt.chunks[slice.chunk_index].buffer;
        unsafe {
            self.encoder
                .as_mut()
                .expect("live upload batch encoder")
                .copy_buffer_to_image(source, image, layout, &[copy])?;
        }
        Ok(slice)
    }

    /// Typed-layout variant of [`Self::write_image_data`].
    ///
    /// # Safety
    ///
    /// The caller must transition `image` into and out of `layout` for the
    /// declared transfer access.
    pub unsafe fn write_image_data_typed(
        &mut self,
        image: &Image,
        layout: crate::TextureLayout,
        upload: ImageUpload,
        data: &[u8],
    ) -> Result<UploadSlice> {
        unsafe { self.write_image_data(image, layout.to_vk(), upload, data) }
    }

    fn stage(&mut self, data: &[u8]) -> Result<UploadSlice> {
        let size = u64::try_from(data.len())
            .map_err(|_| Error::Validation("upload size exceeds u64".into()))?;
        let (chunk_index, offset, previous) = self.belt.reserve(size)?;
        if !self.touched.iter().any(|(index, _)| *index == chunk_index) {
            self.touched.push((chunk_index, previous));
        }
        let chunk = &self.belt.chunks[chunk_index];
        unsafe { chunk.buffer.write(offset, data)? };
        Ok(UploadSlice {
            chunk_index,
            offset,
            size,
        })
    }

    /// Finishes and submits this transaction. Staging memory is retired by the
    /// returned timeline token.
    pub fn submit(self, queue: &Queue, waits: &[SemaphoreWait]) -> Result<FrameToken> {
        self.submit_retained(queue, waits, std::iter::empty())
    }

    /// Finishes and submits this transaction while retaining leases until its
    /// timeline token completes.
    pub fn submit_retained<L>(
        mut self,
        queue: &Queue,
        waits: &[SemaphoreWait],
        leases: L,
    ) -> Result<FrameToken>
    where
        L: IntoIterator<Item = crate::SubmissionLease>,
    {
        self.validate_queue(queue)?;
        let encoder = self.encoder.take().expect("live upload batch encoder");
        let command = encoder.finish()?;
        let frame = queue.submit_retained([command], waits, leases)?;
        self.commit(frame);
        Ok(frame)
    }

    /// Finishes and submits this transaction, also signalling binary
    /// semaphores for presentation.
    ///
    /// # Safety
    ///
    /// Every signal semaphore must be unsignalled and have no pending signal.
    pub unsafe fn submit_with_binary_signals(
        self,
        queue: &Queue,
        waits: &[SemaphoreWait],
        signals: &[&BinarySemaphore],
    ) -> Result<FrameToken> {
        unsafe {
            self.submit_retained_with_binary_signals(queue, waits, signals, std::iter::empty())
        }
    }

    /// Finishes and submits this transaction, signals binary semaphores, and
    /// retains leases until its timeline token completes.
    ///
    /// # Safety
    ///
    /// Every signal semaphore must be unsignalled and have no pending signal.
    pub unsafe fn submit_retained_with_binary_signals<L>(
        mut self,
        queue: &Queue,
        waits: &[SemaphoreWait],
        signals: &[&BinarySemaphore],
        leases: L,
    ) -> Result<FrameToken>
    where
        L: IntoIterator<Item = crate::SubmissionLease>,
    {
        self.validate_queue(queue)?;
        let encoder = self.encoder.take().expect("live upload batch encoder");
        let command = encoder.finish()?;
        let frame = unsafe {
            queue.submit_retained_with_binary_signals([command], waits, signals, leases)?
        };
        self.commit(frame);
        Ok(frame)
    }

    fn validate_queue(&self, queue: &Queue) -> Result<()> {
        if !Arc::ptr_eq(&self.belt.owner, &queue.owner) {
            return Err(Error::Validation(
                "upload queue was created by a different Device".into(),
            ));
        }
        Ok(())
    }

    fn commit(&mut self, frame: FrameToken) {
        for (index, _) in &self.touched {
            self.belt.chunks[*index].retire_after = frame.value();
        }
        self.submitted = true;
    }
}

impl Drop for UploadBatch<'_> {
    fn drop(&mut self) {
        if self.submitted {
            return;
        }
        for (index, cursor) in self.touched.drain(..) {
            self.belt.chunks[index].cursor = cursor;
        }
    }
}

fn validate_descriptor(descriptor: UploadBeltDescriptor) -> Result<()> {
    if descriptor.chunk_size == 0 || descriptor.max_chunks == 0 || descriptor.max_bytes == 0 {
        return Err(Error::Validation(
            "upload chunk size, maximum chunk count, and byte bound must be non-zero".into(),
        ));
    }
    if descriptor.chunk_size > descriptor.max_bytes {
        return Err(Error::Validation(
            "ordinary upload chunk size exceeds the upload byte bound".into(),
        ));
    }
    if descriptor.offset_alignment < 4 || !descriptor.offset_alignment.is_power_of_two() {
        return Err(Error::Validation(
            "upload offset alignment must be a power of two of at least four".into(),
        ));
    }
    Ok(())
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let mask = alignment.checked_sub(1)?;
    value.checked_add(mask).map(|value| value & !mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_belt_has_a_bounded_32_mib_retention_policy() {
        let descriptor = UploadBeltDescriptor::default();
        assert_eq!(descriptor.max_bytes, 32 * 1024 * 1024);
        assert!(validate_descriptor(descriptor).is_ok());
    }

    #[test]
    fn upload_alignment_is_strict_and_overflow_checked() {
        assert_eq!(align_up(1, 256), Some(256));
        assert_eq!(align_up(256, 256), Some(256));
        assert_eq!(align_up(u64::MAX, 256), None);
        assert!(
            validate_descriptor(UploadBeltDescriptor {
                offset_alignment: 3,
                ..UploadBeltDescriptor::default()
            })
            .is_err()
        );
    }
}
