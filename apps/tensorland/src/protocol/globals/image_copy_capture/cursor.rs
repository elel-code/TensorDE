use std::sync::{Arc, Mutex, Weak};

use tensor_util::Point;
use wayland_protocols::ext::{
    image_capture_source::v1::server::ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    image_copy_capture::v1::server::{
        ext_image_copy_capture_cursor_session_v1::{self, ExtImageCopyCaptureCursorSessionV1},
        ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
    },
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, New, Resource, backend::ClientId,
    protocol::wl_pointer::WlPointer,
};

use super::{
    BufferConstraints, ImageCopyCaptureHandler, SessionRef, create_cursor_capture_session,
};
use crate::protocol::{
    dispatch::DispatchDelegate, globals::image_capture_source::ImageCaptureSource,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::protocol) struct CursorSessionUpdate {
    pub(in crate::protocol) constraints: BufferConstraints,
    pub(in crate::protocol) position: Option<Point>,
    pub(in crate::protocol) hotspot: Point,
}

#[derive(Debug)]
struct CursorSessionInner {
    source: ImageCaptureSource,
    pointer: WlPointer,
    stopped: bool,
    capture_session_created: bool,
    capture_session: Option<SessionRef>,
    constraints: Option<BufferConstraints>,
    position: Option<Point>,
    hotspot: Point,
}

#[derive(Clone, Debug)]
pub(in crate::protocol) struct CursorSessionRef {
    object: ExtImageCopyCaptureCursorSessionV1,
    inner: Arc<Mutex<CursorSessionInner>>,
}

impl PartialEq for CursorSessionRef {
    fn eq(&self, other: &Self) -> bool {
        self.object == other.object
    }
}

impl CursorSessionRef {
    pub(in crate::protocol) fn source(&self) -> ImageCaptureSource {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .source
            .clone()
    }

    pub(in crate::protocol) fn pointer(&self) -> WlPointer {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pointer
            .clone()
    }

    pub(super) fn stopped(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stopped
    }

    pub(in crate::protocol) fn constraints(&self) -> Option<BufferConstraints> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .constraints
    }

    pub(in crate::protocol) fn hotspot(&self) -> Point {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .hotspot
    }

    pub(super) fn downgrade(&self) -> CursorSessionWeak {
        CursorSessionWeak {
            object: self.object.clone(),
            inner: Arc::downgrade(&self.inner),
        }
    }

    pub(super) fn attach_capture_session(&self, session: SessionRef) {
        let constraints = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .constraints;
        if let Some(constraints) = constraints {
            session.update_constraints(constraints);
        }
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .capture_session = Some(session);
    }

    pub(in crate::protocol) fn apply_update(&self, update: CursorSessionUpdate) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.stopped || !self.object.is_alive() {
            return;
        }
        if inner.constraints != Some(update.constraints) {
            inner.constraints = Some(update.constraints);
            if let Some(session) = inner.capture_session.as_ref() {
                session.update_constraints(update.constraints);
            }
        }
        match (inner.position, update.position) {
            (None, Some(position)) => {
                self.object.enter();
                self.object.position(position.x, position.y);
                self.object.hotspot(update.hotspot.x, update.hotspot.y);
            }
            (Some(_), None) => self.object.leave(),
            (Some(previous), Some(position)) => {
                if previous != position {
                    self.object.position(position.x, position.y);
                }
                if inner.hotspot != update.hotspot {
                    self.object.hotspot(update.hotspot.x, update.hotspot.y);
                }
            }
            (None, None) => {}
        }
        inner.position = update.position;
        inner.hotspot = update.hotspot;
    }

    pub(in crate::protocol) fn stop(&self) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.stopped {
            return;
        }
        inner.stopped = true;
        inner.constraints = None;
        if let Some(session) = inner.capture_session.as_ref() {
            session.stop();
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::protocol) struct CursorSessionWeak {
    object: ExtImageCopyCaptureCursorSessionV1,
    inner: Weak<Mutex<CursorSessionInner>>,
}

impl CursorSessionWeak {
    pub(super) fn upgrade(&self) -> Option<CursorSessionRef> {
        Some(CursorSessionRef {
            object: self.object.clone(),
            inner: self.inner.upgrade()?,
        })
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct CursorSession(CursorSessionRef);

impl CursorSession {
    pub(in crate::protocol) fn as_ref(&self) -> &CursorSessionRef {
        &self.0
    }
}

impl Drop for CursorSession {
    fn drop(&mut self) {
        self.0.stop();
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct CursorSessionData {
    inner: Arc<Mutex<CursorSessionInner>>,
}

pub(super) fn create_pointer_cursor_session<D>(
    state: &mut D,
    session: New<ExtImageCopyCaptureCursorSessionV1>,
    source: ExtImageCaptureSourceV1,
    pointer: WlPointer,
    data_init: &mut DataInit<'_, D>,
) where
    D: Dispatch<ExtImageCopyCaptureCursorSessionV1, CursorSessionData>,
    D: ImageCopyCaptureHandler,
{
    let source =
        ImageCaptureSource::from_resource(&source).unwrap_or_else(ImageCaptureSource::invalid);
    let inner = Arc::new(Mutex::new(CursorSessionInner {
        source,
        pointer,
        stopped: false,
        capture_session_created: false,
        capture_session: None,
        constraints: None,
        position: None,
        hotspot: Point::new(0, 0),
    }));
    let object = data_init.init(
        session,
        CursorSessionData {
            inner: Arc::clone(&inner),
        },
    );
    let session = CursorSessionRef { object, inner };
    let Some(update) = state.cursor_capture_update(&session) else {
        session.stop();
        return;
    };
    session.apply_update(update);
    state.new_cursor_capture_session(CursorSession(session));
}

impl<D> DispatchDelegate<ExtImageCopyCaptureCursorSessionV1, D> for CursorSessionData
where
    D: Dispatch<ExtImageCopyCaptureSessionV1, super::SessionData>,
    D: ImageCopyCaptureHandler,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        cursor: &ExtImageCopyCaptureCursorSessionV1,
        request: ext_image_copy_capture_cursor_session_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_image_copy_capture_cursor_session_v1::Request::GetCaptureSession { session } => {
                {
                    let mut inner = self
                        .inner
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if inner.capture_session_created {
                        cursor.post_error(
                            ext_image_copy_capture_cursor_session_v1::Error::DuplicateSession,
                            "cursor capture session already created",
                        );
                        return;
                    }
                    inner.capture_session_created = true;
                }
                create_cursor_capture_session(
                    state,
                    &CursorSessionRef {
                        object: cursor.clone(),
                        inner: Arc::clone(&self.inner),
                    },
                    session,
                    data_init,
                );
            }
            ext_image_copy_capture_cursor_session_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut D,
        _client: ClientId,
        cursor: &ExtImageCopyCaptureCursorSessionV1,
    ) {
        state.cursor_capture_session_destroyed(&CursorSessionRef {
            object: cursor.clone(),
            inner: Arc::clone(&self.inner),
        });
    }
}
