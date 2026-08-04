use std::collections::VecDeque;

use vulkan_renderer::{
    Buffer, BufferDescriptor, BufferState, BufferUsages, ColorImageBufferCopy,
    CommandEncoderDescriptor, Device, Extent2D, Image, MemoryAllocator, MemoryLocation, Origin2D,
    RetainedColorTargetPool, RetainedColorTargetPoolDescriptor, RetainedColorTargetRequest,
    RetainedColorTargetReservation, TextureFormat, TextureState, TextureUsages,
};

use crate::render::{OutputCapturePixels, OutputCaptureRequest, OutputCaptureResult};

const MAX_CAPTURE_TARGETS: usize = 4;
const MAX_CAPTURE_RETAINED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_CAPTURE_EXTENT: Extent2D = Extent2D::new(3_840, 2_160);

pub(super) struct PreparedCaptureTap {
    pub(super) request: OutputCaptureRequest,
    pub(super) image: Image,
    reservation: RetainedColorTargetReservation,
}

struct PendingReadback {
    request: OutputCaptureRequest,
    format: TextureFormat,
    buffer: Buffer,
    timeline_value: u64,
}

pub(super) struct CaptureReadbackManager {
    targets: RetainedColorTargetPool,
    allocator: MemoryAllocator,
    pending: VecDeque<PendingReadback>,
    completed: VecDeque<OutputCaptureResult>,
}

impl CaptureReadbackManager {
    pub(super) fn new(allocator: MemoryAllocator) -> vulkan_renderer::Result<Self> {
        Ok(Self {
            targets: RetainedColorTargetPool::new(
                allocator.clone(),
                capture_target_pool_descriptor(),
            )?,
            allocator,
            pending: VecDeque::with_capacity(MAX_CAPTURE_TARGETS),
            completed: VecDeque::with_capacity(MAX_CAPTURE_TARGETS),
        })
    }

    pub(super) fn prepare(
        &mut self,
        request: OutputCaptureRequest,
        format: TextureFormat,
        completed_timeline: u64,
    ) -> vulkan_renderer::Result<PreparedCaptureTap> {
        if request.region.x < 0 || request.region.y < 0 {
            return Err(vulkan_renderer::Error::Validation(
                "capture tap origin must be output-local and non-negative".into(),
            ));
        }
        let extent = Extent2D::new(request.region.width, request.region.height);
        let acquired = self.targets.acquire(
            RetainedColorTargetRequest {
                extent,
                format,
                additional_usage: TextureUsages::COPY_SOURCE | TextureUsages::COPY_DESTINATION,
            },
            completed_timeline,
        )?;
        Ok(PreparedCaptureTap {
            request,
            image: acquired.target.image.clone(),
            reservation: acquired.reservation(),
        })
    }

    pub(super) fn release(&mut self, capture: PreparedCaptureTap) {
        let _ = self.targets.release(capture.reservation);
    }

    pub(super) fn frame_submitted(
        &mut self,
        renderer: &Device,
        capture: PreparedCaptureTap,
        format: TextureFormat,
        frame_timeline: u64,
    ) {
        let id = capture.request.id;
        if let Err(error) = self.queue_readback(renderer, capture, format, frame_timeline) {
            self.completed.push_back(OutputCaptureResult::Failed {
                id,
                reason: error.to_string(),
            });
        }
    }

