//! Tensor-owned `ext-image-copy-capture-v1` wire and session lifetime.

mod cursor;
mod frame;

use std::sync::{Arc, Mutex};

use tensor_util::BufferSize;
use wayland_protocols::ext::image_copy_capture::v1::server::{
    ext_image_copy_capture_cursor_session_v1::ExtImageCopyCaptureCursorSessionV1,
    ext_image_copy_capture_manager_v1::{self, ExtImageCopyCaptureManagerV1},
    ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, New, Resource, WEnum,
    backend::{ClientId, GlobalId},
    protocol::wl_shm,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    globals::image_capture_source::ImageCaptureSource,
    state::RuntimeState,
};

pub(in crate::protocol) use cursor::{
    CursorSession, CursorSessionData, CursorSessionRef, CursorSessionUpdate, CursorSessionWeak,
};
use frame::FrameData;
pub(in crate::protocol) use frame::{CaptureFailureReason, Frame, FrameRef};

const VERSION: u32 = 1;

pub(crate) struct ImageCopyCaptureProtocol {
    _global: GlobalId,
}

impl ImageCopyCaptureProtocol {
    pub(crate) fn new(
        display: &DisplayHandle,
        filter: impl Fn(&Client) -> bool + Send + Sync + 'static,
    ) -> Self {
        let global = display.create_global::<RuntimeState, ExtImageCopyCaptureManagerV1, _>(
            VERSION,
            ImageCopyCaptureGlobalData {
                filter: Box::new(filter),
            },
        );
        Self { _global: global }
    }
}

pub(in crate::protocol) struct ImageCopyCaptureGlobalData {
    filter: Box<dyn Fn(&Client) -> bool + Send + Sync>,
}

#[derive(Debug)]
pub(in crate::protocol) struct ImageCopyCaptureManagerData;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::protocol) struct BufferConstraints {
    pub(in crate::protocol) size: BufferSize<i32>,
    pub(in crate::protocol) shm: [wl_shm::Format; 2],
    pub(in crate::protocol) shm_count: u8,
}

impl BufferConstraints {
    fn send(self, session: &ExtImageCopyCaptureSessionV1) {
        session.buffer_size(self.size.w as u32, self.size.h as u32);
        for format in self.shm_formats() {
            session.shm_format(format);
        }
        session.done();
    }

    pub(in crate::protocol) fn accepts(
        self,
        width: i32,
        height: i32,
        format: wl_shm::Format,
    ) -> bool {
        width >= self.size.w && height >= self.size.h && self.shm_formats().any(|f| f == format)
    }

    fn shm_formats(self) -> impl Iterator<Item = wl_shm::Format> {
        self.shm
            .into_iter()
            .take(usize::from(self.shm_count.min(2)))
    }
}

#[derive(Debug)]
struct SessionInner {
    source: SessionSource,
    constraints: Option<BufferConstraints>,
    stopped: bool,
    frame_live: bool,
    draw_cursors: bool,
}

#[derive(Debug)]
enum SessionSource {
    Image(ImageCaptureSource),
    Cursor(CursorSessionWeak),
}

#[derive(Clone, Debug)]
pub(in crate::protocol) struct SessionRef {
    object: ExtImageCopyCaptureSessionV1,
    inner: Arc<Mutex<SessionInner>>,
}

impl PartialEq for SessionRef {
    fn eq(&self, other: &Self) -> bool {
        self.object == other.object
    }
}

impl SessionRef {
    pub(in crate::protocol) fn source(&self) -> ImageCaptureSource {
        match &self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .source
        {
            SessionSource::Image(source) => source.clone(),
            SessionSource::Cursor(cursor) => cursor
                .upgrade()
                .map(|cursor| cursor.source())
                .unwrap_or_else(ImageCaptureSource::invalid),
        }
    }

    pub(in crate::protocol) fn cursor(&self) -> Option<CursorSessionRef> {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match &inner.source {
            SessionSource::Cursor(cursor) => cursor.upgrade(),
            SessionSource::Image(_) => None,
        }
    }

    pub(in crate::protocol) fn draw_cursors(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .draw_cursors
    }

    pub(super) fn stopped(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stopped
    }

