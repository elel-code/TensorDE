use std::sync::{Arc, Mutex};
use std::time::Duration;

use wayland_protocols::ext::image_copy_capture::v1::server::ext_image_copy_capture_frame_v1::{
    self, ExtImageCopyCaptureFrameV1,
};
use wayland_server::{
    Client, DataInit, DisplayHandle, Resource,
    backend::ClientId,
    protocol::{wl_buffer::WlBuffer, wl_output},
};

use super::{BufferConstraints, ImageCopyCaptureHandler, SessionRef};
use crate::protocol::{
    dispatch::{DispatchDelegate, delegate_dispatch},
    globals::shm::shm_buffer,
    state::RuntimeState,
};

pub(in crate::protocol) use ext_image_copy_capture_frame_v1::FailureReason as CaptureFailureReason;

#[derive(Debug)]
struct FrameInner {
    buffer: Option<WlBuffer>,
    constraints: Option<BufferConstraints>,
    capture_requested: bool,
    completed: bool,
}

#[derive(Clone, Debug)]
pub(in crate::protocol) struct FrameRef {
    object: ExtImageCopyCaptureFrameV1,
    inner: Arc<Mutex<FrameInner>>,
    session: SessionRef,
}

impl PartialEq for FrameRef {
    fn eq(&self, other: &Self) -> bool {
        self.object == other.object
    }
}

impl FrameRef {
    pub(in crate::protocol) fn buffer(&self) -> WlBuffer {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .buffer
            .clone()
            .expect("capture request validated an attached buffer")
    }

    pub(in crate::protocol) fn session_stopped(&self) -> bool {
        self.session.stopped()
    }

    pub(in crate::protocol) fn is_alive(&self) -> bool {
        self.object.is_alive()
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct Frame {
    reference: FrameRef,
    resolved: bool,
}

impl Frame {
    fn new(reference: FrameRef) -> Self {
        Self {
            reference,
            resolved: false,
        }
    }

    pub(in crate::protocol) fn as_ref(&self) -> &FrameRef {
        &self.reference
    }

    pub(in crate::protocol) fn buffer(&self) -> WlBuffer {
        self.reference.buffer()
    }

    pub(in crate::protocol) fn session_stopped(&self) -> bool {
        self.reference.session_stopped()
    }

    pub(in crate::protocol) fn is_alive(&self) -> bool {
        self.reference.is_alive()
    }

    pub(in crate::protocol) fn success(mut self, presented: Duration) {
        let constraints = {
            let inner = self
                .reference
                .inner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !inner.capture_requested || inner.completed || !self.reference.object.is_alive() {
                self.resolved = true;
                return;
            }
            inner.constraints
        };

        self.reference
            .object
            .transform(wl_output::Transform::Normal);
        if let Some(constraints) = constraints {
            self.reference
                .object
                .damage(0, 0, constraints.size.w, constraints.size.h);
        }
        let seconds = presented.as_secs();
        self.reference.object.presentation_time(
            (seconds >> 32) as u32,
            seconds as u32,
            presented.subsec_nanos(),
        );
        self.reference.object.ready();
        self.reference
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .completed = true;
        self.resolved = true;
    }

    pub(in crate::protocol) fn fail(mut self, reason: CaptureFailureReason) {
        self.reference.fail(reason);
        self.resolved = true;
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        if !self.resolved {
            self.reference.fail(CaptureFailureReason::Unknown);
        }
    }
}

impl FrameRef {
    fn fail(&self, reason: CaptureFailureReason) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.completed {
            return;
        }
        inner.completed = true;
        if inner.capture_requested && self.object.is_alive() {
            self.object.failed(reason);
        }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct FrameData {
    inner: Arc<Mutex<FrameInner>>,
    session: SessionRef,
}

impl FrameData {
    pub(super) fn new(session: SessionRef, constraints: Option<BufferConstraints>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(FrameInner {
                buffer: None,
                constraints,
                capture_requested: false,
                completed: false,
            })),
            session,
        }
    }

    fn reference(&self, object: &ExtImageCopyCaptureFrameV1) -> FrameRef {
        FrameRef {
            object: object.clone(),
            inner: Arc::clone(&self.inner),
            session: self.session.clone(),
        }
    }
}

impl<D> DispatchDelegate<ExtImageCopyCaptureFrameV1, D> for FrameData
where
    D: ImageCopyCaptureHandler,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        frame: &ExtImageCopyCaptureFrameV1,
        request: ext_image_copy_capture_frame_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_image_copy_capture_frame_v1::Request::Destroy => {}
            ext_image_copy_capture_frame_v1::Request::AttachBuffer { buffer } => {
                let mut inner = self
                    .inner
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if inner.capture_requested {
                    frame.post_error(
                        ext_image_copy_capture_frame_v1::Error::AlreadyCaptured,
                        "cannot attach a buffer after capture",
                    );
                    return;
                }
                inner.buffer = Some(buffer);
            }
            ext_image_copy_capture_frame_v1::Request::DamageBuffer {
                x,
                y,
                width,
                height,
            } => {
                let inner = self
                    .inner
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if inner.capture_requested {
                    frame.post_error(
                        ext_image_copy_capture_frame_v1::Error::AlreadyCaptured,
                        "cannot add damage after capture",
                    );
                } else if x < 0 || y < 0 || width <= 0 || height <= 0 {
                    frame.post_error(
                        ext_image_copy_capture_frame_v1::Error::InvalidBufferDamage,
                        "capture buffer damage must have a non-negative origin and positive size",
                    );
                }
            }
            ext_image_copy_capture_frame_v1::Request::Capture => {
                let reference = self.reference(frame);
                let failure = {
                    let mut inner = self
                        .inner
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if inner.capture_requested {
                        frame.post_error(
                            ext_image_copy_capture_frame_v1::Error::AlreadyCaptured,
                            "capture was already requested",
                        );
                        return;
                    }
                    let Some(buffer) = inner.buffer.as_ref() else {
                        frame.post_error(
                            ext_image_copy_capture_frame_v1::Error::NoBuffer,
                            "capture requires an attached buffer",
                        );
                        return;
                    };
                    let failure = validate_buffer(buffer, inner.constraints).err();
                    inner.capture_requested = true;
                    failure
                };
                let owned = Frame::new(reference);
                if self.session.stopped() {
                    owned.fail(CaptureFailureReason::Stopped);
                } else if let Some(reason) = failure {
                    owned.fail(reason);
                } else {
                    state.capture_frame(&self.session, owned);
                }
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut D, _client: ClientId, frame: &ExtImageCopyCaptureFrameV1) {
        self.session.frame_destroyed();
        state.capture_frame_aborted(&self.reference(frame));
    }
}

fn validate_buffer(
    buffer: &WlBuffer,
    constraints: Option<BufferConstraints>,
) -> Result<(), CaptureFailureReason> {
    if !buffer.is_alive() {
        return Err(CaptureFailureReason::BufferConstraints);
    }
    let constraints = constraints.ok_or(CaptureFailureReason::BufferConstraints)?;
    let metadata = shm_buffer(buffer).ok_or(CaptureFailureReason::BufferConstraints)?;
    let metadata = metadata.metadata();
    constraints
        .accepts(metadata.width, metadata.height, metadata.format)
        .then_some(())
        .ok_or(CaptureFailureReason::BufferConstraints)
}

delegate_dispatch!(RuntimeState, ExtImageCopyCaptureFrameV1, FrameData);