    fn queue_readback(
        &mut self,
        renderer: &Device,
        capture: PreparedCaptureTap,
        format: TextureFormat,
        frame_timeline: u64,
    ) -> vulkan_renderer::Result<()> {
        let submitted = (|| {
            let byte_len = capture_byte_len(capture.request)?;
            let buffer = self.allocator.create_buffer(&BufferDescriptor {
                label: Some("tensor-output-capture-readback".into()),
                size: byte_len,
                usage: BufferUsages::COPY_DESTINATION,
                memory: MemoryLocation::Readback,
            })?;
            let mut encoder = renderer.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("tensor-output-capture-readback".into()),
            })?;
            encoder.transition_image(
                &capture.image,
                TextureState::TransferDestination,
                TextureState::TransferSource,
            )?;
            encoder.transition_buffer(
                &buffer,
                BufferState::Undefined,
                BufferState::TransferDestination,
            )?;
            unsafe {
                encoder.copy_color_image_to_buffer(
                    &capture.image,
                    &buffer,
                    &[ColorImageBufferCopy {
                        buffer_offset: 0,
                        buffer_row_length: 0,
                        buffer_image_height: 0,
                        source_mip_level: 0,
                        source_base_array_layer: 0,
                        source_origin: Origin2D::new(0, 0),
                        extent: Extent2D::new(
                            capture.request.region.width,
                            capture.request.region.height,
                        ),
                        layer_count: 1,
                    }],
                )?;
            }
            let command = encoder.finish()?;
            let submission = renderer.queue().submit([command])?;
            Ok::<_, vulkan_renderer::Error>((buffer, submission))
        })();
        let (buffer, submission) = match submitted {
            Ok(submitted) => submitted,
            Err(error) => {
                // The frame-side tap was submitted even though its deferred
                // readback was not. Retire only after that frame completes.
                let _ = self.targets.retire(capture.reservation, frame_timeline);
                return Err(error);
            }
        };
        self.targets
            .retire(capture.reservation, submission.value())?;
        self.pending.push_back(PendingReadback {
            request: capture.request,
            format,
            buffer,
            timeline_value: submission.value(),
        });
        debug_assert!(submission.value() > frame_timeline);
        Ok(())
    }

    pub(super) fn drain_completed(&mut self, completed_timeline: u64) -> Vec<OutputCaptureResult> {
        let mut still_pending = VecDeque::with_capacity(self.pending.capacity());
        while let Some(readback) = self.pending.pop_front() {
            if readback.timeline_value > completed_timeline {
                still_pending.push_back(readback);
                continue;
            }
            let mut bytes = vec![0; usize::try_from(readback.buffer.size()).unwrap_or(0)];
            let result = unsafe { readback.buffer.read(0, &mut bytes) };
            self.completed.push_back(match result {
                Ok(()) => OutputCaptureResult::Ready(OutputCapturePixels {
                    id: readback.request.id,
                    size: readback.request.extent(),
                    format: readback.format,
                    bytes,
                }),
                Err(error) => OutputCaptureResult::Failed {
                    id: readback.request.id,
                    reason: error.to_string(),
                },
            });
        }
        self.pending = still_pending;
        self.completed.drain(..).collect()
    }

    pub(super) fn reject(&mut self, request: OutputCaptureRequest, reason: String) {
        let already_tracked = self
            .pending
            .iter()
            .any(|pending| pending.request.id == request.id)
            || self
                .completed
                .iter()
                .any(|result| result.id() == request.id);
        if !already_tracked {
            self.completed.push_back(OutputCaptureResult::Failed {
                id: request.id,
                reason,
            });
        }
    }
}

fn capture_byte_len(request: OutputCaptureRequest) -> vulkan_renderer::Result<u64> {
    u64::from(request.region.width)
        .checked_mul(u64::from(request.region.height))
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|bytes| *bytes > 0)
        .ok_or_else(|| {
            vulkan_renderer::Error::Validation("capture readback byte size overflowed".into())
        })
}

fn capture_target_pool_descriptor() -> RetainedColorTargetPoolDescriptor {
    RetainedColorTargetPoolDescriptor {
        label: Some("tensor-output-capture-tap".into()),
        max_targets: MAX_CAPTURE_TARGETS,
        max_retained_bytes: MAX_CAPTURE_RETAINED_BYTES,
        max_extent: MAX_CAPTURE_EXTENT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::OutputCaptureId;
    use tensor_util::Rect;

    #[test]
    fn capture_pool_is_bounded_to_protocol_queue_depth() {
        let descriptor = capture_target_pool_descriptor();
        assert_eq!(descriptor.max_targets, MAX_CAPTURE_TARGETS);
        assert_eq!(descriptor.max_extent, Extent2D::new(3_840, 2_160));
    }

    #[test]
    fn capture_byte_size_is_four_bytes_per_pixel() {
        let request = OutputCaptureRequest {
            id: OutputCaptureId::new(1),
            region: Rect::new(0, 0, 1920, 1080),
            draw_cursors: false,
        };
        assert_eq!(capture_byte_len(request).unwrap(), 1920 * 1080 * 4);
    }
}