    pub(super) fn update_constraints(&self, constraints: BufferConstraints) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !self.object.is_alive() || inner.stopped {
            return;
        }
        constraints.send(&self.object);
        inner.constraints = Some(constraints);
    }

    pub(super) fn frame_destroyed(&self) {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .frame_live = false;
    }

    pub(super) fn stop(&self) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.stopped {
            return;
        }
        inner.stopped = true;
        inner.constraints = None;
        if self.object.is_alive() {
            self.object.stopped();
        }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct Session(SessionRef);

impl Session {
    pub(in crate::protocol) fn as_ref(&self) -> &SessionRef {
        &self.0
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let mut inner = self
            .0
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.stopped {
            return;
        }
        inner.stopped = true;
        inner.constraints = None;
        if self.0.object.is_alive() {
            self.0.object.stopped();
        }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct SessionData {
    inner: Arc<Mutex<SessionInner>>,
}

impl SessionData {
    fn new(source: SessionSource, draw_cursors: bool) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SessionInner {
                source,
                constraints: None,
                stopped: false,
                frame_live: false,
                draw_cursors,
            })),
        }
    }

    fn stopped() -> Self {
        let data = Self::new(SessionSource::Image(ImageCaptureSource::invalid()), false);
        data.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stopped = true;
        data
    }

    fn cursor(cursor: CursorSessionWeak) -> Self {
        Self::new(SessionSource::Cursor(cursor), false)
    }
}

pub(super) trait ImageCopyCaptureHandler: 'static {
    fn capture_constraints(&self, source: &ImageCaptureSource) -> Option<BufferConstraints>;
    fn new_capture_session(&mut self, session: Session);
    fn capture_session_destroyed(&mut self, session: &SessionRef);
    fn capture_frame(&mut self, session: &SessionRef, frame: Frame);
    fn capture_frame_aborted(&mut self, frame: &FrameRef);
    fn new_cursor_capture_session(&mut self, session: CursorSession);
    fn cursor_capture_session_destroyed(&mut self, session: &CursorSessionRef);
    fn cursor_capture_update(&self, session: &CursorSessionRef) -> Option<CursorSessionUpdate>;
}

impl ImageCopyCaptureHandler for RuntimeState {
    fn capture_constraints(&self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        self.capture_constraints_for_source(source)
    }

    fn new_capture_session(&mut self, session: Session) {
        self.store_capture_session(session);
    }

    fn capture_session_destroyed(&mut self, session: &SessionRef) {
        self.drop_capture_session(session);
    }

    fn capture_frame(&mut self, session: &SessionRef, frame: Frame) {
        self.handle_capture_frame(session, frame);
    }

    fn capture_frame_aborted(&mut self, frame: &FrameRef) {
        self.abort_capture_frame(frame);
    }

    fn new_cursor_capture_session(&mut self, session: CursorSession) {
        self.store_cursor_capture_session(session);
    }

    fn cursor_capture_session_destroyed(&mut self, session: &CursorSessionRef) {
        self.drop_cursor_capture_session(session);
    }

    fn cursor_capture_update(&self, session: &CursorSessionRef) -> Option<CursorSessionUpdate> {
        self.cursor_capture_update_for_session(session)
    }
}

impl<D> GlobalDispatchDelegate<ExtImageCopyCaptureManagerV1, D> for ImageCopyCaptureGlobalData
where
    D: Dispatch<ExtImageCopyCaptureManagerV1, ImageCopyCaptureManagerData> + 'static,
{
    fn bind(
        &self,
        _state: &mut D,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ExtImageCopyCaptureManagerV1>,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(resource, ImageCopyCaptureManagerData);
    }

    fn can_view(&self, client: &Client) -> bool {
        (self.filter)(client)
    }
}

impl<D> DispatchDelegate<ExtImageCopyCaptureManagerV1, D> for ImageCopyCaptureManagerData
where
    D: Dispatch<ExtImageCopyCaptureSessionV1, SessionData>,
    D: Dispatch<ExtImageCopyCaptureCursorSessionV1, CursorSessionData>,
    D: ImageCopyCaptureHandler,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        manager: &ExtImageCopyCaptureManagerV1,
        request: ext_image_copy_capture_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_image_copy_capture_manager_v1::Request::CreateSession {
                session,
                source,
                options,
            } => {
                let WEnum::Value(options) = options else {
                    manager.post_error(
                        ext_image_copy_capture_manager_v1::Error::InvalidOption,
                        "unknown image-copy-capture option bits",
                    );
                    return;
                };
                if options.bits() & !ext_image_copy_capture_manager_v1::Options::all().bits() != 0 {
                    manager.post_error(
                        ext_image_copy_capture_manager_v1::Error::InvalidOption,
                        "unknown image-copy-capture option bits",
                    );
                    return;
                }
                let Some(source) = ImageCaptureSource::from_resource(&source) else {
                    create_stopped_session(session, data_init);
                    return;
                };
                let data = SessionData::new(
                    SessionSource::Image(source.clone()),
                    options.contains(ext_image_copy_capture_manager_v1::Options::PaintCursors),
                );
                let object = data_init.init(
                    session,
                    SessionData {
                        inner: Arc::clone(&data.inner),
                    },
                );
                let session = SessionRef {
                    object,
                    inner: data.inner,
                };
                if let Some(constraints) = state.capture_constraints(&source) {
                    session.update_constraints(constraints);
                    state.new_capture_session(Session(session));
                } else {
                    session
                        .inner
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .stopped = true;
                    session.object.stopped();
                }
            }
            ext_image_copy_capture_manager_v1::Request::CreatePointerCursorSession {
                session,
                source,
                pointer,
            } => {
                cursor::create_pointer_cursor_session(state, session, source, pointer, data_init);
            }
            ext_image_copy_capture_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

pub(super) fn create_cursor_capture_session<D>(
    state: &mut D,
    cursor: &CursorSessionRef,
    session: New<ExtImageCopyCaptureSessionV1>,
    data_init: &mut DataInit<'_, D>,
) where
    D: Dispatch<ExtImageCopyCaptureSessionV1, SessionData>,
    D: ImageCopyCaptureHandler,
{
    let data = SessionData::cursor(cursor.downgrade());
    let object = data_init.init(
        session,
        SessionData {
            inner: Arc::clone(&data.inner),
        },
    );
    let session = SessionRef {
        object,
        inner: data.inner,
    };
    if cursor.stopped() {
        session.stop();
        return;
    }
    cursor.attach_capture_session(session.clone());
    if let Some(update) = state.cursor_capture_update(cursor) {
        cursor.apply_update(update);
    }
    if session
        .inner
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .constraints
        .is_some()
    {
        state.new_capture_session(Session(session));
    } else {
        session.stop();
    }
}

fn create_stopped_session<D>(
    session: New<ExtImageCopyCaptureSessionV1>,
    data_init: &mut DataInit<'_, D>,
) where
    D: Dispatch<ExtImageCopyCaptureSessionV1, SessionData> + 'static,
{
    let object = data_init.init(session, SessionData::stopped());
    object.stopped();
}

impl<D> DispatchDelegate<ExtImageCopyCaptureSessionV1, D> for SessionData
where
    D: Dispatch<wayland_protocols::ext::image_copy_capture::v1::server::ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1, FrameData>,
    D: ImageCopyCaptureHandler,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        session: &ExtImageCopyCaptureSessionV1,
        request: ext_image_copy_capture_session_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_image_copy_capture_session_v1::Request::CreateFrame { frame } => {
                let mut inner = self
                    .inner
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if inner.frame_live {
                    session.post_error(
                        ext_image_copy_capture_session_v1::Error::DuplicateFrame,
                        "a capture frame already exists for this session",
                    );
                    return;
                }
                inner.frame_live = true;
                let session = SessionRef {
                    object: session.clone(),
                    inner: Arc::clone(&self.inner),
                };
                let data = FrameData::new(session, inner.constraints);
                drop(inner);
                data_init.init(frame, data);
            }
            ext_image_copy_capture_session_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut D, _client: ClientId, session: &ExtImageCopyCaptureSessionV1) {
        state.capture_session_destroyed(&SessionRef {
            object: session.clone(),
            inner: Arc::clone(&self.inner),
        });
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ExtImageCopyCaptureManagerV1,
    ImageCopyCaptureGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ExtImageCopyCaptureManagerV1,
    ImageCopyCaptureManagerData
);
delegate_dispatch!(RuntimeState, ExtImageCopyCaptureSessionV1, SessionData);
delegate_dispatch!(
    RuntimeState,
    ExtImageCopyCaptureCursorSessionV1,
    CursorSessionData
);
